//! The concrete questions asked of every problem. A shared core (the spike's
//! seven, worded as measured), with the technique ladder per dataset. Changing
//! a question's wording or options changes its answers, so bump `VERSION`:
//! answers are cached and recorded under it.

use myapps_challenges::dataset::Dataset;

use super::llm::LETTERS;

pub const VERSION: &str = "r1";

pub struct Question {
    /// The tag kind is `rubric.<kind>`.
    pub kind: &'static str,
    pub text: &'static str,
    pub options: &'static [&'static str],
    /// An ordered scale, read as the expected option index (0-based). The
    /// others are yes/no, read as P(yes).
    pub scale: bool,
}

const YES_NO: &[&str] = &["yes", "no"];

pub fn questions(dataset: Dataset) -> [Question; 7] {
    let technique: &'static [&'static str] = match dataset {
        Dataset::HendrycksMath => &[
            "only arithmetic",
            "routine school algebra or one standard formula",
            "a standard competition technique such as modular arithmetic, casework counting or Vieta's formulas",
            "a clever non-standard idea or an advanced theorem",
        ],
        // UGPhysics' own skill ladder, which its levels follow.
        Dataset::Ugphysics => &[
            "recalling a definition or a single fact",
            "applying one physical law or formula directly",
            "combining several laws or principles",
            "a derivation that needs advanced mathematics such as calculus, differential equations or linear algebra",
        ],
    };
    [
        Question {
            kind: "steps",
            text: "How many reasoning steps does the solution to this problem take?",
            options: &[
                "a single step",
                "two or three steps",
                "four to six steps",
                "more than six steps",
            ],
            scale: true,
        },
        Question {
            kind: "technique",
            text: "What is the most advanced technique the solution needs?",
            options: technique,
            scale: true,
        },
        Question {
            kind: "computation",
            text: "How much computation does the solution involve?",
            options: &["almost none", "light", "heavy"],
            scale: true,
        },
        Question {
            kind: "insight",
            text: "Does solving this need a non-obvious insight or trick rather than a routine procedure?",
            options: YES_NO,
            scale: false,
        },
        Question {
            kind: "casework",
            text: "Does the solution split into several cases?",
            options: YES_NO,
            scale: false,
        },
        Question {
            kind: "direct",
            text: "Can the answer be found by plugging numbers into a single well-known formula?",
            options: YES_NO,
            scale: false,
        },
        Question {
            kind: "constraints",
            text: "Does the problem combine several conditions that must all hold at once?",
            options: YES_NO,
            scale: false,
        },
    ]
}

/// What the problems are, in the system prompts.
pub fn field(dataset: Dataset) -> &'static str {
    match dataset {
        Dataset::HendrycksMath => "maths",
        Dataset::Ugphysics => "physics",
    }
}

pub fn system(dataset: Dataset) -> String {
    format!(
        "You analyse {} problems. Answer with a single letter.",
        field(dataset)
    )
}

/// The problem first and the question after it: the server's prompt cache
/// then processes the problem once for all seven. Putting the questions first
/// (cached once for every problem) was faster but broke the yes/no answers.
pub fn prompt(problem: &str, q: &Question) -> String {
    let menu: Vec<String> = q
        .options
        .iter()
        .zip(LETTERS)
        .map(|(o, l)| format!("{l}. {o}"))
        .collect();
    format!(
        "{problem}\n\nQuestion: {}\n{}\n\nAnswer with the letter only.",
        q.text,
        menu.join("\n")
    )
}

pub fn value(q: &Question, p: &[f64]) -> f64 {
    if q.scale {
        p.iter().enumerate().map(|(i, x)| i as f64 * x).sum()
    } else {
        p[0]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_question_fits_the_letters() {
        for dataset in Dataset::ALL {
            for q in questions(dataset) {
                assert!((2..=LETTERS.len()).contains(&q.options.len()), "{}", q.kind);
                assert_eq!(q.scale, q.options != YES_NO, "{}", q.kind);
            }
        }
    }

    #[test]
    fn the_prompt_matches_the_measured_layout() {
        let q = &questions(Dataset::HendrycksMath)[2];
        assert_eq!(
            prompt("Find x.", q),
            "Find x.\n\nQuestion: How much computation does the solution involve?\n\
             A. almost none\nB. light\nC. heavy\n\nAnswer with the letter only."
        );
        assert_eq!(value(q, &[0.0, 0.5, 0.5]), 1.5);
        assert_eq!(value(&questions(Dataset::Ugphysics)[3], &[0.8, 0.2]), 0.8);
    }
}
