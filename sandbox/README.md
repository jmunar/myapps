# Sandboxed development

One sandbox per branch. It holds a clone of the repository and the host's own
toolchain, read-only, in a set of namespaces with no network in them — and no
credentials. The host keeps `~/.claude`, the GitHub credential, `~/.ssh`, the
real `.env` and the real `data/`, and hands out narrow, audited access to them.

It is built from `bwrap` and `socat`, with three small host daemons. There is no
virtual machine, no image to build and nothing running between commands.

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

**Out of scope:** a kernel privilege escalation, a compromised broker, and
abuse of a capability deliberately granted. The sandbox can push to this
repository when `github` is granted; that is the point of the capability, not a
hole in it.

That second list is the honest cost of not using a virtual machine. Namespaces
are a kernel boundary, not a hypervisor one, so a kernel LPE escapes this where
it would not escape a microVM. What it buys is everything in
[CLAUDE.md](CLAUDE.md) that is no longer there: no guest image, no disk quotas,
no port claiming, no bearer tokens, no lifecycle, and a sandbox that starts in
milliseconds.

## Setup, once

```bash
./devbox.sh doctor           # says what is missing
./devbox.sh build-broker     # the three host daemons
```

You also need:

- **bubblewrap and socat**, and a kernel with unprivileged user namespaces
  enabled. `doctor` proves it by building a throwaway sandbox rather than
  reading a sysctl.
- **A Claude login on the host** — `claude` once, so
  `~/.claude/.credentials.json` exists. The broker follows that file; it never
  copies it, and the sandbox never sees it.
- **A GitHub credential**, either
  - `~/.config/devbox/github-app.json` describing a GitHub App, in which case
    `devbox.sh` mints a fresh installation token per sandbox start and that
    token expires an hour later — the recommended route, see
    [brokers/github/mint-token.sh](brokers/github/mint-token.sh); or
  - `~/.config/devbox/github-token` (`chmod 600`) holding a fine-grained PAT,
    which works and never expires.

The host's own toolchain is what the sandbox runs: `rustup`, `cargo`, `node`
and Claude Code are bind-mounted read-only from your home directory. Upgrading
them on the host upgrades every sandbox at once.

### What the token may do

Unlike every other credential here, this one is *in* the sandbox. Keeping it
out needs TLS interception, and the egress proxy deliberately has none — it
tunnels bytes it cannot read. What bounds the token instead is its own scope
and lifetime.

Repository access: **only this repository**.

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
./devbox.sh create FEAT-101            # clone, render, first-boot
./devbox.sh seed FEAT-101              # build once, create a dev user, seed
./devbox.sh claude FEAT-101            # Claude Code, in the sandbox
./devbox.sh shell FEAT-101
./devbox.sh grant FEAT-101 preview-lan # open :3000 to the phone
./devbox.sh selftest FEAT-101          # prove it still cannot reach anything
./devbox.sh list
./devbox.sh config FEAT-101            # the exact bwrap command line
./devbox.sh remove FEAT-101            # refuses to bin unpushed work
```

**A sandbox exists for as long as the command running in it.** There is no
`up`, no `stop` and nothing running between commands: each command starts the
brokers, the proxy and the relays it needs, and kills them on the way out. A
capability granted with `grant` applies to the next command you run.

Because the sandbox is the blast radius, running Claude Code inside it with
permission prompts off is a defensible choice in a way it is not on the host.

## Capabilities

A capability is one fragment under `capabilities/`. `render.sh` merges the
profile and the granted fragments into the `bwrap` command line and the host
plan in `.devbox/<branch>/` — the whole of what the sandbox may do. Read them,
or `./devbox.sh config <branch>`, when in doubt.

```bash
./devbox.sh capabilities        # what each one grants
```

Default set: `anthropic, github, rust-deps, node-deps, cargo-cache-shared,
preview`. Three deserve a second thought before granting:

- **`cargo-cache-shared`** is the one writable surface shared between sandboxes.
  It exists so each branch does not recompile the dependency tree, and it is
  also a path by which a build script in one sandbox can plant sources another
  compiles. Revoke it for a branch whose dependencies you have not read.
- **`preview-lan`** publishes the dev server beyond loopback. Grant it for a
  phone pass, revoke it after.
- **`prod-readonly`** points the sandbox at the real deployment. Every verb is
  a read and the snapshot is scrubbed, but it is still a line to the machine
  that holds your money and your notes. Grant it for the session that needs it.

Egress is deny-by-default and has no route at all to fall back on: the sandbox
is in an empty network namespace, and the allow-list is exactly the union of
what the granted capabilities asked for. A set that asks for nothing gets no
proxy.

## What the sandbox gets

A standalone clone of the repository, bind-mounted at `/workspace`, plus:

| Sandbox path | Source | Mode | |
|---|---|---|---|
| `/workspace` | `../myapps-<branch>` | rw | the clone; the only host directory it writes |
| `/workspace/target` | `~/.cache/devbox/target/<branch>` | rw | per branch, reclaimed by `remove` |
| `~/.cargo` | `~/.cache/devbox/cargo-home/<branch>` | rw | per branch |
| `~/.cargo/{registry,git}` | `~/.cache/devbox/cargo` | rw | only with `cargo-cache-shared` |
| `~/.claude` | `~/.cache/devbox/claude/<branch>` | rw | agent state, per branch |
| `/workspace/models` | `models/` | ro | whisper models: large and immutable |
| `~/.rustup`, `~/.cargo/bin` | the host's | ro | the toolchain itself |
| `~/.local/share/claude` | the host's | ro | Claude Code itself |
| `/opt/devbox/bin`, `/opt/devbox/bootstrap` | `sandbox/guest`, `sandbox/bootstrap` | ro | the helpers |

Everything else is absent rather than denied: `/` is a tmpfs, `/usr` is the
host's read-only, `/etc` holds a hand-written list of files the toolchain
actually reads, and `$HOME` is an empty tmpfs with the mounts above in it.
There is no `/etc/resolv.conf`, so a name that is not resolved by the proxy is
not resolved at all.

`.env`, `deploy/*.env` and the real `data/*.db` are never mounted. `first-boot`
writes a throwaway `.env` — a fresh `ENCRYPTION_KEY`, `BIND_ADDR=127.0.0.1:3000`
— and `seed` adds VAPID keys, a `dev` user and seed data after the first build.
Every secret inside the sandbox is generated there and worthless outside it.

`data/` is not a mount of its own — it sits inside the clone, so the dev
database and anything FileClipboard stores during testing live on the host, in
`../myapps-<branch>/data/`. Gitignored, and removed with the clone, but a 5 GiB
test upload does land in your Projects directory.

The host keeps its own checkout as the place you review, and fetches the branch
from GitHub like any other, because the sandbox pushed it there.

Disk is just disk: there are no per-mount quotas, because there is no virtual
block device to put them on. `devbox.sh list` shows what `target/` has grown
to and `remove` reclaims it.

## Proving it

```bash
./devbox.sh selftest FEAT-101
```

A mount list is a thing you can get wrong silently — one stray `--bind` and the
whole directory is undone — so the boundary is tested rather than reviewed. The
self-test asserts, from inside the sandbox, that `~/.ssh` is unreadable, that
there is no Claude credentials file, that the host checkout and cache are not
visible, that nothing reaches the network directly, that the proxy refuses an
unlisted host and that there is no resolver — and then that the clone is still
writable, the PID namespace is private, and an allowed host still works.

Run it after touching `render.sh`, and add a line to it whenever you add a
mount.

## The egress proxy

`brokers/proxy` is the sandbox's only way out. It listens on a Unix socket, a
`socat` inside the namespace relays `127.0.0.1:3128` to it, and `HTTPS_PROXY`
points there — so cargo, git, npm, gh and Claude Code all go through it and
anything that ignores the proxy environment reaches nothing at all. Failing
closed is the point.

It implements `CONNECT` and nothing else. The hostname it names has to match
the allow-list **exactly** — no suffix matching, because `github.com` would
otherwise mean every subdomain anyone can register under it — and the proxy
resolves that name itself, on the host, *after* the check, so an allowed name
cannot be pointed at an address the sandbox picked. IP literals are refused
outright, which is why `169.254.169.254` and the rest of your LAN need no deny
rule: they are not names.

There is no TLS interception and no certificate to install. The proxy decides
who the sandbox may talk to, never what it may say, and the tunnel is opaque
once established. Every decision appends a line to
`~/.local/state/devbox/audit.jsonl`.

## The Anthropic broker

`brokers/anthropic` is the only process that sees the Anthropic credential. One
per sandbox invocation, listening on a Unix socket in that sandbox's runtime
directory, relayed to `127.0.0.1:8080` inside the namespace by another `socat`.

There is no bearer token, because the socket does what a token used to: which
sandbox can reach this broker is decided by `--bind $RUNTIME_DIR /run/devbox`
and nothing else, and a sandbox that was not given that directory has no way to
name the socket. The directory is mode 0700 and the socket 0600.

It discards whatever `Authorization` and `x-api-key` the sandbox sent, injects
the real credential, forwards only to `api.anthropic.com` and only on
`--allow-paths`, streams SSE straight through, and appends a line per request
to the same audit log.

When the host's token expires you get a 401 saying so; running `claude` once on
the host refreshes the file the broker follows.

Extra arguments reach it through `DEVBOX_BROKER_ARGS`:

```bash
DEVBOX_BROKER_ARGS="--max-requests-per-hour 200" ./devbox.sh claude FEAT-101
```

## The prod broker

`brokers/prod` is the answer to "I cannot reproduce this on seed data". It
holds the Odroid SSH key — which [CLAUDE.md](CLAUDE.md) says permanently never
enters a sandbox — and answers three verbs on a second Unix socket, relayed to
`127.0.0.1:8081` exactly like the Anthropic one.

```bash
./devbox.sh grant FEAT-101 prod-readonly
./devbox.sh shell FEAT-101

devbox-prod snapshot              # the prod database, scrubbed, into data/myapps.db
devbox-prod logs --since 2h       # journalctl for the service
devbox-prod logs --unit nginx --lines 500
devbox-prod status
devbox-prod health                # what this broker will answer, and for which units
```

What makes it read-only is its shape, not a filter. There is no endpoint that
takes a command, a path or a query: the sandbox picks a verb and fills
parameters into templates the broker owns, each validated against a closed set
first — `--since` accepts `30s`, `15m`, `2h`, `7d` or a date and nothing else,
`--unit` one of two names, `--lines` a number under a ceiling. The three
command strings in `remote.rs` are the entire vocabulary, and none of them
writes: `journalctl`, `systemctl status`, and `sqlite3 -readonly … .dump`,
which streams the database out in a read transaction without even leaving a
temporary file behind on the Odroid.

### What a snapshot contains

The point is debugging against real data, so the data stays: transactions,
descriptions, counterparties, balances, notes, thoughts, form inputs. What the
host strips on the way past is anything that is a credential or a capability.

| | |
|---|---|
| Dropped | `sessions`, `invites`, `push_subscriptions`, `leanfin_pending_links`, `leanfin_api_payloads` |
| Blanked | `leanfin_user_settings.enable_banking_key` and `.enable_banking_app_id`, `leanfin_accounts.iban` and `.session_id` |
| Rewritten | every `users.password_hash`, to a real Argon2 hash of one known password — the snapshot is a database you can log into, and `devbox-prod` prints the password |

`leanfin_api_payloads` is dropped whole rather than scrubbed column-wise
because it holds verbatim provider requests and responses: bearer tokens and
account data in text columns no schema describes.

The database never crosses the boundary as it is. `.dump` streams it to the
host, the host rebuilds it, scrubs it and `VACUUM`s — so the pages the deleted
rows occupied are gone rather than merely unreferenced — and only that copy is
served. A `grep` over a finished snapshot finds none of the strings it removed.

**A table that nobody has classified stops the snapshot.** `scrub.rs` holds
every table in one of three lists, checked against the snapshot's own
`sqlite_master` rather than against this repository's migrations, and a table
in none of them fails the request by name. This app gains tables regularly;
without that check the failure would not be a wrong rule, it would be a table
added next year quietly carrying its contents into a sandbox.

**FileClipboard and VoiceToText keep their metadata and lose their bytes.**
Those live outside SQLite, so a snapshot has rows whose files are not there and
whose downloads 404. Restoring the rows is still the right call — it is what
makes the list pages render — but `services::retention` will reconcile them
away if you leave it running.

### Costs and limits

A snapshot is a full database dump over the LAN and a rebuild on the host, so
the broker refuses a second one within `--min-snapshot-interval-secs`
(60 by default), serialises concurrent requests, and abandons a dump past
`--max-dump-mb`. Working files live under `~/.cache/devbox/prod/<branch>`,
which `remove` reclaims.

`DEVBOX_PROD_ENV` picks the deployment; it defaults to `deploy/prod.env`, the
same file `deploy.sh` reads, so prod is described in exactly one place.

```bash
DEVBOX_PROD_ENV=deploy/stage.env ./devbox.sh shell FEAT-101
DEVBOX_PROD_BROKER_ARGS="--min-snapshot-interval-secs 600" ./devbox.sh shell FEAT-101
```

The snapshot needs the deploy user to be able to run `sqlite3` as the service
user on the Odroid — the same thing the manual backup procedure in
[deployment docs](../docs/deployment.md#backups-and-rollback) needs. If its
sudoers is the restricted list, that is one entry to add.

## Where it stands

Run end to end on bubblewrap 0.11.2 and Linux 7.1:

| Behaviour | Verified by |
|---|---|
| The sandbox has no route off the machine | `curl https://example.com` refused with no proxy in the environment |
| …and no resolver either | `getent hosts github.com` fails |
| The host's credentials are not there | `~/.ssh` unreadable, no `.credentials.json`, the host checkout and cache absent |
| The proxy allows exactly the granted hosts | `static.crates.io` answered, `example.com` refused through the proxy itself |
| Claude Code reaches the model through the broker | `claude -p` answered over the Unix socket, audit line written |
| `cargo fetch` and a full `cargo check --workspace --all-targets` | the whole dependency tree fetched through the proxy, the workspace checked in 57s |
| git authenticates to GitHub from inside | `git ls-remote origin` over HTTPS with the minted token and `GIT_ASKPASS` |
| `preview` publishes to host loopback only | reachable on `127.0.0.1:3000`, refused on the LAN address |
| The prod broker's vocabulary is closed | `health` answered; `?since=nonsense` refused by name |
| Chromium runs for the walkthrough | `--dump-dom` against the host's Playwright build, read-only |
| Two sandboxes do not share a broker | one runtime directory per invocation; nothing else is mounted |
| Capability conflicts are refused | `preview-lan` alongside `preview` refused at render, the old set restored |

## Effect on existing workflows

- **`/finish-development`** runs inside the sandbox with the `github`
  capability: version bump, push, `gh pr create`.
- **`/frontend-walkthrough`** needs the `browser` capability; screenshots land
  in `/workspace`, so they appear on the host.
- **`.claude/hooks/cargo-audit.sh`** runs in the sandbox unchanged, using the
  host's `cargo-audit`.
- **`.claude/settings.local.json`** — permissions granted inside a sandbox live
  in the clone, so `remove` merges the `permissions.allow` array back.
- **frontend-tester** and `make check` are unchanged — they are pure cargo.
- **`deploy.sh` and `make deploy-*` stay on the host.**

## To do

- **One real PR from inside a sandbox.** `git ls-remote` and an authenticated
  fetch are proven; push and `gh pr create` are not yet exercised.
- **`/frontend-walkthrough` end to end**, which needs the `browser` capability
  and a real run against the dev server.
- **One real `devbox-prod snapshot`.** `logs` and `status` are proven against
  the real Odroid, and the scrub is proven end to end on the real schema — but
  the two have never met: no snapshot has been taken from the live database.
  That run is also what proves the deploy user may read it, which needs a
  sudoers entry the sample rules in
  [deployment docs](../docs/deployment.md#deploy-user-setup) did not have.
- **A seccomp filter** denying `unshare`, `ptrace` and the rest of the
  namespace-creation surface, via `bwrap --seccomp`. The sandbox currently
  relies on the mount and network namespaces alone.
- **A `llama` capability** for the command bar, in the shape the prod broker
  establishes: a host daemon with a vocabulary, not a tunnel with a filter.
- **Swap the PAT for the GitHub App** once the MVP has carried a few branches;
  `mint-token.sh` is already written.

## Worth keeping in mind

- **The boundary is the mount list.** Read `./devbox.sh config <branch>` when
  in doubt, and let `selftest` decide rather than your memory of it.
- **Namespaces are not a hypervisor.** A kernel LPE gets out of this. That is
  the trade; see *Why*.
- **`ANTHROPIC_BASE_URL` disables Remote Control** and, without
  `ENABLE_TOOL_SEARCH=true`, MCP tool search.
- **The host's toolchain is shared.** A `rustup update` on the host lands in
  every sandbox at once, and the sandbox cannot update it back.
- **Convenience pressure.** The failure mode here is not a breach, it is quietly
  binding `~/.ssh` "just this once" at 11pm. A capability that is needed often
  should be designed and reviewed, not improvised with a `cli --bind`.

## References

- [bubblewrap](https://github.com/containers/bubblewrap) ·
  [socat](http://www.dest-unreach.org/socat/doc/socat.html)
- [Claude Code environment variables](https://code.claude.com/docs/en/env-vars)
- [Permissions for fine-grained PATs](https://docs.github.com/en/rest/authentication/permissions-required-for-fine-grained-personal-access-tokens)
