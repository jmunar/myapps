//! Which problem comes next, and how an answer moves your level.
//!
//! Pure functions over an injected RNG; `ops::draw` wires them to SQL.
//!
//! Above the staircase sits the top level, where it has nowhere left to send
//! you. There problems are ranked by the grader instead: each gets its
//! percentile on [`GRADE_TAG`] within its subject's top level, your ability is
//! replayed from your answers there (an Elo-style update on a logistic
//! curve), and the draw aims where you would get [`TARGET`] of them right.
//! See docs/ideas-03-graded-math-physics.md, "The real attempts".

use rand::Rng;

/// Where the target level is drawn from, relative to your current level `L`:
/// (offset, weight). Leans upward on purpose — the staircase only climbs on
/// evidence, and an occasional harder problem is that evidence.
const NEIGHBOURHOOD: [(i64, u32); 3] = [(-1, 15), (0, 60), (1, 25)];

/// The grader's tag that ranks problems within the top level: the prior from
/// the rubric flags and lengths, which predicted the real failures better than
/// the judge's grade did.
pub const GRADE_TAG: &str = "grade_prior";

/// The chance of a right answer the top-level draw aims for: where the
/// staircase settles below it (2 up, 1 down is ~71%).
const TARGET: f64 = 0.7;
/// How fast that chance falls with the percentile: P(right) =
/// σ((ability − percentile) / SPREAD). With STEP, picked by simulation to land
/// near TARGET within about 20 answers whether the true curve is half or one
/// and a half times as steep.
const SPREAD: f64 = 0.2;
/// How far one answer moves the ability, in percentile units.
const STEP: f64 = 0.1;
/// The ability before any top-level answer: the median problem.
const START: f64 = 0.5;
/// The draw takes any unseen problem this far below or above the target,
/// leaning upward like the staircase's neighbourhood, so the ability keeps
/// meeting evidence from both sides.
const BELOW: f64 = 0.05;
const ABOVE: f64 = 0.10;

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

/// Each grade's mid-rank percentile among `grades`, in `[0, 1]`: ties share
/// one, and a single problem sits at 0.5.
pub fn percentiles(grades: &[f64]) -> Vec<f64> {
    let mut sorted = grades.to_vec();
    sorted.sort_by(f64::total_cmp);
    let n = sorted.len() as f64;
    grades
        .iter()
        .map(|g| {
            let below = sorted.partition_point(|x| x < g);
            let through = sorted.partition_point(|x| x <= g);
            (below + through) as f64 / 2.0 / n
        })
        .collect()
}

fn chance(ability: f64, percentile: f64) -> f64 {
    1.0 / (1.0 + (-(ability - percentile) / SPREAD).exp())
}

/// Your ability in one subject's top level, replayed from your answers there
/// in order, each as (the problem's percentile, right).
pub fn ability(history: impl IntoIterator<Item = (f64, bool)>) -> f64 {
    history.into_iter().fold(START, |a, (q, right)| {
        a + STEP * (f64::from(u8::from(right)) - chance(a, q))
    })
}

/// The percentile at which `ability` gets TARGET right. Above 1 (or below 0)
/// once you are past every problem, which the draw reads as "the hardest".
pub fn graded_target(ability: f64) -> f64 {
    ability - SPREAD * (TARGET / (1.0 - TARGET)).ln()
}

/// A problem from `candidates` (id, percentile) near `target`: uniformly among
/// those from BELOW under it to ABOVE over it, else the nearest one, the
/// harder on a tie.
pub fn pick_graded(candidates: &[(i64, f64)], target: f64, rng: &mut impl Rng) -> Option<i64> {
    let near: Vec<i64> = candidates
        .iter()
        .filter(|&&(_, q)| (target - BELOW..=target + ABOVE).contains(&q))
        .map(|&(id, _)| id)
        .collect();
    if !near.is_empty() {
        return Some(near[rng.random_range(0..near.len())]);
    }
    candidates
        .iter()
        .min_by(|a, b| {
            let d = |q: f64| (q - target).abs();
            d(a.1).total_cmp(&d(b.1)).then(b.1.total_cmp(&a.1))
        })
        .map(|&(id, _)| id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;
    use std::collections::HashSet;

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

    #[test]
    fn percentiles_are_mid_ranks() {
        assert_eq!(percentiles(&[3.0, 1.0, 2.0, 2.0]), [0.875, 0.125, 0.5, 0.5]);
        assert_eq!(percentiles(&[7.0]), [0.5]);
        assert!(percentiles(&[]).is_empty());
    }

    #[test]
    fn ability_rises_on_right_answers_and_falls_on_wrong_ones() {
        assert_eq!(ability([]), START);
        let up = ability([(0.5, true)]);
        let down = ability([(0.5, false)]);
        assert!((up - 0.55).abs() < 1e-9 && (down - 0.45).abs() < 1e-9);
        // An easy problem right says less than a hard one right.
        assert!(ability([(0.1, true)]) < ability([(0.9, true)]));
        // Order matters: the latest answers count most.
        assert!(ability([(0.5, false), (0.5, true)]) > ability([(0.5, true), (0.5, false)]));
    }

    #[test]
    fn the_target_is_where_the_chance_is_the_target() {
        for a in [0.2, 0.5, 0.9] {
            assert!((chance(a, graded_target(a)) - TARGET).abs() < 1e-9);
            assert!(graded_target(a) < a);
        }
    }

    #[test]
    fn pick_graded_draws_near_the_target_leaning_up() {
        let candidates: Vec<(i64, f64)> = (0..=100).map(|i| (i, i as f64 / 100.0)).collect();
        let mut rng = rng();
        let mut seen = HashSet::new();
        for _ in 0..2_000 {
            seen.insert(pick_graded(&candidates, 0.5, &mut rng).unwrap());
        }
        assert_eq!(seen, (45..=60).collect());
    }

    #[test]
    fn pick_graded_takes_the_nearest_when_none_is_close() {
        let candidates = [(1, 0.1), (2, 0.3), (3, 0.7)];
        assert_eq!(pick_graded(&candidates, 1.4, &mut rng()), Some(3));
        assert_eq!(pick_graded(&candidates, -0.5, &mut rng()), Some(1));
        // 0.5 is as far from 0.3 as from 0.7: the harder wins.
        assert_eq!(pick_graded(&candidates, 0.5, &mut rng()), Some(3));
        assert_eq!(pick_graded(&[], 0.5, &mut rng()), None);
    }

    /// A simulated user whose true curve is the model's: after a warm-up the
    /// draw serves problems they get about TARGET of right, a few points under
    /// it, since the window leans upward (its middle is 0.025 above the target,
    /// where the chance is 0.67).
    #[test]
    fn the_draw_settles_near_the_target() {
        let candidates: Vec<(i64, f64)> = (0..=200).map(|i| (i, i as f64 / 200.0)).collect();
        let mut rng = rng();
        let (mut right, mut total) = (0u32, 0u32);
        for true_ability in [0.4, 0.6, 0.8] {
            let mut history = Vec::new();
            for i in 0..200 {
                let id = pick_graded(
                    &candidates,
                    graded_target(ability(history.clone())),
                    &mut rng,
                );
                let q = candidates[id.unwrap() as usize].1;
                let ok = rng.random_bool(chance(true_ability, q));
                history.push((q, ok));
                if i >= 40 {
                    total += 1;
                    right += u32::from(ok);
                }
            }
        }
        let rate = f64::from(right) / f64::from(total);
        assert!((0.63..=0.72).contains(&rate), "{rate}");
    }
}
