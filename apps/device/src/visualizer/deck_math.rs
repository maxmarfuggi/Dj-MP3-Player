//! Timing for the CDJ-style deck.
//!
//! Remaining time, the zoomed window, and the beat marks all come from the
//! file's BPM and the playhead. Beat 0 is the start of the file. There is no
//! stored beat grid, so a mark every four beats is a bar, and four bars are
//! a phrase.

/// Beats shown in the scrolling waveform.
const DETAIL_BEATS: f64 = 8.0;

/// Seconds in that window when the file has no usable tempo.
const FALLBACK_SPAN: f64 = 4.0;

const BPM_MIN: f64 = 20.0;
const BPM_MAX: f64 = 300.0;

fn usable(bpm: Option<f64>) -> Option<f64> {
    bpm.filter(|bpm| bpm.is_finite() && *bpm > BPM_MIN && *bpm < BPM_MAX)
}

/// Minutes, seconds, and hundredths of a second still to play.
#[derive(Debug, Clone, Copy)]
pub struct Remain {
    pub minutes: u64,
    pub seconds: u64,
    pub hundredths: f64,
}

/// Time left in the track. A position past the end reads as zero.
#[must_use]
pub fn remain(position: f64, length: f64) -> Remain {
    let left = if position.is_finite() && length.is_finite() {
        (length - position).max(0.0)
    } else {
        0.0
    };
    // Round to a millisecond first, so a fraction that displays as the next
    // tenth carries into the seconds instead of reading 100.0.
    let millis = (left * 1000.0).round() as u64;
    Remain {
        minutes: millis / 60_000,
        seconds: (millis / 1000) % 60,
        hundredths: (millis % 1000) as f64 / 10.0,
    }
}

/// The digits of `06M:31S 66.0F` as `["06", "31", "66.0"]`, or dashes when
/// the file does not state a length.
#[must_use]
pub fn remain_digits(position: f64, length: Option<f64>) -> [String; 3] {
    let Some(length) = length.filter(|length| length.is_finite() && *length > 0.0) else {
        return ["--".to_string(), "--".to_string(), "--.-".to_string()];
    };
    let remain = remain(position, length);
    [
        format!("{:02}", remain.minutes),
        format!("{:02}", remain.seconds),
        format!("{:04.1}", remain.hundredths),
    ]
}

/// The tempo split as a CDJ shows it, `("128", ".3")`. Missing tempo is
/// dashes.
#[must_use]
pub fn bpm_digits(bpm: Option<f64>) -> (String, String) {
    match bpm {
        Some(bpm) if bpm.is_finite() && bpm > 0.0 => {
            let tenths = (bpm * 10.0).round() as u64;
            (format!("{}", tenths / 10), format!(".{}", tenths % 10))
        }
        _ => ("---".to_string(), ".-".to_string()),
    }
}

/// Bars from the start of the file, counting four beats to a bar.
#[must_use]
pub fn bars_text(position: f64, bpm: Option<f64>) -> Option<String> {
    let bpm = usable(bpm)?;
    // Four beats to a bar, and a beat is 60 / bpm seconds.
    let bars = position.max(0.0) * bpm / 240.0;
    Some(format!("{bars:04.1}"))
}

/// Where the playhead is in its bar and phrase, each counted 0 to 3.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Count {
    pub beat: usize,
    pub bar: usize,
}

#[must_use]
pub fn counter(position: f64, bpm: Option<f64>) -> Option<Count> {
    let bpm = usable(bpm)?;
    if !position.is_finite() {
        return None;
    }
    let beat = (position.max(0.0) * bpm / 60.0).floor() as u64;
    Some(Count {
        beat: (beat % 4) as usize,
        bar: ((beat / 4) % 4) as usize,
    })
}

/// Seconds across the scrolling waveform: eight beats, or four seconds.
#[must_use]
pub fn detail_span(bpm: Option<f64>) -> f64 {
    match usable(bpm) {
        Some(bpm) => DETAIL_BEATS * 60.0 / bpm,
        None => FALLBACK_SPAN,
    }
}

/// Keeps the playhead in the middle of the window, as a CDJ does. Near the
/// ends of the track the window runs past the file and that part is blank.
#[must_use]
pub fn window_start(position: f64, span: f64) -> f64 {
    if !position.is_finite() || !span.is_finite() || span <= 0.0 {
        return 0.0;
    }
    position.max(0.0) - span / 2.0
}

/// A beat mark. `bar` is every fourth beat, with the downbeat at the start.
#[derive(Debug, Clone, Copy)]
pub struct Beat {
    pub at: f64,
    pub bar: bool,
}

/// Beat marks inside `[start, start + span)`.
#[must_use]
pub fn beats(start: f64, span: f64, bpm: f64) -> Vec<Beat> {
    if !bpm.is_finite() || bpm <= BPM_MIN || bpm >= BPM_MAX || !span.is_finite() || span <= 0.0 {
        return Vec::new();
    }
    let period = 60.0 / bpm;
    let end = start + span;
    let mut index = (start / period).ceil().max(0.0) as i64;
    let mut marks = Vec::new();
    while marks.len() < 64 && index < 1_000_000_000 {
        let at = index as f64 * period;
        if at >= end - 1e-6 {
            break;
        }
        if at + 1e-6 >= start {
            marks.push(Beat {
                at,
                bar: index.rem_euclid(4) == 0,
            });
        }
        index += 1;
    }
    marks
}

#[cfg(test)]
mod tests {
    use super::{
        bars_text, beats, bpm_digits, counter, detail_span, remain_digits, window_start, Count,
    };

    #[test]
    fn remain_reads_like_a_cdj() {
        assert_eq!(
            remain_digits(0.0, Some(6.0 * 60.0 + 31.66)),
            ["06", "31", "66.0"]
        );
        assert_eq!(
            remain_digits(10.0, Some(6.0 * 60.0 + 31.66)),
            ["06", "21", "66.0"]
        );
        assert_eq!(remain_digits(0.0, None), ["--", "--", "--.-"]);
        assert_eq!(remain_digits(5.0, Some(0.0)), ["--", "--", "--.-"]);
    }

    #[test]
    fn a_rounded_fraction_carries_into_the_next_second() {
        assert_eq!(remain_digits(0.0, Some(0.9995)), ["00", "01", "00.0"]);
        assert_eq!(remain_digits(0.0, Some(59.9999)), ["01", "00", "00.0"]);
    }

    #[test]
    fn tempo_and_bars_use_the_file_bpm() {
        assert_eq!(
            bpm_digits(Some(128.36)),
            ("128".to_string(), ".4".to_string())
        );
        assert_eq!(
            bpm_digits(Some(127.96)),
            ("128".to_string(), ".0".to_string())
        );
        assert_eq!(bpm_digits(None), ("---".to_string(), ".-".to_string()));
        assert_eq!(bars_text(0.0, Some(120.0)).as_deref(), Some("00.0"));
        assert_eq!(bars_text(8.0, Some(120.0)).as_deref(), Some("04.0"));
        assert_eq!(bars_text(1.0, None), None);
        assert_eq!(bars_text(1.0, Some(10.0)), None);
    }

    #[test]
    fn the_counter_walks_beats_then_bars() {
        assert_eq!(counter(0.0, Some(120.0)), Some(Count { beat: 0, bar: 0 }));
        assert_eq!(counter(1.6, Some(120.0)), Some(Count { beat: 3, bar: 0 }));
        assert_eq!(counter(2.0, Some(120.0)), Some(Count { beat: 0, bar: 1 }));
        assert_eq!(counter(8.0, Some(120.0)), Some(Count { beat: 0, bar: 0 }));
        assert_eq!(counter(1.0, None), None);
    }

    #[test]
    fn the_window_is_eight_beats_around_the_playhead() {
        assert_eq!(detail_span(Some(120.0)), 4.0);
        assert_eq!(detail_span(None), 4.0);
        assert_eq!(detail_span(Some(10.0)), 4.0);
        assert_eq!(detail_span(Some(400.0)), 4.0);
        assert_eq!(window_start(10.0, 4.0), 8.0);
        assert_eq!(window_start(1.0, 4.0), -1.0);
        assert_eq!(window_start(-3.0, 4.0), -2.0);
    }

    #[test]
    fn beats_start_at_zero_and_every_fourth_is_a_bar() {
        let marks = beats(0.0, 2.0, 120.0);
        assert_eq!(marks.len(), 4);
        assert!(marks[0].bar);
        assert!((marks[1].at - 0.5).abs() < 1e-9);
        assert!(!marks[1].bar);
        assert!(!marks[3].bar);

        let later = beats(1.0, 2.0, 120.0);
        assert!(later
            .iter()
            .any(|mark| mark.bar && (mark.at - 2.0).abs() < 1e-9));
        assert!(beats(0.0, 4.0, 10.0).is_empty());
        assert_eq!(beats(-2.0, 2.0, 120.0).len(), 0);
    }
}
