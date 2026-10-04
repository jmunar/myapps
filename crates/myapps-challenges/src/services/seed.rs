use anyhow::Result;
use sqlx::SqlitePool;

use myapps_core::registry::delete_user_app_data;

use crate::dataset::Dataset;

/// (dataset, subject, difficulty, problem, solution, answer)
type Sample = (
    Dataset,
    &'static str,
    i64,
    &'static str,
    &'static str,
    &'static str,
);

/// A handful of hand-written problems, so a demo user has something to draw
/// before any dataset is imported. Only inserted into a dataset that is empty:
/// a real import is never mixed with them.
const SAMPLES: &[Sample] = &[
    (
        Dataset::Ugphysics,
        "Classical Mechanics",
        1,
        "State Newton's second law for a body of constant mass $m$.",
        "The net force equals mass times acceleration: $$\\vec F = m\\vec a.$$",
        "\\boxed{\\vec F = m \\vec a}",
    ),
    (
        Dataset::Ugphysics,
        "Classical Mechanics",
        2,
        "A ball is dropped from rest at height $h$. Neglecting air resistance, find its speed just before it hits the ground.",
        "Energy conservation: $mgh = \\tfrac12 m v^2$, so $$v = \\sqrt{2gh}.$$",
        "\\boxed{\\sqrt{2gh}}",
    ),
    (
        Dataset::Ugphysics,
        "Classical Mechanics",
        3,
        "A pendulum of length $\\ell$ is released from rest at angle $\\theta_0$. Find its speed at the lowest point.",
        "The bob falls a height $\\ell(1-\\cos\\theta_0)$, so $$v = \\sqrt{2g\\ell(1-\\cos\\theta_0)}.$$",
        "\\boxed{\\sqrt{2g\\ell(1-\\cos\\theta_0)}}",
    ),
    (
        Dataset::Ugphysics,
        "Thermodynamics",
        1,
        "Write the ideal gas law for $n$ moles of gas.",
        "$$pV = nRT$$",
        "\\boxed{pV = nRT}",
    ),
    (
        Dataset::Ugphysics,
        "Thermodynamics",
        2,
        "One mole of an ideal gas expands isothermally at temperature $T$ from volume $V$ to $2V$. Find the work done by the gas.",
        "$$W = \\int_V^{2V} p\\,dV' = RT\\ln 2.$$",
        "\\boxed{RT\\ln 2}",
    ),
    (
        Dataset::Ugphysics,
        "Quantum Mechanics",
        2,
        "Find the ground-state energy of a particle of mass $m$ in a one-dimensional infinite well of width $L$.",
        "$$E_n = \\frac{n^2\\pi^2\\hbar^2}{2mL^2},\\quad E_1 = \\frac{\\pi^2\\hbar^2}{2mL^2}.$$",
        "\\boxed{\\frac{\\pi^2\\hbar^2}{2mL^2}}",
    ),
    (
        Dataset::HendrycksMath,
        "Algebra",
        1,
        "Solve $2x + 3 = 11$.",
        "Subtract 3 and divide by 2: $x = \\boxed{4}$.",
        "4",
    ),
    (
        Dataset::HendrycksMath,
        "Algebra",
        3,
        "Find the sum of the roots of $x^2 - 7x + 10 = 0$.",
        "By Vieta's formulas the sum of the roots is $\\boxed{7}$.",
        "7",
    ),
    (
        Dataset::HendrycksMath,
        "Precalculus",
        2,
        "Compute $\\sin^2 15^\\circ + \\cos^2 15^\\circ$.",
        "It is $\\boxed{1}$ for every angle.",
        "1",
    ),
];

pub async fn run(
    pool: &SqlitePool,
    user_id: i64,
    app: &dyn myapps_core::registry::App,
) -> Result<()> {
    delete_user_app_data(pool, app, user_id).await?;

    for dataset in Dataset::ALL {
        let existing: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM challenges_problems WHERE dataset = ?")
                .bind(dataset.key())
                .fetch_one(pool)
                .await?;
        if existing > 0 {
            continue;
        }
        for (i, &(_, subject, difficulty, problem, solution, answer)) in
            SAMPLES.iter().enumerate().filter(|(_, s)| s.0 == dataset)
        {
            sqlx::query(
                "INSERT INTO challenges_problems
                     (dataset, source_key, subject, difficulty, source_level, problem,
                      solution, answer, source_url, license)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(dataset.key())
            .bind(format!("sample/{i}"))
            .bind(subject)
            .bind(difficulty)
            .bind(dataset.level_name(difficulty))
            .bind(problem)
            .bind(solution)
            .bind(answer)
            .bind(dataset.url())
            .bind("sample")
            .execute(pool)
            .await?;
        }
    }

    // A short history, so the stats page has something to show.
    let ids: Vec<i64> = sqlx::query_scalar(
        "SELECT id FROM challenges_problems WHERE dataset = ? AND difficulty <= 2 ORDER BY id LIMIT 3",
    )
    .bind(Dataset::Ugphysics.key())
    .fetch_all(pool)
    .await?;
    for (id, correct) in ids.into_iter().zip([true, true, false]) {
        crate::ops::record_attempt(pool, user_id, id, correct).await?;
    }

    Ok(())
}
