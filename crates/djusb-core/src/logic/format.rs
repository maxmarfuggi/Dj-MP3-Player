//! Display text for durations and tempos.

/// Formats seconds as `m:ss`, or `h:mm:ss` from one hour up.
#[must_use]
pub fn format_time(total_seconds: f64) -> String {
    let whole = if total_seconds.is_finite() {
        total_seconds.floor().max(0.0) as u64
    } else {
        0
    };
    let seconds = whole % 60;
    let minutes = (whole % 3600) / 60;
    let hours = whole / 3600;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

/// Formats the running time of a whole list, for example `47 min` or `2 hr 5 min`.
#[must_use]
pub fn format_total_time(total_seconds: f64) -> String {
    let rounded = if total_seconds.is_finite() {
        (total_seconds / 60.0).round()
    } else {
        0.0
    };
    let total_minutes = rounded.max(1.0) as u64;
    let hours = total_minutes / 60;
    let minutes = total_minutes % 60;
    if hours == 0 {
        format!("{minutes} min")
    } else if minutes == 0 {
        format!("{hours} hr")
    } else {
        format!("{hours} hr {minutes} min")
    }
}

#[must_use]
pub fn format_bpm(bpm: f64) -> String {
    if bpm.is_finite() && bpm.fract() == 0.0 {
        format!("{bpm:.0}")
    } else {
        format!("{bpm:.1}")
    }
}

#[cfg(test)]
mod tests {
    use super::{format_bpm, format_time, format_total_time};

    #[test]
    fn formats_a_position_in_a_track() {
        assert_eq!(format_time(0.0), "0:00");
        assert_eq!(format_time(5.9), "0:05");
        assert_eq!(format_time(65.0), "1:05");
        assert_eq!(format_time(3600.0), "1:00:00");
        assert_eq!(format_time(3661.0), "1:01:01");
        assert_eq!(format_time(-5.0), "0:00");
    }

    #[test]
    fn formats_the_length_of_a_list() {
        assert_eq!(format_total_time(0.0), "1 min");
        assert_eq!(format_total_time(47.0 * 60.0), "47 min");
        assert_eq!(format_total_time(2.0 * 3600.0), "2 hr");
        assert_eq!(format_total_time(2.0 * 3600.0 + 5.0 * 60.0), "2 hr 5 min");
    }

    #[test]
    fn formats_bpm_with_one_decimal_when_it_is_not_whole() {
        assert_eq!(format_bpm(128.0), "128");
        assert_eq!(format_bpm(124.5), "124.5");
    }
}
