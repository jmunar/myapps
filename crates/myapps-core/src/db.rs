use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::borrow::Cow;
use std::ffi::CStr;
use std::os::raw::{c_char, c_int, c_void};
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
        .max_connections(3)
        .connect_with(options)
        .await?;

    migrator(extra_migrators).run(&pool).await?;

    Ok(pool)
}

// ---------------------------------------------------------------------------
// Per-app scoped pools with SQLite authorizer
// ---------------------------------------------------------------------------

// SQLite action codes (from sqlite3.h)
const SQLITE_CREATE_INDEX: c_int = 1;
const SQLITE_CREATE_TABLE: c_int = 2;
const SQLITE_CREATE_TEMP_INDEX: c_int = 3;
const SQLITE_CREATE_TEMP_TABLE: c_int = 4;
const SQLITE_DELETE: c_int = 9;
const SQLITE_DROP_INDEX: c_int = 10;
const SQLITE_DROP_TABLE: c_int = 11;
const SQLITE_DROP_TEMP_INDEX: c_int = 12;
const SQLITE_DROP_TEMP_TABLE: c_int = 13;
const SQLITE_INSERT: c_int = 18;
const SQLITE_READ: c_int = 20;
const SQLITE_SELECT: c_int = 21;
const SQLITE_TRANSACTION: c_int = 22;
const SQLITE_UPDATE: c_int = 23;
const SQLITE_ATTACH: c_int = 24;
const SQLITE_DETACH: c_int = 25;
const SQLITE_ALTER_TABLE: c_int = 26;
const SQLITE_REINDEX: c_int = 27;
const SQLITE_ANALYZE: c_int = 28;
const SQLITE_FUNCTION: c_int = 31;
const SQLITE_SAVEPOINT: c_int = 32;
const SQLITE_RECURSIVE: c_int = 33;

const SQLITE_OK_CODE: c_int = 0;
const SQLITE_DENY_CODE: c_int = 1;

/// Authorizer context: the app's own prefix and the set of other apps' prefixes
/// whose tables are off-limits for reads.
struct AuthorizerCtx {
    /// e.g. `"leanfin_"`
    own_prefix: String,
    /// e.g. `["mindflow_", "voice_to_text_", "classroom_input_"]`
    other_prefixes: Vec<String>,
}

impl AuthorizerCtx {
    /// Returns `true` if `table` belongs to another app.
    fn is_other_app_table(&self, table: &str) -> bool {
        self.other_prefixes
            .iter()
            .any(|p| table.starts_with(p.as_str()))
    }
}

/// SQLite authorizer callback.
///
/// Policy:
/// - **Write** (INSERT/UPDATE/DELETE): only own-prefix tables.
/// - **Read**: own-prefix tables + core tables (no app prefix). Denies reads
///   on other apps' tables. Core tables are readable because foreign-key
///   constraint checks (e.g. `user_id FK -> users.id`) require it.
/// - **DDL**: only own-prefix tables.
/// - **ATTACH/DETACH**: denied (prevent accessing other database files).
///
/// # Safety
///
/// Called by SQLite's C runtime. `user_data` must be a valid
/// `*const AuthorizerCtx` produced by `Box::into_raw`.
unsafe extern "C" fn authorizer_callback(
    user_data: *mut c_void,
    action_code: c_int,
    arg3: *const c_char, // table name for table ops, or index/trigger name
    _arg4: *const c_char,
    _arg5: *const c_char,
    _arg6: *const c_char,
) -> c_int {
    let ctx = unsafe { &*(user_data as *const AuthorizerCtx) };

    match action_code {
        // Write operations — only own prefix + sqlite internals
        SQLITE_INSERT | SQLITE_UPDATE | SQLITE_DELETE => {
            if arg3.is_null() {
                return SQLITE_OK_CODE;
            }
            let table = unsafe { CStr::from_ptr(arg3) }.to_str().unwrap_or("");
            if table.starts_with(ctx.own_prefix.as_str()) || table.starts_with("sqlite_") {
                SQLITE_OK_CODE
            } else {
                SQLITE_DENY_CODE
            }
        }

        // Read — own tables + core tables; deny other apps' tables
        SQLITE_READ => {
            if arg3.is_null() {
                return SQLITE_OK_CODE;
            }
            let table = unsafe { CStr::from_ptr(arg3) }.to_str().unwrap_or("");
            if table.starts_with(ctx.own_prefix.as_str())
                || table.starts_with("sqlite_")
                || !ctx.is_other_app_table(table)
            {
                SQLITE_OK_CODE
            } else {
                SQLITE_DENY_CODE
            }
        }

        // DDL — only allow for own prefix
        SQLITE_CREATE_TABLE | SQLITE_DROP_TABLE | SQLITE_ALTER_TABLE | SQLITE_CREATE_INDEX
        | SQLITE_DROP_INDEX => {
            if arg3.is_null() {
                return SQLITE_DENY_CODE;
            }
            let name = unsafe { CStr::from_ptr(arg3) }.to_str().unwrap_or("");
            if name.starts_with(ctx.own_prefix.as_str()) {
                SQLITE_OK_CODE
            } else {
                SQLITE_DENY_CODE
            }
        }

        // Always allow: bare SELECT, transactions, functions, savepoints, etc.
        SQLITE_SELECT
        | SQLITE_TRANSACTION
        | SQLITE_FUNCTION
        | SQLITE_SAVEPOINT
        | SQLITE_ANALYZE
        | SQLITE_REINDEX
        | SQLITE_CREATE_TEMP_TABLE
        | SQLITE_DROP_TEMP_TABLE
        | SQLITE_CREATE_TEMP_INDEX
        | SQLITE_DROP_TEMP_INDEX
        | SQLITE_RECURSIVE => SQLITE_OK_CODE,

        // Deny attaching other databases
        SQLITE_ATTACH | SQLITE_DETACH => SQLITE_DENY_CODE,

        // Deny anything unknown
        _ => SQLITE_DENY_CODE,
    }
}

/// Install a table-prefix authorizer on a raw SQLite connection.
async fn install_authorizer(
    conn: &mut sqlx::sqlite::SqliteConnection,
    ctx: AuthorizerCtx,
) -> Result<(), sqlx::Error> {
    let mut handle = conn.lock_handle().await?;
    let raw = handle.as_raw_handle().as_ptr();

    // Leak the context so it lives as long as the connection. One small struct
    // per pooled connection (~8 total) — negligible.
    let ctx_ptr = Box::into_raw(Box::new(ctx));

    unsafe {
        libsqlite3_sys::sqlite3_set_authorizer(
            raw,
            Some(authorizer_callback),
            ctx_ptr as *mut c_void,
        );
    }

    Ok(())
}

/// Create a connection pool whose connections can only access tables whose name
/// starts with `allowed_prefix` followed by `_`. Reads on core tables (those
/// not owned by any app) are allowed for foreign-key checks. Used to sandbox
/// each app.
///
/// `other_prefixes` lists every other app's key (e.g. `["mindflow",
/// "voice_to_text"]`) so the authorizer can deny reads on their tables.
pub async fn init_scoped(
    database_url: &str,
    allowed_prefix: &str,
    other_prefixes: &[&str],
) -> Result<SqlitePool, sqlx::Error> {
    let own = format!("{allowed_prefix}_");
    let others: Vec<String> = other_prefixes.iter().map(|p| format!("{p}_")).collect();

    let options = SqliteConnectOptions::from_str(database_url)?
        .create_if_missing(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .after_connect({
            move |conn, _meta| {
                let ctx = AuthorizerCtx {
                    own_prefix: own.clone(),
                    other_prefixes: others.clone(),
                };
                Box::pin(async move { install_authorizer(conn, ctx).await })
            }
        })
        .connect_with(options)
        .await?;

    Ok(pool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::Row;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static TEST_DB_ID: AtomicUsize = AtomicUsize::new(0);

    async fn test_pools(prefix: &str, others: &[&str]) -> (SqlitePool, SqlitePool) {
        let id = TEST_DB_ID.fetch_add(1, Ordering::SeqCst);
        let url = format!("sqlite:file:auth_test_{id}?mode=memory&cache=shared");

        // Unrestricted pool for setup
        let core = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                SqliteConnectOptions::from_str(&url)
                    .unwrap()
                    .create_if_missing(true),
            )
            .await
            .unwrap();

        let scoped = init_scoped(&url, prefix, others).await.unwrap();
        (core, scoped)
    }

    #[tokio::test]
    async fn scoped_pool_allows_own_prefix() {
        let (core, scoped) = test_pools("myapp", &["other"]).await;

        sqlx::query("CREATE TABLE myapp_items (id INTEGER PRIMARY KEY, name TEXT)")
            .execute(&core)
            .await
            .unwrap();

        sqlx::query("INSERT INTO myapp_items (name) VALUES ('hello')")
            .execute(&scoped)
            .await
            .unwrap();

        let row = sqlx::query("SELECT name FROM myapp_items")
            .fetch_one(&scoped)
            .await
            .unwrap();
        assert_eq!(row.get::<String, _>("name"), "hello");
    }

    #[tokio::test]
    async fn scoped_pool_denies_read_on_other_app() {
        let (core, scoped) = test_pools("appA", &["appB"]).await;

        sqlx::query("CREATE TABLE appB_secrets (id INTEGER PRIMARY KEY, data TEXT)")
            .execute(&core)
            .await
            .unwrap();

        let result = sqlx::query("SELECT * FROM appB_secrets")
            .fetch_all(&scoped)
            .await;
        assert!(result.is_err(), "should deny read access to appB_ tables");
    }

    #[tokio::test]
    async fn scoped_pool_denies_write_on_other_app() {
        let (core, scoped) = test_pools("appA", &["appB"]).await;

        sqlx::query("CREATE TABLE appB_data (id INTEGER PRIMARY KEY, val TEXT)")
            .execute(&core)
            .await
            .unwrap();

        let result = sqlx::query("INSERT INTO appB_data (val) VALUES ('hack')")
            .execute(&scoped)
            .await;
        assert!(result.is_err(), "should deny write to appB_ tables");
    }

    #[tokio::test]
    async fn scoped_pool_allows_read_on_core_tables() {
        let (core, scoped) = test_pools("myapp", &["other"]).await;

        sqlx::query("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)")
            .execute(&core)
            .await
            .unwrap();
        sqlx::query("INSERT INTO users (name) VALUES ('alice')")
            .execute(&core)
            .await
            .unwrap();

        // Read is allowed (needed for FK constraint checks)
        let row = sqlx::query("SELECT name FROM users")
            .fetch_one(&scoped)
            .await
            .unwrap();
        assert_eq!(row.get::<String, _>("name"), "alice");
    }

    #[tokio::test]
    async fn scoped_pool_denies_write_on_core_tables() {
        let (core, scoped) = test_pools("myapp", &["other"]).await;

        sqlx::query("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)")
            .execute(&core)
            .await
            .unwrap();

        let result = sqlx::query("INSERT INTO users (name) VALUES ('evil')")
            .execute(&scoped)
            .await;
        assert!(result.is_err(), "should deny write to core tables");
    }

    #[tokio::test]
    async fn scoped_pool_allows_bare_select() {
        let (_core, scoped) = test_pools("myapp", &[]).await;

        let row = sqlx::query("SELECT 1 AS val")
            .fetch_one(&scoped)
            .await
            .unwrap();
        assert_eq!(row.get::<i32, _>("val"), 1);
    }

    #[tokio::test]
    async fn scoped_pool_denies_ddl_on_other_prefix() {
        let (_core, scoped) = test_pools("myapp", &["other"]).await;

        let result = sqlx::query("CREATE TABLE other_stuff (id INTEGER PRIMARY KEY)")
            .execute(&scoped)
            .await;
        assert!(
            result.is_err(),
            "should deny creating tables with wrong prefix"
        );
    }
}
