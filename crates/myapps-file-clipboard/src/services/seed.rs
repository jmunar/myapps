//! Seed data for a freshly seeded user.

use anyhow::Result;
use sqlx::SqlitePool;

use myapps_core::registry::delete_user_app_data;

/// Clear any previous clipboard data for the user.
///
/// Deliberately inserts nothing: a demo row would have to point at a real file,
/// and `seed` has no `Config` in hand to learn where files are kept — resolving
/// the directory a second way is how the two sources of truth drift apart. An
/// empty clipboard is also the honest starting state for this app.
///
/// This clears rows only. The bytes those rows referenced become orphans, which
/// `services::retention` reclaims on its next pass.
pub async fn run(
    pool: &SqlitePool,
    user_id: i64,
    app: &dyn myapps_core::registry::App,
) -> Result<()> {
    delete_user_app_data(pool, app, user_id).await?;
    Ok(())
}
