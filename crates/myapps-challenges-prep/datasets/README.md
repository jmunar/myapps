# Challenges datasets kept in the repository

Small, hand-authored decks that do not come from a Hugging Face dataset. They
are source data for `myapps-challenges-prep`, not bundles: the prep tool is
meant to turn one into the SQLite bundle the server loads. **The local JSONL
reader is not implemented yet**; this file is the starter source of truth for
that work and still needs independent review.

## `data-science.jsonl`

A starter deck for a `data-science` Challenges dataset: **122 rapid-fire
items**, each with a one-line answer, two to four answer bullets, a difficulty,
tags and a source. Sources were checked on 20 October 2026.

| Track | Items | Topics | Difficulty 1 / 2 / 3 |
|---|---|---|---|
| `ml-fundamentals` | 29 | framing, splits, overfitting, regularisation, CV, leakage, algorithms, ensembles, diagnosis, baselines, loss/cost, active learning, drift | 7 / 19 / 3 |
| `metrics` | 27 | confusion matrix, accuracy, precision, recall, specificity, F1, ROC, PR, calibration, regression metrics, imbalance handling, multi-label metrics | 8 / 15 / 4 |
| `statistics` | 32 | descriptive statistics, distributions, sampling, hypothesis testing, power, CIs, tests, causality, probability, bias, ARIMA, pooled SD, CI vs prediction interval, skewness | 10 / 17 / 5 |
| `llm` | 34 | tokenisation, embeddings, transformer, attention, pretraining, fine-tuning, alignment, generation, prompting, RAG, evaluation, security | 13 / 16 / 5 |
| **Total** | **122** | | **38 / 67 / 17** |

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
  passage.

### Validation

```sh
python3 - <<'PY'
import json
rows = [json.loads(line) for line in open("data-science.jsonl")]
assert len({r["id"] for r in rows}) == len(rows)
for r in rows:
    assert r["track"] in {"ml-fundamentals", "metrics", "statistics", "llm"}
    assert r["difficulty"] in (1, 2, 3)
    assert r["question"].strip() and r["answer"].strip()
    assert 2 <= len(r["bullets"]) <= 4
    assert r["source"]["url"].startswith("https://")
print(len(rows), "items")
PY
```

### Licence

The deck is released under **CC BY-SA 4.0**. Sources are cited per item. Some
of them, notably OpenIntro Statistics, are CC BY-SA 3.0; sharing the deck under
CC BY-SA keeps attribution and share-alike simple. This is a data file with its
own licence: the repository's MIT licence does not apply to it.

Facts are not copyrightable, but wording is. Every item here is written in
original words and checked against the cited source; no question text is copied
from a quiz site or benchmark.
