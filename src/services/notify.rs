use crate::config::Config;

/// Send a notification message via Telegram. Logs errors but does not fail.
pub async fn send(config: &Config, message: &str) {
    let (Some(token), Some(chat_id)) = (&config.telegram_bot_token, &config.telegram_chat_id)
    else {
        tracing::debug!("Telegram not configured, skipping notification");
        return;
    };

    let url = format!("https://api.telegram.org/bot{token}/sendMessage");

    let result = reqwest::Client::new()
        .post(&url)
        .json(&serde_json::json!({
            "chat_id": chat_id,
            "text": message,
        }))
        .send()
        .await;

    match result {
        Ok(resp) if resp.status().is_success() => {
            tracing::debug!("Telegram notification sent");
        }
        Ok(resp) => {
            tracing::warn!("Telegram API returned {}", resp.status());
        }
        Err(e) => {
            tracing::warn!("Failed to send Telegram notification: {e}");
        }
    }
}
