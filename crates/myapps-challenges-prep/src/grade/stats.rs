//! The arithmetic behind `rank`: Spearman correlation, an out-of-fold ridge
//! regression for the prior, and a Bradley-Terry fit on soft verdicts. Small
//! enough to write out rather than pull in a linear-algebra crate.

/// Ranks starting at 1, ties sharing their average rank.
fn ranks(x: &[f64]) -> Vec<f64> {
    let mut order: Vec<usize> = (0..x.len()).collect();
    order.sort_by(|&a, &b| x[a].total_cmp(&x[b]));
    let mut r = vec![0.0; x.len()];
    let mut i = 0;
    while i < order.len() {
        let mut j = i;
        while j + 1 < order.len() && x[order[j + 1]] == x[order[i]] {
            j += 1;
        }
        let rank = (i + j) as f64 / 2.0 + 1.0;
        for &k in &order[i..=j] {
            r[k] = rank;
        }
        i = j + 1;
    }
    r
}

fn pearson(x: &[f64], y: &[f64]) -> f64 {
    let n = x.len() as f64;
    let (mx, my) = (x.iter().sum::<f64>() / n, y.iter().sum::<f64>() / n);
    let (mut sxy, mut sxx, mut syy) = (0.0, 0.0, 0.0);
    for (a, b) in x.iter().zip(y) {
        sxy += (a - mx) * (b - my);
        sxx += (a - mx) * (a - mx);
        syy += (b - my) * (b - my);
    }
    if sxx == 0.0 || syy == 0.0 {
        return f64::NAN;
    }
    sxy / (sxx * syy).sqrt()
}

/// NaN when either side is constant or there are fewer than three points.
pub fn spearman(x: &[f64], y: &[f64]) -> f64 {
    if x.len() < 3 {
        return f64::NAN;
    }
    pearson(&ranks(x), &ranks(y))
}

/// Solve `a · w = b` for a symmetric positive-definite `a` (Cholesky).
fn solve_spd(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Vec<f64> {
    let n = b.len();
    for j in 0..n {
        for k in 0..j {
            let l = a[j][k];
            for row in a.iter_mut().skip(j) {
                row[j] -= row[k] * l;
            }
        }
        let d = a[j][j].sqrt();
        for row in a.iter_mut().skip(j) {
            row[j] /= d;
        }
    }
    for i in 0..n {
        for k in 0..i {
            b[i] -= a[i][k] * b[k];
        }
        b[i] /= a[i][i];
    }
    for i in (0..n).rev() {
        for k in i + 1..n {
            b[i] -= a[k][i] * b[k];
        }
        b[i] /= a[i][i];
    }
    b
}

/// Ridge on standardised features, fitted on `train` and applied to `test`.
fn ridge_fit_predict(
    x: &[Vec<f64>],
    y: &[f64],
    train: &[usize],
    test: &[usize],
    lambda: f64,
) -> Vec<f64> {
    let d = x[0].len();
    let n = train.len() as f64;
    let mean: Vec<f64> = (0..d)
        .map(|j| train.iter().map(|&i| x[i][j]).sum::<f64>() / n)
        .collect();
    let sd: Vec<f64> = (0..d)
        .map(|j| {
            let v = train
                .iter()
                .map(|&i| (x[i][j] - mean[j]).powi(2))
                .sum::<f64>()
                / n;
            if v > 1e-12 { v.sqrt() } else { 1.0 }
        })
        .collect();
    let z = |i: usize| -> Vec<f64> { (0..d).map(|j| (x[i][j] - mean[j]) / sd[j]).collect() };
    let my = train.iter().map(|&i| y[i]).sum::<f64>() / n;

    let mut a = vec![vec![0.0; d]; d];
    let mut b = vec![0.0; d];
    for &i in train {
        let zi = z(i);
        for r in 0..d {
            b[r] += zi[r] * (y[i] - my);
            for c in 0..d {
                a[r][c] += zi[r] * zi[c];
            }
        }
    }
    for (r, row) in a.iter_mut().enumerate() {
        row[r] += lambda;
    }
    let w = solve_spd(a, b);
    test.iter()
        .map(|&i| my + z(i).iter().zip(&w).map(|(a, b)| a * b).sum::<f64>())
        .collect()
}

/// Out-of-fold ridge predictions: each point predicted by a model that never
/// saw it, so a prior built from the levels can be checked against them. The
/// penalty is picked from a grid by out-of-fold squared error. Returns the
/// predictions and the penalty.
pub fn ridge_out_of_fold(x: &[Vec<f64>], y: &[f64], fold: &[usize]) -> (Vec<f64>, f64) {
    let folds = fold.iter().max().map_or(0, |m| m + 1);
    let mut best: Option<(f64, Vec<f64>, f64)> = None;
    for lambda in [0.01, 0.1, 1.0, 10.0, 100.0, 1000.0] {
        let mut pred = vec![0.0; y.len()];
        for f in 0..folds {
            let train: Vec<usize> = (0..y.len()).filter(|&i| fold[i] != f).collect();
            let test: Vec<usize> = (0..y.len()).filter(|&i| fold[i] == f).collect();
            if train.is_empty() || test.is_empty() {
                continue;
            }
            for (&i, p) in test
                .iter()
                .zip(ridge_fit_predict(x, y, &train, &test, lambda))
            {
                pred[i] = p;
            }
        }
        let mse = pred
            .iter()
            .zip(y)
            .map(|(p, t)| (p - t).powi(2))
            .sum::<f64>();
        if best.as_ref().is_none_or(|b| mse < b.0) {
            best = Some((mse, pred, lambda));
        }
    }
    let (_, pred, lambda) = best.expect("a non-empty grid");
    (pred, lambda)
}

/// One verdict: P(problem `first`, shown first, is the harder).
pub struct Obs {
    pub first: usize,
    pub second: usize,
    pub p: f64,
}

pub struct Fit {
    /// The grade per problem, on the logit scale of the verdicts.
    pub score: Vec<f64>,
    /// Logit added to whichever problem is shown first.
    pub position: f64,
    /// Logit units per unit of prior.
    pub scale: f64,
}

fn sigmoid(z: f64) -> f64 {
    1.0 / (1.0 + (-z).exp())
}

fn softplus(z: f64) -> f64 {
    z.max(0.0) + (-z.abs()).exp().ln_1p()
}

/// Bradley-Terry on soft verdicts: P(first harder) = σ(sᶠ − sˢ + position),
/// with each score held near `scale · prior` by a Gaussian of width `tau`.
/// `scale` and `position` are fitted too, so the prior only has to rank, not
/// be on the verdicts' scale. Newton's method with conjugate-gradient steps:
/// the Hessian is sparse (one verdict touches two scores), so it is never
/// built, only multiplied by.
pub fn bradley_terry(prior: &[f64], obs: &[Obs], tau: f64) -> Fit {
    let n = prior.len();
    let mean = prior.iter().sum::<f64>() / n as f64;
    let m: Vec<f64> = prior.iter().map(|p| p - mean).collect();
    let (ia, ib) = (n, n + 1); // scale, position
    let prec = 1.0 / (tau * tau);
    let weak = 1.0 / 100.0; // a loose prior on scale and position keeps them finite

    let loss = |x: &[f64]| -> f64 {
        let mut f = 0.0;
        for o in obs {
            let z = x[o.first] - x[o.second] + x[ib];
            f += o.p * softplus(-z) + (1.0 - o.p) * softplus(z);
        }
        for i in 0..n {
            f += 0.5 * prec * (x[i] - x[ia] * m[i]).powi(2);
        }
        f + 0.5 * weak * (x[ia] * x[ia] + x[ib] * x[ib])
    };
    let gradient = |x: &[f64]| -> Vec<f64> {
        let mut g = vec![0.0; n + 2];
        for o in obs {
            let r = sigmoid(x[o.first] - x[o.second] + x[ib]) - o.p;
            g[o.first] += r;
            g[o.second] -= r;
            g[ib] += r;
        }
        for i in 0..n {
            let d = prec * (x[i] - x[ia] * m[i]);
            g[i] += d;
            g[ia] -= m[i] * d;
        }
        g[ia] += weak * x[ia];
        g[ib] += weak * x[ib];
        g
    };
    let hessian_times = |x: &[f64], v: &[f64]| -> Vec<f64> {
        let mut h = vec![0.0; n + 2];
        for o in obs {
            let s = sigmoid(x[o.first] - x[o.second] + x[ib]);
            let u = s * (1.0 - s) * (v[o.first] - v[o.second] + v[ib]);
            h[o.first] += u;
            h[o.second] -= u;
            h[ib] += u;
        }
        for i in 0..n {
            let e = prec * (v[i] - m[i] * v[ia]);
            h[i] += e;
            h[ia] -= m[i] * e;
        }
        h[ia] += weak * v[ia];
        h[ib] += weak * v[ib];
        h
    };
    let dot = |a: &[f64], b: &[f64]| a.iter().zip(b).map(|(x, y)| x * y).sum::<f64>();

    let mut x = vec![0.0; n + 2];
    for _ in 0..50 {
        let g = gradient(&x);
        if g.iter().fold(0.0f64, |a, v| a.max(v.abs())) < 1e-8 {
            break;
        }
        // Conjugate gradient for H·d = −g.
        let mut d = vec![0.0; n + 2];
        let mut r: Vec<f64> = g.iter().map(|v| -v).collect();
        let mut p = r.clone();
        let mut rr = dot(&r, &r);
        for _ in 0..500 {
            let hp = hessian_times(&x, &p);
            let alpha = rr / dot(&p, &hp);
            for k in 0..d.len() {
                d[k] += alpha * p[k];
                r[k] -= alpha * hp[k];
            }
            let next = dot(&r, &r);
            if next.sqrt() < 1e-10 {
                break;
            }
            for k in 0..p.len() {
                p[k] = r[k] + next / rr * p[k];
            }
            rr = next;
        }
        // Backtracking, in case a full step overshoots.
        let (f0, slope) = (loss(&x), dot(&g, &d));
        let mut t = 1.0;
        loop {
            let trial: Vec<f64> = x.iter().zip(&d).map(|(a, b)| a + t * b).collect();
            if loss(&trial) <= f0 + 1e-4 * t * slope || t < 1e-6 {
                x = trial;
                break;
            }
            t /= 2.0;
        }
    }
    Fit {
        position: x[ib],
        scale: x[ia],
        score: x[..n].to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spearman_handles_ties_and_reversal() {
        assert!((spearman(&[1.0, 2.0, 3.0, 4.0], &[10.0, 20.0, 30.0, 40.0]) - 1.0).abs() < 1e-12);
        assert!((spearman(&[1.0, 2.0, 3.0], &[3.0, 2.0, 1.0]) + 1.0).abs() < 1e-12);
        assert_eq!(ranks(&[5.0, 1.0, 5.0, 2.0]), [3.5, 1.0, 3.5, 2.0]);
        assert!(spearman(&[1.0, 1.0, 1.0], &[1.0, 2.0, 3.0]).is_nan());
    }

    #[test]
    fn ridge_recovers_a_linear_target_out_of_fold() {
        let x: Vec<Vec<f64>> = (0..60)
            .map(|i| vec![i as f64, ((i * 7) % 11) as f64])
            .collect();
        let y: Vec<f64> = x.iter().map(|r| 2.0 * r[0] - r[1] + 1.0).collect();
        let fold: Vec<usize> = (0..60).map(|i| i % 5).collect();
        let (pred, lambda) = ridge_out_of_fold(&x, &y, &fold);
        assert_eq!(lambda, 0.01);
        assert!(pred.iter().zip(&y).all(|(p, t)| (p - t).abs() < 0.5));
    }

    #[test]
    fn bradley_terry_recovers_an_order_and_the_position_bias() {
        // Five problems of rising difficulty; every ordered pair asked, with
        // the true probabilities plus a pull towards whichever comes first.
        let truth = [-2.0, -1.0, 0.0, 1.0, 2.0];
        let mut obs = Vec::new();
        for a in 0..5 {
            for b in 0..5 {
                if a != b {
                    let p = sigmoid(truth[a] - truth[b] + 0.5);
                    obs.push(Obs {
                        first: a,
                        second: b,
                        p,
                    });
                }
            }
        }
        // A flat prior: the verdicts alone decide.
        let fit = bradley_terry(&[0.0; 5], &obs, 10.0);
        assert!((fit.position - 0.5).abs() < 0.05, "{}", fit.position);
        assert!(fit.score.windows(2).all(|w| w[0] < w[1]));

        // No verdicts: the prior's order, at whatever scale.
        let fit = bradley_terry(&[3.0, 1.0, 2.0], &[], 1.0);
        assert_eq!(fit.score.iter().map(|s| s.abs()).sum::<f64>(), 0.0);
    }
}
