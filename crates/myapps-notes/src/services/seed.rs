use anyhow::Result;
use sqlx::SqlitePool;

use myapps_core::registry::delete_user_app_data;

pub async fn run(
    pool: &SqlitePool,
    user_id: i64,
    app: &dyn myapps_core::registry::App,
) -> Result<()> {
    delete_user_app_data(pool, app, user_id).await?;

    let notes: &[(&str, &str, bool)] = &[
        (
            "Meeting Notes — Project Kickoff",
            "# Project Kickoff\n\n## Attendees\n- Alice, Bob, Carol\n\n## Key Decisions\n1. **Timeline**: 6-week sprint starting March 24\n2. **Stack**: Rust + HTMX (already decided)\n3. **Priority**: MVP first, polish later\n\n## Action Items\n- [ ] Alice: set up CI pipeline\n- [ ] Bob: design database schema\n- [ ] Carol: wireframe the UI\n\n> \"Ship early, iterate fast\" — the team motto",
            true,
        ),
        (
            "Rust Tips",
            "# Rust Tips & Tricks\n\n## Pattern Matching\n```rust\nmatch value {\n    Some(x) if x > 0 => println!(\"positive: {x}\"),\n    Some(_) => println!(\"non-positive\"),\n    None => println!(\"nothing\"),\n}\n```\n\n## Useful Traits\n- `From`/`Into` — type conversions\n- `Display` — user-facing formatting\n- `Default` — sensible defaults\n\n## Links\n- [The Rust Book](https://doc.rust-lang.org/book/)\n- [Rust by Example](https://doc.rust-lang.org/rust-by-example/)",
            false,
        ),
        (
            "Shopping List",
            "# Shopping List\n\n- [x] Olive oil\n- [x] Bread\n- [ ] Tomatoes (vine-ripened)\n- [ ] Mozzarella\n- [ ] Fresh basil\n- [ ] Pasta (penne)\n- [ ] Garlic\n- [ ] Lemons\n\n---\n\n*For the caprese salad recipe on Saturday*",
            false,
        ),
        (
            "Book Notes — Designing Data-Intensive Applications",
            "# DDIA — Key Takeaways\n\n## Chapter 1: Reliable, Scalable, Maintainable\n- **Reliability**: system works correctly even when faults occur\n- **Scalability**: ability to cope with increased load\n- **Maintainability**: ease of making changes over time\n\n## Chapter 3: Storage & Retrieval\n- LSM-trees vs B-trees\n- Write-optimized (LSM) vs read-optimized (B-tree)\n- SSTables: sorted string tables\n\n## Favourite Quote\n> *\"An architecture that is appropriate for one level of load is unlikely to cope with 10 times that load.\"*\n\n## Rating: 5/5",
            true,
        ),
    ];

    let mut count = 0u64;
    for (title, body, pinned) in notes {
        let result = sqlx::query(
            "INSERT INTO notes_notes (user_id, title, body, pinned) VALUES (?, ?, ?, ?)",
        )
        .bind(user_id)
        .bind(title)
        .bind(body)
        .bind(*pinned as i32)
        .execute(pool)
        .await?;
        count += result.rows_affected();
    }
    tracing::info!("Seeded {count} notes");
    tracing::info!("Notes seed complete");

    Ok(())
}
