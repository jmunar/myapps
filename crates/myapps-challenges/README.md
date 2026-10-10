# Challenges

One undergraduate maths or physics problem at a time, at a level that follows
how you are doing. Read the problem, work it out, reveal the solution and mark
yourself right or wrong.

The design, and the research behind the dataset choice, is in
[docs/ideas-03-graded-math-physics.md](../../docs/ideas-03-graded-math-physics.md).

## Screenshots

<p align="center">
  <img src="../../docs/screenshots/challenges-picker.png" width="270" alt="Dataset picker" />
  <img src="../../docs/screenshots/challenges-problem.png" width="270" alt="Problem with solution" />
  <img src="../../docs/screenshots/challenges-stats.png" width="270" alt="Stats" />
</p>

## Datasets

| Dataset | Problems | Levels | Licence |
|---|---|---|---|
| [UGPhysics](https://huggingface.co/datasets/UGPhysics/ugphysics) (English) | 5,314 in 13 subjects | 3, from its skill type: Knowledge Recall → Laws Application → Derivation / Practical | CC BY-NC-SA 4.0 |
| [Hendrycks MATH](https://huggingface.co/datasets/EleutherAI/hendrycks_math) | 11,372 in 7 subjects | 5, as published | MIT |

Neither ships in the binary or the repo. Each is prepared on a workstation by
[`myapps-challenges-prep`](../myapps-challenges-prep), which fetches it, maps
its rows and writes a **bundle**: one SQLite file per dataset, in the format
defined (and versioned) in [`src/bundle.rs`](src/bundle.rs). The server only
loads bundles; it never fetches or computes anything itself.

```sh
cargo run --release -p myapps-challenges-prep -- hendrycks-math   # → hendrycks-math.sqlite
myapps import --app challenges --dataset hendrycks-math.sqlite    # or: ./deploy.sh prod import-dataset …
```

A bundle is the whole dataset. Loading one upserts its problems on their key,
so ids and the attempts that point at them survive a reload, and *retires* the
problems it no longer has: they are never drawn again, but stay for the
history, and come back if a later bundle has them. The load is one
transaction that validates every row first, so a bad bundle changes nothing.

Left out during preparation: UGPhysics problems without a `level`; Hendrycks
problems tagged `Level ?` or drawn with Asymptote (`[asy]`), which a browser
cannot render.

## How the next problem is chosen

- The **subject** is uniformly random among the dataset's subjects.
- Your **level** is tracked per subject, starting at 1. Two right answers in a
  row move it up, one wrong moves it down. Until your first wrong answer in a
  subject, a single right answer moves it up, so the cold start is quick.
- The **target level** is drawn from L-1 / L / L+1 with weights 15 / 60 / 25.
- Problems you have **never attempted** come first, falling back to other
  levels by distance from yours; once a subject is exhausted, the problem seen
  longest ago comes back.

Once drawn, a problem stays yours until you mark it or skip it: each dataset
has one practice URL that always shows its problem in progress, across reloads
and server restarts. Marking and *Skip* swap the next problem into the page
rather than navigating, so the browser's back and forward buttons leave the
practice page instead of stepping through problems. *Skip* draws another
problem without recording anything. A subject with nothing left but the problem
you just finished hands over to another subject rather than repeating it.

## Features

- Dataset picker with problem counts and your overall accuracy; *Continue*
  when a problem is in progress; datasets you hide move to a list below it
- Practice page with the answer, the worked solution and the right/wrong
  buttons behind *Show solution*; LaTeX typeset by KaTeX
- A level-change notice above the next problem when an answer moves you up or down
- Stats: accuracy and current level per subject, and accuracy per level
- Command bar: `next_problem` (opens the problem in progress, optionally per
  dataset), `stats`
