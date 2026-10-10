# Idea 03: Challenges — graded maths and physics practice

## Summary

A new app, **Challenges** (key `challenges`, crate `myapps-challenges`), that
serves one undergraduate problem at a time from a public dataset, adapts the
difficulty to how you have been doing, and keeps your accuracy.

The MVP has no answer checker: you read the problem, work it out on paper, reveal
the solution, and mark yourself right or wrong. Mechanical grading (SymPy, EED,
the UGPhysics answer-type taxonomy) is the obvious next step and is deliberately
left out — see [Later](#later). The research behind the dataset choice is kept
below, under [Sources](#sources).

MVP scope:

1. **Dataset selector** — UGPhysics (physics) or Hendrycks MATH (maths).
2. **Problem selector** — subject chosen uniformly at random; problem chosen at
   random from the neighbourhood of your current level in that subject; cold
   start at the easiest level. *Show solution*, then *I got it right* / *I got
   it wrong*.
3. **Stats** — accuracy per subject and per level, and your current level in
   each subject.

## Status (2026-10-04)

The MVP below is implemented in `crates/myapps-challenges/`. Imported counts
from a local run of both imports:

| Dataset | Imported | Left out | By difficulty |
|---|---|---|---|
| UGPhysics | 5,314 | 206 with no `level` | 1: 584 · 2: 1,957 · 3: 2,773 |
| Hendrycks MATH | 11,372 | 1,126 with `[asy]` diagrams, 2 `Level ?` | 1: 915 · 2: 2,060 · 3: 2,506 · 4: 2,646 · 5: 3,245 |

Four Hendrycks solutions have no `\boxed{}` answer; their page shows the worked
solution only.

Differences from the plan below, found while building it:

- **The import is paced.** The datasets-server answers 429 after about fifty
  back-to-back requests, so the importer waits 1.5 s between pages and backs
  off (honouring `Retry-After`) on 429 and 5xx. UGPhysics takes about five
  minutes, Hendrycks MATH about eight.
- **The seed inserts nine hand-written sample problems**, but only into a
  dataset with no problems at all, so a demo user (and the README screenshots)
  have something to draw without network access. A real import is never mixed
  with them.
- **Datasets are prepared offline and loaded as bundles.** `serve` first
  imported on its own, straight from the datasets-server. That was replaced
  once diagrams and extracted features made preparation too heavy for the
  Odroid: `myapps-challenges-prep` now writes one SQLite bundle per dataset on
  a workstation, and the server only loads it (see the decision below).
- **The stats table has four columns** (subject, level, correct / attempts,
  accuracy), so it fits a phone as a two-line card.

## Decisions

### Datasets are prepared offline, as one bundle per dataset

Rendering Asymptote diagrams and extracting features (concepts, techniques,
problem type) per problem are both too heavy for a 4 GB box shared with
whisper.cpp and llama.cpp, so preparation is a separate stage run on a
workstation, and the server does nothing but load its output.

- **One SQLite file per dataset** (`src/bundle.rs`): self-contained, readable
  with tools sqlx already brings, inspectable with `sqlite3`. Its format is
  versioned, and the server refuses a format it was not built for.
- **The prep tool is a separate crate** (`myapps-challenges-prep`), never
  built for the server, so its dependencies cost the Odroid nothing; it shares
  the bundle types with the app, so the two sides cannot drift.
- **A bundle is the whole dataset.** Problems are keyed by `(dataset,
  source_key)`, never by server row id; ones a bundle drops are retired, never
  deleted, because attempts reference them.
- **Features will differ per dataset**, each from its own fixed taxonomy, and
  are meant to be stored as generic `(problem, kind, value)` tags so one query
  answers "where do I fail" for any dataset.
- **No import on `serve`.** One way to fill the catalogue, rather than a second,
  featureless one.

### Datasets: UGPhysics and Hendrycks MATH, not U-MATH

The selector needs a difficulty per problem, and *show solution* needs a
solution. Checked against the datasets themselves (HF datasets-server, October
2026):

| | UGPhysics (EN) | Hendrycks MATH | U-MATH |
|---|---|---|---|
| Problems | 5,520 | 12,500 (7,500 train + 5,000 test) | 1,100 |
| Subjects | 13 | 7 | 6 |
| Difficulty field | `level` — a **skill type**, not a difficulty | `level` — "Level 1"…"Level 5", human-assigned | **none** |
| Worked solution | yes | yes (final answer in `\boxed{}`) | **no** — golden answer only |
| Diagrams | none | some, as Asymptote source (`[asy]`) | 20%, as base64 PNG |
| Licence | CC BY-NC-SA 4.0 | MIT | MIT |

U-MATH is the better *topic* fit (university calculus, series, multivariable)
but has neither of the two fields the MVP depends on. Hendrycks MATH is
high-school competition maths (prealgebra, algebra, number theory, counting &
probability, geometry, intermediate algebra, precalculus): a different muscle,
but it has a real 1–5 scale and full solutions. U-MATH comes back once
difficulty is learned from attempts rather than read from the dataset.

### UGPhysics `level` becomes a three-step ladder

UGPhysics' `level` says what kind of skill a problem exercises. Counts over the
English split:

| `level` | Problems | Tier |
|---|---|---|
| Knowledge Recall | 565 | 1 |
| Laws Application | 1,957 | 2 |
| Math Derivation | 2,220 | 3 |
| Practical Application | 553 | 3 |
| *(empty)* | 206 | dropped at import |

It is ordinal enough to drive "start easy, move up", but coarse: tier 3 holds
half the dataset, and its spread is uneven per subject (Quantum Mechanics has
526 of 1,019 in Math Derivation; Geometrical Optics has 58 problems in total).

### Difficulty is stored per problem as a small integer

`difficulty` is 1–3 for UGPhysics and 1–5 for Hendrycks, with the dataset's
maximum held in code (`Dataset::max_level`). Levels are never compared across
datasets, so there is no need to normalise them.

### The user's level is per (dataset, subject), moved by a staircase

The subject is drawn uniformly, so a single level per dataset would be pulled
around by whichever subjects you happen to draw. Rusty in QM and sharp in
mechanics are both true at once, so the level is per subject.

- Start at level 1 (the cold start the brief asks for).
- **2-up / 1-down**: two correct in a row moves up one level, a wrong answer
  moves down one (never below 1, never above `max_level`). This staircase
  converges on the level you get right about 71% of the time — hard enough to
  be worth doing, easy enough to keep going.
- **Fast start**: until the first wrong answer in a subject, *one* correct
  moves up. Otherwise reaching Hendrycks level 5 takes at least 8 correct answers per
  subject — 56 across the seven — most of them well below your level. One wrong
  answer turns the fast start off for that subject for good.

### "Neighbourhood" is a weighted draw over L-1, L, L+1

With current level `L`, the target level is drawn from:

| Level | Weight |
|---|---|
| L - 1 | 15% |
| L | 60% |
| L + 1 | 25% |

Weights for levels outside `1..=max_level` are dropped and the rest
renormalised (at L = 1: 0% / 71% / 29%). The upward lean is deliberate: the
staircase only moves up on evidence, and an occasional harder problem is that
evidence.

### Unseen first, then fall back

Within the chosen subject and level, draw uniformly among problems you have
never attempted. If there are none, try the other two neighbourhood levels
(L, then L+1, then L-1), then any level in that subject. If you have seen every
problem in the subject, repeat the one you attempted longest ago. Small subjects
(Geometrical Optics: 58) will reach this point; that is fine.

### No answer key until you ask for it

The solution and final answer are rendered on the problem page inside a
`<details>` (no JavaScript), so revealing them costs nothing. The right/wrong
buttons are *inside* the `<details>` as well, so you cannot mark yourself
without having looked at the answer.

### Datasets are imported at runtime, not shipped in the binary

~13,000 problems are on the order of 20 MB of text. That is fine in SQLite but
not in an `include_str!` on a 4 GB box, and redistributing CC BY-NC-SA content
in this repo is something we should not have to think about. So `serve`
imports any dataset not yet imported in a background task on start (see
[Status](#status-2026-10-04)), and the same import is available by hand:

```sh
myapps import --app challenges --dataset ugphysics
myapps import --app challenges --dataset hendrycks-math
```

Both read the HF **datasets-server `/rows` JSON API** (100 rows per request,
`reqwest` + `serde_json`, both already in the workspace). That gives one code
path for both datasets and avoids a Parquet/Arrow dependency: Hendrycks MATH on
HF is Parquet-only. Rows with non-empty `truncated_cells` are logged and
skipped (no long cells are expected in either dataset, but a silently cut
solution would be worse than a missing one).

The import is idempotent: an upsert keyed on `(dataset, source_key)`, so
re-running it after the source changes keeps problem ids — and therefore the
attempt history — stable.

## Design

### Tables

One new migration, `crates/myapps-challenges/migrations/<ts>_challenges.sql`.
Every table carries the `challenges_` prefix (the authorizer enforces it).

```sql
-- Catalogue, shared by all users: no user_id, so delete-user-app-data leaves it alone.
CREATE TABLE challenges_problems (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    dataset       TEXT    NOT NULL,          -- 'ugphysics' | 'hendrycks-math'
    source_key    TEXT    NOT NULL,          -- stable id within the dataset (below)
    subject       TEXT    NOT NULL,          -- display name, e.g. 'Classical Mechanics'
    topic         TEXT,                      -- UGPhysics `topic`; NULL for Hendrycks
    difficulty    INTEGER NOT NULL,          -- 1..=max_level of the dataset
    source_level  TEXT    NOT NULL,          -- raw `level`, e.g. 'Math Derivation' / 'Level 4'
    problem       TEXT    NOT NULL,          -- LaTeX-in-text
    solution      TEXT    NOT NULL,
    answer        TEXT    NOT NULL,          -- UGPhysics `answers`; Hendrycks: last \boxed{…} of solution
    answer_type   TEXT,                      -- UGPhysics only; kept for the grader, unused in the MVP
    unit          TEXT,
    source_url    TEXT    NOT NULL,
    license       TEXT    NOT NULL,
    imported_at   TEXT    NOT NULL DEFAULT (datetime('now')),
    UNIQUE (dataset, source_key)
);
CREATE INDEX idx_challenges_problems_pick
    ON challenges_problems(dataset, subject, difficulty);

CREATE TABLE challenges_attempts (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id     INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    problem_id  INTEGER NOT NULL REFERENCES challenges_problems(id),
    correct     INTEGER NOT NULL,            -- 0 | 1
    level_at    INTEGER NOT NULL,            -- the user's level when it was drawn
    created_at  TEXT    NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_challenges_attempts_user ON challenges_attempts(user_id, problem_id);

CREATE TABLE challenges_progress (
    user_id     INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    dataset     TEXT    NOT NULL,
    subject     TEXT    NOT NULL,
    level       INTEGER NOT NULL DEFAULT 1,
    streak      INTEGER NOT NULL DEFAULT 0,  -- consecutive correct at this level
    fast_start  INTEGER NOT NULL DEFAULT 1,  -- cleared on the first wrong answer
    PRIMARY KEY (user_id, dataset, subject)
);
```

`challenges_progress` is derivable from `challenges_attempts` by replaying it,
but the staircase is path-dependent and replaying on every draw is pointless
work. The attempt insert and the progress update run in one transaction, in
`ops::record_attempt`.

`attempts.problem_id` has no `ON DELETE CASCADE` on purpose: the import only
ever upserts, so nothing deletes catalogue rows, and if something ever does, the
foreign key should fail rather than quietly delete history.

### Import mapping

| | UGPhysics | Hendrycks MATH |
|---|---|---|
| Configs read | 13 subjects × split `en` | 7 subjects × splits `train`, `test` |
| `source_key` | `<config>/<index>` | `<config>/<split>/<row_idx>` |
| `subject` | `subject` field | `type` field |
| `difficulty` | tier table above | digit of `level` |
| Dropped | empty `level` (206) | `Level ?` (2); any `[asy]` in `problem` (count is logged) |
| `answer` | `answers` | last `\boxed{…}` in `solution` (brace-matched, not a regex) |
| `source_url` | dataset URL + `source_key` | same |
| `license` | `CC BY-NC-SA 4.0` | `MIT` |

`Dataset` is a Rust enum (`Ugphysics`, `HendrycksMath`) with `key()`,
`name()`, `max_level()`, the HF dataset id, its configs/splits and a row-mapping
function. It is the only place the two datasets differ; the selector, routes and
stats only see `Dataset`. Adding U-MATH later means a third variant.

The `import` hook is new: neither `seed` (per user) nor `backfill` (per user,
days-based) fits. Add to `App`:

```rust
/// Load or refresh a shared catalogue, invoked by `myapps import`.
/// Returns `None` if the app has nothing to import.
fn import<'a>(
    &'a self,
    _pool: &'a SqlitePool,
    _config: &'a Config,
    _what: &'a str,
) -> Option<BoxFuture<'a, anyhow::Result<()>>> {
    None
}
```

and a `Command::Import { app: String, dataset: String }` in `cli.rs`, mirroring
`Backfill`.

### Selector (`crates/myapps-challenges/src/selector.rs`)

Pure functions over an injected `rand::Rng`, so they are unit-tested with a
seeded RNG and no database:

- `pick_subject(subjects, rng)` — uniform.
- `pick_level(level, max_level, rng)` — the weighted neighbourhood draw.
- `fallback_order(target, level, max_level)` — the level order to try when the
  target level has no unseen problems.
- `advance(progress, correct) -> Progress` — the staircase, including fast start.

`ops::draw(pool, user_id, dataset, rng)` wires them to SQL: read the subject
list (`SELECT DISTINCT subject … WHERE dataset = ?`), read or default the
progress row, then for each candidate level run

```sql
SELECT id FROM challenges_problems p
WHERE dataset = ? AND subject = ? AND difficulty = ?
  AND NOT EXISTS (SELECT 1 FROM challenges_attempts a
                  WHERE a.user_id = ? AND a.problem_id = p.id)
ORDER BY random() LIMIT 1
```

`ORDER BY random()` on at most ~1,000 rows per (subject, difficulty) is fine.

### Routes

All under `/challenges`, all server-rendered, plain forms (no HTMX needed for
the MVP):

| Method | Path | Does |
|---|---|---|
| GET | `/` | Dataset picker: one card per dataset, with problem count and your overall accuracy; *Start* button per card. Links to stats. |
| POST | `/draw` | Form field `dataset`. Calls `ops::draw`, `303` to `/problems/{id}`. |
| GET | `/problems/{id}` | Dataset, subject, topic, level ("Level 2 of 3 — Laws Application"), the problem. `<details>`: *Show solution* → final answer, unit, worked solution, then the two buttons. Footer: source link and licence. *Skip* (POST `/draw` again; nothing recorded). |
| POST | `/problems/{id}/attempt` | Form field `correct`. `ops::record_attempt`, then `303` to `/draw` for the same dataset — via a small interstitial if the level changed ("Level up: Quantum Mechanics → 3"), straight on otherwise. |
| GET | `/stats` | Per dataset: a table of subject / current level / attempts / correct / accuracy, a totals row, and a per-level accuracy row. `table-cards` for phone width. |

`/problems/{id}` accepts any id, not only the one just drawn: viewing and
marking a problem you navigated to directly is harmless, and bookmarking a
problem is a feature.

Re-POSTing an attempt (back button, double tap) would record it twice. Guard it
by ignoring an attempt for the same `(user, problem)` within 10 seconds of the
last one.

### Rendering maths

Problems and solutions are LaTeX inside prose (`$…$`, `$$…$$`, `\[…\]`,
UGPhysics uses `\begin{aligned}` and `\mathrm`; Hendrycks uses `\boxed`, `\frac`,
`\dfrac`). Rendered client-side with **KaTeX** + its `auto-render` extension:

- Vendor `katex.min.js`, `auto-render.min.js`, `katex.min.css` and the `woff2`
  fonts under `static/katex/` (served by the existing `ServeDir`, shipped by the
  existing `rsync` of `static/`). Nothing server-side, so no memory cost on the
  Odroid.
- Server side: `html_escape` the text, wrap it in an element with
  `white-space: pre-wrap` and `data-challenges-math`. KaTeX auto-render works on
  text nodes, so escaping first is safe and is still required (this is
  third-party text).
- `crates/myapps-challenges/static/challenges-math.js`: on load, run
  `renderMathInElement` over every `[data-challenges-math]`, with
  `throwOnError: false` so one bad formula shows as red source rather than
  killing the page. Per CLAUDE.md: no interpolation in the script, the base path
  from `<html data-base>`, and a test asserting `data-challenges-math` is still
  rendered.
- Wide display maths scrolls sideways inside its block (`overflow-x: auto`),
  which `nav-swipe.js` already respects via its overflow check. Verify on a
  phone.

### Command bar

`ops.rs` gets `draw` and `record_attempt`; the command bar gets one action,
`next_problem` (param: `dataset`, defaulting to the last one used), which
returns a link to the drawn problem. Marking right/wrong stays a page action: it
only makes sense after reading the solution.

### i18n

App strings in `crates/myapps-challenges/src/i18n.rs`, EN and ES. Problem text
stays English (both datasets are English-only for our purposes; UGPhysics' `zh`
split is not imported). Subject names are shown as the dataset spells them.

## Implementation steps

Each step leaves `make check` green.

1. **Scaffold** — `/add-app Challenges`: crate, workspace wiring, launcher
   entry, empty router, `i18n.rs`, `static/style.css` (every class prefixed
   `challenges-`).
2. **Migration + `Dataset` enum** — the three tables; `Dataset` with
   `key/name/max_level`; the UGPhysics tier mapping and the Hendrycks `level`
   parse, unit-tested.
3. **`import` hook and CLI** — `App::import`, `Command::Import`; the
   datasets-server pager; both row mappers, including the `\boxed{}` extractor
   (unit-tested on nested braces) and the `[asy]` / empty-level filters; upsert.
   Run it locally and record the actual imported counts in this doc
   (done: see [Status](#status-2026-10-04)).
4. **Selector** — `selector.rs` with seeded-RNG tests: uniform subject, weights
   at L = 1 / middle / max, every fallback branch, the staircase (2-up/1-down,
   fast start, clamping).
5. **`ops.rs`** — `draw` and `record_attempt` (transaction, duplicate guard);
   tests against an in-memory database with a handful of fixture problems.
6. **Pages** — picker, problem, stats; `table-cards` for stats. Design at
   390 px first.
7. **KaTeX** — vendor files, `challenges-math.js`, the attribute test. Check it
   actually runs in a browser (CI does not parse `static/*.js`).
8. **Command bar** — `next_problem`.
9. **Tests** — `frontend-tester` for routes and rendered HTML (including an
   XSS fixture: a problem containing `</script><img onerror>`); then
   `/frontend-walkthrough` on a phone-sized viewport for maths rendering, the
   `<details>` reveal, and horizontal scroll on long equations vs. tab swipe.
10. **Docs and deploy** — README for the crate; `docs/deployment.md`: add
    `challenges` to `DEPLOY_APPS` and run the two `myapps import` commands once
    after the first deploy (they need outbound HTTPS to `huggingface.co`). No
    new environment variables. `/finish-development`.

## Later

In rough order of value:

- **Answer checking.** Type an answer instead of self-marking. NV with
  tolerance and units first (the largest UGPhysics type), then EX/EQ via SymPy
  equivalence, then PHYBench's EED score for partial credit. `answer_type` is
  already stored for this. SymPy means Python on the Odroid: weigh it against
  the memory budget.
- **Problem grader.** A one-off, expensive run (likely an LLM, so not
  reproducible) that estimates each problem's difficulty. Its output must not
  live only in a database — databases are disposable, and prod migrates with no
  backup — so the grader writes a versioned file in the repo, e.g.
  `crates/myapps-challenges/data/grades-v1.jsonl`, one row per problem:
  `dataset`, `source_key`, a hash of the problem text, the estimate, the model
  and the run. No problem text, so it is small (under 1 MB), reviewable, ships
  with the binary and sidesteps UGPhysics' NC-SA licence. The import applies it
  after the catalogue, into a table of its own (`challenges_grades`) rather than
  over `difficulty`, which the upsert rewrites and which is worth keeping to
  compare against. The text hash matters: a Hendrycks `source_key` is a row
  position (`algebra/test/12`), so an upstream reorder must leave a problem
  ungraded, not mis-graded. This also makes a page cache of the HF download
  (to spare a fresh dev database the ~13-minute import) a nice-to-have rather
  than a need.
- **Learned difficulty.** An Elo/IRT-style rating per problem and per
  (user, subject), seeded from `difficulty`, updated on every attempt. This
  replaces the dataset's own levels and is what lets U-MATH (no levels) in.
- **More datasets**: U-MATH, PhysUniBench (needs image storage), SciBench,
  the OpenStax answer keys.
- **Daily problem**: a `cron` push notification with the day's draw.
- **Subject filter**: restrict a session to chosen subjects.
- Hendrycks `[asy]` diagrams: render Asymptote offline to SVG at import time.

## Sources

Research notes gathered before the MVP was scoped, kept as written. The
"steps" they refer to come from the original plan (1: sources, 2: domain and
difficulty, 3: hints, 4: grading). The MVP covers steps 1 and 2; grading is
under [Later](#later), and hints are not planned yet.

**Target** (revised): one problem per day, **undergraduate** level, **physics and maths**,
**answer-based** (a final answer that can be checked mechanically, not a proof to be judged).
User background: physics degree, several years rusty.

Assessed for **private, personal, non-commercial study use**. Licence notes flag
redistribution limits; none of them stop you reading the problem.

---

### The headline: "answer-based" in undergrad physics means *symbolic*, not numeric

Very few undergraduate physics answers are a number. Most are an expression —
`v = sqrt(2gh(1-cos θ))`. So the step-4 checker is **SymPy equivalence**, not a float
comparison, and it must accept algebraically equivalent rearrangements.

Two projects have already solved exactly this and you should borrow from both:

- **[PHYBench](https://huggingface.co/datasets/Eureka-Lab/PHYBench)** defines the **EED score**
  (Expression Edit Distance) — SymPy expression trees + tree edit distance, so a near-miss
  scores 60–100 rather than simply "wrong". That is a far better daily-practice signal than
  pass/fail, and it is MIT licensed.
- **[UGPhysics](https://huggingface.co/datasets/UGPhysics/ugphysics)** ships an **answer-type
  taxonomy** — NV (numerical), MC, TF, EX, EQ (equation), IN (interval), KR (recall) — with a
  judging routine per type. That taxonomy is essentially the spec for your grader, for free.

---

### Tier A — the core three (physics)

| Source | Size | Level | Fields | Licence |
|---|---|---|---|---|
| **[UGPhysics](https://huggingface.co/datasets/UGPhysics/ugphysics)** | **5,520** | **Undergraduate** | `problem`, **`solution` (full)**, `answers`, `answer_type`, `unit`, `subject`, `topic`, `level`, `language` | CC BY-NC-SA 4.0 |
| **[PHYBench](https://huggingface.co/datasets/Eureka-Lab/PHYBench)** | ~500–1,000 | HS → undergrad → Olympiad | `content`, `solution` (~3k chars, 10+ steps), `answer` (single symbolic expression), `tag` | MIT |
| **[PhysUniBench](https://huggingface.co/datasets/PrismaX/PhysUniBench)** | **3,304** | **Undergraduate** | Multimodal — every problem has a **diagram**; MCQ and open-ended splits, EN/ZH | CC BY 4.0 |

**UGPhysics is the single best fit.** Its 13 subjects are a physics degree: Classical Mechanics,
Classical Electromagnetism, Electrodynamics, Quantum Mechanics, Statistical Mechanics,
Thermodynamics, Theoretical Mechanics, Relativity, Atomic, Solid-State, Semiconductor,
Geometrical Optics, Wave Optics. It has a `level` field and an `answer_type` field, so steps 2
(domain + difficulty) and 4 (grading) are both largely pre-solved. Start here.

PhysUniBench matters for a different reason: the diagrams. A physics problem without a figure
is a restricted diet, and it is also the only dataset here that exercises the photo pipeline
from both ends.

### Tier A — maths, undergraduate and answer-based

| Source | Size | Level | Fields | Licence |
|---|---|---|---|---|
| [U-MATH](https://huggingface.co/datasets/toloka/u-math) | 1,100 | University | Problem, golden answer, `subject` (6 topics), 20% with diagrams | MIT |
| [Hendrycks MATH](https://huggingface.co/datasets/EleutherAI/hendrycks_math) | 12,500 | HS comp → undergrad | Problem, **full worked solution**, `subject` (7), **`level` 1–5** | MIT |
| [SciBench](https://github.com/mandyyyyii/scibench) | 695 | **College, from real textbooks** | Open-ended, numeric answers **with units**; physics + chemistry + maths | MIT |
| [JEEBench](https://github.com/dair-iitd/jeebench) | 515 | Hard pre-university | Physics/chem/maths, **integer, numeric and MCQ types** — built for auto-checking | MIT |
| [OlympiadBench](https://huggingface.co/datasets/Hothan/OlympiadBench) | 8,476 | Olympiad | **Maths and physics**, expert step-by-step, multimodal | see repo |

U-MATH's six topics are Precalculus, Algebra, Differential Calculus, Integral Calculus,
Multivariable Calculus, Sequences & Series — i.e. the maths a physicist actually uses.
Hendrycks MATH is the best **difficulty calibration set** you will find: 5 human-assigned
levels, so you can fit your step-2 scorer against it rather than inventing a scale.

### Tier B — scrape, clean licence, effectively unlimited

- **[OpenStax University Physics Vols 1–3](https://openstax.org/details/books/university-physics-volume-1)** — CC BY-NC-SA 4.0. **Answer keys for all odd-numbered** Problems, Additional Problems and Challenge Problems, e.g. [Vol 1 Ch 1 answers](https://openstax.org/books/university-physics-volume-1/pages/chapter-1), [Ch 16 challenge problems](https://openstax.org/books/university-physics-volume-1/pages/16-challenge-problems). Clean HTML, predictable URLs. Chapter number gives you a free domain label and a rough difficulty ordering (Problems < Additional < Challenge).
- **[OpenStax Calculus Vols 1–3](https://openstax.org/books/calculus-volume-1/pages/chapter-1)** — same deal for the maths side.
- **[Physics LibreTexts](https://phys.libretexts.org/)** — CC BY-NC-SA, hosts the OpenStax books plus [answer-key appendices](https://phys.libretexts.org/Bookshelves/University_Physics/University_Physics_(OpenStax)/Book%3A_University_Physics_I_-_Mechanics_Sound_Oscillations_and_Waves_(OpenStax)/18%3A_Answer_Key_to_Selected_Problems) and much else.
- **MIT OCW physics** — CC BY-NC-SA, problem sets **with full solution PDFs**: [8.01SC Classical Mechanics](https://ocw.mit.edu/courses/8-01sc-classical-mechanics-fall-2016/pages/assignments/), [8.04 Quantum Physics I](https://ocw.mit.edu/courses/8-04-quantum-physics-i-spring-2013/pages/assignments/) (10 sets, each with solutions), [8.02X E&M](https://ocw.mit.edu/courses/8-02x-physics-ii-electricity-magnetism-with-an-experimental-focus-spring-2005/), [RES.8-009 Oscillations and Waves](https://ocw.mit.edu/courses/res-8-009-introduction-to-oscillations-and-waves-summer-2017/pages/problem-sets/). Cost: PDF → LaTeX extraction.
- **[Physics Stack Exchange](https://physics.stackexchange.com)** via the [Sept 2025 community dump](https://archive.org/details/stackexchange_20250930) — CC BY-SA 4.0, attribution requires keeping the post URL, which you wanted anyway. Caveat: heavily conceptual, a minority are answer-checkable homework problems.

### Tier C — personal use, don't redistribute

- **Physics GRE released tests** — GR8677, GR9277, GR9677, GR0177, GR0877, GR1177. 100 multiple-choice questions each, pitched at exactly "an undergraduate physics degree, all of it", and **MCQ is trivially auto-gradable**. Worked solutions at [grephysics.net](http://grephysics.net). Official ETS booklets: [practice-book-physics.pdf](https://www.ets.org/content/dam/ets-india/pdfs/gre/practice-book-physics.pdf), [GR0177 mirror](http://sites.apam.columbia.edu/courses/apph4903x/exam_GR0177.pdf). ETS copyright — read, don't republish. **Use these first as a self-assessment** to find out how rusty you actually are before the app picks a difficulty.
- **[Isaac Physics](https://isaacphysics.org)** (University of Cambridge) — free, university-level, **already does online answer checking with a graduated hint structure**. Do not scrape it. Read it as the reference design for step 3: their hint scaffolding is the thing you are trying to build.
- **[IPhO](https://www.ipho-new.org/documentations/)** / [phoXiv mirror](https://phoxiv.org/olympiads/ipho) / [ipho.olimpicos.net](https://ipho.olimpicos.net/) — 1967→present with official solutions. Harder than undergrad coursework but answer-based; keep as a stretch pool.
- **[Putnam Archive](https://kskedlaya.org/putnam-archive/)** — maths, TeX sources 1985–2025. Problems © MAA; maintainers ask you to link rather than reproduce solutions. Link-only stretch problems.

### Dropped from the earlier (graduate) shortlist

Omni-MATH, ProofNet, MathOverflow/Nemotron, and the PhD qualifying-exam archives were the right
answer for graduate maths. They are the wrong answer here: all proof-heavy, and a proof cannot
be auto-graded against a key. Keep ProofNet and the qual archives on file for much later.

---

### A ramp that matches "rusty"

1. **Calibrate** — one Physics GRE released test, cold. 100 MCQs, scored, no app needed.
2. **Rebuild** — OpenStax University Physics odd-numbered problems + SciBench. Textbook-level, numeric answers with units, forgiving.
3. **Main loop** — UGPhysics, selected by `subject` and `level`, with PhysUniBench mixed in for diagram problems.
4. **Stretch** — PHYBench, OlympiadBench physics, IPhO.
5. **Maths on the side** — U-MATH and Hendrycks MATH level 3–5 to keep the calculus and series machinery warm.

### Suggested record schema

```
id, problem_text (LaTeX), solution_text, final_answer, answer_type,
unit|null, is_symbolic, domain, subdomain, difficulty,
source_name, source_url, source_license, author|null, retrieved_at, images[]
```

`answer_type` is lifted from UGPhysics (NV/MC/TF/EX/EQ/IN/KR) and drives which checker runs
in step 4. Keep `source_url` on every record — CC BY-SA sources legally require it.
