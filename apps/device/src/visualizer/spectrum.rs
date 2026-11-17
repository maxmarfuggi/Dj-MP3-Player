//! A windowed FFT of the mono audio, in dB, for the equalizer bands.
//!
//! The window is the Blackman window the Web Audio analyser uses, and the
//! magnitudes are smoothed in the linear domain before the conversion to dB.
//! Dividing by the FFT size keeps a full-scale sine near the top of the
//! equalizer's meter whatever the size.

use std::sync::Arc;

use rustfft::num_complex::Complex;
use rustfft::{Fft, FftPlanner};

/// Frames in one spectrum. The scope keeps twice this, so a read never wraps
/// into samples the writer is replacing.
pub const FFT_SIZE: usize = 4096;

/// The desktop analyser's smoothing, applied to the linear magnitudes.
pub const SMOOTHING: f32 = 0.5;

pub struct Spectrum {
    fft: Arc<dyn Fft<f32>>,
    window: Vec<f32>,
    timed: Vec<Complex<f32>>,
    scratch: Vec<Complex<f32>>,
    smoothed: Vec<f32>,
}

impl Spectrum {
    #[must_use]
    pub fn new(fft_size: usize) -> Self {
        let mut planner = FftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(fft_size);
        let scratch_len = fft.get_inplace_scratch_len();
        Self {
            window: blackman(fft_size),
            timed: vec![Complex::default(); fft_size],
            scratch: vec![Complex::default(); scratch_len],
            smoothed: vec![0.0; fft_size / 2],
            fft,
        }
    }

    /// Fills `decibels` with one bin per `fft_size / 2` frequencies.
    ///
    /// `smoothing` is the share of the previous frame that stays: 0 follows
    /// this frame, 1 keeps the last one. Silence is negative infinity.
    pub fn read(&mut self, mono: &[f32], decibels: &mut [f32], smoothing: f32) {
        let size = self.window.len();
        for (index, sample) in self.timed.iter_mut().enumerate() {
            let value = mono.get(index).copied().unwrap_or(0.0) * self.window[index];
            *sample = Complex::new(value, 0.0);
        }
        self.fft
            .process_with_scratch(&mut self.timed, &mut self.scratch);
        let scale = 1.0 / size as f32;
        let smoothing = smoothing.clamp(0.0, 1.0);
        let follow = 1.0 - smoothing;
        for bin in 0..self.smoothed.len() {
            let magnitude = self.timed[bin].re.hypot(self.timed[bin].im) * scale;
            let mixed = smoothing * self.smoothed[bin] + follow * magnitude;
            self.smoothed[bin] = mixed;
            if let Some(slot) = decibels.get_mut(bin) {
                *slot = linear_to_db(mixed);
            }
        }
    }
}

fn linear_to_db(linear: f32) -> f32 {
    if linear > 0.0 {
        20.0 * linear.log10()
    } else {
        f32::NEG_INFINITY
    }
}

/// Symmetric Blackman window. The ends are zero, so a frame joins the next
/// without a click in the spectrum.
fn blackman(size: usize) -> Vec<f32> {
    if size <= 1 {
        return vec![1.0; size];
    }
    let span = (size - 1) as f32;
    (0..size)
        .map(|index| {
            let n = index as f32;
            let turn = std::f32::consts::PI * n / span;
            0.42 - 0.5 * (2.0 * turn).cos() + 0.08 * (4.0 * turn).cos()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::Spectrum;

    fn sine(samples: usize, bin: usize, rate: f64) -> Vec<f32> {
        let freq = bin as f64 * rate / samples as f64;
        (0..samples)
            .map(|n| (std::f64::consts::TAU * freq * n as f64 / rate).sin() as f32)
            .collect()
    }

    fn peak(decibels: &[f32]) -> (usize, f32) {
        decibels
            .iter()
            .enumerate()
            .max_by(|left, right| left.1.partial_cmp(right.1).unwrap())
            .map(|(index, level)| (index, *level))
            .unwrap()
    }

    #[test]
    fn silence_is_silent() {
        let mut spectrum = Spectrum::new(4096);
        let mut decibels = vec![1.0; 2048];
        spectrum.read(&vec![0.0; 4096], &mut decibels, 0.0);
        assert!(
            decibels.iter().all(|level| *level < -120.0),
            "loudest {}",
            peak(&decibels).1
        );
    }

    #[test]
    fn a_sine_peaks_in_its_bin_below_full_scale() {
        let mut spectrum = Spectrum::new(4096);
        let mut decibels = vec![f32::NEG_INFINITY; 2048];
        spectrum.read(&sine(4096, 40, 48_000.0), &mut decibels, 0.0);
        let (bin, level) = peak(&decibels);
        assert!((bin as i32 - 40).abs() <= 1, "peak bin {bin}");
        assert!(
            (-40.0..0.0).contains(&level),
            "a full-scale sine should sit near the top of the meter, got {level}"
        );
    }

    #[test]
    fn full_smoothing_keeps_silence_and_half_smoothing_catches_up() {
        let tone = sine(4096, 40, 48_000.0);
        let mut held = Spectrum::new(4096);
        let mut decibels = vec![0.0; 2048];
        held.read(&tone, &mut decibels, 1.0);
        assert!(decibels.iter().all(|level| *level < -120.0));

        let mut follows = Spectrum::new(4096);
        follows.read(&tone, &mut decibels, 0.5);
        let first = peak(&decibels).1;
        follows.read(&tone, &mut decibels, 0.5);
        let second = peak(&decibels).1;
        assert!(second > first, "{first} then {second}");
    }
}
