//! Log-spaced FFT bands. A port of `equalizerBands.ts`.

/// A band's share of an FFT's bins: `start` inclusive, `end` exclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BinRange {
    pub start: usize,
    pub end: usize,
}

/// Splits `min_hz`–`max_hz` into `count` bands of equal width in octaves.
/// Bands narrower than a bin still get one.
pub fn log_band_ranges(
    sample_rate: f64,
    fft_size: usize,
    count: usize,
    min_hz: f64,
    max_hz: f64,
) -> Vec<BinRange> {
    let bin_count = fft_size / 2;
    if count == 0 || bin_count < 2 || sample_rate <= 0.0 || min_hz <= 0.0 || max_hz <= min_hz {
        return Vec::new();
    }
    let bin_width = sample_rate / fft_size as f64;
    let edge = |index: usize| min_hz * (max_hz / min_hz).powf(index as f64 / count as f64);
    (0..count)
        .map(|index| {
            let start = round_bin(edge(index) / bin_width).clamp(1, bin_count - 1);
            let end = round_bin(edge(index + 1) / bin_width).clamp(start + 1, bin_count);
            BinRange { start, end }
        })
        .collect()
}

fn round_bin(value: f64) -> usize {
    if value.is_finite() && value > 0.0 {
        value.round() as usize
    } else {
        0
    }
}

/// The band's total level in dB, adding up the power of its bins.
pub fn band_level(decibels: &[f32], range: BinRange) -> f32 {
    let end = range.end.min(decibels.len());
    let start = range.start.min(end);
    let mut power = 0.0_f64;
    for db in &decibels[start..end] {
        let db = f64::from(*db);
        if db.is_finite() {
            power += 10.0_f64.powf(db / 10.0);
        }
    }
    if power > 0.0 {
        (10.0 * power.log10()) as f32
    } else {
        f32::NEG_INFINITY
    }
}

#[cfg(test)]
mod tests {
    use super::{band_level, log_band_ranges, BinRange};

    fn ranges() -> Vec<BinRange> {
        log_band_ranges(48_000.0, 16_384, 72, 35.0, 16_000.0)
    }

    #[test]
    fn gives_every_band_at_least_one_bin_in_order_and_inside_the_fft() {
        let ranges = ranges();
        assert_eq!(ranges.len(), 72);
        for (index, range) in ranges.iter().enumerate() {
            assert!(range.end > range.start);
            assert!(range.start >= 1);
            assert!(range.end <= 16_384 / 2);
            if index > 0 {
                assert!(range.start >= ranges[index - 1].start);
            }
        }
    }

    #[test]
    fn covers_the_requested_frequencies() {
        let bin_width = 48_000.0 / 16_384.0;
        let ranges = ranges();
        let first = ranges[0].start as f64 * bin_width;
        let last = ranges.last().unwrap().end as f64 * bin_width;
        assert!((first - 35.0).abs() < 5.0, "low edge {first}");
        assert!((last - 16_000.0).abs() < 5.0, "high edge {last}");
    }

    #[test]
    fn makes_bands_wider_in_hertz_as_they_go_up() {
        let ranges = ranges();
        let width = |range: &BinRange| range.end - range.start;
        assert!(width(ranges.last().unwrap()) > width(&ranges[36]));
        assert!(width(&ranges[36]) > width(&ranges[0]));
    }

    #[test]
    fn adds_up_the_power_of_the_bins_in_the_band() {
        let decibels = [-100.0, -20.0, -20.0, -100.0];
        let expected = -20.0 + 10.0 * 2.0_f32.log10();
        let level = band_level(&decibels, BinRange { start: 1, end: 3 });
        assert!((level - expected).abs() < 1.0e-4, "{level} vs {expected}");
    }

    #[test]
    fn is_silent_when_every_bin_is() {
        let level = band_level(
            &[f32::NEG_INFINITY, f32::NEG_INFINITY],
            BinRange { start: 0, end: 2 },
        );
        assert!(level.is_infinite() && level.is_sign_negative());
    }
}
