//! HTTP `Range` headers for a file of a known size.

/// First and last byte of a range, both inclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteRange {
    pub start: u64,
    pub end: u64,
}

/// Result of parsing one `Range` header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParsedRange {
    /// No header, or a form this parser does not handle, such as multiple ranges.
    Absent,
    /// The range lies outside the resource.
    Unsatisfiable,
    Satisfiable(ByteRange),
}

/// Parses an HTTP `Range` header for a resource of `size` bytes.
///
/// A missing header, or a form this parser does not handle, is [`ParsedRange::Absent`]:
/// the whole resource should be sent. A range that lies outside the resource is
/// [`ParsedRange::Unsatisfiable`].
#[must_use]
pub fn parse_range_header(header: Option<&str>, size: u64) -> ParsedRange {
    let Some(header) = header else {
        return ParsedRange::Absent;
    };
    let Some(spec) = header.strip_prefix("bytes=") else {
        return ParsedRange::Absent;
    };
    let Some((first, last)) = spec.split_once('-') else {
        return ParsedRange::Absent;
    };
    // `bytes=0-10,20-30` and anything else that is not one number pair.
    if first.bytes().any(|byte| !byte.is_ascii_digit())
        || last.bytes().any(|byte| !byte.is_ascii_digit())
    {
        return ParsedRange::Absent;
    }

    if first.is_empty() {
        if last.is_empty() {
            return ParsedRange::Absent;
        }
        // `bytes=-N` asks for the final N bytes.
        let suffix_length = parse_bound(last);
        if suffix_length == 0 || size == 0 {
            return ParsedRange::Unsatisfiable;
        }
        return ParsedRange::Satisfiable(ByteRange {
            start: size.saturating_sub(suffix_length),
            end: size - 1,
        });
    }

    let start = parse_bound(first);
    if start >= size {
        return ParsedRange::Unsatisfiable;
    }
    let end = if last.is_empty() {
        size - 1
    } else {
        parse_bound(last).min(size - 1)
    };
    if start > end {
        ParsedRange::Unsatisfiable
    } else {
        ParsedRange::Satisfiable(ByteRange { start, end })
    }
}

/// A digit string that does not fit in `u64` is treated as larger than any file.
fn parse_bound(text: &str) -> u64 {
    text.parse().unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::{parse_range_header, ByteRange, ParsedRange};

    fn bytes(start: u64, end: u64) -> ParsedRange {
        ParsedRange::Satisfiable(ByteRange { start, end })
    }

    #[test]
    fn returns_absent_when_there_is_no_usable_range() {
        assert_eq!(parse_range_header(None, 1000), ParsedRange::Absent);
        assert_eq!(
            parse_range_header(Some("bytes=-"), 1000),
            ParsedRange::Absent
        );
        assert_eq!(
            parse_range_header(Some("bytes=0-10,20-30"), 1000),
            ParsedRange::Absent
        );
    }

    #[test]
    fn parses_open_and_closed_ranges() {
        assert_eq!(parse_range_header(Some("bytes=0-"), 1000), bytes(0, 999));
        assert_eq!(
            parse_range_header(Some("bytes=200-299"), 1000),
            bytes(200, 299)
        );
    }

    #[test]
    fn clamps_the_end_to_the_file_size() {
        assert_eq!(
            parse_range_header(Some("bytes=900-5000"), 1000),
            bytes(900, 999)
        );
    }

    #[test]
    fn parses_suffix_ranges() {
        assert_eq!(
            parse_range_header(Some("bytes=-100"), 1000),
            bytes(900, 999)
        );
        assert_eq!(parse_range_header(Some("bytes=-5000"), 1000), bytes(0, 999));
    }

    #[test]
    fn reports_ranges_outside_the_file() {
        assert_eq!(
            parse_range_header(Some("bytes=1000-"), 1000),
            ParsedRange::Unsatisfiable
        );
        assert_eq!(
            parse_range_header(Some("bytes=500-400"), 1000),
            ParsedRange::Unsatisfiable
        );
        assert_eq!(
            parse_range_header(Some("bytes=-0"), 1000),
            ParsedRange::Unsatisfiable
        );
        assert_eq!(
            parse_range_header(Some("bytes=0-"), 0),
            ParsedRange::Unsatisfiable
        );
    }
}
