//! The per-request log line.
//!
//! It has to say which route ran and who ran it, and it must not carry the
//! values behind that route: a note id, a search term, a token. Those are the
//! parts that would turn a log file into a copy of user data.

#[allow(dead_code)]
mod harness;

use std::io::Write;
use std::sync::{Arc, Mutex};

/// An `io::Write` sink shared with the test, so the formatted log can be read
/// back once the request has finished.
#[derive(Clone, Default)]
struct LogBuffer(Arc<Mutex<Vec<u8>>>);

impl Write for LogBuffer {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl LogBuffer {
    fn contents(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap()).into_owned()
    }
}

#[tokio::test]
async fn request_log_names_route_template_and_user_id() {
    let app = harness::spawn_app().await;
    app.login_as("test", "pass").await;

    let log = LogBuffer::default();
    let writer = log.clone();
    // `set_default` is thread-local and `#[tokio::test]` uses a current-thread
    // runtime, so this captures the request without touching the global
    // subscriber that tests running in parallel might use.
    let subscriber = tracing_subscriber::fmt()
        .with_writer(move || writer.clone())
        .with_max_level(tracing::Level::INFO)
        .with_ansi(false)
        .finish();

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

    let log = log.contents();
    for template in [
        "path=/launcher/edit",
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
