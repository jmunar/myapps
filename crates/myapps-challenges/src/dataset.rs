//! The two problem datasets, and everything about them that differs.
//!
//! The selector, routes and stats only ever see a `Dataset`; adding a source
//! means a variant here and a reader for it in `myapps-challenges-prep`, which
//! is also where a dataset's own levels are mapped to a difficulty.

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
}
