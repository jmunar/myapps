# Idea 04: Challenges — a data-science rapid-fire deck

> **Status: implemented (integration B), 21 October 2026.** `Dataset::DataScience`
> (`data-science`); `myapps-challenges-prep bundle data-science` reads the
> deck from `datasets/data-science.jsonl` and checks every item; bundle
> format 4 carries a source per problem, which the page links under the
> problem; a text answer (`answer_type = 'text'`) is shown as the first line
> of the solution, in plain text, with the bullets under it. All 122 starter
> items were reviewed and 38 corrected (see the deck's README); a 40-item
> difficulty-3 extension on 25 October 2026 brought the deck to 162 items.
> Still open: a human domain review (including the added items), the pilot,
> and the quiz UI (C).

## Summary

Add a third Challenges dataset, key `data-science`: short recall questions
with a one-line answer and two to four bullets, meant to be read, answered out
loud, revealed and self-marked in about 15–25 seconds. The starter deck is in
[`crates/myapps-challenges-prep/datasets/data-science.jsonl`](../crates/myapps-challenges-prep/datasets/data-science.jsonl):
**122 items** across four tracks — core ML, metrics, statistics and LLM basics.

The deck is hand-authored and source-checked rather than imported. The public
data-science benchmarks that exist are either code and analysis tasks, or
multiple-choice exams that test recognition and carry a measurable error rate;
none of them gives a short answer plus bullets, and several of the quiz banks
have no licence at all. Existing sets are useful as coverage checklists, not as
a source to ship.

The MVP needs no new UI: the existing *show solution* → *right* / *wrong* loop
is exactly the rapid-fire loop. It does need a local source in the prep tool and
a new `Dataset` variant. The full version also needs two small rendering/bundle
changes for prose answers and per-item sources, described under
[Integration](#integration-with-the-current-app).

## Why rapid-fire, and what it changes about the items

Retrieval practice beats rereading on delayed tests: in Roediger and Karpicke's
experiments, repeated testing produced better retention a week later than
repeated study, even though restudy felt more effective and did better on an
immediate test (Roediger & Karpicke, 2006). The app already supplies the two
ingredients that make a quiz item work — a retrieval attempt and immediate
feedback — so the deck should optimise for *many short attempts*, not for a few
long problems.

That gives concrete item rules:

- **One fact or one decision per item.** If the answer needs two independent
  facts, split it. The prompt is one sentence, at most about 20 words.
- **The answer is sayable.** One sentence, at most about 20 words. The bullets
  explain the answer; they do not restate it.
- **Bullets add mechanism, contrast or a rule of thumb.** Two to four bullets,
  each short enough to scan after the reveal. This is the "why" that stops a
  wrong answer from being just a memorised label.
- **No multi-step work.** Numeric items use one operation and show the
  arithmetic in the bullets. The existing maths datasets are for derivations;
  this deck is for recall and judgement.
- **No "which is NOT", "all of the above", or double negatives.** Self-marking
  hides some ambiguity, but a question with two defensible answers is still a
  bad item.
- **Difficulty is the kind of thinking, not topic obscurity.** 1 = define or
  recognise; 2 = compare, apply or read a metric; 3 = a trap or a judgement
  call (p-value interpretation, leakage, rare-positive metrics).

## What already exists online

Checked against the sources themselves in October 2026.

| Source | What it is | Size and shape | Licence | Fit |
|---|---|---|---|---|
| **MMLU** (Hendrycks et al., 2021) | 57-subject exam questions | 14,042 test items, 4 options; ML 112, high-school statistics 216, college CS 100 | MIT | Recognition, not recall; no answer bullets; known errors |
| **MMLU-Pro** (Wang et al., 2024) | Harder MMLU with 10 options | 12,032 test items; computer science 410; no statistics track | MIT | Deeper reasoning and longer; wrong tempo |
| **DataSciBench** (Zhang et al., 2025) | LLM agent benchmark for data analysis | 519 test cases, six task types, code execution | CC BY 4.0 data; no code licence declared | Tests doing data science, not explaining it |
| **StatQA** (Zhu et al., 2024) | Statistical-analysis tasks over tables | 11,623 examples (1,163 mini) | GPL-3.0 per StatABench | Tests choosing and applying a test; not rapid-fire definitions |
| **Hugging Face quiz sets** | Course quizzes and scraped interview banks | 10 to ~6k rows | Often none, or an empty card | Small, unlicensed, book-specific or unchecked |
| **GitHub interview lists** | Markdown Q&A collections | hundreds to thousands | Often no `LICENSE` | Useful as a topic checklist; answers are long, mixed or placeholders |

### MMLU is a coverage map, not a source

MMLU is the closest thing to a ready-made concept bank, but two problems rule
it out as an import:

1. **Format.** Four-option multiple choice tests recognition. A rapid-fire deck
   wants free recall, because producing the answer is the point; the reveal is
   the feedback, not a distractor set.
2. **Quality.** MMLU-Redux manually re-reviewed 100 items per subject and
   estimated a 6.49% error rate overall. In the machine-learning subset, 11 of
   100 reviewed items had an error (clarity, options, multiple correct answers
   or wrong ground truth); high-school statistics had 2 and college computer
   science had 3 (Gema et al., 2024). A deck of 25 ML items cannot absorb that
   rate silently.

Its subject list is still a good checklist: classification, evaluation,
overfitting, bias/variance, regularisation, cross-validation, leakage,
precision/recall, ROC, probability, distributions and hypothesis testing all
appear there and in the deck.

### The quiz banks are worse than they look

The Hugging Face and GitHub quiz collections are tempting because they already
have questions and answers. In practice:

- `burtenshaw/exam_questions` is 10 LLM questions with an empty dataset card.
- `mjphayes/machine_learning_questions` has 636 rows but no card or licence,
  and many items are trivia about one book (*Approaching (Almost) Any Machine
  Learning Problem*), not general concepts.
- `K-areem/AI-Interview-Questions` has 5.8k rows, an empty card, and is mostly
  software-engineering interview material.
- `rradfar/in-quiz-questions` has ~70 ML multiple-choice items but no licence;
  the file itself contains duplicate numbering and a reviewer note that one
  answer is "somewhat vague and unsettled".
- `youssefHosni/Data-Science-Interview-Questions-Answers` is the most popular
  list (5.9k stars, 1.3k forks) but also has no `LICENSE`, and many answers are
  truncated or missing; see the closer look below.

### A closer look at `youssefHosni/Data-Science-Interview-Questions-Answers`

<https://github.com/youssefHosni/Data-Science-Interview-Questions-Answers>

This is the one most likely to look like a ready-made deck: 36 machine-learning,
17 statistics, 17 probability and 44 deep-learning questions, plus Python and
SQL files. It is a useful **topic checklist**, but not a source to import.

- **No licence.** There is no `LICENSE` file on either branch; the README's
  licence badge points at `master/LICENSE`, which 404s. Copying its text into a
  redistributed deck would be a copyright problem.
- **It is not maintained as a dataset.** The last push was September 2024.
  Headings vary (`### Q1`, `## Q23`, `##  Q35`, Unicode-bold `Q17`), there are
  no stable IDs, no difficulty and no schema.
- **Many answers are missing or are links.** 8 of the 36 ML answers are
  truncated to a link, most with "The rest of the answer is here", pointing at
  365datascience (Q1–Q6, Q11, Q12). 14 of the 44 deep-learning questions have an
  empty `Answer:` section (Q25, Q29, Q31, Q32, Q34–Q42, Q44); statistics Q14 is
  empty. Probability Q12 and statistics Q12 are images of an answer, not text.
- **Some answers are wrong or misleading.** Statistics Q3 explains the p-value
  as the probability of the result under the existing approach, which is the
  usual misinterpretation. Q8 uses the one-sided p-value 10/1024 for a two-sided
  "biased" alternative; two-sided would be 20/1024. Deep-learning Q26 says
  transformers keep "constant computational complexity" as sequences grow;
  self-attention is O(n²). ML Q7's "logistic regression isn't a classification
  model" is a semantic stance that would confuse a rapid-fire answer.
- **It is interview-shaped, not rapid-fire.** Questions are long and
  multi-part ("Explain … and discuss …"), answers run to hundreds of words, and
  the probability file is mostly puzzle practice rather than basic recall.

What it is good for is **coverage**. Some of its statistics answers — A/B
testing pitfalls, selection bias, multiple testing — are genuinely useful; the
problem is inconsistency, not that every item is bad. Its ML list is broad, and
its statistics list also covers long-tailed distributions, skewness, CLT
conditions, blended means and standard deviations, and confidence versus
prediction intervals — useful checks against our deck. The deck now includes
the gaps that list suggested: loss vs cost, active learning, data vs concept
drift, multi-label metrics, ARIMA, pooled means and standard deviations,
confidence vs prediction intervals, and skewness. The probability puzzles could
inspire a few difficulty-3 numeric items, but they are not the 15–25 second
format.

The lesson is not that these lists are useless; it is that **coverage can be
borrowed, wording and correctness cannot**. The deck uses them to find gaps,
then writes and checks each item against a primary source.

## Source stack and licensing

| Source | Used for | Licence |
|---|---|---|
| [Google Machine Learning Crash Course](https://developers.google.com/machine-learning/crash-course) | ML framing, splits, overfitting, regularisation, classification metrics, embeddings, LLM intro | CC BY 4.0 (code samples Apache 2.0) |
| [scikit-learn user guide](https://scikit-learn.org/stable/) | metrics, cross-validation, leakage, algorithms, calibration, PCA, clustering | BSD-3-Clause |
| [OpenIntro Statistics](https://openintro.org/book/os/) | descriptive statistics, distributions, sampling, inference, regression, causality | CC BY-SA 3.0 |
| [ASA statement on p-values](https://www.amstat.org/asa/files/pdfs/P-ValueStatement.pdf) | p-value interpretation, significance and effect size | Freely available; cite, do not copy |
| [NIST/SEMATECH e-Handbook](https://www.itl.nist.gov/div898/handbook/) | statistical methods and definitions (backup for OpenIntro) | U.S. Government work |
| [Hugging Face LLM Course](https://huggingface.co/learn/llm-course) | tokens, Transformers, pretraining, fine-tuning | Apache 2.0 |
| [Attention Is All You Need](https://arxiv.org/abs/1706.03762), [BERT](https://arxiv.org/abs/1810.04805), [GPT-3](https://arxiv.org/abs/2005.14165) | architecture and pretraining objectives | arXiv author licences; cite |
| [InstructGPT](https://arxiv.org/abs/2203.02155), [DPO](https://arxiv.org/abs/2305.18290) | instruction tuning, RLHF, preference optimisation | arXiv author licences; cite |
| [LoRA](https://arxiv.org/abs/2106.09685), [QLoRA](https://arxiv.org/abs/2305.14314) | parameter-efficient fine-tuning, quantisation | arXiv author licences; cite |
| [RAG](https://arxiv.org/abs/2005.11401) | retrieval-augmented generation | arXiv author licences; cite |
| [Lost in the Middle](https://arxiv.org/abs/2307.03172) | long-context recall | arXiv author licences; cite |
| [OpenAI chat API docs](https://platform.openai.com/docs/api-reference/chat/create) | temperature, top-p, seed behaviour | Provider docs; cite |
| [OWASP LLM01](https://genai.owasp.org/llmrisk/llm01-prompt-injection/) | prompt injection | CC BY-SA 4.0 |

**Licence recommendation: CC BY-SA 4.0 for the deck.** OpenIntro is CC BY-SA
3.0, and a few other references are share-alike. The items are written in
original words and cite facts, but keeping the deck under the same family of
licence avoids arguing about which sentence is a derivative. This is a data
file with its own licence; the repository's MIT licence does not need to change.
If the deck must be MIT or CC BY 4.0, replace the statistics items' OpenIntro
provenance with NIST/public-domain sources and re-check the wording.

## Coverage of the starter deck

The original starter deck had 122 items, 38 at difficulty 1, 67 at 2 and 17
at 3. A 40-item difficulty-3 extension on 25 October 2026 brought it to 162
items (38 / 67 / 57); the current per-track breakdown is in the deck's README.
The tables below describe the original 122 items.

| Track | Items | What it covers |
|---|---|---|
| `ml-fundamentals` | 29 | supervised/unsupervised framing, train/validation/test, overfitting and underfitting, bias–variance, L1/L2, cross-validation, leakage, parameters vs hyperparameters, logistic regression, scaling, kNN, bagging/boosting, random forests, SVM kernels, Naive Bayes, PCA, k-means, diagnosis traps, baselines, loss vs cost, active learning, data vs concept drift |
| `metrics` | 27 | confusion matrix, accuracy, precision, recall, specificity, F1, false positives/negatives, threshold trade-offs, imbalance, ROC-AUC vs PR-AUC, average precision, F-beta, calibration, Brier score, macro/micro averaging, MAE/RMSE, R², imbalance handling, multi-label metrics, arithmetic traps |
| `statistics` | 32 | mean/median, SD, IQR, normal rule, CLT, LLN, p-values, Type I/II errors, power, confidence intervals, standard error, t-test, ANOVA, chi-square, Pearson/Spearman, confounding, Simpson's paradox, Bayes, base rates, multiple comparisons, peeking, correlation vs causation, Poisson, survivorship bias, regression to the mean, ARIMA, pooled SD, confidence vs prediction intervals, skewness |
| `llm` | 34 | tokens, tokenisation, embeddings, Transformer, self-attention, positional information, encoder/decoder architectures, causal and masked LM, pretraining, zero/few-shot prompting, chain-of-thought, system prompts, prompting vs fine-tuning, instruction tuning, RLHF, DPO, temperature, top-k/top-p, logits, beam search, context window, KV cache, perplexity, hallucination, RAG vs fine-tuning, LoRA, quantisation, production failures, benchmark contamination, LLM-as-judge bias, prompt injection |

Each track has 14–17 topics, with two or more items on the topics that carry
the most weight in the brief: overfitting and evaluation metrics. The deck is
deliberately a starter, not a curriculum; 200–300 items would be a comfortable
long-term size, with the additions coming from user misses rather than from
scraping more sources.

## Item schema and mapping to a bundle

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

Mapping to `bundle::Problem`:

| Bundle field | Value |
|---|---|
| `source_key` | `id` |
| `subject` | track display name (`ML fundamentals`, `Metrics`, `Statistics`, `LLM`) |
| `topic` | `topic` |
| `difficulty` | `difficulty` (1–3) |
| `source_level` | `Recall` / `Apply` / `Judge` |
| `problem` | `question` |
| `solution` | bullets joined with newlines, each prefixed with `• ` |
| `answer` | one-line `answer` |
| `answer_type` | `text` (new value) |
| `unit` | empty |

The existing `.challenges-text` has `white-space: pre-wrap`, so the bullets
already render as separate lines. Two things do not fit:

1. **The answer is rendered as maths.** `answer_math()` wraps every answer in
   `\(…\)` unless it already has delimiters. A prose answer such as "TP/(TP+FP)"
   would be typeset as a formula and lose its spaces. `answer_type` is stored
   but not read anywhere, so a text answer needs either an empty `answer` (put
   the sentence first in `solution`) or a small page change: if
   `answer_type == "text"`, use the plain-text renderer instead of
   `answer_math`.
2. **The source is per dataset, not per item.** `bundle::Problem` has no source
   field; `import.rs` copies `manifest.source_url` and `manifest.license` onto
   every row. The deck's most useful property is that each item cites its own
   reference. Options: (a) accept one dataset-level URL, (b) put the source in
   `topic` or `unit` (visible but ugly), or (c) add `source_url` and
   `source_title` to the problem row in a bundle format 3. Option (c) is the
   clean one, and the format bump is cheap because this dataset is new.

## Integration with the current app

Three levels of change, in increasing order of effort:

**A. Minimum: dataset-level source, prose answer in `solution`.**
Add `Dataset::DataScience` to `dataset.rs` (`key`, `name`, `max_level = 3`,
`level_name`, `url`, `license`); teach `myapps-challenges-prep` to read the
JSONL and write a bundle; leave `answer` empty and put the one-line answer as
the first line of `solution`. No bundle format change, no page change. The
source link points at the deck's README or the repo, not the individual item.

**B. Recommended: text answers and per-item sources.**
Do A, plus: make `answer_math` respect `answer_type = "text"`; bump the bundle
to format 3 with `source_url`/`source_title` on `problems`; render the item's
source under the answer, falling back to the dataset URL for existing bundles.
`Bundle::Reader` already has the `OLDEST_READABLE` mechanism, so old bundles
still load. This is the version that makes the deck feel like a reference
deck rather than an anonymous quiz.

Code touch points, whichever option is chosen:

- `dataset.rs`: new `Dataset` variant; `ALL` grows to three; `url()` points at
  the deck README rather than a Hugging Face id; `level_name()` names the three
  levels; the round-trip test grows.
- `myapps-challenges-prep/src/source.rs`: `sources()` is exhaustive over
  `Dataset`, so a local variant needs its own branch. Do not route it through
  the HF row mapper.
- `myapps-challenges-prep/src/main.rs`: `fetch_all` needs a local path that
  reads the JSONL and maps rows, skipping the HTTP cache and diagram renderer.
- `pages.rs`: `answer_math` and the source line are the only rendering changes;
  the reveal and self-marking forms stay as they are.
- `bundle.rs` and `services/import.rs`: only needed for option B (format 3,
  per-item source columns).

**C. Later: quiz-specific UI.**
A timer, a streak, keyboard shortcuts (space to reveal, arrow keys to mark), and
a "missed this" list. The current page already does the important part; these
are engagement features, not blockers. Do not add them before the content is
reviewed.

Do **not** add the deck as a second import path on `serve`: the repo already
decided that preparation is offline and the server only loads bundles. A local
JSONL source fits that decision — the prep tool reads it, the server never sees
it.

## Quality process

1. **Source first.** Every item starts from a primary source, not from memory
   or from another quiz. Write the source URL in the item before writing the
   answer.
2. **Write the answer, then the bullets.** The answer should be usable as a
   flashcard; the bullets should answer "why is that true?" or "when does it
   fail?".
3. **Recompute every number.** The deck has numeric items (TP/FP/FN arithmetic,
   F1, prevalence, alpha/m). A two-line Python check is enough; do not trust
   mental arithmetic.
4. **Adversarial read.** Hide the answer and try to answer from the question
   alone. If two answers are defensible, rewrite the question or add the
   missing assumption to it. If the answer depends on a convention (for
   example, "precision improves as the threshold rises"), say "usually".
5. **Deduplicate.** Normalise case and punctuation and compare questions; also
   compare `(track, topic, tags)` so two items do not teach the same fact with
   different wording.
6. **Domain review.** A second person who teaches the topic reviews a sample
   (at least 10%, and every difficulty-3 item). The reviewer's job is to reject
   ambiguous items, not to rewrite style.
7. **Pilot.** Run 20 items with 3–5 people. Target a median of 15–25 seconds
   per item and under 5% "I was unsure what was being asked". Track misses by
   topic; a topic with a high miss rate may need a second item, not a harder
   one.
8. **Keep item statistics.** Per-item accuracy, skip rate and time are more
   useful than per-subject accuracy for deciding what to rewrite. Do not use
   them to hide items from the user; use them to retire bad items.

## Risks and open questions

- **Ambiguity is the main failure mode.** Short answers compress nuance. The
  bullets and the "usually" hedges are the mitigation; domain review is the
  check.
- **Self-marking is noisy.** The existing datasets have the same limitation.
  Item-level miss rates will be biased by over- and under-confidence, so use
  them for curation, not for grading the user.
- **LLM facts age.** The deck avoids current product names, versions and
  leaderboard positions. The items that remain (context windows, RAG, LoRA,
  sampling) are stable concepts; re-check the quantisation and long-context
  items when the deck is revised.
- **Licensing is a choice, not a detail.** CC BY-SA 4.0 keeps the statistics
  sources simple; a permissive-only deck would need the OpenIntro-derived items
  rewritten from public-domain sources.
- **Coverage vs depth.** 162 items is enough for a demo and a first release,
  not for "data science" as a whole. SQL, pandas, visualisation and deeper
  experiment-design methods are deliberately out of scope for this deck; they
  would be their own tracks or a second dataset.
- **Do not import MMLU to fill gaps.** The error rate and the recognition
  format would undo the quality work. Use it to find missing topics, then write
  the item.

## Next steps

1. ~~Review the 122 items~~ — done by a model review (21 October 2026); a
   second person who teaches the topics should still review a sample.
2. ~~Add `Dataset::DataScience` and a local JSONL reader~~ — done.
3. ~~Decide between integration A and B~~ — B, as bundle format 4 (format 3
   had already gone to the grader's tags). The answer opens the solution
   rather than sitting above it under its own heading.
4. Prepare `data-science.sqlite` and load it with
   `myapps import --app challenges --dataset data-science.sqlite`, after
   deploying a build that reads format 4.
5. Pilot the deck, then expand toward 200 items from observed misses.
6. Add per-topic accuracy and median time to the stats page once there is
   enough data to make them meaningful.

## Sources

Learning science:

- Roediger, H. L., & Karpicke, J. D. (2006). Test-enhanced learning: Taking
  memory tests improves long-term retention. *Psychological Science*, 17(3),
  249–255. <https://doi.org/10.1111/j.1467-9280.2006.01693.x>
- Dunlosky, J., Rawson, K. A., Marsh, E. J., Nathan, M. J., & Willingham, D. T.
  (2013). Improving students' learning with effective learning techniques.
  *Psychological Science in the Public Interest*, 14(1), 4–58.

Datasets and benchmarks:

- Hendrycks, D., Burns, C., Basart, S., Zou, A., Mazeika, M., Song, D., &
  Steinhardt, J. (2021). Measuring Massive Multitask Language Understanding.
  *ICLR*. <https://arxiv.org/abs/2009.03300> · dataset
  <https://huggingface.co/datasets/cais/mmlu>
- Gema, A. P., et al. (2024). Are We Done with MMLU? *NAACL 2025*.
  <https://arxiv.org/abs/2406.04127> · MMLU-Redux
  <https://huggingface.co/datasets/edinburgh-dawg/mmlu-redux>
- Wang, Y., et al. (2024). MMLU-Pro: A More Robust and Challenging
  Multi-Task Language Understanding Benchmark. *NeurIPS Datasets and
  Benchmarks*. <https://arxiv.org/abs/2406.01574> · dataset
  <https://huggingface.co/datasets/TIGER-Lab/MMLU-Pro>
- Zhang, D., et al. (2025). DataSciBench: An LLM Agent Benchmark for Data
  Science. <https://arxiv.org/abs/2502.13897> ·
  <https://datascibench.github.io/>
- Zhu, Y., et al. (2024). Are Large Language Models Good Statisticians?
  *NeurIPS Datasets and Benchmarks*. <https://arxiv.org/abs/2406.07815> ·
  <https://github.com/HKUSTDial/StatQA>
- Hosni, Y. *Data-Science-Interview-Questions-Answers*. GitHub.
  <https://github.com/youssefHosni/Data-Science-Interview-Questions-Answers>
  (no licence; last push September 2024; checked 20 October 2026).
- Workshop quiz banks checked as coverage sources:
  <https://huggingface.co/datasets/burtenshaw/exam_questions>,
  <https://huggingface.co/datasets/mjphayes/machine_learning_questions>,
  <https://huggingface.co/datasets/K-areem/AI-Interview-Questions>,
  <https://github.com/rradfar/in-quiz-questions>.

Content sources:

- Google for Developers. Machine Learning Crash Course.
  <https://developers.google.com/machine-learning/crash-course> (CC BY 4.0).
  Loss: <https://developers.google.com/machine-learning/crash-course/linear-regression/loss>.
- scikit-learn developers. User Guide.
  <https://scikit-learn.org/stable/user_guide.html> (BSD-3-Clause).
- OpenIntro. *OpenIntro Statistics*, 4th ed.
  <https://openintro.org/book/os/> (CC BY-SA 3.0).
- Ren, P., et al. (2021). A Survey of Deep Active Learning.
  <https://arxiv.org/abs/2009.00236> (CC BY 4.0).
- Lesort, T., Caccia, M., & Rish, I. (2021). Understanding Continual Learning
  Settings with Data Distribution Drift Analysis.
  <https://arxiv.org/abs/2104.01678> (CC BY 4.0).
- Hyndman, R. J., & Athanasopoulos, G. *Forecasting: Principles and Practice*,
  3rd ed., ch. 9 (ARIMA models). <https://otexts.com/fpp3/arima.html>
- Wikipedia. Pooled variance.
  <https://en.wikipedia.org/wiki/Pooled_variance> (CC BY-SA 4.0).
- American Statistical Association. The ASA Statement on p-Values: Context,
  Process, and Purpose (2016).
  <https://www.amstat.org/asa/files/pdfs/P-ValueStatement.pdf>
- NIST/SEMATECH. *e-Handbook of Statistical Methods*.
  <https://www.itl.nist.gov/div898/handbook/>
- Hugging Face. *LLM Course*. <https://huggingface.co/learn/llm-course>
  (Apache 2.0). Perplexity of fixed-length models:
  <https://huggingface.co/docs/transformers/en/perplexity>.
- Vaswani, A., et al. (2017). Attention Is All You Need.
  <https://arxiv.org/abs/1706.03762>
- Devlin, J., Chang, M.-W., Lee, K., & Toutanova, K. (2019). BERT:
  Pre-training of Deep Bidirectional Transformers for Language Understanding.
  <https://arxiv.org/abs/1810.04805>
- Brown, T., et al. (2020). Language Models are Few-Shot Learners.
  <https://arxiv.org/abs/2005.14165>
- Wei, J., et al. (2022). Chain-of-Thought Prompting Elicits Reasoning in
  Large Language Models. <https://arxiv.org/abs/2201.11903>
- Ouyang, L., et al. (2022). Training language models to follow instructions
  with human feedback. <https://arxiv.org/abs/2203.02155>
- Rafailov, R., et al. (2023). Direct Preference Optimization: Your Language
  Model is Secretly a Reward Model. <https://arxiv.org/abs/2305.18290>
- Hu, E. J., et al. (2021). LoRA: Low-Rank Adaptation of Large Language
  Models. <https://arxiv.org/abs/2106.09685>
- Dettmers, T., Pagnoni, A., Holtzman, A., & Zettlemoyer, L. (2023). QLoRA:
  Efficient Finetuning of Quantized LLMs. <https://arxiv.org/abs/2305.14314>
- Lewis, P., et al. (2020). Retrieval-Augmented Generation for
  Knowledge-Intensive NLP Tasks. <https://arxiv.org/abs/2005.11401>
- Liu, N. F., et al. (2023). Lost in the Middle: How Language Models Use Long
  Contexts. <https://arxiv.org/abs/2307.03172>
- Zheng, L., et al. (2023). Judging LLM-as-a-Judge with MT-Bench and Chatbot
  Arena. <https://arxiv.org/abs/2306.05685>
- OpenAI. Chat Completions API reference.
  <https://platform.openai.com/docs/api-reference/chat/create>
- OWASP. LLM01: Prompt Injection.
  <https://genai.owasp.org/llmrisk/llm01-prompt-injection/>

All URLs checked 20 October 2026.
