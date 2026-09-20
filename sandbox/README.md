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
./devbox.sh build-broker     # the host-side brokers (Anthropic, prod)
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
./devbox.sh seed FEAT-101              # build once, create a dev user, seed
./devbox.sh claude FEAT-101            # Claude Code, in the VM
./devbox.sh shell FEAT-101
./devbox.sh grant FEAT-101 preview-lan # open :3000 to the phone
./devbox.sh list
./devbox.sh config FEAT-101            # the exact msb command line
./devbox.sh remove FEAT-101            # refuses to bin unpushed work
```

One terminal, no manual step: `create` starts the broker and `claude` talks to
it over the host's loopback. Every command starts whatever it needs.

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

## The Anthropic broker

`brokers/anthropic` is the only process that sees the Anthropic credential. One
per sandbox, listening on a host loopback port — `127.0.0.1:18800` upwards, one
per branch, recorded in `.devbox/<branch>/broker.port`. The guest reaches it at
`host.microsandbox.internal` because its capability set asked for msb's `host`
network group, scoped to that one port; the LAN cannot reach it at all, and the
broker refuses to bind anything but loopback.

Loopback is not authorisation, though: every other process on the host, and
every other sandbox granted `host`, can open the same port. So each sandbox gets
a bearer token, generated at `create`, kept in `.devbox/<branch>/broker.token`
and handed to the guest as `ANTHROPIC_AUTH_TOKEN`; requests without it are
refused and audited. This is weaker than the Unix socket it replaces, where the
socket path *was* the sandbox's identity, and it is the price of dropping the
in-guest bridge. A leaked token spends model tokens on the subscription; it does
not expose the OAuth credential, which the broker substitutes on the way out and
never returns.

The one exception is Claude Code's `/api/hello` probe, a bodyless `HEAD` sent
before the client applies the token. It is forwarded unauthenticated, because
refusing it would 401 at the start of every session, and all it reveals is
whether the host's own login is still valid.

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
curl -s -H "Authorization: Bearer $(cat .devbox/FEAT-101/broker.token)" \
     "http://127.0.0.1:$(cat .devbox/FEAT-101/broker.port)/_broker/health"
```

`./devbox.sh doctor` prints the same line for every sandbox it knows about.

## The prod broker

`brokers/prod` is the answer to "I cannot reproduce this on seed data". It
holds the Odroid SSH key — which [CLAUDE.md](CLAUDE.md) says permanently never
enters a VM — and answers three verbs on a second host loopback port, claimed
and tokened exactly like the Anthropic one.

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
takes a command, a path or a query: the guest picks a verb and fills parameters
into templates the broker owns, each validated against a closed set first —
`--since` accepts `30s`, `15m`, `2h`, `7d` or a date and nothing else, `--unit`
one of two names, `--lines` a number under a ceiling. The three command strings
in `remote.rs` are the entire vocabulary, and none of them writes: `journalctl`,
`systemctl status`, and `sqlite3 -readonly … .dump`, which streams the database
out in a read transaction without even leaving a temporary file behind on the
Odroid.

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
added next year quietly carrying its contents into a VM.

**FileClipboard and VoiceToText keep their metadata and lose their bytes.**
Those live outside SQLite, so a snapshot has rows whose files are not there and
whose downloads 404. Restoring the rows is still the right call — it is what
makes the list pages render — but `services::retention` will reconcile them
away if you leave it running.

### Costs and limits

A snapshot is a full database dump over the LAN and a rebuild on the host, so
the broker refuses a second one within `--min-snapshot-interval-secs`
(60 by default), serialises concurrent requests, and abandons a dump past
`--max-dump-mb`. Working files live under
`~/.cache/msb-devbox/prod/<branch>`, which `remove` reclaims.

`DEVBOX_PROD_ENV` picks the deployment; it defaults to `deploy/prod.env`, the
same file `deploy.sh` reads, so prod is described in exactly one place.

```bash
DEVBOX_PROD_ENV=deploy/stage.env ./devbox.sh up FEAT-101
DEVBOX_PROD_BROKER_ARGS="--min-snapshot-interval-secs 600" ./devbox.sh up FEAT-101
```

The snapshot needs the deploy user to be able to run `sqlite3` as the service
user on the Odroid — the same thing the manual backup procedure in
[deployment docs](../docs/deployment.md#backups-and-rollback) needs. If its
sudoers is the restricted list, that is one entry to add.

## Where it stands

Run end to end on msb 0.7.2:

| Behaviour | Verified by |
|---|---|
| `--net-rule allow@host:tcp:<port>` reaches a host service | a loopback-bound host port answered from inside the VM at `host.microsandbox.internal` |
| …and only that port | a second host port, not in the rules, refused |
| The broker's token is what separates sandboxes | a second sandbox granted the same port got 401, and an audit line |
| `--secret ENV@HOST[,HOST...]` reads `$ENV` on the host | `git ls-remote origin` authenticated with only the placeholder |
| Egress is deny-by-default once any `--net-rule` exists | `example.com` refused, allowed hosts fine |
| `-v SOURCE:DEST[:OPTIONS]`, including `:ro` | the mounts are there |
| `--mount-dir ...:quota=<MiB>` raises the 4 GiB default | 32G on `/workspace/target`, `ro` still honoured, writes reach the host |
| Claude Code reaches the model through the broker | `claude -p` answered, audit line written |
| `cargo fetch` works through the allow-list | full dependency tree fetched |
| One host port per sandbox, kept across restarts | a second branch rendered `:18801` while the first held `:18800` |
| The prod broker's vocabulary is closed | unknown verbs 404; `?unit=sshd` refused; `?since=` rejected `yesterday`, `2h; id`, `$(id)`, `` `id` `` and `1h --output=json` |
| A snapshot carries no credentials | dump, rebuild and scrub over the real merged schema: the five credential tables emptied, IBAN and consent blanked, and `grep` over the finished file found none of the removed strings |
| An unclassified table stops a snapshot | `scrub.rs` refuses by name when `sqlite_master` holds a table in none of its three lists |
| Two brokers on one sandbox stay separate | `prod-readonly` renders a second `allow@host:tcp:` rule and a second `brokers` entry, each on its own port |
| `logs` and `status` against the real Odroid | `health`, `systemctl status myapps` and `journalctl` for both allowed units answered over the real SSH config |
| Neither leaks where prod is | grep for the Odroid's hostname over the real answers: 0 in `logs` (`--no-hostname`) and 0 in `status` (`-n 0`, which drops the journal tail `--no-hostname` cannot reach) |

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

- **One real PR from inside a sandbox.** The `github` capability is proven as far
  as `git ls-remote`; push and `gh pr create` are not yet exercised.
- **`/frontend-walkthrough` end to end**, which needs `build-image --browser`
  and the `walkthrough` profile.
- **Try `--net-strict`** (require inspectable request authority for hostname
  allows), then **`--security restricted`**, in that order. Both are off.
- **One real `devbox-prod snapshot`.** `logs` and `status` are proven against
  the real Odroid, and the scrub is proven end to end on the real schema — but
  the two have never met: no snapshot has been taken from the live database.
  That run is also what proves the deploy user may read it, which needs a
  sudoers entry the sample rules in
  [deployment docs](../docs/deployment.md#deploy-user-setup) did not have.
- **A `llama` capability** for the command bar, in the shape the prod broker now
  establishes: a host daemon with a vocabulary, not a tunnel with a filter.
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
  · [reaching the host](https://docs.microsandbox.dev/networking/overview)
- [Claude Code environment variables](https://code.claude.com/docs/en/env-vars)
- [Permissions for fine-grained PATs](https://docs.github.com/en/rest/authentication/permissions-required-for-fine-grained-personal-access-tokens)
