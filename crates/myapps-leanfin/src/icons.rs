//! Inline SVG icons.
//!
//! Account and label rows used to spell their actions out in words, which on a
//! phone pushed the row past the viewport. These are drawn at the current text
//! colour so `.btn-icon` styling still applies; every call site pairs one with
//! an `aria-label` and a `title`, so the action keeps its name for screen
//! readers and on hover.

/// Open padlock — re-authorize a bank consent.
pub const UNLOCK: &str = r#"<svg class="leanfin-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" focusable="false"><rect x="3" y="11" width="18" height="11" rx="2"/><path d="M7 11V7a5 5 0 0 1 9.9-1"/></svg>"#;

/// Archive box.
pub const ARCHIVE: &str = r#"<svg class="leanfin-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" focusable="false"><rect x="1" y="3" width="22" height="5" rx="1"/><path d="M21 8v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8"/><path d="M10 12h4"/></svg>"#;

/// Archive box with an arrow out of it — restore an archived account.
pub const UNARCHIVE: &str = r#"<svg class="leanfin-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" focusable="false"><rect x="1" y="3" width="22" height="5" rx="1"/><path d="M21 8v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8"/><path d="M12 18v-6"/><path d="M9 15l3-3 3 3"/></svg>"#;

/// Waste bin — delete.
pub const TRASH: &str = r#"<svg class="leanfin-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" focusable="false"><path d="M3 6h18"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/><path d="M10 11v6"/><path d="M14 11v6"/></svg>"#;
