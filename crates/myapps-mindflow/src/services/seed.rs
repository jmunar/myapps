use anyhow::Result;
use sqlx::SqlitePool;

use myapps_core::registry::delete_user_app_data;

pub async fn run(
    pool: &SqlitePool,
    user_id: i64,
    app: &dyn myapps_core::registry::App,
) -> Result<()> {
    delete_user_app_data(pool, app, user_id).await?;

    // Categories
    let categories = &[
        ("Work", "#2196F3", Some("W")),
        ("Health", "#4CAF50", Some("H")),
        ("Finance", "#FF9800", Some("F")),
        ("Personal", "#9C27B0", Some("P")),
        ("Learning", "#00BCD4", Some("L")),
        ("Home", "#795548", Some("Hm")),
    ];

    for (i, (name, color, icon)) in categories.iter().enumerate() {
        sqlx::query(
            "INSERT INTO mindflow_categories (user_id, name, color, icon, position) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(user_id)
        .bind(name)
        .bind(color)
        .bind(*icon)
        .bind(i as i64)
        .execute(pool)
        .await?;
    }

    tracing::info!("Seeded {} categories", categories.len());

    // Get category IDs
    let work_id = cat_id(pool, user_id, "Work").await;
    let health_id = cat_id(pool, user_id, "Health").await;
    let finance_id = cat_id(pool, user_id, "Finance").await;
    let personal_id = cat_id(pool, user_id, "Personal").await;
    let learning_id = cat_id(pool, user_id, "Learning").await;
    let home_id = cat_id(pool, user_id, "Home").await;

    // Thoughts
    let thoughts: &[(&str, Option<i64>)] = &[
        ("Review Q1 project plan with the team", work_id),
        ("Schedule 1:1 with Sarah about the API redesign", work_id),
        ("Draft proposal for new microservice architecture", work_id),
        ("Book dentist appointment for next month", health_id),
        ("Try that new yoga class on Wednesday evenings", health_id),
        ("Research meal prep ideas for the week", health_id),
        ("Check investment portfolio rebalancing", finance_id),
        ("Cancel unused streaming subscription", finance_id),
        ("Compare electricity providers for better rate", finance_id),
        ("Plan weekend trip to the mountains", personal_id),
        ("Call mom for her birthday on Thursday", personal_id),
        ("Read that Rust async book chapter on pinning", learning_id),
        ("Watch the D3.js force graph tutorial", learning_id),
        ("Fix the leaky faucet in the bathroom", home_id),
        ("Order new shelf for the office", home_id),
        // Inbox (uncategorized)
        (
            "Look into that new note-taking tool someone mentioned",
            None,
        ),
        ("Remember to buy birthday present for Alex", None),
        ("Interesting idea: automate weekly report generation", None),
    ];

    let mut thought_count = 0;
    for (content, cat_id) in thoughts {
        let result = sqlx::query(
            "INSERT INTO mindflow_thoughts (user_id, category_id, content) VALUES (?, ?, ?)",
        )
        .bind(user_id)
        .bind(*cat_id)
        .bind(content)
        .execute(pool)
        .await?;
        thought_count += result.rows_affected();
    }

    tracing::info!("Seeded {thought_count} thoughts");

    // Nested sub-thoughts
    let api_thought: Option<(i64, Option<i64>)> = sqlx::query_as(
        "SELECT id, category_id FROM mindflow_thoughts WHERE user_id = ? AND content LIKE '%API redesign%' LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?;

    if let Some((parent_id, cat_id)) = api_thought {
        for sub in &[
            "Review current endpoint naming conventions",
            "Benchmark response times for v1 vs v2",
            "Draft migration guide for consumers",
        ] {
            sqlx::query(
                "INSERT INTO mindflow_thoughts (user_id, category_id, parent_thought_id, content) VALUES (?, ?, ?, ?)",
            )
            .bind(user_id)
            .bind(cat_id)
            .bind(parent_id)
            .bind(sub)
            .execute(pool)
            .await?;
        }
    }

    let meal_thought: Option<(i64, Option<i64>)> = sqlx::query_as(
        "SELECT id, category_id FROM mindflow_thoughts WHERE user_id = ? AND content LIKE '%meal prep%' LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?;

    if let Some((parent_id, cat_id)) = meal_thought {
        for sub in &[
            "Check batch cooking recipes for chicken and rice",
            "Buy glass containers for portion storage",
        ] {
            sqlx::query(
                "INSERT INTO mindflow_thoughts (user_id, category_id, parent_thought_id, content) VALUES (?, ?, ?, ?)",
            )
            .bind(user_id)
            .bind(cat_id)
            .bind(parent_id)
            .bind(sub)
            .execute(pool)
            .await?;
        }
    }

    // Comments on some thoughts
    let first_thought: Option<(i64,)> = sqlx::query_as(
        "SELECT id FROM mindflow_thoughts WHERE user_id = ? AND content LIKE '%Q1 project%' LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?;

    if let Some((tid,)) = first_thought {
        sqlx::query("INSERT INTO mindflow_comments (thought_id, content) VALUES (?, ?)")
            .bind(tid)
            .bind("Need to update the timeline before the review")
            .execute(pool)
            .await?;
        sqlx::query("INSERT INTO mindflow_comments (thought_id, content) VALUES (?, ?)")
            .bind(tid)
            .bind("Check with DevOps about deployment schedule")
            .execute(pool)
            .await?;
    }

    // Actions
    let actionable: Vec<(i64, String)> =
        sqlx::query_as("SELECT id, content FROM mindflow_thoughts WHERE user_id = ? LIMIT 5")
            .bind(user_id)
            .fetch_all(pool)
            .await?;

    let action_defs: &[(&str, &str, Option<&str>)] = &[
        (
            "Set up meeting room for Q1 review",
            "high",
            Some("2026-03-18"),
        ),
        ("Book dentist appointment", "medium", Some("2026-03-20")),
        ("Compare 3 electricity providers", "low", None),
        ("Buy birthday present for Alex", "high", Some("2026-03-16")),
    ];

    let mut action_count = 0u64;
    for (i, (title, priority, due_date)) in action_defs.iter().enumerate() {
        if let Some((thought_id, _)) = actionable.get(i) {
            let result = sqlx::query(
                "INSERT INTO mindflow_actions (thought_id, user_id, title, priority, due_date) VALUES (?, ?, ?, ?, ?)",
            )
            .bind(thought_id)
            .bind(user_id)
            .bind(title)
            .bind(priority)
            .bind(*due_date)
            .execute(pool)
            .await?;
            action_count += result.rows_affected();
        }
    }

    tracing::info!("Seeded {action_count} actions");
    tracing::info!("MindFlow seed complete");

    Ok(())
}

async fn cat_id(pool: &SqlitePool, user_id: i64, name: &str) -> Option<i64> {
    sqlx::query_as::<_, (i64,)>("SELECT id FROM mindflow_categories WHERE user_id = ? AND name = ?")
        .bind(user_id)
        .bind(name)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .map(|r| r.0)
}
