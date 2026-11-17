//! A waveform skimmed from the file.
//!
//! The live audio copy only holds a fraction of a second, so the deck reads
//! the file once in the background and keeps a min and a max for every
//! eightieth of a second. Peaks are handed over as the file is read, so the
//! waveform can draw before the end. The overview and the scrolling window
//! both sample that series.

use std::path::Path;

use crate::playback::decode::Source;

/// Peaks kept per second of audio. A four-second window then has a few
/// hundred columns, which is enough to look continuous.
pub const PEAKS_PER_SECOND: u32 = 80;

/// Bins between hand-overs after the first. Forty bins is half a second.
const HANDOVER_BINS: usize = 40;

/// How far [`scan_file`] got.
#[derive(Debug, PartialEq, Eq)]
pub enum Pass {
    /// The file was read to the end.
    Finished,
    /// `publish` asked the scan to stop.
    Stopped,
}

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

/// Reads `path`, calling `publish` with the peaks so far.
///
/// The first hand-over is the first bin, then about twice a second of audio,
/// then whatever is left at the end. `publish` returns false to stop. The
/// caller runs this off the UI thread.
pub fn scan_file(path: &Path, mut publish: impl FnMut(Peaks) -> bool) -> Result<Pass, String> {
    let mut source = Source::open(path)?;
    let mut bins = Accumulator::new(source.sample_rate());
    let mut frames = Vec::new();
    let mut published = 0;
    while source.read(&mut frames)?.is_some() {
        bins.push_stereo(&frames);
        frames.clear();
        if !hand_over(&bins, &mut published, &mut publish) {
            return Ok(Pass::Stopped);
        }
    }
    let peaks = bins.finish();
    if (peaks.mins.len() != published || published == 0) && !publish(peaks) {
        return Ok(Pass::Stopped);
    }
    Ok(Pass::Finished)
}

/// Publishes complete bins until `published` is caught up. The open bin stays
/// out, so a later hand-over extends the earlier one.
fn hand_over(
    bins: &Accumulator,
    published: &mut usize,
    publish: &mut impl FnMut(Peaks) -> bool,
) -> bool {
    loop {
        let ready = bins.mins.len();
        if ready <= *published {
            return true;
        }
        let take = if *published == 0 {
            1
        } else if ready - *published >= HANDOVER_BINS {
            *published + HANDOVER_BINS
        } else {
            return true;
        };
        *published = take;
        if !publish(bins.prefix(take)) {
            return false;
        }
    }
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

    /// The first `bins` complete bins. The open bin is not included.
    fn prefix(&self, bins: usize) -> Peaks {
        let bins = bins.min(self.mins.len());
        Peaks {
            mins: self.mins[..bins].to_vec(),
            maxs: self.maxs[..bins].to_vec(),
            seconds: (bins * self.frames_per_bin) as f64 / f64::from(self.rate),
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
    use super::{columns, peaks_from_mono, scan_file, Pass, Peaks};

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
    fn a_scan_reports_peaks_before_the_file_ends() {
        let path = tone("early.wav", 16_000, 32_000);
        let mut seen = Vec::new();
        scan_file(&path, |peaks| {
            seen.push(peaks);
            true
        })
        .unwrap();
        assert!(seen.len() >= 2, "the scan reported only once");
        let first = &seen[0];
        let last = seen.last().unwrap();
        assert!(first.seconds < last.seconds);
        assert_eq!(&last.mins[..first.mins.len()], first.mins.as_slice());
        assert_eq!(&last.maxs[..first.maxs.len()], first.maxs.as_slice());
    }

    #[test]
    fn a_scan_stops_when_asked() {
        let path = tone("stop.wav", 16_000, 32_000);
        let full = scan_file(&path, |_| true).unwrap();
        let mut reports = 0;
        let stopped = scan_file(&path, |_| {
            reports += 1;
            false
        })
        .unwrap();
        assert_eq!(reports, 1);
        assert_eq!(stopped, Pass::Stopped);
        assert_eq!(full, Pass::Finished);
    }

    /// A mono 16-bit tone long enough to cross several hand-overs.
    fn tone(name: &str, rate: u32, frames: u32) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("djusb-peaks-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        let data_len = frames * 2;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16_u32.to_le_bytes());
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(&rate.to_le_bytes());
        bytes.extend_from_slice(&(rate * 2).to_le_bytes());
        bytes.extend_from_slice(&2_u16.to_le_bytes());
        bytes.extend_from_slice(&16_u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&data_len.to_le_bytes());
        for frame in 0..frames {
            let sample: i16 = if frame % 2 == 0 { 16_000 } else { -16_000 };
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        std::fs::write(&path, bytes).unwrap();
        path
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
