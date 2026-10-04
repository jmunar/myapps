# Sandboxed development

`devbox.sh` and this directory run development inside a bubblewrap sandbox per
branch: a clone of the repository and the host's own toolchain, read-only, in a
set of namespaces with no network in them and no credentials. Usage,
capabilities and the roadmap are in [README.md](README.md); what follows is only
what will bite you while changing it.

The pieces: `render.sh` turns a profile plus capability fragments into a `bwrap`
command line and a short plan, `devbox.sh` runs both and owns the host side,
`brokers/anthropic` is the only process that ever sees the Anthropic credential,
`brokers/prod` the only one that sees the Odroid SSH key, `brokers/proxy` is the
only way out to the network.

## Gotchas

These are the things that have actually broken, and that reading the code
nearby will not warn you about.

**There is no daemon and no lifecycle.** A sandbox exists for exactly as long
as the command running in it: `devbox.sh shell` starts the brokers, the proxy
and the relays, runs bwrap, and kills all of them on the way out. There is
nothing to `up`, nothing to `stop`, and nothing left behind to go stale. Any
change that introduces a background process outliving the command has to say
what cleans it up, because `cleanup_sandbox` will not.

**The runtime directory is per invocation, not per branch.** Two shells on one
branch each get `$XDG_RUNTIME_DIR/devbox/<branch>.<pid>/`, their own brokers
and their own sockets — otherwise the second one's broker would unlink the
first one's socket and the first session would lose the model mid-turn. That
is also why the bwrap arguments are rendered on every command rather than
cached: the runtime path is in them.

**The socket is the authorisation.** No bearer token anywhere: `--bind
$RUNTIME_DIR /run/devbox` is the *entire* reason a sandbox can reach its own
brokers, and a sandbox that was not given that directory cannot name the
socket. Adding a TCP listener to any broker would undo that and put the token
machinery back; the brokers refuse to listen on anything but a Unix socket for
that reason.

**Nothing in the guest is reachable except through a socket.** `--unshare-net`
leaves the sandbox with loopback and nothing else, so *both* directions are
`socat` over a Unix socket in that directory: inbound, a
`TCP-LISTEN:<port> … UNIX-CONNECT` inside the namespace for each broker, and
outbound, a `UNIX-LISTEN … TCP:127.0.0.1:<port>` pair for a published dev
server. A capability that needs a new channel needs a relay on both sides, and
the inner one belongs in the generated entrypoint, not in the image — there is
no image.

**Ports inside the sandbox are constants, and that is not laziness.** 8080 for
the Anthropic broker, 8081 for prod, 3128 for the proxy. Each sandbox has its
own network namespace, so two sandboxes both listening on 8080 never meet, and
the whole port-claiming apparatus the microVM needed — claim, freeze, skip
what another branch holds — is gone. Do not reintroduce a claimed port.

**The egress proxy is the only way out, and it fails closed.** `HTTPS_PROXY`
points at it, so cargo, git, npm, gh and Claude Code go through it and anything
that ignores the proxy environment reaches nothing at all. It speaks only
`CONNECT`, matches the hostname **exactly** against the allow-list, and
resolves the name itself *after* the check, so an allowed name cannot be
pointed at an address the guest picked. IP literals are refused outright, which
is why `169.254.169.254` needs no deny rule: it is not a name, so it can never
be on a list of names.

`HTTP_PROXY` is deliberately unset. Setting it would send plain-HTTP requests —
including the sandbox's own loopback calls to the brokers — into a proxy that
does not implement them.

**No TLS interception means the GitHub token is really in the sandbox.** This
is the one property the microVM had that this does not: msb substituted the
value outside the guest, and a CONNECT tunnel cannot. What replaces it is the
token's shape — a GitHub App installation token, scoped to this repository and
expiring in an hour, minted per sandbox start. A fine-grained PAT works and
does not expire; `doctor` says so every time. Anything that widens that token's
scope is the decision, not the plumbing.

**The base filesystem is in `render.sh`, not in a profile, on purpose.** The
read-only `/usr`, the curated `/etc`, the tmpfs `$HOME` and the absence of a
resolver are one code path that every sandbox goes through, because a
per-profile copy is a per-profile chance to bind `$HOME` by accident. If you
find yourself adding a mount there rather than to a fragment, ask whether every
sandbox should have it.

**`/etc` is a list, not a bind.** `--ro-bind /etc /etc` would be one line and
would also hand the sandbox every world-readable configuration file on the
machine. `ETC_ENTRIES` is what the toolchain actually reads. When something
fails in a way that looks like a missing library or a missing certificate, that
list is the first place to look — and `/etc/ld.so.cache` in particular, whose
absence makes `execvp` report `No such file or directory` for a binary that is
plainly there.

**`/etc/resolv.conf` is absent deliberately.** There is nothing to resolve
against in an empty network namespace, and every allowed name is resolved by
the proxy on the host. A tool that tries anyway fails immediately instead of
hanging, and `getent hosts github.com` failing is a *passing* line in
`selftest`.

**Mount order is load order.** bwrap performs binds in the order given, so a
mount nested inside another only works if the parent came first:
`cargo-cache-shared` puts the shared registry inside the profile's
`$HOME/.cargo`, and it works only because capabilities are applied after the
profile. A fragment that mounts into a path another fragment provides has to be
granted after it, and `dedup` preserves order for the same reason.

**Not `--new-session`, and that is a considered choice.** It calls `setsid()`,
which costs the sandbox its controlling terminal: no job control, no Ctrl-C,
and Claude Code's TUI misbehaves. Its purpose is to block TIOCSTI injection
back into the parent's terminal, and that syscall has been disabled by default
since Linux 6.2. `doctor` checks `dev.tty.legacy_tiocsti` and warns when it is
enabled. On a kernel where it is on, the trade is real and the flag belongs
back.

**The sandbox runs as you, in a user namespace.** Files it writes to the clone
are yours, which is what makes the arrangement usable — and it means the mount
list is the whole boundary. A kernel LPE escapes it, which a microVM's
hypervisor boundary would not: that is the price paid for deleting the image,
the quotas and ten msb workarounds, and it is written down in README.md under
what is out of scope.

**`selftest` is not a nicety.** A mount list is something you can get wrong
silently, and one stray `--bind` undoes the whole directory. `./devbox.sh
selftest <branch>` asserts the negatives — no `~/.ssh`, no credentials file, no
host checkout, no route out, no resolver — and the positives that prove it is
still usable. Run it after touching `render.sh`, and add a line to it when you
add a mount.

**The environment is a file, not `--setenv`.** `/proc/<pid>/cmdline` is
world-readable and the GitHub token is in that environment, so bwrap gets
`--clearenv` and the entrypoint sources `/run/devbox/env`, written at 0600 in a
0700 directory. Anything that moves a value into an argument vector has undone
that. The same rule is why the brokers take `--listen-unix` and a path, never a
secret.

**The sandbox gets a standalone clone, never a worktree.** A worktree's `.git`
is a file pointing outside the mount, and any arrangement where the sandbox can
write the main repository's `.git` hands it the shared object store and
`.git/hooks` — which the *host* executes later. `git clone --no-hardlinks` is
part of that, not hygiene: without it the clone's objects are the same inodes
as this repository's, and a sandbox that rewrites one corrupts the host's copy.

**`origin` in the clone must be HTTPS.** There is no SSH key in the sandbox by
design, and the proxy only speaks CONNECT to port 443, so an SSH remote fails
in a way that looks like a network problem.

**The host's toolchain is the sandbox's toolchain.** `~/.rustup`,
`~/.cargo/bin` and `~/.local/share/claude` are bind-mounted read-only; there is
no image to build and no second copy of Rust on the disk. Two consequences:
upgrading rustup on the host upgrades every sandbox at once, and the host and
sandbox `target/` directories are interchangeable because the compiler is
literally the same binary. If pinning ever matters more than that, the answer
is a rootfs bound at `/`, not a Dockerfile.

**Pushing anything under `.github/workflows/` needs `Workflows: write`** on the
GitHub token. Adding or removing an environment variable in this repo means
editing `cd.yml`, so a branch that does routine work fails at the very end of
`/finish-development` with an error that reads like a git problem.

**`deploy.sh` stays host-only, permanently.** No capability may grant deploy;
the Odroid SSH key does not enter a sandbox. Same for `make deploy-*`. Prod
access from a sandbox goes through `brokers/prod` — a verb-limited API, never a
tunnel and never a key — and that broker's vocabulary is three read verbs.
Adding a fourth is a decision about what a compromised sandbox can do to
production, not a convenience; anything that takes a command, a path or a query
from the guest has stopped being this shape.

**The scrub list fails closed, and that is the point.** Every table in a
snapshot must appear in `DELETE`, `REWRITE` or `KEEP` in
`brokers/prod/src/scrub.rs`, checked against the snapshot's own `sqlite_master`
rather than this repository's migrations. Add a table to any app and the next
`devbox-prod snapshot` refuses by name until someone classifies it. That is
deliberate: the failure being guarded against is not a wrong rule, it is a
table added next year quietly carrying its contents into a sandbox.
Classifying it takes one line; working around the check hands away the only
thing that makes a snapshot safe to hold.

**Both `--no-hostname` and `-n 0` are there to keep the same promise.** The
sandbox is not supposed to learn where prod is — the capability hands it a
loopback URL and nothing else — and journal output undoes that by default:
`-o short-iso` prints the machine's hostname on every line, so `logs` passes
`--no-hostname`. `systemctl status` appends a journal tail of its own in the
*default* format, which `--no-hostname` does not reach from there, so `status`
passes `-n 0` and drops it. Both were verified by grepping the real Odroid's
answers for its hostname. Anything new the broker forwards wants the same look:
the check is to run it against prod and grep.

**The prod broker's health check must not touch the LAN.** Every sandbox start
that has `prod-readonly` calls `/_broker/health`, so that endpoint answers from
configuration alone and never SSHes. Making it "more useful" by probing the
Odroid would make starting a sandbox fail on a train.

**A snapshot lands under a possibly-running dev server.** `devbox-prod
snapshot` removes `-wal` and `-shm` beside the file it replaces, because SQLite
would otherwise apply the old write-ahead log to the new database — stale WAL
files next to a replaced database are not an empty database, they are a corrupt
one. Stop the server first; the helper says so, it cannot enforce it.

**The brokers are outside the cargo workspace.** `brokers/anthropic`,
`brokers/prod` and `brokers/proxy` each carry their own `[workspace]` and the
root manifest excludes `sandbox/`, so `make check` stays exactly what CI runs.
Keep it that way — and run their own `cargo test` when you change one, because
nothing else will.

**The guest helpers are bind-mounted, not installed.** `sandbox/guest/` lands
at `/opt/devbox/bin` and `sandbox/bootstrap/` at `/opt/devbox/bootstrap`, both
read-only, so editing `devbox-prod` takes effect on the very next command. The
old rule — rebuild the image *and* recreate the sandbox — is gone with the
image.

## Writing a capability

A fragment is a shell script, and sourcing it is how it is read. There is no
parser and no dependency beyond bash: the schema is small and fixed, merging is
appending, and substitution is shell variables.

```sh
describe "git push and gh pr create, with a token that expires in an hour"

credential GH_TOKEN github
allow github.com api.github.com codeload.github.com objects.githubusercontent.com
env GIT_ASKPASS /opt/devbox/bin/devbox-git-askpass
```

Directives: `describe`, `conflicts`, `cpus`, `memory`, `workdir`, `hostname`,
`env`, `mount`, `allow`, `port`, `broker`, `credential`, `cli`. Fragments read
the sandbox's paths from the environment: `BRANCH`, `CLONE`, `TARGET_CACHE`,
`CARGO_HOME_DIR`, `CARGO_CACHE`, `MODELS`, `CLAUDE_STATE`, `GUEST_BIN`,
`BOOTSTRAP_DIR`, `HOME_DIR`, `PLAYWRIGHT_CACHE`, and the three fixed in-sandbox
ports `ANTHROPIC_PORT`, `PROD_PORT`, `PROXY_PORT`.

Later fragments win on scalars and on `env`; lists concatenate and de-duplicate
in order; capabilities apply in the order given, after the profile. Two
capabilities that would both set the same single thing must declare
`conflicts`, because lists concatenate silently — `preview` and `preview-lan`
would otherwise publish port 3000 twice, once to loopback and once to the LAN.

`cli` is the escape hatch for a bwrap flag `render.sh` does not model. Use it
for flags, not for things that belong in a directive.

## Layout

```
devbox.sh            the host side: clone, render, helpers, one bwrap
render.sh            profile + capabilities -> a bwrap command line and a plan
profiles/            base sandboxes (resources, mounts)
capabilities/        one fragment per capability
guest/               bind-mounted read-only at /opt/devbox/bin
bootstrap/           run inside the sandbox from /opt/devbox/bootstrap
brokers/anthropic/   host daemon; holds the Anthropic credential
brokers/prod/        host daemon; holds the Odroid SSH key, three read verbs
brokers/proxy/       host daemon; the allow-listed way out to the network
brokers/github/      GitHub App token minting
.devbox/<branch>/    generated args, plan, env (gitignored)
```
