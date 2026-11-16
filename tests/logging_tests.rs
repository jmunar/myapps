//! The per-request log line.
//!
//! It has to say which route ran and who ran it, and it must not carry the
//! values behind that route: a note id, a search term, a token. Those are the
//! parts that would turn a log file into a copy of user data.

#[allow(dead_code)]
mod harness;

use std::fmt;
use std::sync::{Arc, Mutex};

use tracing::field::{Field, Visit};
use tracing_subscriber::layer::{Context, Layer, SubscriberExt};

/// Collects the `message` field of every event, and nothing else.
///
/// That is exactly what journald stores as `MESSAGE` and therefore what grafito
/// renders in its log list. A value that only lives on the span or in an event
/// field would pass a `fmt`-formatted test and still be invisible in grafito,
/// so this test looks at the same thing the production log viewer does.
#[derive(Clone, Default)]
struct MessageBuffer(Arc<Mutex<Vec<String>>>);

impl<S: tracing::Subscriber> Layer<S> for MessageBuffer {
    fn on_event(&self, event: &tracing::Event<'_>, _context: Context<'_, S>) {
        struct MessageVisitor<'a>(&'a mut Vec<String>);

        impl Visit for MessageVisitor<'_> {
            fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
                if field.name() == "message" {
                    self.0.push(format!("{value:?}"));
                }
            }
        }

        event.record(&mut MessageVisitor(&mut self.0.lock().unwrap()));
    }
}

impl MessageBuffer {
    fn contents(&self) -> String {
        self.0.lock().unwrap().join("\n")
    }
}

#[tokio::test]
async fn request_log_names_route_template_and_user_id() {
    let app = harness::spawn_app().await;
    app.login_as("test", "pass").await;

    let messages = MessageBuffer::default();
    // `set_default` is thread-local and `#[tokio::test]` uses a current-thread
    // runtime, so this captures the request without touching the global
    // subscriber that tests running in parallel might use.
    let subscriber = tracing_subscriber::registry().with(messages.clone());

    {
        let _guard = tracing::subscriber::set_default(subscriber);
        // A core route (matched in the top-level router) and an app route
        // (matched one level deeper, inside its own nested router): both have
        // to resolve to their template, and neither may keep its query.
        app.server.get("/launcher/edit?lang=s3cret-lang").await;
        app.server
            .get("/file_clipboard/files/987654/download?token=s3cret")
            .expect_failure()
            .await;
    }

    let log = messages.contents();
    for template in [
        "method=GET path=/launcher/edit",
        "path=/file_clipboard/files/{file_id}/download",
    ] {
        assert!(
            log.contains(template),
            "route template {template:?} missing from log:\n{log}"
        );
    }
    assert!(
        log.contains("user_id=1"),
        "authenticated user missing from log:\n{log}"
    );
    for leaked in [
        "/files/987654/",
        "token=s3cret",
        "lang=s3cret-lang",
        "username",
    ] {
        assert!(!log.contains(leaked), "{leaked:?} leaked into log:\n{log}");
    }
}
