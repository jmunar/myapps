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

#[tokio::test]
async fn the_swipe_script_runs_before_main_is_parsed() {
    let app = harness::spawn_app().await;
    app.login_as("test", "pass").await;

    for path in EVERY_PAGE {
        let body = app.server.get(path).await.text();
        let head = body.split("</head>").next().unwrap_or("");
        // The script arms the slide-in of the page being swiped *to* by putting
        // `data-nav-swipe-in` on <html>. Moved back to the end of <body> that
        // still works, but only by luck of timing: <main> gets a chance to
        // paint at rest before the attribute lands, so the page appears, jumps
        // offscreen and slides back in.
        assert!(
            head.contains("addEventListener('touchstart'"),
            "{path}'s swipe script is not in the head"
        );
    }
}

#[tokio::test]
async fn the_swipe_entry_animation_is_wired_at_both_ends() {
    let app = harness::spawn_app().await;
    app.login_as("test", "pass").await;

    let script = app.server.get("/").await.text();
    let css = app.server.get("/static/core.css").await.text();

    // The direction crosses a page load as an attribute written by the script
    // and read only by CSS, so nothing fails loudly if one side is renamed —
    // the swipe simply stops animating. These are the two halves of that join.
    assert!(
        script.contains("data-nav-swipe-in"),
        "the script no longer arms the entry animation"
    );
    for dir in ["next", "prev"] {
        assert!(
            script.contains(&format!("'{dir}'")),
            "the script never writes the {dir} direction"
        );
        assert!(
            css.contains(&format!(r#"html[data-nav-swipe-in="{dir}"] main"#)),
            "core.css has no entry animation for the {dir} direction"
        );
        assert!(
            css.contains(&format!("@keyframes nav-swipe-in-{dir}")),
            "core.css is missing the nav-swipe-in-{dir} keyframes"
        );
    }
}

#[test]
fn every_swipe_tunable_is_declared_before_it_is_used() {
    let script = include_str!("../static/nav-swipe.js");

    // Nothing in CI parses this file: it is inlined into the page as a string,
    // so a misspelt name is not a build error but a ReferenceError thrown mid
    // gesture on a phone, which kills the swipe and nothing else. The tunables
    // are all SCREAMING_CASE and all declared in one block at the top, so the
    // cheap version of "does this script resolve" is to check that every such
    // name it mentions is one of the ones it declares.
    let declared: Vec<&str> = script
        .lines()
        .filter_map(|l| l.trim().strip_prefix("var "))
        .filter_map(|l| l.split(" =").next())
        .filter(|n| is_screaming_case(n))
        .collect();

    for name in screaming_case_words(script) {
        // `DOM` only ever appears in prose.
        if name == "DOM" {
            continue;
        }
        assert!(
            declared.contains(&name),
            "nav-swipe.js uses `{name}` but never declares it; \
             declared tunables are {declared:?}"
        );
    }
}

fn is_screaming_case(word: &str) -> bool {
    word.len() >= 3
        && word.starts_with(|c: char| c.is_ascii_uppercase())
        && word
            .chars()
            .all(|c| c.is_ascii_uppercase() || c == '_' || c.is_ascii_digit())
}

/// Every SCREAMING_CASE word in the source, comments included.
fn screaming_case_words(src: &str) -> Vec<&str> {
    src.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|w| is_screaming_case(w))
        .collect()
}
