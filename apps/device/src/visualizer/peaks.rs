//! A waveform skimmed from the file.
//!
//! The live audio copy only holds a fraction of a second, so the deck reads
//! the file once in the background and keeps a min and a max for every
//! eightieth of a second. The overview and the scrolling window both sample
//! that series.

use std::path::Path;

use crate::playback::decode::Source;

/// Peaks kept per second of audio. A four-second window then has a few
/// hundred columns, which is enough to look continuous.
pub const PEAKS_PER_SECOND: u32 = 80;

/// Min and max of successive slices of the file, mixed to one channel.
pub struct Peaks {
    pub mins: Vec<f32>,
    pub maxs: Vec<f32>,
    /// Length of the audio that was read, in seconds.
    pub seconds: f64,
}

/// Folds mono samples into [`PEAKS_PER_SECOND`] bins. The last, shorter bin
/// is kept so the end of the file is not dropped. The file scan uses the
/// same bins; the tests call this with a slice.
#[cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "the tests bin a slice; scanning a file uses the same bins"
    )
)]
#[must_use]
pub fn peaks_from_mono(samples: &[f32], rate: u32) -> Peaks {
    let mut bins = Accumulator::new(rate);
    for sample in samples {
        bins.push_sample(*sample);
    }
    bins.finish()
}

/// Reads `path` and returns its peaks. The caller runs this off the UI thread.
pub fn scan_file(path: &Path) -> Result<Peaks, String> {
    let mut source = Source::open(path)?;
    let mut bins = Accumulator::new(source.sample_rate());
    let mut frames = Vec::new();
    while source.read(&mut frames)?.is_some() {
        bins.push_stereo(&frames);
        frames.clear();
    }
    Ok(bins.finish())
}

/// Min and max for each column of the waveform between `from` and `to`.
///
/// Each peak covers its whole slice of time, so a column between two peak
/// starts still has a shape.
#[must_use]
pub fn columns(peaks: &Peaks, from: f64, to: f64, count: usize) -> Vec<(f32, f32)> {
    let mut out = vec![(0.0, 0.0); count];
    if count == 0
        || peaks.mins.is_empty()
        || !from.is_finite()
        || !to.is_finite()
        || to <= from
        || peaks.seconds <= 0.0
    {
        return out;
    }
    let span = to - from;
    let total = peaks.mins.len();
    for (index, (min, max)) in peaks.mins.iter().zip(&peaks.maxs).enumerate() {
        let bin_start = index as f64 / total as f64 * peaks.seconds;
        let bin_end = (index + 1) as f64 / total as f64 * peaks.seconds;
        if bin_end <= from || bin_start >= to {
            continue;
        }
        let first = ((bin_start - from) / span * count as f64).floor().max(0.0) as usize;
        let last = ((bin_end - from) / span * count as f64).ceil().max(0.0) as usize;
        let first = first.min(count - 1);
        let last = last.clamp(first + 1, count);
        for slot in &mut out[first..last] {
            slot.0 = slot.0.min(*min);
            slot.1 = slot.1.max(*max);
        }
    }
    out
}

struct Accumulator {
    frames_per_bin: usize,
    in_bin: usize,
    min: f32,
    max: f32,
    frames: u64,
    rate: u32,
    mins: Vec<f32>,
    maxs: Vec<f32>,
}

impl Accumulator {
    fn new(rate: u32) -> Self {
        let rate = rate.max(1);
        Self {
            frames_per_bin: (rate / PEAKS_PER_SECOND).max(1) as usize,
            in_bin: 0,
            min: 0.0,
            max: 0.0,
            frames: 0,
            rate,
            mins: Vec::new(),
            maxs: Vec::new(),
        }
    }

    fn push_sample(&mut self, sample: f32) {
        let sample = if sample.is_finite() { sample } else { 0.0 };
        self.min = self.min.min(sample);
        self.max = self.max.max(sample);
        self.in_bin += 1;
        self.frames += 1;
        if self.in_bin == self.frames_per_bin {
            self.flush();
        }
    }

    fn push_stereo(&mut self, frames: &[f32]) {
        for pair in frames.chunks(2) {
            let left = pair.first().copied().unwrap_or(0.0);
            let right = pair.get(1).copied().unwrap_or(left);
            self.push_sample((left + right) * 0.5);
        }
    }

    fn flush(&mut self) {
        if self.in_bin == 0 {
            return;
        }
        self.mins.push(self.min);
        self.maxs.push(self.max);
        self.min = 0.0;
        self.max = 0.0;
        self.in_bin = 0;
    }

    fn finish(mut self) -> Peaks {
        self.flush();
        Peaks {
            mins: self.mins,
            maxs: self.maxs,
            seconds: self.frames as f64 / f64::from(self.rate),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{columns, peaks_from_mono, Peaks};

    #[test]
    fn samples_fold_into_eightieths_of_a_second() {
        let peaks = peaks_from_mono(&[0.2, -0.5, 0.1, 0.4], 160);
        assert_eq!(peaks.mins, vec![-0.5, 0.0]);
        assert_eq!(peaks.maxs, vec![0.2, 0.4]);
        assert!((peaks.seconds - 4.0 / 160.0).abs() < 1e-9);
    }

    #[test]
    fn a_short_tail_is_kept() {
        let peaks = peaks_from_mono(&[0.5, -0.25, 0.1], 160);
        assert_eq!(peaks.maxs, vec![0.5, 0.1]);
        assert_eq!(peaks.mins.len(), 2);
    }

    #[test]
    fn a_peak_fills_every_column_it_covers() {
        let peaks = Peaks {
            mins: vec![-0.2, -0.4],
            maxs: vec![0.5, 0.1],
            seconds: 1.0,
        };
        assert_eq!(columns(&peaks, 0.0, 1.0, 2), vec![(-0.2, 0.5), (-0.4, 0.1)]);
        assert_eq!(columns(&peaks, 0.5, 1.0, 1), vec![(-0.4, 0.1)]);

        let one = Peaks {
            mins: vec![-0.3],
            maxs: vec![0.8],
            seconds: 4.0,
        };
        assert_eq!(columns(&one, 0.0, 4.0, 4), vec![(-0.3, 0.8); 4]);
    }
}
