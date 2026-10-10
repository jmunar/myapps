//! What each dataset looks like upstream, and how a row of it becomes a
//! bundle problem.

use myapps_challenges::bundle::Problem;
use myapps_challenges::dataset::Dataset;

/// Why a row was left out, for the summary line.
#[derive(Debug, PartialEq)]
pub enum Skip {
    NoLevel,
    Diagram,
    Malformed,
}

/// The datasets-server (config, split) pairs a dataset is read from.
pub fn sources(dataset: Dataset) -> Vec<(&'static str, &'static str)> {
    match dataset {
        Dataset::Ugphysics => [
            "AtomicPhysics",
            "ClassicalElectromagnetism",
            "ClassicalMechanics",
            "Electrodynamics",
            "GeometricalOptics",
            "QuantumMechanics",
            "Relativity",
            "SemiconductorPhysics",
            "Solid-StatePhysics",
            "StatisticalMechanics",
            "TheoreticalMechanics",
            "Thermodynamics",
            "WaveOptics",
        ]
        .into_iter()
        .map(|c| (c, "en"))
        .collect(),
        Dataset::HendrycksMath => [
            "algebra",
            "counting_and_probability",
            "geometry",
            "intermediate_algebra",
            "number_theory",
            "prealgebra",
            "precalculus",
        ]
        .into_iter()
        .flat_map(|c| [(c, "train"), (c, "test")])
        .collect(),
    }
}

/// Map the dataset's own `level` string to a `difficulty`, or `None` to
/// leave the problem out.
///
/// UGPhysics' `level` is a skill type, not a difficulty; it is read as a
/// three-step ladder. Hendrycks MATH's is "Level 1".."Level 5".
pub fn difficulty(dataset: Dataset, source_level: &str) -> Option<i64> {
    match dataset {
        Dataset::Ugphysics => match source_level.trim() {
            "Knowledge Recall" => Some(1),
            "Laws Application" => Some(2),
            "Math Derivation" | "Practical Application" => Some(3),
            _ => None,
        },
        Dataset::HendrycksMath => source_level
            .trim()
            .strip_prefix("Level ")
            .and_then(|n| n.parse().ok())
            .filter(|n| (1..=5).contains(n)),
    }
}

pub fn map_row(
    dataset: Dataset,
    config: &str,
    split: &str,
    row_idx: usize,
    row: &serde_json::Value,
) -> Result<Problem, Skip> {
    let text = |key: &str| row.get(key).and_then(|v| v.as_str()).map(str::trim);
    let non_empty = |key: &str| text(key).filter(|s| !s.is_empty()).map(str::to_string);

    let source_level = text("level").unwrap_or("");
    let difficulty = difficulty(dataset, source_level).ok_or(Skip::NoLevel)?;
    let problem = non_empty("problem").ok_or(Skip::Malformed)?;
    let solution = non_empty("solution").ok_or(Skip::Malformed)?;

    match dataset {
        Dataset::Ugphysics => {
            let index = row.get("index").and_then(|v| v.as_i64());
            Ok(Problem {
                source_key: match index {
                    Some(i) => format!("{config}/{i}"),
                    None => format!("{config}/row-{row_idx}"),
                },
                subject: non_empty("subject").unwrap_or_else(|| config.to_string()),
                topic: non_empty("topic"),
                difficulty,
                source_level: source_level.to_string(),
                problem,
                solution,
                answer: non_empty("answers").ok_or(Skip::Malformed)?,
                answer_type: non_empty("answer_type")
                    .map(|s| s.lines().next().unwrap_or("").trim().to_string()),
                unit: non_empty("unit"),
            })
        }
        Dataset::HendrycksMath => {
            // Asymptote diagrams cannot be drawn in the browser, and the
            // problem is not answerable without them.
            if problem.contains("[asy]") {
                return Err(Skip::Diagram);
            }
            Ok(Problem {
                source_key: format!("{config}/{split}/{row_idx}"),
                subject: non_empty("type").unwrap_or_else(|| config.to_string()),
                topic: None,
                difficulty,
                source_level: source_level.to_string(),
                answer: last_boxed(&solution).unwrap_or_default().to_string(),
                problem,
                solution,
                answer_type: None,
                unit: None,
            })
        }
    }
}

/// The contents of the last `\boxed{…}` (or `\fbox{…}`) in `s`, matching
/// braces rather than using a regex, since answers nest them (`\frac{1}{2}`).
pub fn last_boxed(s: &str) -> Option<&str> {
    let start = ["\\boxed{", "\\fbox{"]
        .iter()
        .filter_map(|m| s.rfind(m).map(|i| i + m.len()))
        .max()?;
    let mut depth = 1;
    for (i, c) in s[start..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&s[start..start + i]);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn ugphysics_level_is_a_three_step_ladder() {
        let d = Dataset::Ugphysics;
        assert_eq!(difficulty(d, "Knowledge Recall"), Some(1));
        assert_eq!(difficulty(d, "Laws Application"), Some(2));
        assert_eq!(difficulty(d, "Math Derivation"), Some(3));
        assert_eq!(difficulty(d, "Practical Application"), Some(3));
        assert_eq!(difficulty(d, ""), None);
    }

    #[test]
    fn hendrycks_level_parses_one_to_five() {
        let d = Dataset::HendrycksMath;
        assert_eq!(difficulty(d, "Level 1"), Some(1));
        assert_eq!(difficulty(d, "Level 5"), Some(5));
        assert_eq!(difficulty(d, "Level ?"), None);
        assert_eq!(difficulty(d, "Level 6"), None);
    }

    /// The server refuses a bundle with a difficulty outside the dataset's
    /// range, and the two live in different crates.
    #[test]
    fn every_level_maps_within_the_datasets_range() {
        let levels = [
            "Knowledge Recall",
            "Laws Application",
            "Math Derivation",
            "Practical Application",
            "Level 1",
            "Level 2",
            "Level 3",
            "Level 4",
            "Level 5",
        ];
        for d in Dataset::ALL {
            for level in levels {
                if let Some(n) = difficulty(d, level) {
                    assert!((1..=d.max_level()).contains(&n), "{d:?} {level} -> {n}");
                }
            }
        }
    }

    #[test]
    fn sources_cover_every_subject_and_split() {
        assert_eq!(sources(Dataset::Ugphysics).len(), 13);
        assert_eq!(sources(Dataset::HendrycksMath).len(), 14);
    }

    #[test]
    fn last_boxed_matches_nested_braces() {
        assert_eq!(
            last_boxed(r"so $x = \boxed{\frac{1}{2}}$."),
            Some(r"\frac{1}{2}")
        );
        assert_eq!(last_boxed(r"\boxed{1} then \boxed{2}"), Some("2"));
        assert_eq!(last_boxed(r"\fbox{7}"), Some("7"));
        assert_eq!(last_boxed("no answer"), None);
        assert_eq!(last_boxed(r"\boxed{unclosed"), None);
    }

    #[test]
    fn maps_a_ugphysics_row() {
        let row = json!({
            "index": 1523, "subject": "Classical Mechanics", "topic": "Particle Dynamics",
            "problem": "A rocket…", "solution": "Solve…", "answers": "\\boxed{v}",
            "answer_type": "EX", "unit": null, "level": "Math Derivation",
        });
        let p = map_row(Dataset::Ugphysics, "ClassicalMechanics", "en", 0, &row).unwrap();
        assert_eq!(p.source_key, "ClassicalMechanics/1523");
        assert_eq!(p.subject, "Classical Mechanics");
        assert_eq!(p.difficulty, 3);
        assert_eq!(p.answer, "\\boxed{v}");
        assert_eq!(p.unit, None);
    }

    #[test]
    fn keeps_only_the_first_line_of_a_garbled_answer_type() {
        let row = json!({
            "index": 1, "subject": "S", "problem": "p", "solution": "s", "answers": "a",
            "answer_type": "NV\n   \nThe final answer is…", "level": "Laws Application",
        });
        let p = map_row(Dataset::Ugphysics, "C", "en", 0, &row).unwrap();
        assert_eq!(p.answer_type.as_deref(), Some("NV"));
    }

    #[test]
    fn skips_ugphysics_rows_without_a_level() {
        let row =
            json!({ "index": 1, "problem": "p", "solution": "s", "answers": "a", "level": "" });
        assert_eq!(
            map_row(Dataset::Ugphysics, "C", "en", 0, &row),
            Err(Skip::NoLevel)
        );
    }

    #[test]
    fn maps_a_hendrycks_row_and_extracts_the_answer() {
        let row = json!({
            "problem": "Find x.", "level": "Level 4", "type": "Algebra",
            "solution": "Thus $x = \\boxed{(18, -18)}$.",
        });
        let p = map_row(Dataset::HendrycksMath, "algebra", "test", 12, &row).unwrap();
        assert_eq!(p.source_key, "algebra/test/12");
        assert_eq!(p.subject, "Algebra");
        assert_eq!(p.difficulty, 4);
        assert_eq!(p.answer, "(18, -18)");
    }

    #[test]
    fn skips_hendrycks_diagrams_and_unknown_levels() {
        let diagram = json!({
            "problem": "[asy]draw((0,0)--(1,1));[/asy] Find the area.",
            "level": "Level 2", "type": "Geometry", "solution": "\\boxed{1}",
        });
        assert_eq!(
            map_row(Dataset::HendrycksMath, "geometry", "train", 0, &diagram),
            Err(Skip::Diagram)
        );
        let unknown = json!({
            "problem": "p", "level": "Level ?", "type": "Geometry", "solution": "s",
        });
        assert_eq!(
            map_row(Dataset::HendrycksMath, "geometry", "train", 0, &unknown),
            Err(Skip::NoLevel)
        );
    }
}
