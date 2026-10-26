# Challenges datasets kept in the repository

Small, hand-authored decks that do not come from a Hugging Face dataset. They
are source data for `myapps-challenges-prep`, not bundles: the prep tool
turns one into the SQLite bundle the server loads.

```sh
cargo run --release -p myapps-challenges-prep -- bundle data-science   # → data-science.sqlite
myapps import --app challenges --dataset data-science.sqlite
```

Each track becomes a subject, `difficulty` the level (Recall, Apply, Judge),
`question` the problem, and the bullets the solution, one `• ` line each. The
answer is marked as prose, so the page shows it as the solution's first line
rather than typesetting it as maths, and the item's `source` is the link under
the problem. `tags` are not carried yet.

## `data-science.jsonl`

A starter deck for a `data-science` Challenges dataset: **162 rapid-fire
items**, each with a one-line answer, two to four answer bullets, a difficulty,
tags and a source. Sources were checked on 20 October 2026; the 40 harder
items added later were checked on 25 October 2026.

| Track | Items | Topics | Difficulty 1 / 2 / 3 |
|---|---|---|---|
| `ml-fundamentals` | 39 | framing, splits, overfitting, regularisation, CV, leakage, algorithms, ensembles, diagnosis, baselines, loss/cost, active learning, drift, model selection, imbalance, monitoring, feature engineering, reproducibility, optimisation | 7 / 19 / 13 |
| `metrics` | 37 | confusion matrix, accuracy, precision, recall, specificity, F1, ROC, PR, calibration, regression metrics, imbalance handling, multi-label metrics, ranking, uncertainty, fairness | 8 / 15 / 14 |
| `statistics` | 42 | descriptive statistics, distributions, sampling, hypothesis testing, power, CIs, tests, causality, probability, bias, ARIMA, pooled SD, CI vs prediction interval, skewness, effect size, missing data, regression diagnostics, bootstrap, Bayesian, nonparametric | 10 / 17 / 15 |
| `llm` | 44 | tokenisation, embeddings, transformer, attention, pretraining, fine-tuning, alignment, generation, prompting, RAG, evaluation, security, inference, context, quantisation, agents | 13 / 16 / 15 |
| **Total** | **162** | | **38 / 67 / 57** |

The extension is deliberately concentrated at difficulty 3 and adds new topics
to every track. It asks for a judgement or a design choice rather than a
definition, but every item is still meant to be answered out loud in under 90
seconds.

### Schema

One JSON object per line:

```json
{
  "id": "ml-001",
  "track": "ml-fundamentals",
  "topic": "problem framing",
  "difficulty": 1,
  "question": "What is supervised learning?",
  "answer": "Learning a mapping from inputs to labels using examples that include the target.",
  "bullets": [
    "Labelled training data is required.",
    "Classification predicts discrete labels; regression predicts continuous values.",
    "At prediction time the label is unknown."
  ],
  "tags": ["supervised learning", "problem framing"],
  "source": {
    "title": "scikit-learn: Supervised learning",
    "url": "https://scikit-learn.org/stable/supervised_learning.html"
  }
}
```

- `id` is stable and unique: `<track-prefix>-<number>`.
- `track` is one of `ml-fundamentals`, `metrics`, `statistics`, `llm`.
- `difficulty` is `1` recall, `2` compare/apply, `3` judgement or trap.
- `answer` is the short answer to say out loud; `bullets` explain it and must
  not just repeat it.
- `source` is the primary reference the item was checked against, not a copied
  passage. Its `url` must be `https://`.

The prep tool refuses the whole deck if an item breaks a rule it can check
(unknown track, difficulty outside 1–3, an empty field, other than 2–4
bullets, a duplicate id, a non-https source), and its tests read the committed
file, so `make check` catches a bad edit.

### Review

All 122 original items were reviewed on 21 October 2026 (a model review, item
by item, with every number recomputed and every source URL fetched). 38 items
changed:

- **Wrong or misleading, fixed:** logistic-regression coefficients are a
  change in log-odds per unit, not log-odds (ml-014); a random forest samples
  features at every split, not once per tree (ml-018); "MSE can describe
  either level" contradicted the loss/cost distinction (ml-027); average
  precision is not literally the area under the PR curve (met-015); DPO also
  drops the reward model, not only the RL loop (llm-014); SentencePiece is a
  library, not a third algorithm next to BPE and WordPiece (llm-002); attention
  is order-blind rather than "permutation-invariant" (llm-006).
- **Unhedged convention:** "precision improves with a stricter threshold" now
  says "usually" (met-003); the confusion-matrix axes are named as a library
  choice (met-001).
- **Bullets that restated the answer or another bullet**, replaced with a
  mechanism or contrast: ml-024, met-018, met-020, sta-002, sta-011, sta-018,
  llm-009, llm-015, llm-016, llm-033.
- **Duplicate fact:** met-011 taught the same 99%-accuracy trap as met-022; it
  is now balanced accuracy.
- **Source mismatch:** L1 was cited to Google's L2 page (ml-009), peeking to an
  OpenIntro chapter that does not cover it (sta-024), the KV cache to the
  generation-strategies page (llm-018), and llm-029's source title did not
  match its page.
- **Missing assumption:** the bias–variance split holds for squared error
  (ml-008); the CLT needs independence (sta-005); the combined-variance item
  now says how it differs from the t-test's pooled variance (sta-030);
  llm-024 asked about one benchmark question rather than a benchmark score.

The 40 items added on 25 October 2026 were written as a difficulty-3
extension and their source URLs were fetched, but they have not yet had the
same item-by-item review. A human domain review of a sample (every
difficulty-3 item and at least 10% of the rest) is still the next step.

### Licence

The deck is released under **CC BY-SA 4.0**. Sources are cited per item. Some
of them, notably OpenIntro Statistics, are CC BY-SA 3.0; sharing the deck under
CC BY-SA keeps attribution and share-alike simple. This is a data file with its
own licence: the repository's MIT licence does not apply to it.

Facts are not copyrightable, but wording is. Every item here is written in
original words and checked against the cited source; no question text is copied
from a quiz site or benchmark.
