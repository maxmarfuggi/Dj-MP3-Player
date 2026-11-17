//! The oscilloscope trace. A port of the geometry in `Waveform.tsx`.
//!
//! A solid theme paints the line in one colour. Pulse walks the same colour
//! wheel the desktop used to: three hues along the line, one turn every
//! twenty seconds.

/// Samples read for one trace, matching the desktop analyser's window.
pub const WAVE_FRAMES: usize = 2048;

/// How much of the canvas height a full-scale sample fills, each side of centre.
pub const AMPLITUDE: f32 = 0.42;

/// The colours go once around the colour wheel in this time.
pub const COLOR_CYCLE_SECONDS: f32 = 20.0;

/// Hue offsets of the gradient's three stops, from left to right.
pub const GRADIENT_HUES: [f32; 3] = [0.0, 55.0, 120.0];

/// Earlier traces left on screen. Each is drawn at this fraction of the next
/// newer one's strength: the desktop fades the frame by 0.35, so 0.65 remains.
pub const TRAIL_FRAMES: usize = 12;
pub const TRAIL_KEEP: f32 = 0.65;

/// Hue, in degrees, at `seconds` into the colour cycle.
#[must_use]
pub fn hue_at(seconds: f32) -> f32 {
    seconds / COLOR_CYCLE_SECONDS * 360.0
}

/// `hsl()` as red, green and blue in `0.0..=1.0`. Hue wraps at 360 degrees.
#[must_use]
pub fn hsl(hue: f32, saturation: f32, lightness: f32) -> (f32, f32, f32) {
    let sector = hue.rem_euclid(360.0) / 60.0;
    let chroma = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let across = chroma * (1.0 - ((sector % 2.0) - 1.0).abs());
    let (red, green, blue) = match sector as u8 {
        0 => (chroma, across, 0.0),
        1 => (across, chroma, 0.0),
        2 => (0.0, chroma, across),
        3 => (0.0, across, chroma),
        4 => (across, 0.0, chroma),
        _ => (chroma, 0.0, across),
    };
    let match_light = lightness - chroma / 2.0;
    (red + match_light, green + match_light, blue + match_light)
}

/// Averages each stereo frame into one sample. `mono` is padded with silence.
pub fn downmix(stereo: &[f32], mono: &mut [f32]) {
    let frames = mono.len().min(stereo.len() / 2);
    for (index, sample) in mono.iter_mut().enumerate().take(frames) {
        *sample = (stereo[index * 2] + stereo[index * 2 + 1]) * 0.5;
    }
    mono[frames..].fill(0.0);
}

/// Index of the first rising zero crossing in the first half, or 0.
///
/// The rest of the window is the margin the crossing can land in, so the
/// drawn stretch stays the same length.
#[must_use]
pub fn rising_zero_start(samples: &[f32]) -> usize {
    let visible = samples.len() / 2;
    for index in 1..visible {
        if samples[index - 1] < 0.0 && samples[index] >= 0.0 {
            return index;
        }
    }
    0
}

/// The line drawn for `samples`. `x` runs the width; `y` grows downward.
#[must_use]
pub fn trace(samples: &[f32], width: f32, height: f32) -> Vec<(f32, f32)> {
    let visible = samples.len() / 2;
    if visible < 2 || width <= 0.0 || height <= 0.0 {
        return Vec::new();
    }
    let start = rising_zero_start(samples);
    let middle = height / 2.0;
    let amplitude = height * AMPLITUDE;
    let last = (visible - 1) as f32;
    (0..visible)
        .map(|index| {
            let x = index as f32 / last * width;
            let y = middle - samples[start + index] * amplitude;
            (x, y)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        downmix, hsl, hue_at, rising_zero_start, trace, AMPLITUDE, COLOR_CYCLE_SECONDS,
        GRADIENT_HUES,
    };

    #[test]
    fn the_colour_goes_around_the_wheel_in_one_cycle() {
        assert!((hue_at(0.0) - 0.0).abs() < 1.0e-4);
        assert!((hue_at(COLOR_CYCLE_SECONDS / 2.0) - 180.0).abs() < 1.0e-3);
        assert!((hue_at(COLOR_CYCLE_SECONDS) - 360.0).abs() < 1.0e-3);
        assert_eq!(GRADIENT_HUES, [0.0, 55.0, 120.0]);
    }

    #[test]
    fn hsl_hits_the_primaries_at_half_lightness() {
        let (r, g, b) = hsl(0.0, 1.0, 0.5);
        assert!((r - 1.0).abs() < 1.0e-4 && g.abs() < 1.0e-4 && b.abs() < 1.0e-4);
        let (r, g, b) = hsl(120.0, 1.0, 0.5);
        assert!(r.abs() < 1.0e-4 && (g - 1.0).abs() < 1.0e-4 && b.abs() < 1.0e-4);
        let (r, g, b) = hsl(240.0, 1.0, 0.5);
        assert!(r.abs() < 1.0e-4 && g.abs() < 1.0e-4 && (b - 1.0).abs() < 1.0e-4);
        let (r, g, b) = hsl(360.0, 1.0, 0.5);
        assert!((r - 1.0).abs() < 1.0e-4 && g.abs() < 1.0e-4 && b.abs() < 1.0e-4);
    }

    #[test]
    fn downmix_averages_the_two_channels() {
        let mut mono = [9.0; 3];
        downmix(&[1.0, 3.0, -1.0, 1.0], &mut mono);
        assert_eq!(mono, [2.0, 0.0, 0.0]);
    }

    #[test]
    fn the_trace_starts_at_a_rising_zero_crossing_in_the_first_half() {
        let samples = [-1.0, 0.5, 0.0, 0.0];
        assert_eq!(rising_zero_start(&samples), 1);
        let points = trace(&samples, 100.0, 200.0);
        let amplitude = 200.0 * AMPLITUDE;
        assert_eq!(points.len(), 2);
        assert!((points[0].0 - 0.0).abs() < 1.0e-3);
        assert!((points[0].1 - (100.0 - 0.5 * amplitude)).abs() < 1.0e-3);
        assert!((points[1].0 - 100.0).abs() < 1.0e-3);
        assert!((points[1].1 - 100.0).abs() < 1.0e-3);
    }

    #[test]
    fn a_crossing_past_the_first_half_does_not_move_the_start() {
        let samples = [-0.5, -0.5, -0.5, 0.5];
        assert_eq!(rising_zero_start(&samples), 0);
        let points = trace(&samples, 10.0, 10.0);
        assert_eq!(points.len(), 2);
        let middle = 5.0;
        assert!(points[0].1 > middle && points[1].1 > middle);
        assert!((points[0].1 - points[1].1).abs() < 1.0e-3);
    }
}
