use sqlx::SqlitePool;
use web_push::{
    ContentEncoding, IsahcWebPushClient, SubscriptionInfo, VapidSignatureBuilder,
    WebPushClient, WebPushMessageBuilder,
};

use crate::config::Config;

/// Send a push notification to all subscribed browsers. Logs errors but does not fail.
pub async fn send(pool: &SqlitePool, config: &Config, title: &str, body: &str) {
    let Some(private_key) = config.vapid_private_key.as_deref() else {
        tracing::debug!("VAPID not configured, skipping push notification");
        return;
    };

    let subs: Vec<Subscription> = match sqlx::query_as(
        "SELECT endpoint, p256dh, auth FROM push_subscriptions",
    )
    .fetch_all(pool)
    .await
    {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("Failed to query push subscriptions: {e}");
            return;
        }
    };

    if subs.is_empty() {
        tracing::debug!("No push subscriptions, skipping notification");
        return;
    }

    let partial_builder = match VapidSignatureBuilder::from_base64_no_sub(private_key) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!("Failed to parse VAPID private key: {e}");
            return;
        }
    };

    let client = match IsahcWebPushClient::new() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("Failed to create push client: {e}");
            return;
        }
    };

    let payload = serde_json::json!({ "title": title, "body": body }).to_string();

    let subject = config
        .vapid_subject
        .as_deref()
        .unwrap_or("mailto:noreply@example.com");

    for sub in &subs {
        let info = SubscriptionInfo::new(&sub.endpoint, &sub.p256dh, &sub.auth);

        let mut sig_builder = partial_builder.clone().add_sub_info(&info);
        sig_builder.add_claim("sub", subject);

        let sig = match sig_builder.build() {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("Failed to build VAPID signature: {e}");
                continue;
            }
        };

        let mut msg_builder = WebPushMessageBuilder::new(&info);
        msg_builder.set_payload(ContentEncoding::Aes128Gcm, payload.as_bytes());
        msg_builder.set_vapid_signature(sig);

        let message = match msg_builder.build() {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!("Failed to build push message: {e}");
                continue;
            }
        };

        match client.send(message).await {
            Ok(()) => {
                tracing::debug!("Push notification sent to {}", sub.endpoint);
            }
            Err(e) => {
                let err_str = format!("{e}");
                let is_gone = err_str.contains("410")
                    || err_str.contains("404")
                    || err_str.contains("Gone")
                    || err_str.contains("Not Found");
                if is_gone {
                    tracing::info!("Removing stale subscription: {}", sub.endpoint);
                    let _ = sqlx::query("DELETE FROM push_subscriptions WHERE endpoint = ?")
                        .bind(&sub.endpoint)
                        .execute(pool)
                        .await;
                } else {
                    tracing::warn!("Push notification failed for {}: {e}", sub.endpoint);
                }
            }
        }
    }
}

#[derive(sqlx::FromRow)]
struct Subscription {
    endpoint: String,
    p256dh: String,
    auth: String,
}
