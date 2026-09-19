# Sandboxed development

`devbox.sh` and this directory run development inside a microVM per branch: the
guest holds a clone, the Rust toolchain and Claude Code, and no credentials.
Usage, capabilities and the roadmap are in [README.md](README.md); what follows
is only what will bite you while changing it.

The pieces: `render.sh` turns a profile plus capability fragments into an `msb`
command line, `devbox.sh` runs it and owns the host side, `brokers/anthropic` is
the only process that ever sees the Anthropic credential, `image/` builds the
guest.

## Gotchas

These are the things that have actually broken, and that reading the code
nearby will not warn you about. Most of them are msb 0.7.2 behaving in ways its
documentation does not mention.

**Drive msb by command-line flags, never `--conf`.** `msb --conf` takes a
"sparse single-sandbox configuration" whose schema the binary will not tell you,
while `msb run --help` documents every flag exactly. `render.sh` therefore emits
an argument list, not a config file. Every `msb` invocation lives in one block at
the top of `devbox.sh` — when a flag moves, that block is the only edit.

**Never abort an `msb exec`.** Kill the client — a host-side `timeout`, a
Ctrl-C — and the sandbox stops answering: *every* later exec against it hangs
forever, and the only cure is `remove` and recreate. So nothing here wraps an
exec in `timeout`, the broker health check runs once and is never retried, and
`msb exec --timeout` is used to bound the guest command instead. A long run of
mysterious hangs in this code was self-inflicted, one aborted exec at a time.

**`--no-net` takes the guest agent down with the workload.** A sandbox created
with it answers `msb ping` and hangs on every `msb exec`. A capability set that
grants no egress must still render `--net-default-egress deny`, which is the
same thing for the workload without the wedge. `render.sh` does this; do not
"simplify" it back to `--no-net`.

**A running image command wedges exec the same way.** `msb run --detach` with
any long-lived command, and `--init` with a PID 1 handoff, both leave the
sandbox pingable and exec-dead. Hence `msb create`, which boots it idle, and
hence the guest image's `CMD` stays `/bin/bash`. This is also why the vsock
bridge cannot be a daemon inside the guest and has to be `devbox.sh bridge`, a
foreground exec the user leaves running.

**Every mount gets a 4096 MiB quota unless it asks for a bigger one.** msb puts
a quota on each directory-backed mount and defaults it to 4 GiB — which a debug
build of this workspace exhausts partway through, reported inside the guest as
an ordinary `No space left on device` while the host still has hundreds of
gigabytes free. `--volume` cannot set it; only `--mount-dir` takes
`quota=<MiB>`, so `render.sh` emits a mount as `--mount-dir` exactly when its
spec carries one. The accounting is also write-only: deleting files never gives
the space back, so `cargo clean` empties the directory and the volume goes on
reporting itself full until the sandbox is recreated. Any mount that
accumulates anything needs a quota in its fragment.

**A tag in docker's image cache is invisible to msb.** `build-image` pipes
`docker save` into `msb load -t`; building the image without loading it leaves
`create` pulling a nonexistent image from a registry.

**Do not add CA handling to the guest.** msb installs its TLS-interception CA
itself and points `NODE_EXTRA_CA_CERTS`, `SSL_CERT_FILE`, `CURL_CA_BUNDLE` and
`REQUESTS_CA_BUNDLE` at `/.msb/tls/ca.pem`. An earlier `devbox-trust-ca` script
existed to do this and was deleted; secret substitution needs that interception,
so nothing should disable it either.

**The guest gets a standalone clone, never a worktree.** A worktree's `.git` is
a file pointing outside the mount, and any arrangement where the guest can write
the main repository's `.git` hands it the shared object store and `.git/hooks` —
which the *host* executes later. `git clone --no-hardlinks` is part of that, not
hygiene: without it the clone's objects are the same inodes as this
repository's, and a guest that rewrites one corrupts the host's copy.

**`origin` in the clone must be HTTPS.** Secret substitution only works on HTTP,
and there is no SSH key in the VM by design, so an SSH remote fails in a way
that looks like a network problem.

**A secret's value never appears in a fragment.** `secret NAME host...` names an
environment variable that `devbox.sh` reads on the host and `msb` substitutes
outside the VM; the guest only ever holds the placeholder (`$MSB_GITHUB_TOKEN`).
`msb` rejects an inline `ENV=VALUE@HOST` outright. Anything that puts a real
credential into `env`, a mount or the image is the one mistake this whole
directory exists to prevent.

**The broker follows `~/.claude/.credentials.json`; it does not refresh it.**
Claude Code on the host refreshes that file as you use it, and OAuth refresh
tokens rotate — a second refresh chain would invalidate the host's own login.
`--refresh writeback` exists for when you want the broker to own the chain, and
it rewrites the same file so there is still only one.

**Pushing anything under `.github/workflows/` needs `Workflows: write`** on the
GitHub token. Adding or removing an environment variable in this repo means
editing `cd.yml`, so a branch that does routine work fails at the very end of
`/finish-development` with an error that reads like a git problem.

**`deploy.sh` stays host-only, permanently.** No capability may grant deploy;
the Odroid SSH key does not enter a VM. Same for `make deploy-*`. If prod access
is ever needed from a sandbox it goes through a broker with a verb-limited API,
never a tunnel and never a key.

**The brokers are outside the cargo workspace.** `brokers/anthropic` carries its
own `[workspace]` and the root manifest excludes `sandbox/`, so `make check`
stays exactly what CI runs. Keep it that way.

## Writing a capability

A fragment is a shell script, and sourcing it is how it is read. There is no
parser and no dependency beyond bash: the schema is small and fixed, merging is
appending, and substitution is shell variables.

```sh
describe "git push and gh pr create; the token value never enters the VM"

secret GITHUB_TOKEN github.com api.github.com
secret_file GITHUB_TOKEN "$GITHUB_TOKEN_FILE"

allow github.com api.github.com codeload.github.com objects.githubusercontent.com

env GH_TOKEN '$MSB_GITHUB_TOKEN'
env GIT_ASKPASS /usr/local/bin/devbox-git-askpass
```

Directives: `describe`, `conflicts`, `image`, `cpus`, `memory`, `workdir`,
`user`, `hostname`, `profile`, `env`, `mount`, `allow`, `deny`, `port`,
`secret`, `secret_file`, `broker`, `cli`. Fragments read the sandbox's paths
from the environment: `BRANCH`, `CLONE`, `TARGET_CACHE`, `CARGO_CACHE`,
`MODELS`, `CLAUDE_STATE`, `BROKER_SOCKET`, `PROD_SOCKET`, `GITHUB_TOKEN_FILE`.

Later fragments win on scalars and on `env`; lists concatenate and de-duplicate;
capabilities apply in the order given, after the profile. Two capabilities that
would both set the same single thing must declare `conflicts`, because lists
concatenate silently — `preview` and `preview-lan` would otherwise publish port
3000 twice, once to loopback and once to the LAN.

`cli` is the escape hatch for a flag `render.sh` does not model. Use it for
flags, not for things that belong in a directive.

**Changing the image means rebuilding *and* reloading:** `./devbox.sh
build-image`. A running sandbox keeps the image it was created with.

## Layout

```
devbox.sh            the host side: clone, render, brokers, lifecycle
render.sh            profile + capabilities -> an msb command line
profiles/            base sandboxes (resources, mounts)
capabilities/        one fragment per capability
image/               guest image; image/guest/ is baked into /usr/local/bin
bootstrap/           scripts run inside the guest (first-boot, seed)
brokers/anthropic/   host daemon; holds the OAuth token
brokers/github/      GitHub App token minting
.devbox/<branch>/    generated args and state (gitignored)
```
