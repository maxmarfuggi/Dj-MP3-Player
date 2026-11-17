//! Colours, type, and spacing shared with the desktop.
//!
//! The values live in `docs/design-tokens.json`. Building this crate writes
//! the Rust constants, and the device tests check that
//! `apps/desktop/src/renderer/src/tokens.css` is the CSS for the same file.

#[allow(dead_code, reason = "later screens use tokens this skeleton does not")]
mod generated {
    include!(concat!(env!("OUT_DIR"), "/design_tokens.rs"));
}

pub use generated::{color, font, space};

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use iced::Color;

    use super::{color, font, space};
    use crate::token_gen;

    #[test]
    fn desktop_css_matches_the_tokens_file() {
        let json = include_str!("../../../docs/design-tokens.json");
        let css = token_gen::css(json).unwrap();
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../apps/desktop/src/renderer/src/tokens.css");
        let on_disk = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("reading {}: {error}", path.display()));
        assert_eq!(
            on_disk.replace("\r\n", "\n"),
            css,
            "apps/desktop/src/renderer/src/tokens.css is generated from docs/design-tokens.json"
        );
    }

    #[test]
    fn rust_constants_match_the_tokens_file() {
        let json = include_str!("../../../docs/design-tokens.json");
        let rust = token_gen::rust_constants(json).unwrap();
        let path = PathBuf::from(env!("OUT_DIR")).join("design_tokens.rs");
        let generated = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("reading {}: {error}", path.display()));
        assert_eq!(generated.replace("\r\n", "\n"), rust);
    }

    #[test]
    fn shared_palette_matches_the_desktop() {
        assert_eq!(color::BG, Color::from_rgb8(0x0d, 0x0e, 0x12));
        assert_eq!(color::ACCENT, Color::from_rgb8(0x5c, 0x9d, 0xff));
        assert_eq!(color::HOVER, Color::from_rgba8(255, 255, 255, 0.055));
        assert_eq!(color::ERROR, Color::from_rgb8(0xff, 0x80, 0x80));
        assert_eq!(
            font::FAMILY,
            "-apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, sans-serif"
        );
        assert_eq!(font::size::BASE, 13.0);
        assert_eq!(font::size::WALL, 36.0);
        assert_eq!(font::size::WALL_DETAIL, 26.0);
        assert_eq!(space::SM, 8.0);
        assert_eq!(space::LG, 16.0);
        assert_eq!(space::XXL, 48.0);
        assert_eq!(space::ROW, 6.0);
        assert_eq!(space::INDENT, 28.0);
    }
}
