use anyhow::{Context, Result};
use chrono::{Duration, Utc};
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::time::Instant;

use crate::config::Config;

const API_BASE: &str = "https://api.enablebanking.com";

// ── JWT auth ──────────────────────────────────────────────────────

fn make_jwt(config: &Config) -> Result<String> {
    let (app_id, key_path, _) = config.require_enable_banking()?;

    let pem = std::fs::read(key_path)
        .with_context(|| format!("failed to read private key: {key_path}"))?;
    let key = EncodingKey::from_rsa_pem(&pem).context("invalid RSA private key")?;

    let now = Utc::now().timestamp();
    let claims = JwtClaims {
        iss: "enablebanking.com".into(),
        aud: "api.enablebanking.com".into(),
        iat: now,
        exp: now + 3600,
    };

    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(app_id.to_string());

    jsonwebtoken::encode(&header, &claims, &key).context("failed to sign JWT")
}

#[derive(Serialize)]
struct JwtClaims {
    iss: String,
    aud: String,
    iat: i64,
    exp: i64,
}

fn client(config: &Config) -> Result<reqwest::Client> {
    let token = make_jwt(config)?;
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::AUTHORIZATION,
        format!("Bearer {token}").parse().unwrap(),
    );
    reqwest::Client::builder()
        .default_headers(headers)
        .build()
        .context("failed to build HTTP client")
}

// ── API types ─────────────────────────────────────────────────────

#[derive(Serialize)]
struct AuthRequest {
    access: AuthAccess,
    aspsp: AuthAspsp,
    state: String,
    redirect_url: String,
    psu_type: String,
}

#[derive(Serialize)]
struct AuthAccess {
    valid_until: String,
    balances: bool,
    transactions: bool,
}

#[derive(Serialize)]
struct AuthAspsp {
    name: String,
    country: String,
}

#[derive(Deserialize)]
pub struct AuthResponse {
    pub url: String,
}

#[derive(Serialize)]
struct SessionRequest {
    code: String,
}

#[derive(Deserialize)]
pub struct SessionResponse {
    pub session_id: String,
    pub accounts: Vec<SessionAccount>,
    pub access: SessionAccess,
}

#[derive(Deserialize)]
pub struct SessionAccount {
    pub uid: String,
    pub account_id: Option<AccountId>,
}

#[derive(Deserialize)]
pub struct AccountId {
    pub iban: Option<String>,
}

#[derive(Deserialize)]
pub struct SessionAccess {
    pub valid_until: String,
}

#[derive(Deserialize)]
pub struct TransactionsResponse {
    pub transactions: Vec<BankTransaction>,
    pub continuation_key: Option<String>,
}

#[derive(Deserialize)]
pub struct BankTransaction {
    pub transaction_id: Option<String>,
    pub entry_reference: Option<String>,
    pub booking_date: Option<String>,
    pub value_date: Option<String>,
    pub transaction_amount: TransactionAmount,
    pub credit_debit_indicator: Option<String>,
    pub creditor: Option<Party>,
    pub debtor: Option<Party>,
    pub remittance_information: Option<Vec<String>>,
    pub balance_after_transaction: Option<BalanceAmount>,
}

impl BankTransaction {
    pub fn external_id(&self) -> String {
        self.transaction_id
            .as_deref()
            .or(self.entry_reference.as_deref())
            .unwrap_or("unknown")
            .to_string()
    }

    pub fn date(&self) -> String {
        self.booking_date
            .as_deref()
            .or(self.value_date.as_deref())
            .unwrap_or("unknown")
            .to_string()
    }

    pub fn description(&self) -> String {
        self.remittance_information
            .as_ref()
            .and_then(|v| v.first())
            .cloned()
            .unwrap_or_default()
    }

    pub fn counterparty(&self) -> Option<String> {
        self.creditor
            .as_ref()
            .or(self.debtor.as_ref())
            .and_then(|p| p.name.clone())
    }
}

#[derive(Deserialize)]
pub struct TransactionAmount {
    pub currency: String,
    pub amount: String,
}

#[derive(Deserialize)]
pub struct Party {
    pub name: Option<String>,
}

#[derive(Deserialize)]
pub struct BalanceAmount {
    pub amount: Option<String>,
}

#[derive(Deserialize)]
pub struct BalancesResponse {
    pub balances: Vec<BankBalance>,
}

#[derive(Deserialize)]
pub struct BankBalance {
    pub balance_amount: BalanceAmountFull,
    pub balance_type: String,
    #[serde(default)]
    pub reference_date: Option<String>,
}

#[derive(Deserialize)]
pub struct BalanceAmountFull {
    pub amount: String,
    pub currency: String,
}

/// Pick the most useful balance by type priority.
pub fn pick_best_balance(balances: &[BankBalance]) -> Option<&BankBalance> {
    const PRIORITY: &[&str] = &["ITAV", "CLAV", "XPCD", "ITBD", "CLBD"];
    for prio in PRIORITY {
        if let Some(b) = balances.iter().find(|b| b.balance_type == *prio) {
            return Some(b);
        }
    }
    balances.first()
}

// ── Payload logging ──────────────────────────────────────────────

async fn save_payload(
    pool: &SqlitePool,
    account_id: Option<i64>,
    method: &str,
    endpoint: &str,
    request_body: Option<&str>,
    response_body: &str,
    status_code: u16,
    duration_ms: u64,
) {
    if let Err(e) = sqlx::query(
        r#"INSERT INTO api_payloads (account_id, method, endpoint, request_body, response_body, status_code, duration_ms)
           VALUES (?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(account_id)
    .bind(method)
    .bind(endpoint)
    .bind(request_body)
    .bind(response_body)
    .bind(status_code as i32)
    .bind(duration_ms as i64)
    .execute(pool)
    .await
    {
        tracing::warn!("Failed to save API payload: {e:#}");
    }
}

// ── API calls ─────────────────────────────────────────────────────

/// Start bank authorization. Returns the URL to redirect the user to.
pub async fn start_auth(
    pool: &SqlitePool,
    config: &Config,
    bank_name: &str,
    country: &str,
    state: &str,
    valid_days: i64,
) -> Result<AuthResponse> {
    let (_, _, redirect_uri) = config.require_enable_banking()?;
    let valid_until = (Utc::now() + Duration::days(valid_days))
        .format("%Y-%m-%dT00:00:00Z")
        .to_string();

    let body = AuthRequest {
        access: AuthAccess {
            valid_until,
            balances: true,
            transactions: true,
        },
        aspsp: AuthAspsp {
            name: bank_name.to_string(),
            country: country.to_string(),
        },
        state: state.to_string(),
        redirect_url: redirect_uri,
        psu_type: "personal".to_string(),
    };

    let request_json = serde_json::to_string(&body).unwrap_or_default();
    let start = Instant::now();

    let resp = client(config)?
        .post(format!("{API_BASE}/auth"))
        .json(&body)
        .send()
        .await
        .context("failed to send auth request")?;

    let status_code = resp.status().as_u16();
    let text = resp.text().await.context("failed to read auth response")?;
    let duration_ms = start.elapsed().as_millis() as u64;

    save_payload(pool, None, "POST", "/auth", Some(&request_json), &text, status_code, duration_ms).await;

    if status_code >= 400 {
        anyhow::bail!("Enable Banking auth failed ({status_code}): {text}");
    }

    serde_json::from_str(&text).context("failed to parse auth response")
}

/// Exchange authorization code for a session with account list.
pub async fn create_session(pool: &SqlitePool, config: &Config, code: &str) -> Result<SessionResponse> {
    let request_body = SessionRequest {
        code: code.to_string(),
    };
    let request_json = serde_json::to_string(&request_body).unwrap_or_default();
    let start = Instant::now();

    let resp = client(config)?
        .post(format!("{API_BASE}/sessions"))
        .json(&request_body)
        .send()
        .await
        .context("failed to send session request")?;

    let status_code = resp.status().as_u16();
    let text = resp.text().await.context("failed to read session response")?;
    let duration_ms = start.elapsed().as_millis() as u64;

    save_payload(pool, None, "POST", "/sessions", Some(&request_json), &text, status_code, duration_ms).await;

    if status_code >= 400 {
        anyhow::bail!("Enable Banking session failed ({status_code}): {text}");
    }

    serde_json::from_str(&text).context("failed to parse session response")
}

/// Fetch transactions for an account. Handles pagination automatically.
pub async fn get_transactions(
    pool: &SqlitePool,
    config: &Config,
    account_uid: &str,
    date_from: &str,
    account_id: Option<i64>,
) -> Result<Vec<BankTransaction>> {
    let http = client(config)?;
    let mut all = Vec::new();
    let mut continuation_key: Option<String> = None;

    let mut page = 0u32;

    loop {
        page += 1;

        let url = format!("{API_BASE}/accounts/{account_uid}/transactions");
        let req = match &continuation_key {
            Some(key) => http.get(&url).query(&[("continuation_key", key.as_str())]),
            None => http.get(&url).query(&[("date_from", date_from)]),
        };

        let start = Instant::now();

        let raw = req
            .send()
            .await
            .context("failed to send transactions request")?;

        let status_code = raw.status().as_u16();
        let text = raw.text().await.context("failed to read transactions response")?;
        let duration_ms = start.elapsed().as_millis() as u64;

        save_payload(pool, account_id, "GET", "/accounts/{uid}/transactions", None, &text, status_code, duration_ms).await;

        if status_code >= 400 {
            anyhow::bail!("Enable Banking transactions failed ({status_code}): {text}");
        }

        let resp: TransactionsResponse = serde_json::from_str(&text)
            .context("failed to parse transactions response")?;

        tracing::info!(
            "Transactions page {page}: {} items (total so far: {})",
            resp.transactions.len(),
            all.len() + resp.transactions.len(),
        );

        all.extend(resp.transactions);

        match resp.continuation_key {
            Some(key) if !key.is_empty() => continuation_key = Some(key),
            _ => break,
        }
    }

    Ok(all)
}

/// Fetch balances for an account.
pub async fn get_balances(pool: &SqlitePool, config: &Config, account_uid: &str, account_id: Option<i64>) -> Result<Vec<BankBalance>> {
    let start = Instant::now();

    let resp = client(config)?
        .get(format!("{API_BASE}/accounts/{account_uid}/balances"))
        .send()
        .await
        .context("failed to send balances request")?;

    let status_code = resp.status().as_u16();
    let text = resp.text().await.context("failed to read balances response")?;
    let duration_ms = start.elapsed().as_millis() as u64;

    save_payload(pool, account_id, "GET", "/accounts/{uid}/balances", None, &text, status_code, duration_ms).await;

    if status_code >= 400 {
        anyhow::bail!("Enable Banking balances failed ({status_code}): {text}");
    }

    let data: BalancesResponse = serde_json::from_str(&text).context("failed to parse balances response")?;
    Ok(data.balances)
}
