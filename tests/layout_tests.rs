// ── The shared page shell ────────────────────────────────────
//
// Everything here is rendered by `myapps_core::layout::render_page`, so a
// regression is a platform-wide one: it takes a behaviour off every page of
// every app at once, which no single app's tests would notice.
//
// The swipe itself — the drag, the spring-back, the slide — happens on a phone
// under a finger and is invisible from here. What a test at this level can hold
// on to is the material the browser is handed: that the script reaches every
// page early enough to matter, that the markup it looks for is in the markup
// that is served, and that the names it shares with core.css still line up on
// both sides. Everything below is one of those three, and nothing below
// pretends to have seen the gesture run.

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

/// The same list without the launcher, whose nav is only the brand and Log out:
/// it has no tabs, so there is nothing there for a swipe to move between.
const EVERY_APP_PAGE: [&str; 6] = [
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
        // The drag moves `<main>` itself, and the script gives up on a page
        // without one. A page that renders its content into some other element
        // would still carry the script and still do nothing.
        assert!(
            body.contains("<main>"),
            "{path} has no <main> for the drag to move"
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
        let (head, _) = body
            .split_once("</head>")
            .unwrap_or_else(|| panic!("{path} has no <head> to look in"));
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
async fn every_app_page_marks_the_tab_the_swipe_starts_from() {
    let app = harness::spawn_app().await;
    app.login_as("test", "pass").await;

    for path in EVERY_APP_PAGE {
        let body = app.server.get(path).await.text();
        let stops = swipe_stops(&body);

        // Working out where a swipe goes starts with `nav a.active`, and none of
        // that lookup is defensive: a tab the layout stopped marking, or one
        // whose href no longer matches any nav link, leaves the current index at
        // -1 and the script drops the gesture on the floor. There is no error to
        // notice afterwards either, because a swipe that does nothing is
        // indistinguishable from a swipe nobody made.
        let active = active_href(&body).unwrap_or_else(|| panic!("{path} marks no nav tab active"));
        assert!(
            stops.contains(&active),
            "{path}'s active tab {active} is not one of the swipe stops {stops:?}"
        );

        // `brand` and `nav-right` are how the script tells a tab from the two
        // links that are not one. Rename either here and MyApps and Log out
        // become swipe destinations, which shifts every tab's index along with
        // it — so the failure is not a dead swipe but a swipe that lands one tab
        // off, in the app that is hardest to notice it in.
        let classes: Vec<String> = nav_links(&body).into_iter().map(|(_, c)| c).collect();
        assert!(
            classes.iter().any(|c| c == "brand"),
            "{path}'s home link is no longer marked `brand`, so the swipe counts it as a tab"
        );
        assert!(
            classes.iter().any(|c| c == "nav-right"),
            "{path}'s log out link is no longer marked `nav-right`, so the swipe counts it as a tab"
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

    // The script skips the animation for a reader who has asked for less motion,
    // but it cannot skip one that was armed before the setting changed: the
    // direction is already sitting in sessionStorage by then and the next page
    // load puts the attribute up regardless. This rule is what stops that stale
    // one from playing.
    let reduced = rule_body(&css, "@media (prefers-reduced-motion: reduce)")
        .expect("core.css no longer has a reduced-motion block");
    assert!(
        reduced.contains("html[data-nav-swipe-in] main") && reduced.contains("animation: none"),
        "core.css no longer suppresses a stale entry animation under reduced motion"
    );
    assert!(
        script.contains("prefers-reduced-motion: reduce"),
        "the script no longer asks whether motion is wanted"
    );
}

#[tokio::test]
async fn the_exit_animation_hands_over_to_the_entry_animation_unchanged() {
    let app = harness::spawn_app().await;
    app.login_as("test", "pass").await;

    let script = app.server.get("/").await.text();
    let css = app.server.get("/static/core.css").await.text();

    // A commit is two animations on two documents — an inline transition the
    // script writes on the way out, a keyframe animation core.css runs on the
    // way in — and it only reads as one continuous movement if they last the
    // same time. Nothing holds the two numbers together but a comment on each
    // side saying to keep them in step, and a page load in between where a
    // mismatch shows up as a hitch rather than as anything broken.
    let exit_ms = js_duration_ms(&script, "EXIT_MS");
    for dir in ["next", "prev"] {
        let entry_ms = css_animation_ms(&css, &format!("nav-swipe-in-{dir}"));
        assert!(
            (entry_ms - exit_ms).abs() < 1.0,
            "the exit lasts {exit_ms}ms but the {dir} entry lasts {entry_ms}ms"
        );
    }

    // Both sides also reach for the same easing token. A timing function that
    // does not resolve is not a linear animation, it is an invalid declaration
    // the browser throws away whole — so renaming the token in core.css would
    // take the entry animation with it.
    assert!(
        script.contains("var(--ease)"),
        "the drag no longer eases with the shared token"
    );
    assert!(
        css.contains("--ease:"),
        "core.css no longer defines the easing token the drag asks for"
    );
}

#[tokio::test]
async fn the_classes_the_drag_writes_are_styled_under_the_same_breakpoint() {
    let app = harness::spawn_app().await;
    app.login_as("test", "pass").await;

    let script = app.server.get("/").await.text();
    let css = app.server.get("/static/core.css").await.text();
    let phone = phone_blocks(&css);

    // The script refuses the gesture above 640px and the stylesheet only dresses
    // it below 640px. That is one decision spelled twice, in two languages, with
    // nothing keeping the two spellings together: move one and there is a band
    // of screen widths where a finger drags <main> around a page that has none
    // of the styling the drag assumes.
    assert!(
        script.contains("(max-width: 640px)"),
        "the script no longer limits the gesture to phone widths"
    );

    // The peek label is built by the script and styled only here, and the class
    // plus the two `data-side` values are the whole of that contract. Unstyled
    // it does not quietly disappear — it is a `position: static` div appended to
    // <body>, so the destination tab's name lands at the bottom of the page
    // mid-drag and the document grows to fit it.
    for selector in [
        ".nav-swipe-peek",
        r#".nav-swipe-peek[data-side="right"]"#,
        r#".nav-swipe-peek[data-side="left"]"#,
        ".nav-swipe-dragging",
        "html[data-nav-swipe-in=",
    ] {
        assert!(
            phone.contains(selector),
            "`{selector}` is not styled inside a max-width: 640px block"
        );
    }
    // Specifically that rule, and not only the two that place it against an
    // edge: taking the label out of the flow is the whole of what keeps it from
    // extending the page it is drawn over.
    let peek = rule_body(&phone, ".nav-swipe-peek").expect("the peek label has no rule of its own");
    assert!(
        peek.contains("position: fixed"),
        "the peek label is laid out in the flow, so it will lengthen the page mid-drag"
    );

    for name in [
        "'nav-swipe-peek'",
        "'nav-swipe-dragging'",
        "'right'",
        "'left'",
    ] {
        assert!(
            script.contains(name),
            "the script no longer writes {name}, which core.css still styles"
        );
    }
}

#[tokio::test]
async fn the_page_cannot_scroll_sideways_into_the_gap_a_drag_opens() {
    let app = harness::spawn_app().await;
    app.login_as("test", "pass").await;

    let css = app.server.get("/static/core.css").await.text();
    let phone = phone_blocks(&css);

    // A translated <main> still counts towards the document's scrollable
    // overflow, so while a finger holds the page half off screen the browser
    // would let the whole document chase it sideways — the drag would be
    // fighting a scrollbar it had just created. Nothing in the nav-swipe rules
    // prevents that; a plain `overflow-x: hidden` a few hundred lines above them
    // does, and it predates the gesture by years. That is exactly the kind of
    // rule someone tidies away, having no reason to connect the two.
    let rule = rule_body(&phone, "html, body")
        .expect("core.css no longer has an `html, body` rule for phone widths");
    assert!(
        rule.contains("overflow-x: hidden"),
        "phone widths no longer clip horizontal overflow, so a drag can scroll the document"
    );
}

#[test]
fn every_swipe_tunable_is_declared_before_it_is_used() {
    // Read the code without its prose. The comments in this file talk about the
    // DOM and about CSS, and a test that scanned them would be a test that fails
    // when someone writes a sentence.
    let script = without_comments(include_str!("../static/nav-swipe.js"));

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

    for name in screaming_case_words(&script) {
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

/// Every SCREAMING_CASE word in the source.
fn screaming_case_words(src: &str) -> Vec<&str> {
    src.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|w| is_screaming_case(w))
        .collect()
}

/// The source with `//` and `/* … */` comments taken out, each replaced by a
/// newline so the remaining lines still read as lines. Good enough for CSS and
/// for this one script, neither of which has a string literal containing a
/// comment marker.
fn without_comments(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut rest = src;
    loop {
        let (start, closer) = match (rest.find("//"), rest.find("/*")) {
            (Some(line), Some(block)) if line < block => (line, "\n"),
            (Some(_), Some(block)) => (block, "*/"),
            (Some(line), None) => (line, "\n"),
            (None, Some(block)) => (block, "*/"),
            (None, None) => {
                out.push_str(rest);
                return out;
            }
        };
        out.push_str(&rest[..start]);
        out.push('\n');
        rest = match rest[start..].find(closer) {
            Some(at) => &rest[start + at + closer.len()..],
            None => return out,
        };
    }
}

/// Everything inside the stylesheet's `@media (max-width: 640px)` blocks,
/// concatenated. Comments come out first, because one of them quotes a rule.
fn phone_blocks(css: &str) -> String {
    const QUERY: &str = "@media (max-width: 640px)";
    let css = without_comments(css);
    let mut out = String::new();
    let mut rest = css.as_str();
    while let Some(at) = rest.find(QUERY) {
        let after = &rest[at + QUERY.len()..];
        let Some(open) = after.find('{') else { break };
        let body = &after[open + 1..];
        let mut depth = 1usize;
        let mut end = body.len();
        for (i, c) in body.char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = i;
                        break;
                    }
                }
                _ => {}
            }
        }
        out.push_str(&body[..end]);
        rest = &body[end..];
    }
    out
}

/// The declarations of the first flat rule with this selector.
fn rule_body<'a>(css: &'a str, selector: &str) -> Option<&'a str> {
    let after = css.split_once(selector)?.1;
    let open = after.find('{')?;
    let close = after.find('}')?;
    (open < close).then(|| &after[open + 1..close])
}

/// A `var NAME = 220;` tunable, in milliseconds.
fn js_duration_ms(script: &str, name: &str) -> f64 {
    let after = script
        .split_once(&format!("var {name} = "))
        .unwrap_or_else(|| panic!("nav-swipe.js no longer declares {name}"))
        .1;
    after
        .split(|c: char| !(c.is_ascii_digit() || c == '.'))
        .next()
        .and_then(|n| n.parse().ok())
        .unwrap_or_else(|| panic!("{name} is not a plain number of milliseconds"))
}

/// The duration of the `animation:` shorthand that runs these keyframes, in
/// milliseconds, written either way round.
fn css_animation_ms(css: &str, keyframes: &str) -> f64 {
    let after = css
        .split_once(&format!("animation: {keyframes} "))
        .unwrap_or_else(|| panic!("core.css no longer runs the {keyframes} animation"))
        .1;
    let token = after.split_whitespace().next().unwrap_or_default();
    if let Some(ms) = token.strip_suffix("ms") {
        ms.parse()
            .unwrap_or_else(|_| panic!("{keyframes} has an unreadable duration {token}"))
    } else {
        let s: f64 = token
            .trim_end_matches('s')
            .parse()
            .unwrap_or_else(|_| panic!("{keyframes} has an unreadable duration {token}"));
        s * 1000.0
    }
}

/// Every `<a>` in the page's nav, as (href, class). The swipe reads the nav as
/// the browser hands it over, so this reads it the same way.
fn nav_links(html: &str) -> Vec<(String, String)> {
    let nav = html
        .split_once("<nav>")
        .and_then(|(_, rest)| rest.split_once("</nav>"))
        .map(|(inner, _)| inner)
        .expect("the page has no <nav>");
    nav.split("<a ")
        .skip(1)
        .map(|tag| {
            let tag = tag.split_once('>').expect("unterminated <a>").0;
            (attr(tag, "href"), attr(tag, "class"))
        })
        .collect()
}

fn attr(tag: &str, name: &str) -> String {
    match tag.split_once(&format!("{name}=\"")) {
        Some((_, rest)) => rest.split('"').next().unwrap_or_default().to_string(),
        None => String::new(),
    }
}

/// The tabs a swipe can land on, worked out the way `tabs()` in nav-swipe.js
/// works them out: the brand and the right-aligned links are not tabs, and an
/// app that lists one destination twice (its own name, then its first tab) has
/// one tab there, not two. Only the served half of that agreement is under test
/// here — this is a copy of the script's rule, not the script's rule.
fn swipe_stops(html: &str) -> Vec<String> {
    let mut stops: Vec<String> = Vec::new();
    for (href, class) in nav_links(html) {
        let classes: Vec<&str> = class.split_whitespace().collect();
        if classes.contains(&"brand") || classes.contains(&"nav-right") {
            continue;
        }
        if href.is_empty() || stops.contains(&href) {
            continue;
        }
        stops.push(href);
    }
    stops
}

/// The href of the nav link marked active, which is where the script starts
/// counting from.
fn active_href(html: &str) -> Option<String> {
    nav_links(html)
        .into_iter()
        .find(|(_, class)| class.split_whitespace().any(|c| c == "active"))
        .map(|(href, _)| href)
}
