// ── The shared page shell ────────────────────────────────────
//
// Everything here is rendered by `myapps_core::layout::render_page`, so a
// regression is a platform-wide one: it takes a behaviour off every page of
// every app at once, which no single app's tests would notice.

// Only a couple of the harness's spawners are needed here.
#[allow(dead_code)]
mod harness;

/// The landing page of each registered app, plus the launcher.
const EVERY_PAGE: [&str; 7] = [
    "/",
    "/leanfin",
    "/mindflow",
    "/voice",
    "/forms",
    "/notes",
    "/file_clipboard",
];

#[tokio::test]
async fn every_page_carries_the_swipe_script() {
    let app = harness::spawn_app().await;
    app.login_as("test", "pass").await;

    for path in EVERY_PAGE {
        let body = app.server.get(path).await.text();
        // The gesture is the only way to change tab one-handed on a phone, and
        // the script is inlined by the layout — so a page that stops going
        // through `render_page` loses it silently.
        assert!(
            body.contains("addEventListener('touchstart'"),
            "{path} has no swipe handler"
        );
        assert!(
            body.contains("nav a.active"),
            "{path}'s swipe handler cannot tell which tab it is on"
        );
    }
}

#[tokio::test]
async fn the_swipe_script_is_inlined_rather_than_fetched() {
    let app = harness::spawn_app().await;
    app.login_as("test", "pass").await;

    let body = app.server.get("/").await.text();
    // Inlined on purpose: a separate request would let the first swipe of a
    // cold page do nothing. A `src=` here means someone split it back out.
    assert!(
        !body.contains("nav-swipe.js"),
        "the script is being fetched"
    );
    assert_eq!(
        body.matches("addEventListener('touchstart'").count(),
        1,
        "the script is inlined more than once"
    );
}
