# Idea 02: Make `sandbox` generic — split the broker contract from its implementations

## Summary

`sandbox/` is being considered for reuse in other repos. Everything in it is
mkb + docker + shell except the brokers, which are Rust and need a toolchain.
The question was whether Rust is the right choice for the Anthropic broker.

The conclusion is that the language is the second question, not the first.
The thing blocking reuse is that generic `sandbox` would ship an *Anthropic*
broker at all — and, as of `maint-105`, a **prod** broker that is unshippable
anywhere but this repo. Define the broker as a contract, carry no
implementations in the generic core, and the language question stops being
load-bearing. Rust then stays, delivered as a container so no adopter needs
`cargo`.

## What the broker actually has to do

From `sandbox/brokers/anthropic/src/` (691 lines), the constraints that rule
options in and out:

1. **Stream SSE straight through** — `Body::from_stream(response.bytes_stream())`
   in `main.rs`; buffering holds every event back until the turn ends.
2. **Re-read the credential on every request** (`credentials.rs`, `Source::current`)
   — the host's own Claude Code rotates `~/.claude/.credentials.json` underneath
   the broker, and *following* that file is what avoids a second OAuth refresh
   chain invalidating the host's login.
3. **Rewrite headers per request** — strip the 12 in `STRIPPED`, inject
   `authorization`/`x-api-key`, merge `anthropic-beta`.
4. **Peek the JSON body** for `model`, to label the audit line.

(1) rules out shell. (2) and (4) rule out a static reverse-proxy config.

## What it costs today

| | |
|---|---|
| Crates in `Cargo.lock` | 172 |
| `target/` after a release build | 891 MB |
| Release binary | 4.0 MB |
| Source | 691 lines |

`devbox.sh` otherwise requires only `msb`, `git`, `jq`, `docker` and `curl`.
**`cargo` is the single host dependency that exists solely for the broker**,
and `doctor` hard-codes a `broker binary` check that fails until you run
`./devbox.sh build-broker`.

## Alternatives considered

### nginx / Caddy config — not viable

The natural "no code" answer. It fails on constraint (2): the config is static,
the OAuth token is not. You would need an inotify watcher regenerating config
and reloading on every refresh, and you would still lose the expiry check, the
actionable 401 text, and the `model` audit field. OpenResty/njs recovers them
but swaps a Rust toolchain for a heavier runtime — and you are writing code
again regardless.

### Go, single file — marginal

`httputil.ReverseProxy` with a `Director` is almost exactly this program, and it
already flushes `text/event-stream` immediately. ~150 lines, near-zero dep tree,
fast build. But it is still a toolchain and a build step — the same category as
Rust, only cheaper. It only wins if prebuilt per-platform binaries ship with
releases.

### Node, single file, zero deps — tempting, but rejected

`http` + `https` core; `req.pipe(upstream)` / `upstream.pipe(res)` gives
streaming and backpressure free, which is precisely the fiddly part.
`crypto.timingSafeEqual` replaces `secret_eq`; cert validation is on by default.
Realistically ~200 lines, no lockfile, no build. Cost: `node` joins the host
requirements, which mkb + docker + shell does not imply.

### Python, stdlib only — worse than Node

More universally present, but `http.server` is the weak link for constraint (1):
you must force `protocol_version = "HTTP/1.1"`, hand-roll chunked response
framing, and it is thread-per-connection. More sharp edges for no gain.

### Rust in a container — recommended delivery

`docker` is already required. Multi-stage build once, then
`docker run --name devbox-broker-<branch> -p 127.0.0.1:$port:$port`. Nobody
needs `cargo`. Container lifecycle also replaces the `setsid` + pid-file dance
in `start_anthropic_broker`/`stop_broker`.

Two caveats:

- With `--refresh writeback`, the atomic `rename` in `credentials.rs::write_back`
  fails onto a bind-mounted *file*. That mode needs `~/.claude` mounted as a
  directory; a read-only single-file mount is fine for the default `off`.
- Publishing a loopback port avoids `--network host`, which matters if an
  adopter is on macOS. (Where `~/.claude/.credentials.json` may not exist at
  all — Claude Code can keep credentials in the Keychain. An adopter on macOS
  needs the `--api-key` path.)

## Recommended approach

1. **Split the contract from the implementations.** `render.sh` and
   `ensure_brokers` already treat a broker as *a name in `$state/brokers`*. Make
   that a real interface — `brokers/<name>/run`, an executable taking
   `--listen --token-file --sandbox` — and have generic `sandbox` carry zero
   broker implementations. Repos that want model access drop one in; repos that
   do not never notice.
2. **Make `doctor` follow the contract.** Replace the hard-coded `broker binary`
   check with: for each declared broker, is its `run` executable? This is what
   currently makes a fresh clone fail `doctor` for a capability it may not want.
3. **Ship the Anthropic broker as an OCI image** — the reference implementation,
   costing an adopter a `docker pull` rather than a toolchain.
4. **Do not rewrite it.** It works, it is the one component holding the OAuth
   credential, and trading audited security code for ~490 fewer lines is a bad
   bet. If, after the split, it should still be smaller, Go is the honest pick —
   but by then it is optional, which was the point.

## Why `maint-105` settles this

This branch adds `sandbox/brokers/prod` — a second Rust broker, 1,249 lines
across `main.rs`, `remote.rs` and `scrub.rs`, holding an SSH key and answering
three read verbs against the Odroid.

It is the strongest argument for the split, in two ways. It is **irreducibly
MyApps-specific** — prod snapshots, credential scrubbing, one particular
Odroid — so generic `sandbox` can never ship it, and any design that assumes the
core carries its brokers is already wrong. And it establishes the broker as a
*pattern* rather than a one-off, which argues for shared scaffolding (token
file, loopback refusal, audit log, path allowlist) over a rewrite of the first
one. Note also that it is not an HTTP proxy to a public API, so a delivery story
resting on "containerize it" needs a second thought for the SSH key.

## Next steps

- [ ] Specify the `brokers/<name>/run` interface — arguments, health endpoint,
      what the host guarantees (token file, claimed port), what the broker owes.
- [ ] Factor the shared parts out of `brokers/anthropic` and `brokers/prod`
      now that there are two: token file reading, loopback refusal, audit log,
      path allowlist.
- [ ] Rework `doctor` to check declared brokers rather than one binary path.
- [ ] Prototype the Anthropic broker as a container; confirm SSE still streams
      unbuffered through a published loopback port.
- [ ] Decide where the reference implementations live once the core is generic —
      same repo behind an opt-in, or a separate `sandbox-brokers`.
