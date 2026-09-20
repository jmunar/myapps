//! What may leave the host.
//!
//! A snapshot exists so a sandbox can debug against real data, so the content
//! stays: transactions, descriptions, counterparties, notes, thoughts, form
//! inputs. What is removed is everything that is a *credential* or a
//! capability — a session someone could replay, an invite that still mints a
//! user, a push endpoint that still reaches a real phone, the Enable Banking
//! key, and the raw provider payloads, which carry both.
//!
//! Every table in the snapshot must appear in exactly one of the three lists
//! below. A table in none of them fails the request by name rather than
//! passing through, because the failure this guards against is not a wrong
//! rule — it is a table added next year that nobody thought about, quietly
//! carrying its contents into a VM. `myapps` gains tables regularly and
//! nothing else here would notice.
//!
//! Absent tables are fine: prod may be older than this file, or an app may not
//! be deployed. Only *unclassified* ones are an error.

use anyhow::{Result, bail};

/// Dropped entirely. Each is a live credential or a record of one.
const DELETE: &[&str] = &[
    // Replayable: a row here is a logged-in browser.
    "sessions",
    // Still mints a user on the real deployment.
    "invites",
    // Endpoint + p256dh + auth still reaches a real phone.
    "push_subscriptions",
    // Carries the OAuth `state` of a bank link in flight.
    "leanfin_pending_links",
    // The worst of the lot: verbatim Enable Banking requests and responses,
    // so bearer tokens and full account data in text columns no schema
    // describes. Nothing can be scrubbed column-wise here.
    "leanfin_api_payloads",
];

/// Kept, with named columns blanked. The rows matter (a user has to exist for
/// anything to render); these columns do not.
const REWRITE: &[(&str, &str)] = &[
    // The Enable Banking private key, encrypted at rest with the *prod*
    // ENCRYPTION_KEY — which the guest does not have, so it could not decrypt
    // it anyway. Dropped regardless: a ciphertext that leaves the host is a
    // ciphertext someone can work on offline.
    (
        "leanfin_user_settings",
        "UPDATE leanfin_user_settings SET enable_banking_key = NULL, enable_banking_app_id = NULL",
    ),
    // `session_id` is the provider's live consent; the IBAN identifies a real
    // account at a real bank. Bank name, balances and type all stay, so the
    // account list still renders truthfully.
    (
        "leanfin_accounts",
        "UPDATE leanfin_accounts SET iban = NULL, session_id = ''",
    ),
];

/// Passed through whole. Listed rather than defaulted: the point of the check
/// is that adding a table forces a decision here.
const KEEP: &[&str] = &[
    "users", // rewritten separately — the hash is computed at run time
    "user_settings",
    "user_app_visibility",
    "push_notifications",
    "leanfin_transactions",
    "leanfin_balance_snapshots",
    "leanfin_labels",
    "leanfin_label_rules",
    "leanfin_label_groups",
    "leanfin_allocations",
    "mindflow_thoughts",
    "mindflow_categories",
    "mindflow_comments",
    "mindflow_actions",
    "notes_notes",
    "notes_notes_new",
    "notes_note_updates",
    "form_input_form_types",
    "form_input_inputs",
    "form_input_row_sets",
    // Metadata only; the bytes live under FILE_CLIPBOARD_DIR and are not in a
    // snapshot. Kept so the list page has something to render — the downloads
    // 404, which is the honest result.
    "voice_to_text_jobs",
    "file_clipboard_files",
    "file_clipboard_settings",
    // sqlx's own ledger. Dropping it would make the guest re-run every
    // migration against an already-migrated database.
    "_sqlx_migrations",
    // SQLite's, not ours.
    "sqlite_sequence",
];

fn classified(table: &str) -> bool {
    DELETE.contains(&table)
        || KEEP.contains(&table)
        || REWRITE.iter().any(|(name, _)| *name == table)
}

/// SQLite string literal: the only character that needs escaping is `'`.
fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

/// The scrub script for one snapshot, or an error naming the tables nobody has
/// classified yet.
///
/// `tables` is what the snapshot actually contains, read back from its own
/// `sqlite_master` rather than assumed from this repository's migrations —
/// prod is what it is, not what `main` says it should be.
pub fn script(tables: &[String], password_hash: &str) -> Result<String> {
    let unknown: Vec<&str> = tables
        .iter()
        .map(String::as_str)
        .filter(|table| !table.starts_with("sqlite_") && !classified(table))
        .collect();
    if !unknown.is_empty() {
        bail!(
            "refusing to release a snapshot: {} is not classified in scrub.rs. \
             Add it to DELETE, REWRITE or KEEP — a table nobody has looked at \
             does not leave the host.",
            unknown.join(", ")
        );
    }

    let present = |table: &str| tables.iter().any(|name| name == table);
    let mut sql = String::from(
        "PRAGMA foreign_keys = OFF;\n\
         BEGIN;\n",
    );

    for table in DELETE {
        if present(table) {
            sql.push_str(&format!("DELETE FROM {table};\n"));
        }
    }
    for (table, statement) in REWRITE {
        if present(table) {
            sql.push_str(statement);
            sql.push_str(";\n");
        }
    }
    // Every account gets the same known password, so the snapshot is a
    // database you can log into. The hash is real Argon2 over that password,
    // not a placeholder: a value the app cannot verify would leave the guest
    // with a login screen it can never pass.
    if present("users") {
        sql.push_str(&format!(
            "UPDATE users SET password_hash = {};\n",
            quote(password_hash)
        ));
    }

    sql.push_str("COMMIT;\n");
    // Rebuilt from a dump and then emptied of several tables, so it is worth
    // the one pass: it also drops the freed pages rather than shipping them.
    sql.push_str("VACUUM;\n");
    Ok(sql)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tables(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn an_unclassified_table_is_refused_by_name() {
        let err = script(&tables(&["users", "leanfin_secrets_v2"]), "hash").unwrap_err();
        assert!(err.to_string().contains("leanfin_secrets_v2"), "{err}");
    }

    #[test]
    fn sqlite_internals_are_not_unclassified() {
        assert!(script(&tables(&["sqlite_stat1", "users"]), "hash").is_ok());
    }

    #[test]
    fn absent_tables_are_skipped_rather_than_failing() {
        let sql = script(&tables(&["users"]), "hash").unwrap();
        assert!(!sql.contains("sessions"));
        assert!(sql.contains("UPDATE users SET password_hash = 'hash'"));
    }

    #[test]
    fn every_credential_table_present_is_emptied() {
        let sql = script(&tables(DELETE), "hash").unwrap();
        for table in DELETE {
            assert!(sql.contains(&format!("DELETE FROM {table};")), "{table}");
        }
    }

    #[test]
    fn a_hash_containing_a_quote_cannot_end_the_literal() {
        let sql = script(&tables(&["users"]), "it's").unwrap();
        assert!(sql.contains("'it''s'"), "{sql}");
    }
}
