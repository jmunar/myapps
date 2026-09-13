//! Retention sweep and orphan collection.
//!
//! Two jobs, both idempotent:
//!
//! 1. Delete files past their `expires_at`.
//! 2. Reclaim bytes with no metadata row. Rows disappear without the files
//!    being touched in several places — `registry::delete_user_app_data` is
//!    pure SQL, and `delete-user` relies on `ON DELETE CASCADE` — and a crashed
//!    upload leaves a `.part` behind. Scanning disk against the table catches
//!    all of those without needing a hook in every deletion path.

use std::collections::HashSet;
use std::path::Path;
use std::time::{Duration, SystemTime};

use anyhow::Result;
use sqlx::SqlitePool;

use crate::storage;

/// How often the in-process sweep runs. The daily system cron is the primary
/// trigger, but it is only installed when `DEPLOY_CRON_ENABLED=true`, and a
/// clipboard that never expires anything fills the disk the database lives on.
const SWEEP_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

/// Grace period before an abandoned `.part` file is collected. Long enough that
/// a slow upload in flight is never deleted out from under itself.
const PART_GRACE: Duration = Duration::from_secs(24 * 60 * 60);

pub async fn sweep(pool: &SqlitePool, base_dir: &str) -> Result<()> {
    let expired = delete_expired(pool, base_dir).await?;
    let orphans = collect_orphans(pool, base_dir).await?;
    if expired > 0 || orphans > 0 {
        tracing::info!("FileClipboard: swept {expired} expired file(s), {orphans} orphan(s)");
    }
    Ok(())
}

/// Delete every file past its expiry, bytes first so a failure mid-way leaves
/// an orphan (which the next pass reclaims) rather than a row pointing at
/// nothing.
async fn delete_expired(pool: &SqlitePool, base_dir: &str) -> Result<usize> {
    let rows: Vec<(i64, i64, String)> = sqlx::query_as(
        "SELECT id, user_id, stored_name FROM file_clipboard_files
         WHERE expires_at <= datetime('now')",
    )
    .fetch_all(pool)
    .await?;

    for (id, user_id, stored) in &rows {
        storage::remove(base_dir, *user_id, stored).await;
        sqlx::query("DELETE FROM file_clipboard_files WHERE id = ?")
            .bind(id)
            .execute(pool)
            .await?;
    }

    Ok(rows.len())
}

/// Remove files on disk with no matching row, and stale partial uploads.
async fn collect_orphans(pool: &SqlitePool, base_dir: &str) -> Result<usize> {
    let base = Path::new(base_dir);
    if !base.exists() {
        return Ok(0);
    }

    let known: HashSet<String> = sqlx::query_scalar("SELECT stored_name FROM file_clipboard_files")
        .fetch_all(pool)
        .await?
        .into_iter()
        .collect();

    let mut removed = 0;
    let mut user_dirs = tokio::fs::read_dir(base).await?;

    while let Some(user_dir) = user_dirs.next_entry().await? {
        if !user_dir
            .file_type()
            .await
            .map(|t| t.is_dir())
            .unwrap_or(false)
        {
            continue;
        }

        let mut files = tokio::fs::read_dir(user_dir.path()).await?;
        let mut left_behind = 0;
        while let Some(entry) = files.next_entry().await? {
            let name = entry.file_name().to_string_lossy().into_owned();

            let stale = if let Some(stem) = name.strip_suffix(".part") {
                // Only collect a partial upload once it cannot still be in
                // flight, and never one whose final file has since been recorded.
                !known.contains(stem) && older_than(&entry, PART_GRACE).await
            } else {
                !known.contains(&name)
            };

            if !stale {
                left_behind += 1;
                continue;
            }

            match tokio::fs::remove_file(entry.path()).await {
                Ok(()) => {
                    tracing::info!("FileClipboard: reclaimed orphan {}", entry.path().display());
                    removed += 1;
                }
                Err(e) => {
                    left_behind += 1;
                    tracing::warn!(
                        "FileClipboard: could not remove orphan {}: {e}",
                        entry.path().display()
                    );
                }
            }
        }

        // A user whose files are all gone (deleted account, expired clipboard)
        // leaves an empty directory behind.
        if left_behind == 0 {
            let _ = tokio::fs::remove_dir(user_dir.path()).await;
        }
    }

    Ok(removed)
}

/// True when the entry was last modified longer ago than `age`. Unknown
/// timestamps count as fresh, so an unreadable mtime never causes a deletion.
async fn older_than(entry: &tokio::fs::DirEntry, age: Duration) -> bool {
    let Ok(meta) = entry.metadata().await else {
        return false;
    };
    let Ok(modified) = meta.modified() else {
        return false;
    };
    SystemTime::now()
        .duration_since(modified)
        .map(|elapsed| elapsed > age)
        .unwrap_or(false)
}

/// Background sweeper started by `on_serve`.
pub fn spawn(pool: SqlitePool, base_dir: String) {
    tokio::spawn(async move {
        loop {
            if let Err(e) = sweep(&pool, &base_dir).await {
                tracing::error!("FileClipboard: retention sweep failed: {e:#}");
            }
            tokio::time::sleep(SWEEP_INTERVAL).await;
        }
    });
}
