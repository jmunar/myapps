//! Rows from the Hugging Face datasets-server JSON API.

use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use myapps_challenges::dataset::Dataset;

const ROWS_URL: &str = "https://datasets-server.huggingface.co/rows";
/// The most the datasets-server returns per request.
pub const PAGE: usize = 100;
const ATTEMPTS: u32 = 6;
/// The datasets-server answers 429 after roughly fifty back-to-back requests,
/// so pace them. A full fetch is a one-off; a few minutes is fine.
const PAUSE: Duration = Duration::from_millis(1500);

#[derive(Deserialize)]
pub struct RowsPage {
    pub rows: Vec<RowEntry>,
    pub num_rows_total: usize,
}

#[derive(Deserialize)]
pub struct RowEntry {
    pub row_idx: usize,
    pub row: serde_json::Value,
    #[serde(default)]
    pub truncated_cells: Vec<String>,
}

pub fn client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .user_agent(concat!(
            "myapps-challenges-prep/",
            env!("CARGO_PKG_VERSION")
        ))
        .timeout(Duration::from_secs(60))
        .build()?)
}

pub async fn page(
    client: &reqwest::Client,
    dataset: Dataset,
    config: &str,
    split: &str,
    offset: usize,
) -> Result<RowsPage> {
    let context = || format!("{config}/{split} @ {offset}");
    let mut backoff = Duration::from_secs(5);
    for attempt in 1..=ATTEMPTS {
        tokio::time::sleep(PAUSE).await;
        let response = client
            .get(ROWS_URL)
            .query(&[
                ("dataset", dataset.hf_id()),
                ("config", config),
                ("split", split),
                ("offset", &offset.to_string()),
                ("length", &PAGE.to_string()),
            ])
            .send()
            .await;
        let retry_in = match response {
            Ok(r) if r.status().is_success() => {
                return r
                    .json()
                    .await
                    .with_context(|| format!("{}: unexpected response", context()));
            }
            Ok(r)
                if r.status() == reqwest::StatusCode::TOO_MANY_REQUESTS
                    || r.status().is_server_error() =>
            {
                let retry_after = r
                    .headers()
                    .get(reqwest::header::RETRY_AFTER)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse().ok())
                    .map(Duration::from_secs);
                tracing::warn!("{}: HTTP {}, retrying", context(), r.status());
                retry_after.unwrap_or(backoff)
            }
            Ok(r) => bail!("{}: HTTP {}", context(), r.status()),
            Err(e) if attempt < ATTEMPTS => {
                tracing::warn!("{}: {e}, retrying", context());
                backoff
            }
            Err(e) => return Err(e).with_context(context),
        };
        tokio::time::sleep(retry_in).await;
        backoff *= 2;
    }
    bail!("{}: still failing after {ATTEMPTS} attempts", context())
}
