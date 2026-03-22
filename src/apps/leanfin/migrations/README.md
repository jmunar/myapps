# LeanFin Database Schema

### leanfin_accounts

| Column             | Type    | Notes                                     |
|--------------------|---------|-------------------------------------------|
| id                 | INTEGER | PK, autoincrement                         |
| user_id            | INTEGER | FK → users                                |
| bank_name          | TEXT    | Bank name (or account name for manual)    |
| bank_country       | TEXT    | ISO 3166-1 alpha-2 (empty for manual)     |
| iban               | TEXT    | Nullable                                  |
| session_id         | TEXT    | Enable Banking session ID (placeholder for manual) |
| account_uid        | TEXT    | Enable Banking account UID, UNIQUE (generated UUID for manual) |
| balance_amount     | REAL    | Nullable, latest balance                  |
| balance_currency   | TEXT    | Nullable, ISO 4217 currency               |
| session_expires_at | TEXT    | ISO 8601, when consent expires            |
| account_type       | TEXT    | 'bank' or 'manual', default 'bank'       |
| account_name       | TEXT    | Nullable, display name for manual accounts |
| asset_category     | TEXT    | Nullable, e.g. investment, real_estate, vehicle, loan, crypto |
| archived           | INTEGER | 0 or 1, default 0. Archived accounts are read-only |
| created_at         | TEXT    | ISO 8601                                  |

### leanfin_user_settings

| Column                | Type    | Notes                                          |
|-----------------------|---------|-------------------------------------------------|
| user_id               | INTEGER | PK, FK → users, ON DELETE CASCADE               |
| enable_banking_app_id | TEXT    | Nullable, Enable Banking application ID         |
| enable_banking_key    | BLOB    | Nullable, AES-256-GCM encrypted RSA private key (nonce prepended) |
| updated_at            | TEXT    | ISO 8601                                        |

### leanfin_pending_links

| Column             | Type    | Notes                                          |
|--------------------|---------|------------------------------------------------|
| state              | TEXT    | PK, CSRF token for OAuth callback              |
| user_id            | INTEGER | FK → users                                     |
| bank_name          | TEXT    | Bank being linked                              |
| country            | TEXT    | Country code                                   |
| reauth_account_id  | INTEGER | Nullable, FK → accounts (set for re-auth flow) |
| created_at         | TEXT    | ISO 8601                                       |

### leanfin_transactions

| Column          | Type    | Notes                              |
|-----------------|---------|------------------------------------|
| id              | INTEGER | PK, autoincrement                  |
| account_id      | INTEGER | FK → accounts                      |
| external_id     | TEXT    | Transaction ID from Enable Banking |
| date            | TEXT    | Booking date, ISO 8601             |
| amount          | REAL    | Signed (negative = debit)          |
| currency        | TEXT    | ISO 4217 (EUR, USD, etc.)          |
| description     | TEXT    | From remittance information        |
| counterparty    | TEXT    | Nullable                           |
| balance_after   | REAL    | Nullable                           |
| created_at      | TEXT    | When we first stored it            |
| snapshot_id     | INTEGER | Nullable FK → balance_snapshots (ON DELETE SET NULL) |
| UNIQUE(external_id, account_id) |  | Deduplication constraint  |

### leanfin_labels

| Column | Type    | Notes             |
|--------|---------|-------------------|
| id     | INTEGER | PK, autoincrement |
| user_id| INTEGER | FK → users        |
| name   | TEXT    | NOT NULL          |
| color  | TEXT    | Hex color, e.g. #4CAF50 |
| UNIQUE(user_id, name) | | |

### leanfin_label_rules

| Column    | Type    | Notes                                  |
|-----------|---------|----------------------------------------|
| id        | INTEGER | PK, autoincrement                      |
| label_id  | INTEGER | FK → labels                            |
| field     | TEXT    | 'description', 'counterparty', 'amount_range' |
| pattern   | TEXT    | Keyword for text fields; "min,max" for amount_range |
| priority  | INTEGER | Higher wins on conflict, default 0     |

### leanfin_balance_snapshots

| Column       | Type    | Notes                                          |
|--------------|---------|------------------------------------------------|
| id           | INTEGER | PK, autoincrement                              |
| account_id   | INTEGER | FK → accounts                                  |
| timestamp    | TEXT    | Full ISO 8601 datetime of the snapshot         |
| date         | TEXT    | Date portion (YYYY-MM-DD), redundant for indexing |
| balance      | REAL    | Balance at this point in time                  |
| balance_type | TEXT    | ITAV, CLAV, XPCD, ITBD, CLBD, or MANUAL       |
| created_at   | TEXT    | ISO 8601                                       |
| UNIQUE(account_id, balance_type, timestamp) | | |

### leanfin_api_payloads

| Column        | Type     | Notes                                          |
|---------------|----------|------------------------------------------------|
| id            | INTEGER  | PK, autoincrement                              |
| account_id    | INTEGER  | Nullable, FK → accounts (ON DELETE SET NULL)   |
| provider      | TEXT     | NOT NULL, default 'enable_banking'             |
| method        | TEXT     | NOT NULL, 'GET' or 'POST'                      |
| endpoint      | TEXT     | NOT NULL, e.g. '/accounts/{uid}/transactions'  |
| request_body  | TEXT     | Nullable, JSON string (NULL for GET requests)  |
| response_body | TEXT     | Nullable, raw JSON response                    |
| status_code   | INTEGER  | NOT NULL, HTTP status code                     |
| duration_ms   | INTEGER  | NOT NULL, round-trip time in milliseconds      |
| created_at    | DATETIME | NOT NULL, default now                          |

Indexes: `account_id`, `created_at`.

### leanfin_transaction_labels

| Column         | Type    | Notes                      |
|----------------|---------|----------------------------|
| transaction_id | INTEGER | FK → transactions          |
| label_id       | INTEGER | FK → labels                |
| source         | TEXT    | 'auto' or 'manual'        |
| PRIMARY KEY (transaction_id, label_id) | | |
