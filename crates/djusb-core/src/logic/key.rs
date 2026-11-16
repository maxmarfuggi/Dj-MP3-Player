//! Camelot wheel positions for musical keys.

/// A for minor keys, B for major keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CamelotLetter {
    A,
    B,
}

/// A key's position on the Camelot wheel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CamelotKey {
    /// Position on the wheel, 1–12.
    pub number: u8,
    pub letter: CamelotLetter,
}

const PITCH_CLASSES: [(char, i32); 7] = [
    ('C', 0),
    ('D', 2),
    ('E', 4),
    ('F', 5),
    ('G', 7),
    ('A', 9),
    ('B', 11),
];

/// A key's Camelot code, or `None` if the text is not a key we recognise.
///
/// Accepts Camelot codes (`8A`) and note names (`Am`, `F#`, `Bbmin`). A lone
/// `M` means major; `m`, `min`, and `minor` mean minor.
#[must_use]
pub fn camelot_key(key: &str) -> Option<CamelotKey> {
    let text = key.trim();
    if text.starts_with(|character: char| character.is_ascii_digit()) {
        return parse_camelot_code(text);
    }
    parse_note_name(text)
}

fn parse_camelot_code(text: &str) -> Option<CamelotKey> {
    let (digits, letter) = text.split_at_checked(text.len().checked_sub(1)?)?;
    if digits.is_empty() || digits.len() > 2 || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let letter = match letter {
        "A" | "a" => CamelotLetter::A,
        "B" | "b" => CamelotLetter::B,
        _ => return None,
    };
    let number: u8 = digits.parse().ok()?;
    (1..=12)
        .contains(&number)
        .then_some(CamelotKey { number, letter })
}

fn parse_note_name(text: &str) -> Option<CamelotKey> {
    let mut chars = text.chars();
    let note = chars.next()?.to_ascii_uppercase();
    let pitch = PITCH_CLASSES
        .iter()
        .find(|(name, _)| *name == note)
        .map(|(_, pitch)| *pitch)?;
    let rest: String = chars.collect();
    let (accidental, rest) = if let Some(rest) = rest.strip_prefix(['#', '♯']) {
        (1, rest)
    } else if let Some(rest) = rest.strip_prefix(['b', '♭']) {
        (-1, rest)
    } else {
        (0, rest.as_str())
    };
    let rest = rest.trim_start();
    let is_minor = mode_is_minor(rest)?;
    let pitch_class = (pitch + accidental + 12) % 12;
    // A minor key shares its number with the major key three semitones up.
    let major_pitch = if is_minor {
        (pitch_class + 3) % 12
    } else {
        pitch_class
    };
    // Each step around the wheel is a fifth; C major is 8.
    let number = u8::try_from(((major_pitch * 7) % 12 + 7) % 12 + 1).ok()?;
    Some(CamelotKey {
        number,
        letter: if is_minor {
            CamelotLetter::A
        } else {
            CamelotLetter::B
        },
    })
}

/// `Some(true)` for minor, `Some(false)` for major, `None` when `mode` is not a mode.
fn mode_is_minor(mode: &str) -> Option<bool> {
    if mode.is_empty() {
        return Some(false);
    }
    // The single letter is case-sensitive: `m` is minor and `M` is major.
    if mode == "m" {
        return Some(true);
    }
    if mode.eq_ignore_ascii_case("min") || mode.eq_ignore_ascii_case("minor") {
        return Some(true);
    }
    if mode.eq_ignore_ascii_case("m")
        || mode.eq_ignore_ascii_case("maj")
        || mode.eq_ignore_ascii_case("major")
    {
        return Some(false);
    }
    None
}

/// Position of a key on the Camelot wheel (1–12), or `None` if unrecognised.
#[must_use]
pub fn camelot_number(key: &str) -> Option<u8> {
    camelot_key(key).map(|key| key.number)
}

/// Hue, in degrees, of a key in the usual Camelot wheel colours.
#[must_use]
pub fn key_hue(key: &str) -> Option<i32> {
    camelot_number(key).map(camelot_hue)
}

/// Hue, in degrees, of a Camelot wheel number.
///
/// 1 is teal-green; the colours then run backwards through the rainbow.
#[must_use]
pub fn camelot_hue(number: u8) -> i32 {
    let hue = 165 - (i32::from(number) - 1) * 30;
    hue.rem_euclid(360)
}

#[cfg(test)]
mod tests {
    use super::{camelot_hue, camelot_key, CamelotKey, CamelotLetter};

    fn key(number: u8, letter: CamelotLetter) -> CamelotKey {
        CamelotKey { number, letter }
    }

    #[test]
    fn reads_camelot_codes_and_note_names() {
        assert_eq!(camelot_key("8a"), Some(key(8, CamelotLetter::A)));
        assert_eq!(camelot_key("Am"), Some(key(8, CamelotLetter::A)));
        assert_eq!(camelot_key("C"), Some(key(8, CamelotLetter::B)));
        assert_eq!(camelot_key("F#min"), Some(key(11, CamelotLetter::A)));
        assert_eq!(camelot_key("13A"), None);
        assert_eq!(camelot_key("H"), None);
    }

    #[test]
    fn a_capital_m_is_major() {
        assert_eq!(camelot_key("AM"), Some(key(11, CamelotLetter::B)));
        assert_eq!(camelot_key("Bb"), Some(key(6, CamelotLetter::B)));
    }

    #[test]
    fn camelot_hue_starts_at_teal_and_wraps() {
        assert_eq!(camelot_hue(1), 165);
        assert_eq!(camelot_hue(8), 315);
    }
}
