//! Turns `docs/design-tokens.json` into CSS variables and Rust constants.
//!
//! The desktop stylesheet and the wall player both read this one file, so a
//! colour, a font size, or a space cannot change on one side only.

use serde_json::{Map, Value};

struct Tokens {
    colors: Vec<ColorToken>,
    family: String,
    sizes: Vec<(String, u32)>,
    weights: Vec<(String, u32)>,
    tracking: Vec<(String, String)>,
    spaces: Vec<(String, u32)>,
}

struct ColorToken {
    key: String,
    #[cfg_attr(
        not(test),
        allow(
            dead_code,
            reason = "the build script emits Rust constants; the tests compare this CSS text"
        )
    )]
    css: String,
    channels: Channels,
}

enum Channels {
    Opaque { r: u8, g: u8, b: u8 },
    Alpha { r: u8, g: u8, b: u8, alpha: String },
}

/// CSS custom properties for the desktop stylesheet.
#[cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "the build script emits Rust constants; the tests compare this CSS"
    )
)]
pub fn css(json: &str) -> Result<String, String> {
    let tokens = parse(json)?;
    let mut out =
        String::from("/* Generated from docs/design-tokens.json. Do not edit. */\n:root {\n");
    for color in &tokens.colors {
        let key = &color.key;
        let value = &color.css;
        out.push_str(&format!("  --{key}: {value};\n"));
    }
    let family = &tokens.family;
    out.push_str(&format!("  --font-family: {family};\n"));
    for (key, value) in &tokens.sizes {
        out.push_str(&format!("  --font-size-{key}: {value}px;\n"));
    }
    for (key, value) in &tokens.weights {
        out.push_str(&format!("  --font-weight-{key}: {value};\n"));
    }
    for (key, value) in &tokens.tracking {
        out.push_str(&format!("  --font-tracking-{key}: {value};\n"));
    }
    for (key, value) in &tokens.spaces {
        out.push_str(&format!("  --space-{key}: {value}px;\n"));
    }
    out.push_str("}\n");
    Ok(out)
}

/// Rust constants for the wall player. `iced::Color` is named so the device
/// crate can include the text unchanged.
pub fn rust_constants(json: &str) -> Result<String, String> {
    let tokens = parse(json)?;
    let mut out = String::from(
        "// Generated from docs/design-tokens.json. Edit that file, then rebuild.\nuse iced::Color;\n\npub mod color {\n    use super::Color;\n\n",
    );
    for color in &tokens.colors {
        let name = const_name(&color.key);
        match &color.channels {
            Channels::Opaque { r, g, b } => out.push_str(&format!(
                "    pub const {name}: Color = Color::from_rgb8(0x{r:02x}, 0x{g:02x}, 0x{b:02x});\n"
            )),
            Channels::Alpha { r, g, b, alpha } => out.push_str(&format!(
                "    pub const {name}: Color = Color::from_rgba8({r}, {g}, {b}, {alpha});\n"
            )),
        }
    }
    let family = &tokens.family;
    out.push_str("}\n\npub mod font {\n");
    out.push_str(&format!("    pub const FAMILY: &str = \"{family}\";\n\n"));
    out.push_str("    pub mod size {\n");
    for (key, value) in &tokens.sizes {
        let name = const_name(key);
        out.push_str(&format!("        pub const {name}: f32 = {value}.0;\n"));
    }
    out.push_str("    }\n\n    pub mod weight {\n");
    for (key, value) in &tokens.weights {
        let name = const_name(key);
        out.push_str(&format!("        pub const {name}: u16 = {value};\n"));
    }
    out.push_str("    }\n\n    pub mod tracking {\n");
    for (key, value) in &tokens.tracking {
        let name = const_name(key);
        out.push_str(&format!("        pub const {name}: &str = \"{value}\";\n"));
    }
    out.push_str("    }\n}\n\npub mod space {\n");
    for (key, value) in &tokens.spaces {
        let name = const_name(key);
        out.push_str(&format!("    pub const {name}: f32 = {value}.0;\n"));
    }
    out.push_str("}\n");
    Ok(out)
}

fn parse(json: &str) -> Result<Tokens, String> {
    let Value::Object(mut root) = serde_json::from_str(json)
        .map_err(|error| format!("design tokens are not valid JSON: {error}"))?
    else {
        return Err("design tokens must be an object".into());
    };
    let colors = colors(&mut root)?;
    let mut font = take_object(&mut root, "font")?;
    let family = family(&mut font)?;
    let sizes = numbers(&mut font, "font.size", 1024)?;
    let weights = numbers(&mut font, "font.weight", 1000)?;
    let tracking = tracking(&mut font)?;
    unknown(&font, "font")?;
    let spaces = numbers(&mut root, "space", 1024)?;
    unknown(&root, "design tokens")?;
    Ok(Tokens {
        colors,
        family,
        sizes,
        weights,
        tracking,
        spaces,
    })
}

fn colors(root: &mut Map<String, Value>) -> Result<Vec<ColorToken>, String> {
    let map = take_object(root, "color")?;
    let mut colors = Vec::new();
    let mut names = Vec::new();
    for (key, value) in map {
        check_key(&key, "color")?;
        remember(&mut names, &key, "color")?;
        let css = value
            .as_str()
            .ok_or_else(|| format!("color.{key} must be a string"))?
            .to_string();
        let channels = parse_color(&css).map_err(|error| format!("color.{key} {error}"))?;
        colors.push(ColorToken { key, css, channels });
    }
    Ok(colors)
}

fn family(font: &mut Map<String, Value>) -> Result<String, String> {
    let value = font.remove("family").ok_or("font is missing family")?;
    let text = value.as_str().ok_or("font.family must be a string")?;
    if text.is_empty()
        || text
            .chars()
            .any(|symbol| matches!(symbol, ';' | '{' | '}' | '"' | '\\' | '\n' | '\r'))
    {
        return Err("font.family must be a CSS font-family list".into());
    }
    Ok(text.to_string())
}

fn numbers(
    parent: &mut Map<String, Value>,
    path: &str,
    max: u32,
) -> Result<Vec<(String, u32)>, String> {
    let map = take_object(parent, path)?;
    let mut numbers = Vec::new();
    let mut names = Vec::new();
    for (key, value) in map {
        check_key(&key, path)?;
        remember(&mut names, &key, path)?;
        let number = whole_number(&value, &format!("{path}.{key}"), max)?;
        numbers.push((key, number));
    }
    Ok(numbers)
}

fn tracking(font: &mut Map<String, Value>) -> Result<Vec<(String, String)>, String> {
    let map = take_object(font, "font.tracking")?;
    let mut tracking = Vec::new();
    let mut names = Vec::new();
    for (key, value) in map {
        check_key(&key, "font.tracking")?;
        remember(&mut names, &key, "font.tracking")?;
        let text = value
            .as_str()
            .ok_or_else(|| format!("font.tracking.{key} must be a string"))?;
        if !is_em(text) {
            return Err(format!(
                "font.tracking.{key} must be a length in em, such as 0.08em"
            ));
        }
        tracking.push((key, text.to_string()));
    }
    Ok(tracking)
}

fn take_object(parent: &mut Map<String, Value>, path: &str) -> Result<Map<String, Value>, String> {
    let key = path.rsplit('.').next().unwrap_or(path);
    match parent.remove(key) {
        Some(Value::Object(map)) if !map.is_empty() => Ok(map),
        Some(Value::Object(_)) => Err(format!("{path} is empty")),
        Some(_) => Err(format!("{path} must be an object")),
        None => Err(format!("{path} is missing")),
    }
}

fn unknown(map: &Map<String, Value>, what: &str) -> Result<(), String> {
    match map.keys().next() {
        Some(key) => Err(format!("{what} has unknown entry '{key}'")),
        None => Ok(()),
    }
}

fn remember(names: &mut Vec<String>, key: &str, path: &str) -> Result<(), String> {
    let name = const_name(key);
    if names.iter().any(|seen| seen == &name) {
        return Err(format!("{path}.{key} repeats the name {name}"));
    }
    names.push(name);
    Ok(())
}

fn check_key(key: &str, path: &str) -> Result<(), String> {
    let ok = key.split('-').all(is_name_part);
    if ok {
        Ok(())
    } else {
        Err(format!("{path}.{key} must be a kebab-case name"))
    }
}

fn is_name_part(part: &str) -> bool {
    let mut bytes = part.bytes();
    match bytes.next() {
        Some(first) if first.is_ascii_lowercase() => {
            bytes.all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        }
        _ => false,
    }
}

fn const_name(key: &str) -> String {
    key.split('-')
        .map(str::to_ascii_uppercase)
        .collect::<Vec<_>>()
        .join("_")
}

fn whole_number(value: &Value, path: &str, max: u32) -> Result<u32, String> {
    let Some(number) = value.as_u64() else {
        return Err(format!("{path} must be a whole number"));
    };
    let number = u32::try_from(number).unwrap_or(u32::MAX);
    if (1..=max).contains(&number) {
        Ok(number)
    } else {
        Err(format!("{path} must be from 1 to {max}"))
    }
}

fn parse_color(css: &str) -> Result<Channels, String> {
    if let Some(hex) = css.strip_prefix('#') {
        return parse_hex(hex);
    }
    if let Some(body) = css
        .strip_prefix("rgba(")
        .and_then(|body| body.strip_suffix(')'))
    {
        return parse_rgba(body);
    }
    Err("is not a #rrggbb colour or an rgba() colour".into())
}

fn parse_hex(hex: &str) -> Result<Channels, String> {
    if hex.len() == 6
        && hex
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
    {
        let r = u8::from_str_radix(&hex[0..2], 16).unwrap();
        let g = u8::from_str_radix(&hex[2..4], 16).unwrap();
        let b = u8::from_str_radix(&hex[4..6], 16).unwrap();
        return Ok(Channels::Opaque { r, g, b });
    }
    Err("is not a #rrggbb colour or an rgba() colour".into())
}

fn parse_rgba(body: &str) -> Result<Channels, String> {
    let parts: Vec<&str> = body.split(',').map(str::trim).collect();
    let [r, g, b, alpha] = parts.as_slice() else {
        return Err("is not a #rrggbb colour or an rgba() colour".into());
    };
    if !is_alpha(alpha) {
        return Err("has an alpha outside 0 to 1".into());
    }
    Ok(Channels::Alpha {
        r: channel(r)?,
        g: channel(g)?,
        b: channel(b)?,
        alpha: (*alpha).to_string(),
    })
}

fn channel(text: &str) -> Result<u8, String> {
    if !matches!(text.as_bytes(), [b'0'] | [b'1'..=b'9', ..])
        || !text.bytes().all(|byte| byte.is_ascii_digit())
        || text.len() > 1 && text.starts_with('0')
    {
        return Err("is not a #rrggbb colour or an rgba() colour".into());
    }
    text.parse::<u8>()
        .map_err(|_| "is not a #rrggbb colour or an rgba() colour".to_string())
}

fn is_alpha(text: &str) -> bool {
    let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
    if !matches!(whole, "0" | "1") {
        return false;
    }
    if fraction.is_empty() {
        return !text.contains('.');
    }
    fraction.bytes().all(|byte| byte.is_ascii_digit())
        && (whole == "0" || fraction.bytes().all(|byte| byte == b'0'))
}

fn is_em(text: &str) -> bool {
    let Some(number) = text.strip_suffix("em") else {
        return false;
    };
    let unsigned = number.strip_prefix('-').unwrap_or(number);
    match unsigned.split_once('.') {
        None => is_whole(unsigned),
        Some((whole, fraction)) => {
            is_whole(whole)
                && !fraction.is_empty()
                && fraction.bytes().all(|byte| byte.is_ascii_digit())
        }
    }
}

fn is_whole(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes == b"0" {
        return true;
    }
    let Some((first, rest)) = bytes.split_first() else {
        return false;
    };
    first.is_ascii_digit() && *first != b'0' && rest.iter().all(u8::is_ascii_digit)
}

#[cfg(test)]
mod tests {
    use super::{css, rust_constants};

    fn sample() -> &'static str {
        r##"{
  "color": {
    "text": "#f2f3f7",
    "bg": "#0d0e12",
    "hover": "rgba(255, 255, 255, 0.055)"
  },
  "font": {
    "family": "system-ui, sans-serif",
    "size": { "base": 13, "wall-detail": 26 },
    "weight": { "semibold": 600 },
    "tracking": { "tight": "-0.02em" }
  },
  "space": { "sm": 8 }
}"##
    }

    #[test]
    fn css_variables_follow_the_tokens_file() {
        let css = css(sample()).unwrap();
        assert_eq!(
            css,
            "\
/* Generated from docs/design-tokens.json. Do not edit. */
:root {
  --text: #f2f3f7;
  --bg: #0d0e12;
  --hover: rgba(255, 255, 255, 0.055);
  --font-family: system-ui, sans-serif;
  --font-size-base: 13px;
  --font-size-wall-detail: 26px;
  --font-weight-semibold: 600;
  --font-tracking-tight: -0.02em;
  --space-sm: 8px;
}
"
        );
    }

    #[test]
    fn rust_constants_follow_the_tokens_file() {
        let rust = rust_constants(sample()).unwrap();
        assert_eq!(
            rust,
            "\
// Generated from docs/design-tokens.json. Edit that file, then rebuild.
use iced::Color;

pub mod color {
    use super::Color;

    pub const TEXT: Color = Color::from_rgb8(0xf2, 0xf3, 0xf7);
    pub const BG: Color = Color::from_rgb8(0x0d, 0x0e, 0x12);
    pub const HOVER: Color = Color::from_rgba8(255, 255, 255, 0.055);
}

pub mod font {
    pub const FAMILY: &str = \"system-ui, sans-serif\";

    pub mod size {
        pub const BASE: f32 = 13.0;
        pub const WALL_DETAIL: f32 = 26.0;
    }

    pub mod weight {
        pub const SEMIBOLD: u16 = 600;
    }

    pub mod tracking {
        pub const TIGHT: &str = \"-0.02em\";
    }
}

pub mod space {
    pub const SM: f32 = 8.0;
}
"
        );
    }

    #[test]
    fn the_shared_file_lists_the_desktop_palette() {
        let json = include_str!("../../../docs/design-tokens.json");
        let css = css(json).unwrap();
        assert!(css.contains("  --bg: #0d0e12;\n"), "{css}");
        assert!(css.contains("  --accent: #5c9dff;\n"), "{css}");
        assert!(
            css.contains("  --hover: rgba(255, 255, 255, 0.055);\n"),
            "{css}"
        );
        assert!(
            css.contains("  --font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, sans-serif;\n"),
            "{css}"
        );
        assert!(css.contains("  --font-size-base: 13px;\n"), "{css}");
        assert!(css.contains("  --font-size-wall: 36px;\n"), "{css}");
        assert!(css.contains("  --space-sm: 8px;\n"), "{css}");

        let rust = rust_constants(json).unwrap();
        assert!(
            rust.contains("    pub const BG: Color = Color::from_rgb8(0x0d, 0x0e, 0x12);\n"),
            "{rust}"
        );
        assert!(
            rust.contains("    pub const ACCENT: Color = Color::from_rgb8(0x5c, 0x9d, 0xff);\n"),
            "{rust}"
        );
        assert!(
            rust.contains(
                "    pub const HOVER: Color = Color::from_rgba8(255, 255, 255, 0.055);\n"
            ),
            "{rust}"
        );
        assert!(
            rust.contains("        pub const WALL: f32 = 36.0;\n"),
            "{rust}"
        );
        assert!(
            rust.contains("        pub const WALL_DETAIL: f32 = 26.0;\n"),
            "{rust}"
        );
        assert!(rust.contains("    pub const XXL: f32 = 48.0;\n"), "{rust}");
        assert!(
            rust.contains("    pub const INDENT: f32 = 28.0;\n"),
            "{rust}"
        );
    }

    #[test]
    fn rejects_a_colour_that_would_not_round_trip() {
        let error = css(&sample().replace("#0d0e12", "#0D0E12")).unwrap_err();
        assert!(error.contains("color.bg"), "{error}");
    }

    #[test]
    fn rejects_entries_outside_colour_type_and_spacing() {
        let error = css(
            r##"{"color":{"bg":"#0d0e12"},"font":{"family":"system-ui","size":{"base":13},"weight":{"medium":500},"tracking":{"label":"0.08em"}},"space":{"sm":8},"radius":{"sm":4}}"##,
        )
        .unwrap_err();
        assert!(error.contains("radius"), "{error}");
    }

    #[test]
    fn rejects_a_size_of_zero() {
        let error = css(
            r##"{"color":{"bg":"#0d0e12"},"font":{"family":"system-ui","size":{"base":0},"weight":{"medium":500},"tracking":{"label":"0.08em"}},"space":{"sm":8}}"##,
        )
        .unwrap_err();
        assert!(error.contains("font.size.base"), "{error}");
    }
}
