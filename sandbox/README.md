# Sandboxed development

One microVM per branch. The VM holds a clone of the repository, the Rust
toolchain and Claude Code — and no credentials. The host keeps `~/.claude`, the
GitHub token, `~/.ssh`, the real `.env` and the real `data/`, and hands out
narrow, audited access to them.

Changing any of this: [CLAUDE.md](CLAUDE.md).

## Why

Development already runs code nobody read. `cargo build` executes `build.rs` for
every crate in the tree, `npm install` runs install scripts, and an agent acting
on a web page or an issue body can be steered by text in it. All of it runs as
you, with read access to `~/.claude/.credentials.json`, `~/.ssh`,
`deploy/*.env`, `.env` (`ENCRYPTION_KEY`, the VAPID private key) and
`data/myapps.db` — and the last three sit inside the checkout itself.

**In scope:** a hostile dependency, a hostile prompt, or an agent mistake that
tries to read credentials, phone home, or touch the host filesystem.

**Out of scope:** a hypervisor escape, a backdoored base image, a compromised
broker, and abuse of a capability deliberately granted. The guest can push to
this repository when `github` is granted; that is the point of the capability,
not a hole in it.

## Setup, once

```bash
./devbox.sh doctor           # says what is missing
./devbox.sh build-broker     # the host-side Anthropic broker
./devbox.sh build-image      # the guest image (add --browser for Playwright)
```

You also need:

- **microsandbox** — <https://docs.microsandbox.dev>, and read access to
  `/dev/kvm`.
- **A Claude login on the host** — `claude` once, so
  `~/.claude/.credentials.json` exists. The broker follows that file; it never
  copies it.
- **A GitHub credential**, either
  - `~/.config/msb-devbox/github-token` (`chmod 600`) holding a fine-grained
    PAT — the web UI is the only way to create one, since the API cannot; or
  - `~/.config/msb-devbox/github-app.json` describing a GitHub App, in which
    case `devbox.sh` mints a fresh installation token per sandbox start and that
    token expires an hour later. See
    [brokers/github/mint-token.sh](brokers/github/mint-token.sh).

### What the token may do

Repository access: **only this repository**. The allow-list decides where the
token can go; only its own scope decides what it can do once there, and a
sandboxed agent uses it freely.

| Permission | Level | Why |
|---|---|---|
| Metadata | Read | Mandatory; selected automatically |
| Contents | Read and write | clone, fetch, push |
| Pull requests | Read and write | `gh pr create`, view, comment |
| Workflows | Read and write | pushes touching `.github/workflows/` — not optional here |
| Actions | Read | optional — `gh run list/watch` to follow CI |
| Commit statuses | Read | optional — read commit statuses |

Nothing else: not Administration, Secrets, Variables, Environments, Deployments
or Webhooks.

There is no **Checks** permission for a fine-grained PAT. GitHub documents this
as a feature gap — the Checks API is reachable only by a GitHub App — so `gh pr
checks` can come back thin on a PAT, and `gh run list` / `gh run watch` is the
dependable way to follow CI from inside a sandbox.

With the App route the equivalent is:

```json
"permissions": {
  "contents": "write", "pull_requests": "write", "workflows": "write",
  "actions": "read", "checks": "read"
}
```

An App token may also request a *subset* of what the installation was granted,
so the narrow-by-default version is to install the App with the full set and
have `mint-token.sh` ask for `workflows` only on the branches that need it.

## Day to day

```bash
./devbox.sh create FEAT-101            # clone, render, boot, first-boot
./devbox.sh bridge FEAT-101            # in its OWN terminal, leave it running
./devbox.sh seed FEAT-101              # build once, create a dev user, seed
./devbox.sh claude FEAT-101            # Claude Code, in the VM
./devbox.sh shell FEAT-101
./devbox.sh grant FEAT-101 preview-lan # open :3000 to the phone
./devbox.sh list
./devbox.sh config FEAT-101            # the exact msb command line
./devbox.sh remove FEAT-101            # refuses to bin unpushed work
```

**`bridge` is the one manual step.** It forwards a port in the guest to the host
broker's socket, and Claude Code in the sandbox cannot reach the model without
it. It has to stay in the foreground in its own terminal, and interrupting it
wedges the sandbox — `remove` and recreate if you do. Removing the step
altogether is the first item under *To do*.

Because the VM is the blast radius, running Claude Code inside it with
permission prompts off is a defensible choice in a way it is not on the host.

## Capabilities

A capability is one fragment under `capabilities/`. `render.sh` merges the
profile and the granted fragments into the `msb` command line in
`.devbox/<branch>/msb-args` — the whole of what the sandbox may do. Read it, or
`./devbox.sh config <branch>`, when in doubt.

```bash
./devbox.sh capabilities        # what each one grants
```

Default set: `anthropic, github, rust-deps, node-deps, cargo-cache-shared,
preview`. Two deserve a second thought before granting:

- **`cargo-cache-shared`** is the one writable surface shared between sandboxes.
  It exists so each branch does not recompile the dependency tree, and it is
  also a path by which a build script in one sandbox can plant sources another
  compiles. Revoke it for a branch whose dependencies you have not read.
- **`preview-lan`** publishes the dev server beyond loopback. Grant it for a
  phone pass, revoke it after.

Egress is deny-by-default: the allow-list is exactly the union of what the
granted capabilities asked for, and a set that asks for nothing reaches nothing.

## What the guest gets

A standalone clone of the repository, bind-mounted at `/workspace`, plus:

| Guest path | Source | Mode | Quota | |
|---|---|---|---|---|
| `/workspace` | `../myapps-<branch>` | rw | 16G | the clone; the only host directory the guest writes |
| `/workspace/target` | `~/.cache/msb-devbox/target/<branch>` | rw | 32G | per branch, reclaimed by `remove` |
| `/home/dev/.cargo/{registry,git}` | `~/.cache/msb-devbox/cargo` | rw | 16G / 8G | only with `cargo-cache-shared` |
| `/home/dev/.claude` | `~/.cache/msb-devbox/claude/<branch>` | rw | 4G | agent state, per branch |
| `/workspace/models` | `models/` | ro | 4G | whisper models: large and immutable |

Each mount is capped: msb quotas every one of them and defaults to 4 GiB, which
is too small for a build. A guest that reports `No space left on device` while
the host has room has hit its mount's quota, and because the accounting never
releases on delete, `cargo clean` will not clear it — `./devbox.sh up <branch>`
recreates the sandbox and does.

`.env`, `deploy/*.env` and the real `data/*.db` are never mounted. `first-boot`
writes a throwaway `.env` — a fresh `ENCRYPTION_KEY`, `BIND_ADDR=0.0.0.0:3000` —
and `seed` adds VAPID keys, a `dev` user and seed data after the first build.
Every secret inside the guest is generated there and worthless outside it.

`data/` is not a mount of its own — it sits inside the clone, so the dev
database and anything FileClipboard stores during testing live on the host, in
`../myapps-<branch>/data/`. Gitignored, and removed with the clone, but a 5 GiB
test upload does land in your Projects directory.

The host keeps its own checkout as the place you review, and fetches the branch
from GitHub like any other, because the guest pushed it there.

## The broker

`brokers/anthropic` is the only process that sees the Anthropic credential. One
per sandbox, listening on `$XDG_RUNTIME_DIR/msb-devbox/<branch>-anthropic.sock`
— the socket path is the sandbox's identity, so there is no token to manage and
nothing on the host or the LAN can reach it.

It discards whatever `Authorization` and `x-api-key` the guest sent, injects the
real credential, forwards only to `api.anthropic.com` and only on
`--allow-paths`, streams SSE straight through, and appends a line per request to
`~/.local/state/msb-devbox/audit.jsonl`.

When the host's token expires you get a 401 saying so; running `claude` once on
the host refreshes the file the broker follows.

Extra arguments reach it through `DEVBOX_BROKER_ARGS`:

```bash
DEVBOX_BROKER_ARGS="--max-requests-per-hour 200" ./devbox.sh up FEAT-101
```

Health, and how long the token has left:

```bash
curl -s --unix-socket ~/.../msb-devbox/FEAT-101-anthropic.sock \
     http://localhost/_broker/health
```

## Where it stands

Run end to end on msb 0.7.2:

| Behaviour | Verified by |
|---|---|
| `--vsock HOST_PATH:PORT`, guest connects at CID 2 | `/_broker/health` answered from inside the VM |
| `--secret ENV@HOST[,HOST...]` reads `$ENV` on the host | `git ls-remote origin` authenticated with only the placeholder |
| Egress is deny-by-default once any `--net-rule` exists | `example.com` refused, allowed hosts fine |
| `-v SOURCE:DEST[:OPTIONS]`, including `:ro` | the mounts are there |
| `--mount-dir ...:quota=<MiB>` raises the 4 GiB default | 32G on `/workspace/target`, `ro` still honoured, writes reach the host |
| Claude Code reaches the model through the broker | `claude -p` answered, audit line written |
| `cargo fetch` works through the allow-list | full dependency tree fetched |
| `devbox.sh bridge` alongside normal use | other execs, `claude`, `cargo`, `git` unaffected |

## Effect on existing workflows

- **`/finish-development`** runs inside the guest with the `github` capability:
  version bump, push, `gh pr create`.
- **`/frontend-walkthrough`** needs the `browser` capability; screenshots land in
  `/workspace`, so they appear on the host.
- **`.claude/hooks/cargo-audit.sh`** runs in the guest unchanged, and
  `cargo-audit` is in the image.
- **`.claude/settings.local.json`** — permissions granted inside a sandbox live
  in the clone, so `remove` merges the `permissions.allow` array back.
- **sccache** is replaced by `cargo-cache-shared`; the host `RUSTC_WRAPPER`
  advice applies only to builds still run on the host.
- **frontend-tester** and `make check` are unchanged — they are pure cargo.
- **`deploy.sh` and `make deploy-*` stay on the host.**

## To do

- **Move the broker to TCP and drop the bridge.** Today the guest reaches the
  broker over a vsock route, which needs a translator process inside the guest,
  which on msb 0.7.2 cannot be a daemon — hence the spare terminal. Running the
  broker on a host TCP port and letting the guest reach it through msb's `host`
  network group at `host.microsandbox.internal` needs no guest process at all.
  What is missing is TCP support in the broker, `allow host` in place of the
  vsock route, and the authorisation that replaces the socket path: a host TCP
  port is reachable by any process on the host and by every other sandbox, so
  the broker would have to require a per-sandbox bearer token, generated at
  `create` and handed to the guest as `ANTHROPIC_AUTH_TOKEN`. A leaked token
  would let someone spend model tokens through the subscription; it would not
  expose the OAuth token, which the broker never returns. That is a real
  weakening — the socket path is currently the sandbox's identity — so it is
  written down here rather than quietly implemented.
- **One real PR from inside a sandbox.** The `github` capability is proven as far
  as `git ls-remote`; push and `gh pr create` are not yet exercised.
- **`/frontend-walkthrough` end to end**, which needs `build-image --browser`
  and the `walkthrough` profile.
- **Try `--net-strict`** (require inspectable request authority for hostname
  allows), then **`--security restricted`**, in that order. Both are off.
- **The prod broker**, for debugging against real data. `prod-readonly.sh`
  already routes a second host socket; the broker would hold the Odroid SSH key
  behind a verb-limited API rather than a tunnel —

  ```
  GET /snapshot/leanfin.sqlite    -> ssh odroid 'sqlite3 … .dump' | scrub | gzip
  GET /logs?unit=myapps&since=1h  -> ssh odroid 'journalctl -u myapps …'
  ```

  read-only by construction, because the broker never forwards a guest-supplied
  command, only fills parameters into templates it owns. The same shape covers a
  `llama` capability for the command bar later.
- **Swap the PAT for the GitHub App** once the MVP has carried a few branches;
  `mint-token.sh` is already written.

## Worth keeping in mind

- **microsandbox is beta.** Flags will move.
- **OAuth injection is not a supported path.** It may break; the broker's
  credential source is one swappable piece for exactly that reason, and
  `--api-key` is the fallback.
- **`ANTHROPIC_BASE_URL` disables Remote Control** and, without
  `ENABLE_TOOL_SEARCH=true`, MCP tool search.
- **Disk.** A sandbox is multiple GB between its root disk and `target/`;
  `devbox.sh list` shows the usage and `remove` reclaims it.
- **Convenience pressure.** The failure mode here is not a breach, it is quietly
  mounting `~/.ssh` "just this once" at 11pm. A capability that is needed often
  should be designed and reviewed, not improvised with a flag.

## References

- [microsandbox docs](https://docs.microsandbox.dev/getting-started/introduction)
  · [networking](https://docs.microsandbox.dev/networking/overview)
  · [secrets](https://docs.microsandbox.dev/sandboxes/secrets)
  · [host sockets](https://docs.microsandbox.dev/networking/host-sockets)
- [Claude Code environment variables](https://code.claude.com/docs/en/env-vars)
- [Permissions for fine-grained PATs](https://docs.github.com/en/rest/authentication/permissions-required-for-fine-grained-personal-access-tokens)
