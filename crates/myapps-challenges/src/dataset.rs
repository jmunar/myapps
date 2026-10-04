//! The two problem datasets, and everything about them that differs.
//!
//! The selector, routes and stats only ever see a `Dataset`; adding a source
//! means a variant here and a row mapper in `services::import`.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dataset {
    Ugphysics,
    HendrycksMath,
}

impl Dataset {
    pub const ALL: [Dataset; 2] = [Dataset::Ugphysics, Dataset::HendrycksMath];

    /// Stored in `challenges_problems.dataset` and used in URLs and forms.
    pub fn key(self) -> &'static str {
        match self {
            Dataset::Ugphysics => "ugphysics",
            Dataset::HendrycksMath => "hendrycks-math",
        }
    }

    pub fn from_key(key: &str) -> Option<Dataset> {
        Self::ALL.into_iter().find(|d| d.key() == key)
    }

    pub fn name(self) -> &'static str {
        match self {
            Dataset::Ugphysics => "UGPhysics",
            Dataset::HendrycksMath => "Hendrycks MATH",
        }
    }

    /// The highest `difficulty` this dataset's problems carry.
    pub fn max_level(self) -> i64 {
        match self {
            Dataset::Ugphysics => 3,
            Dataset::HendrycksMath => 5,
        }
    }

    pub fn hf_id(self) -> &'static str {
        match self {
            Dataset::Ugphysics => "UGPhysics/ugphysics",
            Dataset::HendrycksMath => "EleutherAI/hendrycks_math",
        }
    }

    pub fn url(self) -> String {
        format!("https://huggingface.co/datasets/{}", self.hf_id())
    }

    pub fn license(self) -> &'static str {
        match self {
            Dataset::Ugphysics => "CC BY-NC-SA 4.0",
            Dataset::HendrycksMath => "MIT",
        }
    }

    /// The datasets-server (config, split) pairs the import reads.
    pub fn sources(self) -> Vec<(&'static str, &'static str)> {
        match self {
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
    pub fn difficulty(self, source_level: &str) -> Option<i64> {
        match self {
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

    /// The dataset's name for a level, for display next to the difficulty.
    pub fn level_name(self, difficulty: i64) -> String {
        match self {
            Dataset::Ugphysics => match difficulty {
                1 => "Knowledge Recall".into(),
                2 => "Laws Application".into(),
                _ => "Derivation / Practical".into(),
            },
            Dataset::HendrycksMath => format!("Level {difficulty}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_round_trip() {
        for d in Dataset::ALL {
            assert_eq!(Dataset::from_key(d.key()), Some(d));
        }
        assert_eq!(Dataset::from_key("u-math"), None);
    }

    #[test]
    fn ugphysics_level_is_a_three_step_ladder() {
        let d = Dataset::Ugphysics;
        assert_eq!(d.difficulty("Knowledge Recall"), Some(1));
        assert_eq!(d.difficulty("Laws Application"), Some(2));
        assert_eq!(d.difficulty("Math Derivation"), Some(3));
        assert_eq!(d.difficulty("Practical Application"), Some(3));
        assert_eq!(d.difficulty(""), None);
    }

    #[test]
    fn hendrycks_level_parses_one_to_five() {
        let d = Dataset::HendrycksMath;
        assert_eq!(d.difficulty("Level 1"), Some(1));
        assert_eq!(d.difficulty("Level 5"), Some(5));
        assert_eq!(d.difficulty("Level ?"), None);
        assert_eq!(d.difficulty("Level 6"), None);
    }

    #[test]
    fn sources_cover_every_subject_and_split() {
        assert_eq!(Dataset::Ugphysics.sources().len(), 13);
        assert_eq!(Dataset::HendrycksMath.sources().len(), 14);
    }
}
