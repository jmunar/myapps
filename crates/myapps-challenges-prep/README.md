# myapps-challenges-prep

Prepares a Challenges dataset on a workstation, into the bundle the server
loads with `myapps import --app challenges --dataset <file>` (or
`./deploy.sh <env> import-dataset <file>`). It is never built for or deployed
to the server. The bundle format is defined in
[`myapps-challenges/src/bundle.rs`](../myapps-challenges/src/bundle.rs).

```sh
cargo run --release -p myapps-challenges-prep -- ugphysics        # → ugphysics.sqlite
cargo run --release -p myapps-challenges-prep -- hendrycks-math   # → hendrycks-math.sqlite
```

Use the prep from the same commit as the deployed binary: the server refuses a
bundle format newer than its own. It still loads older ones it knows how to
read; a format-1 bundle, from before diagrams, loads as one with no diagrams,
so a dataset without any (UGPhysics) need not be prepared again.

## Stages

1. **Fetch** every row from the Hugging Face datasets-server, paced to stay
   under its rate limit (about 8 minutes for Hendrycks MATH the first time),
   and map it to a problem. Pages are cached, so later runs skip this;
   `--refetch` fetches again, which is how an upstream update gets in.
2. **Render** each Asymptote block (`[asy]…[/asy]`) to SVG, about a minute for
   all 2,493 on 12 cores. Results, failures included, are cached by the hash of
   the block, under a directory keyed by the `asy` version, the preamble and the
   modules, so changing any of them renders everything again.
   `--retry-failed` retries the cached failures.
3. **Drop** the problems whose own text has a diagram that failed; a solution's
   failed diagram becomes a placeholder on the page instead.
4. **Write** the bundle, as `<file>.partial` renamed into place at the end.

## Rendering setup (Hendrycks MATH only)

- **Asymptote and TeX**: `sudo pacman -S asymptote` (pulls in `texlive-basic`).
- **bubblewrap**: every block runs in its own sandbox with no network, a
  read-only system, an empty `/home` and only its work directory writable.
  `asy` disables its system calls by default, but can still read files, and a
  label could carry one into the SVG that ends up on the server.
- **AoPS modules**: the blocks were written for AoPS, which provides
  `olympiad` and `cse5` and imports them implicitly. They are not part of
  Asymptote and are not ours to redistribute, so they are not in this repo.
  Put `olympiad.asy` and `cse5.asy` in `~/.cache/myapps-challenges-prep/modules/`
  (or pass `--modules <dir>`); the prep imports them implicitly when present
  and warns when not. Copies live in
  [vEnhance/dotfiles](https://github.com/vEnhance/dotfiles/tree/main/dot/asy).
  In `olympiad.asy`, change `include graph;` and `include math;` to `import`:
  `include` copies the module in, so a block's own `import graph` defines
  every axis function twice, and about 80 blocks fail as "ambiguous".
  `TrigMacros`, which 32 blocks import, is not published anywhere, so those
  fail.

A block that sets no size (`size`, `unitsize`) gets `size(180)`: Asymptote's
default of one point per unit draws a triangle with sides of 10 at the height
of its own labels. Errors are in `~/.cache/myapps-challenges-prep/asy/<key>/<hash>.err`,
starting with `asy`'s own error line, then the full output and the source.
