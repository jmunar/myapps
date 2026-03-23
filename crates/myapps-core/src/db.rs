use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::borrow::Cow;
use std::str::FromStr;

/// Build a migrator that merges core migrations with extra app migrations.
pub fn migrator(extra: &[sqlx::migrate::Migrator]) -> sqlx::migrate::Migrator {
    let core = sqlx::migrate!();

    let mut all: Vec<_> = core.migrations.into_owned();
    for m in extra {
        all.extend(m.migrations.iter().cloned());
    }
    all.sort_by_key(|m| m.version);

    sqlx::migrate::Migrator {
        migrations: Cow::Owned(all),
        ..sqlx::migrate::Migrator::DEFAULT
    }
}

pub async fn init(
    database_url: &str,
    extra_migrators: &[sqlx::migrate::Migrator],
) -> Result<SqlitePool, sqlx::Error> {
    let options = SqliteConnectOptions::from_str(database_url)?
        .create_if_missing(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;

    migrator(extra_migrators).run(&pool).await?;

    Ok(pool)
}
