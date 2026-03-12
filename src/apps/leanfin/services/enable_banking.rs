use anyhow::{Context, Result};
use chrono::{Duration, Utc};
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use serde::{Deserialize, Serialize};

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

// ── API calls ─────────────────────────────────────────────────────

/// Start bank authorization. Returns the URL to redirect the user to.
pub async fn start_auth(
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

    let resp = client(config)?
        .post(format!("{API_BASE}/auth"))
        .json(&body)
        .send()
        .await
        .context("failed to send auth request")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("Enable Banking auth failed ({status}): {body}");
    }

    resp.json()
        .await
        .context("failed to parse auth response")
}

/// Exchange authorization code for a session with account list.
pub async fn create_session(config: &Config, code: &str) -> Result<SessionResponse> {
    let resp = client(config)?
        .post(format!("{API_BASE}/sessions"))
        .json(&SessionRequest {
            code: code.to_string(),
        })
        .send()
        .await
        .context("failed to send session request")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("Enable Banking session failed ({status}): {body}");
    }

    resp.json()
        .await
        .context("failed to parse session response")
}

/// Fetch transactions for an account. Handles pagination automatically.
pub async fn get_transactions(
    config: &Config,
    account_uid: &str,
    date_from: &str,
) -> Result<Vec<BankTransaction>> {
    let http = client(config)?;
    let mut all = Vec::new();
    let mut continuation_key: Option<String> = None;

    loop {
        let mut req = http
            .get(format!("{API_BASE}/accounts/{account_uid}/transactions"))
            .query(&[("date_from", date_from)]);

        if let Some(key) = &continuation_key {
            req = req.query(&[("continuation_key", key.as_str())]);
        }

        let raw = req
            .send()
            .await
            .context("failed to send transactions request")?;

        if !raw.status().is_success() {
            let status = raw.status();
            let body = raw.text().await.unwrap_or_default();
            anyhow::bail!("Enable Banking transactions failed ({status}): {body}");
        }

        let resp: TransactionsResponse = raw.json().await?;

        all.extend(resp.transactions);

        match resp.continuation_key {
            Some(key) if !key.is_empty() => continuation_key = Some(key),
            _ => break,
        }
    }

    Ok(all)
}
