use anyhow::Result;
use sqlx::SqlitePool;

use myapps_core::registry::delete_user_app_data;

// Notes seed is a no-op now that body and title both live in the CRDT.
// We don't have a Rust-side Markdown→ProseMirror serializer to produce
// realistic seed bodies as Y.Doc updates, so seeding fresh demo content
// would require either an empty-body workaround or a heavyweight serializer.
// Instead, just clear any pre-existing seed and let the user start clean.
pub async fn run(
    pool: &SqlitePool,
    user_id: i64,
    app: &dyn myapps_core::registry::App,
) -> Result<()> {
    delete_user_app_data(pool, app, user_id).await?;
    tracing::info!("Notes seed: cleared existing notes (seed body is empty)");
    Ok(())
}
