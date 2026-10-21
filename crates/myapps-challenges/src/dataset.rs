//! The problem datasets, and everything about them that differs.
//!
//! The selector, routes and stats only ever see a `Dataset`; adding a source
//! means a variant here and a reader for it in `myapps-challenges-prep`, which
//! is also where a dataset's own levels are mapped to a difficulty.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dataset {
    Ugphysics,
    HendrycksMath,
    DataScience,
}

impl Dataset {
    pub const ALL: [Dataset; 3] = [
        Dataset::Ugphysics,
        Dataset::HendrycksMath,
        Dataset::DataScience,
    ];

    /// Stored in `challenges_problems.dataset` and used in URLs and forms.
    pub fn key(self) -> &'static str {
        match self {
            Dataset::Ugphysics => "ugphysics",
            Dataset::HendrycksMath => "hendrycks-math",
            Dataset::DataScience => "data-science",
        }
    }

    pub fn from_key(key: &str) -> Option<Dataset> {
        Self::ALL.into_iter().find(|d| d.key() == key)
    }

    pub fn name(self) -> &'static str {
        match self {
            Dataset::Ugphysics => "UGPhysics",
            Dataset::HendrycksMath => "Hendrycks MATH",
            Dataset::DataScience => "Data science",
        }
    }

    /// The highest `difficulty` this dataset's problems carry.
    pub fn max_level(self) -> i64 {
        match self {
            Dataset::Ugphysics => 3,
            Dataset::HendrycksMath => 5,
            Dataset::DataScience => 3,
        }
    }

    /// Where the prep tool fetches it from; `None` for a deck kept in the
    /// repository.
    pub fn hf_id(self) -> Option<&'static str> {
        match self {
            Dataset::Ugphysics => Some("UGPhysics/ugphysics"),
            Dataset::HendrycksMath => Some("EleutherAI/hendrycks_math"),
            Dataset::DataScience => None,
        }
    }

    pub fn url(self) -> String {
        match self.hf_id() {
            Some(id) => format!("https://huggingface.co/datasets/{id}"),
            None => "https://github.com/jmunar/myapps/blob/main/crates/myapps-challenges-prep/datasets/README.md".into(),
        }
    }

    pub fn license(self) -> &'static str {
        match self {
            Dataset::Ugphysics => "CC BY-NC-SA 4.0",
            Dataset::HendrycksMath => "MIT",
            Dataset::DataScience => "CC BY-SA 4.0",
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
            Dataset::DataScience => match difficulty {
                1 => "Recall".into(),
                2 => "Apply".into(),
                _ => "Judge".into(),
            },
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
