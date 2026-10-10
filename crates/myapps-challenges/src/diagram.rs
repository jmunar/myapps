//! Asymptote diagrams in problem text: `[asy]…[/asy]` blocks, as the
//! Hendrycks MATH dataset inherited them from AoPS.
//!
//! A browser cannot draw Asymptote, so `myapps-challenges-prep` renders each
//! block to SVG and ships it in the bundle, named by the hash of its source.
//! The page swaps each block for that SVG. Both sides find the blocks and hash
//! them through this module, so they cannot disagree on either.

use sha2::{Digest, Sha256};

const OPEN: &str = "[asy]";
const CLOSE: &str = "[/asy]";

#[derive(Debug, PartialEq)]
pub enum Segment<'a> {
    Text(&'a str),
    /// The Asymptote source between the tags, and its hash.
    Diagram {
        source: &'a str,
        hash: String,
    },
}

/// The hash a diagram is stored and served under: SHA-256 of its source, as
/// lowercase hex.
pub fn hash(source: &str) -> String {
    let digest = Sha256::digest(source.trim().as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// `text` cut into plain text and diagrams, in order. An `[asy]` with no
/// closing tag is left as text.
pub fn split(text: &str) -> Vec<Segment<'_>> {
    let mut segments = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find(OPEN) {
        let body = &rest[start + OPEN.len()..];
        let Some(end) = body.find(CLOSE) else { break };
        if start > 0 {
            segments.push(Segment::Text(&rest[..start]));
        }
        let source = &body[..end];
        segments.push(Segment::Diagram {
            source,
            hash: hash(source),
        });
        rest = &body[end + CLOSE.len()..];
    }
    if !rest.is_empty() {
        segments.push(Segment::Text(rest));
    }
    segments
}

/// The hash of every diagram in `text`, in order.
pub fn hashes(text: &str) -> Vec<String> {
    split(text)
        .into_iter()
        .filter_map(|s| match s {
            Segment::Diagram { hash, .. } => Some(hash),
            Segment::Text(_) => None,
        })
        .collect()
}

/// Whether `s` is shaped like a hash this module produces, before it gets
/// anywhere near a query.
pub fn is_hash(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Whether `svg` looks like a plain drawing. Diagrams are only ever shown
/// through `<img>`, where no script runs, and served with a sandboxing CSP for
/// anyone opening one directly; this keeps anything active out of the
/// catalogue in the first place. TeX can emit raw SVG through dvisvgm
/// specials, so a hostile `label` could otherwise put markup here.
pub fn is_safe_svg(svg: &str) -> bool {
    let lower = svg.to_ascii_lowercase();
    let starts_right = {
        let head = lower.trim_start();
        head.starts_with("<?xml") || head.starts_with("<svg")
    };
    let has_handler = lower.match_indices("on").any(|(i, _)| {
        let before = lower[..i].chars().next_back();
        let after = &lower[i + 2..];
        let name_len = after.bytes().take_while(u8::is_ascii_alphabetic).count();
        matches!(before, Some(c) if c.is_ascii_whitespace())
            && name_len > 0
            && after[name_len..].trim_start().starts_with('=')
    });
    starts_right
        && !has_handler
        && ![
            "<script",
            "foreignobject",
            "javascript:",
            "<iframe",
            "<!entity",
        ]
        .iter()
        .any(|bad| lower.contains(bad))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_text_and_diagrams_in_order() {
        let text = "Find $x$. [asy]draw((0,0)--(1,1));[/asy] Then [asy] dot((0,0)); [/asy]";
        let segments = split(text);
        assert_eq!(segments.len(), 4);
        assert_eq!(segments[0], Segment::Text("Find $x$. "));
        assert!(matches!(
            segments[1],
            Segment::Diagram {
                source: "draw((0,0)--(1,1));",
                ..
            }
        ));
        assert_eq!(segments[2], Segment::Text(" Then "));
        assert!(matches!(
            segments[3],
            Segment::Diagram {
                source: " dot((0,0)); ",
                ..
            }
        ));
    }

    #[test]
    fn an_unclosed_block_stays_text() {
        assert_eq!(split("a [asy]draw"), [Segment::Text("a [asy]draw")]);
        assert_eq!(split(""), []);
    }

    #[test]
    fn the_hash_ignores_surrounding_whitespace_only() {
        assert_eq!(hash("draw(A);"), hash("\n draw(A);\n"));
        assert_ne!(hash("draw(A);"), hash("draw(B);"));
        assert!(is_hash(&hash("draw(A);")));
        assert_eq!(
            hashes("[asy]a[/asy] and [asy]b[/asy]"),
            [hash("a"), hash("b")]
        );
    }

    #[test]
    fn only_plain_drawings_are_safe() {
        let ok = r#"<?xml version='1.0'?><svg xmlns="http://www.w3.org/2000/svg"><path d="M0 0" stroke-width="1"/><text font-family="cmr10">ion</text></svg>"#;
        assert!(is_safe_svg(ok));
        for bad in [
            r#"<svg><script>alert(1)</script></svg>"#,
            r#"<svg><g onload="alert(1)"/></svg>"#,
            r#"<svg><g ONCLICK = "x"/></svg>"#,
            r#"<svg><foreignObject><div/></foreignObject></svg>"#,
            r#"<svg><a href="javascript:x"/></svg>"#,
            "not svg",
        ] {
            assert!(!is_safe_svg(bad), "{bad}");
        }
    }

    #[test]
    fn is_hash_refuses_anything_else() {
        assert!(!is_hash("../../etc/passwd"));
        assert!(!is_hash(&"A".repeat(64)));
        assert!(!is_hash(&"a".repeat(63)));
    }
}
