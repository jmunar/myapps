//! Deterministic group colours.
//!
//! A group's colour is a pure function of its row id, so it is fixed the moment
//! the group is created and can never change — nothing is stored, and the user
//! is never offered a colour picker.
//!
//! The palette is the eight-slot categorical set validated for a light surface:
//! taken in this order it clears the CVD and normal-vision separation gates for
//! adjacent pairs, which is the pairlist that matters here (groups are listed,
//! and every swatch is rendered next to its name). Badges darken these values in
//! CSS to clear 3:1 text contrast — see `.label-badge` in `static/style.css`.

const PALETTE: [&str; 8] = [
    "#2a78d6", // blue
    "#eb6834", // orange
    "#1baf7a", // aqua
    "#eda100", // yellow
    "#e87ba4", // magenta
    "#008300", // green
    "#4a3aa7", // violet
    "#e34948", // red
];

/// The colour for a group, derived from its id. Stable for the life of the row.
pub fn group_color(group_id: i64) -> &'static str {
    let slot = group_id.rem_euclid(PALETTE.len() as i64) as usize;
    PALETTE[slot]
}

/// Colour for a label, i.e. the colour of the group it belongs to. Labels whose
/// group is momentarily missing fall back to neutral grey.
pub fn label_color(group_id: Option<i64>) -> &'static str {
    match group_id {
        Some(id) => group_color(id),
        None => "#6B6B6B",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_color_is_stable_and_in_palette() {
        for id in 1..50i64 {
            assert_eq!(group_color(id), group_color(id));
            assert!(PALETTE.contains(&group_color(id)));
        }
    }

    #[test]
    fn group_color_cycles_through_every_slot() {
        let used: std::collections::HashSet<&str> = (1..=8).map(group_color).collect();
        assert_eq!(used.len(), PALETTE.len());
    }

    #[test]
    fn missing_group_falls_back_to_grey() {
        assert_eq!(label_color(None), "#6B6B6B");
        assert_eq!(label_color(Some(3)), group_color(3));
    }
}
