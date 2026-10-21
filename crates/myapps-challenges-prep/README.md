# myapps-challenges-prep

Prepares a Challenges dataset on a workstation, into the bundle the server
loads with `myapps import --app challenges --dataset <file>` (or
`./deploy.sh <env> import-dataset <file>`). It is never built for or deployed
to the server. The bundle format is defined in
[`myapps-challenges/src/bundle.rs`](../myapps-challenges/src/bundle.rs).

```sh
cargo run --release -p myapps-challenges-prep -- bundle ugphysics        # → ugphysics.sqlite
cargo run --release -p myapps-challenges-prep -- bundle hendrycks-math   # → hendrycks-math.sqlite
cargo run --release -p myapps-challenges-prep -- bundle data-science     # → data-science.sqlite, in seconds
```

Use the prep from the same commit as the deployed binary: the server refuses a
bundle format newer than its own. It still loads older ones it knows how to
read; each format only added to the one before (2: diagrams, 3: tags, 4: a
source per problem), so an older bundle loads as one with that table empty,
or those columns null, and need not be prepared again.

## Local decks

`datasets/` holds hand-authored decks that do not come from the Hugging Face
datasets-server, such as `data-science.jsonl`. They are source data, not
bundles: `bundle <key>` reads `datasets/<key>.jsonl` (or `--deck <file>`)
instead of fetching, checks every item against the deck's rules, and writes a
bundle in which each problem carries its own source and a prose answer
(`answer_type = 'text'`). A deck sets each item's level by hand, so `grade`
refuses it. See [`datasets/README.md`](datasets/README.md) for the schema,
counts, licence and review status.

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
4. **Attach grades** from the grader's output for the dataset,
   `grades/<dataset>-v1.jsonl` in this crate (or `--grades <file>`), as the
   bundle's tags. A row applies only while the problem's text still has the
   SHA-256 it was graded with; the rest are counted as stale and logged, so an
   upstream edit leaves a problem ungraded rather than mis-graded. No file
   means the dataset is not graded yet, and the bundle has no tags.
5. **Write** the bundle, as `<file>.partial` renamed into place at the end.

## Grading

`grade` writes the grades file that `bundle` attaches: a difficulty estimate
per problem, plus the answers to the dataset's rubric questions. The reasoning
and measurements behind it are in
[ideas-03, "Grader spike"](../../docs/ideas-03-graded-math-physics.md#grader-spike-2026-10-04).
It talks to one `llama-server` chat model that you start yourself, alone on the
GPU (two models at once have set off the OOM killer on the dev laptop):

```sh
llama-server -m ~/Models/Qwen3-8B-Q4_K_M.gguf -ngl 99 -c 8192 --parallel 1 --port 8091
```

Then, per dataset, three stages:

```sh
cargo run --release -p myapps-challenges-prep -- grade flags hendrycks-math   # ~2.4 s a problem
cargo run --release -p myapps-challenges-prep -- grade judge hendrycks-math   # 0.6–1.1 s a question, 6 per problem
cargo run --release -p myapps-challenges-prep -- grade rank hendrycks-math    # seconds; writes grades/hendrycks-math-v1.jsonl
```

1. **`flags`** asks every problem the rubric's seven questions
   (`src/grade/rubric.rs`), the problem first and the question after it, so the
   server's prompt cache processes each problem once.
2. **`judge`** asks "which is harder?" of random pairs within a subject,
   `--rounds` (6) comparisons per problem, each pair both ways round.
3. **`rank`** fits a ridge regression from the flags and lengths to the
   dataset's levels, out-of-fold, as each problem's prior, then Bradley-Terry
   over the verdicts with the prior as a Gaussian (`--tau`, 1.0). It prints
   how prior and grade agree with the levels, per subject, and how consistent
   the judge was, and writes the grades file. Hand-checked pairs in
   `grades/<dataset>-checks.jsonl` (`{"harder": key, "easier": key}` per line)
   are scored too.

### Checking the grades against a solver

`solve` is not part of the grades but a check on them that needs no expert: a
stronger model attempts a sample of the problems, and `rank` then reports
which of the levels, the prior and the grade best predicts where it failed
(and how long it had to think). The solver thinks before answering, which
takes minutes a problem, so give it a sample, and a server of its own (no
other model on the GPU):

```sh
llama-server -m ~/Models/Qwen3.6-35B-A3B-UD-Q4_K_M.gguf -ngl 99 -c 16384 --cache-ram 0 --parallel 1 --port 8091 \
  --reasoning-budget 8192 --reasoning-budget-message "Time is up, so I'll give my final answer now."
cargo run --release -p myapps-challenges-prep -- grade solve ugphysics --limit 200   # ~2–7 min a problem
cargo run --release -p myapps-challenges-prep -- grade rank ugphysics                # reports "Solved by …"
```

The solver writes its answer in `\boxed{}`; it counts as right when it matches
the dataset's answer once spacing and sizing are ignored, or as a number to
1%, and otherwise the same model, thinking off, is asked whether the two are
equivalent (one token, as for the flags). Each attempt is kept with its
answer, verdict, token count and reply in `solve.jsonl` under the solver's
cache directory. `--solver` names its model file, `--attempts N` samples each
problem N times with different seeds, for a solve rate rather than right or
wrong. The first `--limit` problems are the same ones as the flags and judge
sample runs used.

To compare another solver, run `solve` with its model loaded and `--solver`,
then reload the server with the checker (Qwen3.6, `--checker`) and run
`check`, which asks it about every answer the other solver checked itself;
`rank --solver …` then uses the common checker's verdicts:

```sh
# server: llama-server -m ~/Models/gemma-4-26B-A4B-it-UD-Q4_K_M.gguf … (same options as above)
cargo run --release -p myapps-challenges-prep -- grade solve ugphysics --limit 40 --solver gemma-4-26B-A4B-it-UD-Q4_K_M.gguf
# server: back to Qwen3.6
cargo run --release -p myapps-challenges-prep -- grade check ugphysics --solver gemma-4-26B-A4B-it-UD-Q4_K_M.gguf
cargo run --release -p myapps-challenges-prep -- grade rank ugphysics --limit 300 --solver gemma-4-26B-A4B-it-UD-Q4_K_M.gguf
```

`solve-compare.sh` in this crate does all of that unattended, Qwen3.6 then
Gemma 4 on the first `LIMIT` (40) problems: it starts and stops each server
itself, one model at a time, keeps the laptop awake, logs to
`~/solve-compare.log`, and resumes when run again. `SOLVERS=qwen` runs
Qwen3.6 alone, to solve more problems for `rank` (problems already solved are
skipped):

```sh
LIMIT=200 SOLVERS=qwen crates/myapps-challenges-prep/solve-compare.sh
```

`--cache-ram 0` matters: llama-server keeps every finished request's state in
a RAM prompt cache of up to 8 GiB, which no later problem reuses, and beside
Qwen3.6's 21.7 GB on the 30 GB laptop it grew until the OOM killer stopped the
first run after 14 problems. The script also starts each server in a systemd
scope of its own, so an OOM kill stops only the server, and then restarts it
and resumes the stage (up to `TRIES`, 3); a stage whose server stops answering
exits rather than skipping the problems left.

Sampling follows each family's model card (Gemma: temperature 1.0, top-k 64;
otherwise Qwen's 0.6, top-k 20).

Answers are cached under `~/.cache/myapps-challenges-prep/grade/<dataset>/<model>/`
one line at a time, keyed on the problem text's hash, so a killed run resumes
where it stopped and an edited problem is asked again. `--limit N` (the same N
problems every time) and `--subject` make quick runs; `rank` on such a run only
reports, unless given `--out`. `--model` names the file the server must have
loaded. Changing a rubric question means bumping `rubric::VERSION`, which
starts its answers afresh.



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
