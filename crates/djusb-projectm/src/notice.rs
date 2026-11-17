//! The LGPL notice and the written offer of projectM's source.

/// Lines for the licences screen. The last lines are the written offer.
#[must_use]
pub fn licence_lines() -> &'static [&'static str] {
    &[
        "projectM",
        "Copyright (C) the projectM Team.",
        "GNU Lesser General Public License, version 2.1.",
        "projectM is a shared library. You may replace the copy this player loads.",
        "Source: https://github.com/projectM-visualizer/projectm",
        "Written offer: for three years after you receive this player,",
        "ask the person who gave it to you for the projectM source.",
    ]
}

#[cfg(test)]
mod tests {
    use super::licence_lines;

    #[test]
    fn the_notice_names_the_licence_the_shared_library_and_the_offer() {
        let text = licence_lines().join(" ");
        assert!(text.contains("Lesser General Public License"));
        assert!(text.contains("shared library"));
        assert!(text.contains("three years"));
        assert!(text.contains("projectm"));
    }
}
