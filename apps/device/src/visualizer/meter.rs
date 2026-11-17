//! Bar levels and the falling peak caps. The same motion as the desktop equalizer.

use super::bands::{band_level, BinRange};

pub const BAR_COUNT: usize = 96;
pub const FLOOR_DB: f32 = -76.0;
pub const CEILING_DB: f32 = -16.0;
const FALL_PER_SECOND: f32 = 1.1;
const PEAK_HOLD_SECONDS: f32 = 0.6;
const PEAK_FALL_PER_SECOND: f32 = 0.35;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Peak {
    pub level: f32,
    pub held_for: f32,
}

impl Default for Peak {
    fn default() -> Self {
        Self {
            level: 0.0,
            held_for: 0.0,
        }
    }
}

/// Lowest and highest frequencies the bars cover, in hertz.
pub const MIN_HZ: f64 = 35.0;
pub const MAX_HZ: f64 = 16_000.0;

/// Moves `levels` and `peaks` toward the bands in `decibels`.
/// `seconds` is clamped to a tenth, as one late frame must not empty the meters.
pub fn step_bars(
    decibels: &[f32],
    ranges: &[BinRange],
    levels: &mut [f32],
    peaks: &mut [Peak],
    seconds: f32,
) {
    let seconds = seconds.clamp(0.0, 0.1);
    let bars = ranges.len().min(levels.len()).min(peaks.len());
    for bar in 0..bars {
        let heard = ((band_level(decibels, ranges[bar]) - FLOOR_DB) / (CEILING_DB - FLOOR_DB))
            .clamp(0.0, 1.0);
        levels[bar] = heard.max(levels[bar] - FALL_PER_SECOND * seconds);
        let peak = &mut peaks[bar];
        if levels[bar] >= peak.level {
            peak.level = levels[bar];
            peak.held_for = 0.0;
        } else {
            peak.held_for += seconds;
            if peak.held_for > PEAK_HOLD_SECONDS {
                peak.level = levels[bar].max(peak.level - PEAK_FALL_PER_SECOND * seconds);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::bands::BinRange;
    use super::{step_bars, Peak, CEILING_DB, FLOOR_DB};

    fn close(actual: f32, expected: f32) {
        assert!((actual - expected).abs() < 1.0e-3, "{actual} vs {expected}");
    }

    #[test]
    fn the_ceiling_is_full_and_the_floor_is_empty() {
        let ranges = [BinRange { start: 0, end: 1 }, BinRange { start: 1, end: 2 }];
        let mut levels = [0.0; 2];
        let mut peaks = [Peak::default(); 2];
        let mid = (FLOOR_DB + CEILING_DB) / 2.0;
        step_bars(
            &[CEILING_DB, FLOOR_DB, mid],
            &ranges,
            &mut levels,
            &mut peaks,
            0.0,
        );
        close(levels[0], 1.0);
        close(levels[1], 0.0);
        close(peaks[0].level, 1.0);

        let mut levels = [0.0];
        let mut peaks = [Peak::default()];
        step_bars(
            &[mid],
            &[BinRange { start: 0, end: 1 }],
            &mut levels,
            &mut peaks,
            0.0,
        );
        close(levels[0], 0.5);
    }

    #[test]
    fn bars_fall_peaks_hold_and_a_late_frame_only_counts_a_tenth() {
        let ranges = [BinRange { start: 0, end: 1 }];
        let mut levels = [0.0];
        let mut peaks = [Peak::default()];
        step_bars(&[CEILING_DB], &ranges, &mut levels, &mut peaks, 0.0);
        // The peak stays put through 0.6s and lets go on the next tenth.
        for frame in 1..=6 {
            step_bars(&[f32::NEG_INFINITY], &ranges, &mut levels, &mut peaks, 0.1);
            close(peaks[0].level, 1.0);
            close(levels[0], 1.0 - 1.1 * 0.1 * frame as f32);
        }
        step_bars(&[f32::NEG_INFINITY], &ranges, &mut levels, &mut peaks, 0.1);
        close(levels[0], 1.0 - 1.1 * 0.7);
        close(peaks[0].level, 1.0 - 0.35 * 0.1);

        step_bars(&[CEILING_DB], &ranges, &mut levels, &mut peaks, 0.0);
        close(levels[0], 1.0);
        close(peaks[0].level, 1.0);
        close(peaks[0].held_for, 0.0);

        let mut levels = [1.0];
        let mut peaks = [Peak {
            level: 1.0,
            held_for: 0.0,
        }];
        step_bars(&[f32::NEG_INFINITY], &ranges, &mut levels, &mut peaks, 5.0);
        close(levels[0], 1.0 - 1.1 * 0.1);
    }
}
