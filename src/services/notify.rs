use crate::config::Config;

/// Send a notification message via ntfy. Logs errors but does not fail.
pub async fn send(config: &Config, message: &str) {
    let Some(topic) = &config.ntfy_topic else {
        tracing::debug!("ntfy not configured, skipping notification");
        return;
    };

    let base = config
        .ntfy_url
        .as_deref()
        .unwrap_or("https://ntfy.sh");

    let url = format!("{base}/{topic}");

    let result = reqwest::Client::new()
        .post(&url)
        .header("Title", "MyApps")
        .body(message.to_string())
        .send()
        .await;

    match result {
        Ok(resp) if resp.status().is_success() => {
            tracing::debug!("ntfy notification sent");
        }
        Ok(resp) => {
            tracing::warn!("ntfy returned {}", resp.status());
        }
        Err(e) => {
            tracing::warn!("Failed to send ntfy notification: {e}");
        }
    }
}
