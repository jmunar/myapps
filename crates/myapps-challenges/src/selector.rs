//! Which problem comes next, and how an answer moves your level.
//!
//! Pure functions over an injected RNG; `ops::draw` wires them to SQL.

use rand::Rng;

/// Where the target level is drawn from, relative to your current level `L`:
/// (offset, weight). Leans upward on purpose — the staircase only climbs on
/// evidence, and an occasional harder problem is that evidence.
const NEIGHBOURHOOD: [(i64, u32); 3] = [(-1, 15), (0, 60), (1, 25)];

/// One (user, dataset, subject)'s staircase state; mirrors `challenges_progress`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub level: i64,
    /// Consecutive correct answers at this level.
    pub streak: i64,
    /// Until the first wrong answer, one correct answer moves up.
    pub fast_start: bool,
}

impl Default for Progress {
    /// The cold start: the easiest level.
    fn default() -> Self {
        Self {
            level: 1,
            streak: 0,
            fast_start: true,
        }
    }
}

pub fn pick_subject<'a>(subjects: &'a [String], rng: &mut impl Rng) -> Option<&'a str> {
    if subjects.is_empty() {
        return None;
    }
    Some(&subjects[rng.random_range(0..subjects.len())])
}

/// Draw a target level from the neighbourhood of `level`. Weights for levels
/// outside `1..=max_level` are dropped and the rest renormalised.
pub fn pick_level(level: i64, max_level: i64, rng: &mut impl Rng) -> i64 {
    let candidates: Vec<(i64, u32)> = NEIGHBOURHOOD
        .iter()
        .map(|&(offset, weight)| (level + offset, weight))
        .filter(|&(l, _)| (1..=max_level).contains(&l))
        .collect();
    let total: u32 = candidates.iter().map(|&(_, w)| w).sum();
    if total == 0 {
        return level.clamp(1, max_level);
    }
    let mut roll = rng.random_range(0..total);
    for (l, w) in candidates {
        if roll < w {
            return l;
        }
        roll -= w;
    }
    unreachable!("roll is below the total weight")
}

/// The levels to try, in order, when looking for an unseen problem: the target,
/// then the rest of the neighbourhood (L, L+1, L-1), then every other level by
/// distance from `L`, the harder one first on a tie.
pub fn fallback_order(target: i64, level: i64, max_level: i64) -> Vec<i64> {
    let mut order = vec![target];
    let mut others: Vec<i64> = (1..=max_level).collect();
    others.sort_by_key(|&l| ((l - level).abs(), l < level));
    for l in others {
        if !order.contains(&l) {
            order.push(l);
        }
    }
    order
}

/// The staircase: two correct in a row (one, during the fast start) moves up a
/// level, a wrong answer moves down one and ends the fast start for good.
pub fn advance(p: Progress, correct: bool, max_level: i64) -> Progress {
    if correct {
        let streak = p.streak + 1;
        let needed = if p.fast_start { 1 } else { 2 };
        if streak >= needed {
            Progress {
                level: (p.level + 1).min(max_level),
                streak: 0,
                ..p
            }
        } else {
            Progress { streak, ..p }
        }
    } else {
        Progress {
            level: (p.level - 1).max(1),
            streak: 0,
            fast_start: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    fn rng() -> StdRng {
        StdRng::seed_from_u64(7)
    }

    /// Frequencies of `pick_level` over many draws, indexed by level.
    fn frequencies(level: i64, max_level: i64) -> Vec<f64> {
        let mut rng = rng();
        let n = 20_000;
        let mut counts = vec![0u32; (max_level + 1) as usize];
        for _ in 0..n {
            counts[pick_level(level, max_level, &mut rng) as usize] += 1;
        }
        counts.into_iter().map(|c| c as f64 / n as f64).collect()
    }

    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 0.02,
            "expected ~{expected}, got {actual}"
        );
    }

    #[test]
    fn pick_subject_is_uniform() {
        let subjects: Vec<String> = ["a", "b", "c", "d"].map(String::from).to_vec();
        let mut rng = rng();
        let mut counts = [0u32; 4];
        for _ in 0..20_000 {
            let s = pick_subject(&subjects, &mut rng).unwrap();
            counts[subjects.iter().position(|x| x == s).unwrap()] += 1;
        }
        for c in counts {
            assert_close(c as f64 / 20_000.0, 0.25);
        }
        assert_eq!(pick_subject(&[], &mut rng), None);
    }

    #[test]
    fn pick_level_in_the_middle_uses_all_three_weights() {
        let f = frequencies(3, 5);
        assert_close(f[2], 0.15);
        assert_close(f[3], 0.60);
        assert_close(f[4], 0.25);
        assert_eq!(f[1] + f[5], 0.0);
    }

    #[test]
    fn pick_level_at_the_bottom_renormalises() {
        let f = frequencies(1, 5);
        assert_close(f[1], 60.0 / 85.0);
        assert_close(f[2], 25.0 / 85.0);
    }

    #[test]
    fn pick_level_at_the_top_renormalises() {
        let f = frequencies(3, 3);
        assert_close(f[2], 15.0 / 75.0);
        assert_close(f[3], 60.0 / 75.0);
    }

    #[test]
    fn fallback_tries_the_neighbourhood_then_the_rest_by_distance() {
        assert_eq!(fallback_order(3, 3, 5), vec![3, 4, 2, 5, 1]);
        assert_eq!(fallback_order(4, 3, 5), vec![4, 3, 2, 5, 1]);
        assert_eq!(fallback_order(2, 3, 5), vec![2, 3, 4, 5, 1]);
        assert_eq!(fallback_order(1, 1, 3), vec![1, 2, 3]);
        assert_eq!(fallback_order(3, 3, 3), vec![3, 2, 1]);
    }

    #[test]
    fn fast_start_climbs_on_every_correct_answer() {
        let mut p = Progress::default();
        for expected in [2, 3, 4, 5, 5] {
            p = advance(p, true, 5);
            assert_eq!(p.level, expected);
        }
        assert!(p.fast_start);
    }

    #[test]
    fn a_wrong_answer_ends_the_fast_start_for_good() {
        let p = advance(
            Progress {
                level: 3,
                ..Default::default()
            },
            false,
            5,
        );
        assert_eq!(
            p,
            Progress {
                level: 2,
                streak: 0,
                fast_start: false
            }
        );
        // Now it takes two in a row.
        let p = advance(p, true, 5);
        assert_eq!((p.level, p.streak), (2, 1));
        let p = advance(p, true, 5);
        assert_eq!((p.level, p.streak), (3, 0));
        assert!(!p.fast_start);
    }

    #[test]
    fn a_wrong_answer_resets_the_streak() {
        let p = Progress {
            level: 3,
            streak: 1,
            fast_start: false,
        };
        assert_eq!(advance(p, false, 5).streak, 0);
    }

    #[test]
    fn levels_stay_within_bounds() {
        let bottom = Progress {
            level: 1,
            streak: 0,
            fast_start: false,
        };
        assert_eq!(advance(bottom, false, 5).level, 1);
        let top = Progress {
            level: 3,
            streak: 1,
            fast_start: false,
        };
        assert_eq!(advance(top, true, 3), Progress { streak: 0, ..top });
    }
}
