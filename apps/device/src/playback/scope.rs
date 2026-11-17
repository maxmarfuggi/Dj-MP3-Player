//! The copy of the audio that the visualizers read.
//!
//! The output callback writes every stereo frame it plays here, before the
//! volume is applied, so the visualizers do not fade with the volume. The
//! buffer keeps the most recent frames and overwrites the oldest, and any
//! number of readers can look at it without slowing the callback. A reader
//! racing the writer can see a few frames from the next lap, which a waveform
//! or a spectrum does not show.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

/// Frames kept for the visualizers: about 170 ms at 48 kHz, enough for a
/// 4096-point spectrum and a full-width waveform.
pub const SCOPE_FRAMES: usize = 8192;

pub struct Scope {
    /// Interleaved left and right samples, stored as `f32` bits.
    samples: Box<[AtomicU32]>,
    /// Frames written since the player started.
    written: AtomicU64,
    rate: AtomicU32,
}

impl Scope {
    #[must_use]
    pub fn new(frames: usize) -> Self {
        Self {
            samples: (0..frames.max(1) * 2).map(|_| AtomicU32::new(0)).collect(),
            written: AtomicU64::new(0),
            rate: AtomicU32::new(0),
        }
    }

    /// Frames the buffer keeps.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.samples.len() / 2
    }

    /// Sample rate of the frames, or 0 before the output opens.
    #[must_use]
    pub fn sample_rate(&self) -> u32 {
        self.rate.load(Ordering::Relaxed)
    }

    /// Frames written since the player started.
    #[must_use]
    pub fn written(&self) -> u64 {
        self.written.load(Ordering::Acquire)
    }

    pub(super) fn set_sample_rate(&self, rate: u32) {
        self.rate.store(rate, Ordering::Relaxed);
    }

    /// Appends interleaved stereo frames. Only the output callback writes.
    pub(super) fn push(&self, stereo: &[f32]) {
        let start = self.written.load(Ordering::Relaxed);
        let capacity = self.capacity() as u64;
        for (frame, [left, right]) in stereo.as_chunks::<2>().0.iter().enumerate() {
            let slot = ((start + frame as u64) % capacity) as usize * 2;
            self.samples[slot].store(left.to_bits(), Ordering::Relaxed);
            self.samples[slot + 1].store(right.to_bits(), Ordering::Relaxed);
        }
        let frames = (stereo.len() / 2) as u64;
        self.written.store(start + frames, Ordering::Release);
    }

    /// Fills `out` with the most recent `out.len() / 2` frames, oldest first.
    /// Frames from before playback started are silence.
    pub fn latest(&self, out: &mut [f32]) {
        let end = self.written();
        let wanted = (out.len() / 2).min(self.capacity()) as u64;
        out.fill(0.0);
        let offset = out.len() / 2 - wanted as usize;
        for index in 0..wanted {
            let Some(frame) = (end + index).checked_sub(wanted) else {
                continue;
            };
            let at = (offset + index as usize) * 2;
            let (left, right) = self.frame(frame);
            out[at] = left;
            out[at + 1] = right;
        }
    }

    /// Appends the frames written since `cursor` and moves the cursor to the
    /// end. A reader more than the buffer behind skips to the oldest frame
    /// still kept.
    pub fn read_since(&self, cursor: &mut u64, out: &mut Vec<f32>) {
        let end = self.written();
        let start = (*cursor).max(end.saturating_sub(self.capacity() as u64));
        for frame in start..end {
            let (left, right) = self.frame(frame);
            out.push(left);
            out.push(right);
        }
        *cursor = end;
    }

    fn frame(&self, frame: u64) -> (f32, f32) {
        let slot = (frame % self.capacity() as u64) as usize * 2;
        (
            f32::from_bits(self.samples[slot].load(Ordering::Relaxed)),
            f32::from_bits(self.samples[slot + 1].load(Ordering::Relaxed)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::Scope;

    fn frames(values: &[f32]) -> Vec<f32> {
        values.iter().flat_map(|value| [*value, -*value]).collect()
    }

    #[test]
    fn latest_pads_with_silence_before_playback() {
        let scope = Scope::new(8);
        scope.push(&frames(&[1.0, 2.0]));
        let mut out = vec![9.0; 8];
        scope.latest(&mut out);
        assert_eq!(out, vec![0.0, 0.0, 0.0, 0.0, 1.0, -1.0, 2.0, -2.0]);
    }

    #[test]
    fn latest_keeps_the_newest_frames_after_wrapping() {
        let scope = Scope::new(4);
        scope.push(&frames(&[1.0, 2.0, 3.0]));
        scope.push(&frames(&[4.0, 5.0, 6.0]));
        let mut out = vec![0.0; 6];
        scope.latest(&mut out);
        assert_eq!(out, frames(&[4.0, 5.0, 6.0]));
    }

    #[test]
    fn read_since_returns_each_frame_once() {
        let scope = Scope::new(8);
        let mut cursor = 0;
        let mut out = Vec::new();
        scope.push(&frames(&[1.0, 2.0]));
        scope.read_since(&mut cursor, &mut out);
        scope.push(&frames(&[3.0]));
        scope.read_since(&mut cursor, &mut out);
        scope.read_since(&mut cursor, &mut out);
        assert_eq!(out, frames(&[1.0, 2.0, 3.0]));
        assert_eq!(cursor, 3);
    }

    #[test]
    fn a_slow_reader_skips_to_the_oldest_kept_frame() {
        let scope = Scope::new(2);
        let mut cursor = 0;
        let mut out = Vec::new();
        scope.push(&frames(&[1.0, 2.0, 3.0, 4.0, 5.0]));
        scope.read_since(&mut cursor, &mut out);
        assert_eq!(out, frames(&[4.0, 5.0]));
    }
}
