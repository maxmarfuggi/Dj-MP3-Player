//! Converts decoded audio to the output's sample rate.
//!
//! Everything here is interleaved stereo. A track at the output's rate passes
//! through untouched. Otherwise rubato's FFT resampler converts it in chunks,
//! and [`Converter::finish`] flushes the tail so the converted track is as
//! long as the original.

use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Indexing, Resampler};

/// Input frames per resampler chunk.
const CHUNK_FRAMES: usize = 1024;

pub struct Converter {
    input_rate: u32,
    stage: Option<Stage>,
}

struct Stage {
    resampler: Fft<f32>,
    input_rate: u32,
    output_rate: u32,
    /// Input frames waiting for a full chunk.
    input: Vec<f32>,
    output: Vec<f32>,
    /// Leading output frames that are the resampler's delay, not audio.
    delay_left: usize,
    frames_in: u64,
    frames_out: u64,
}

impl Converter {
    #[must_use]
    pub fn new(input_rate: u32, output_rate: u32) -> Self {
        let stage = if input_rate == output_rate {
            None
        } else {
            match Fft::<f32>::new(
                input_rate as usize,
                output_rate as usize,
                CHUNK_FRAMES,
                2,
                FixedSync::Input,
            ) {
                Ok(resampler) => Some(Stage::new(resampler, input_rate, output_rate)),
                Err(error) => {
                    eprintln!("Playing {input_rate} Hz audio at the wrong speed: {error}");
                    None
                }
            }
        };
        Self { input_rate, stage }
    }

    /// The rate this converter expects. A track whose rate changes mid-way
    /// needs a new converter.
    #[must_use]
    pub fn input_rate(&self) -> u32 {
        self.input_rate
    }

    /// Converts `samples` and appends whatever is ready to `out`.
    pub fn push(&mut self, samples: &[f32], out: &mut Vec<f32>) {
        match &mut self.stage {
            None => out.extend_from_slice(samples),
            Some(stage) => stage.push(samples, out),
        }
    }

    /// Appends the rest of the track. The converter is then ready for a new one.
    pub fn finish(&mut self, out: &mut Vec<f32>) {
        if let Some(stage) = &mut self.stage {
            stage.finish(out);
        }
    }
}

impl Stage {
    fn new(resampler: Fft<f32>, input_rate: u32, output_rate: u32) -> Self {
        let delay_left = resampler.output_delay();
        Self {
            resampler,
            input_rate,
            output_rate,
            input: Vec::new(),
            output: Vec::new(),
            delay_left,
            frames_in: 0,
            frames_out: 0,
        }
    }

    fn push(&mut self, samples: &[f32], out: &mut Vec<f32>) {
        self.input.extend_from_slice(samples);
        self.frames_in += (samples.len() / 2) as u64;
        while self.input.len() / 2 >= self.resampler.input_frames_next() {
            self.process(None, u64::MAX, out);
        }
    }

    fn finish(&mut self, out: &mut Vec<f32>) {
        let expected = u64::try_from(
            (u128::from(self.frames_in) * u128::from(self.output_rate)
                + u128::from(self.input_rate / 2))
                / u128::from(self.input_rate),
        )
        .unwrap_or(u64::MAX);
        let remaining = self.input.len() / 2;
        if remaining > 0 {
            self.process(Some(remaining), expected, out);
        }
        // Silent chunks push the last real frames out through the delay.
        let mut flushes = 0;
        while self.frames_out < expected && flushes < 8 {
            self.process(Some(0), expected, out);
            flushes += 1;
        }
        self.resampler.reset();
        self.input.clear();
        self.delay_left = self.resampler.output_delay();
        self.frames_in = 0;
        self.frames_out = 0;
    }

    /// Runs one chunk. `partial` is the number of real frames when the input
    /// is short of a full chunk. Output stops at `limit` frames.
    fn process(&mut self, partial: Option<usize>, limit: u64, out: &mut Vec<f32>) {
        let needed = self.resampler.input_frames_next();
        if self.input.len() < needed * 2 {
            self.input.resize(needed * 2, 0.0);
        }
        let capacity = self.resampler.output_frames_max();
        self.output.resize(capacity * 2, 0.0);

        let indexing = partial.map(|frames| Indexing::new().partial_len(frames));
        let result = match (
            InterleavedSlice::new(&self.input[..needed * 2], 2, needed),
            InterleavedSlice::new_mut(&mut self.output, 2, capacity),
        ) {
            (Ok(input), Ok(mut output)) => {
                self.resampler
                    .process_into_buffer(&input, &mut output, indexing.as_ref())
            }
            _ => {
                self.input.clear();
                return;
            }
        };
        let (used, produced) = match result {
            Ok(counts) => counts,
            Err(error) => {
                eprintln!("Resampling failed: {error}");
                self.input.clear();
                return;
            }
        };
        let used = partial.map_or(used, |_| self.input.len() / 2);
        self.input.drain(..used * 2);

        let skipped = self.delay_left.min(produced);
        self.delay_left -= skipped;
        let room = usize::try_from(limit.saturating_sub(self.frames_out)).unwrap_or(usize::MAX);
        let kept = (produced - skipped).min(room);
        out.extend_from_slice(&self.output[skipped * 2..(skipped + kept) * 2]);
        self.frames_out += kept as u64;
    }
}

/// Appends `interleaved` as stereo. Mono is copied to both sides; anything
/// wider keeps its first two channels, which are front left and right.
pub fn to_stereo(interleaved: &[f32], channels: usize, out: &mut Vec<f32>) {
    match channels {
        0 => {}
        1 => {
            for sample in interleaved {
                out.push(*sample);
                out.push(*sample);
            }
        }
        2 => out.extend_from_slice(interleaved),
        _ => {
            for frame in interleaved.chunks_exact(channels) {
                out.push(frame[0]);
                out.push(frame[1]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{to_stereo, Converter};

    fn sine(rate: u32, seconds: f32) -> Vec<f32> {
        let frames = (rate as f32 * seconds) as usize;
        (0..frames)
            .flat_map(|frame| {
                let value = (frame as f32 * 440.0 * std::f32::consts::TAU / rate as f32).sin();
                [value, value]
            })
            .collect()
    }

    fn convert(input_rate: u32, output_rate: u32, input: &[f32], piece: usize) -> Vec<f32> {
        let mut converter = Converter::new(input_rate, output_rate);
        let mut out = Vec::new();
        for chunk in input.chunks(piece * 2) {
            converter.push(chunk, &mut out);
        }
        converter.finish(&mut out);
        out
    }

    #[test]
    fn the_same_rate_passes_through() {
        let input = sine(48_000, 0.1);
        assert_eq!(convert(48_000, 48_000, &input, 333), input);
    }

    #[test]
    fn a_converted_track_keeps_its_length() {
        let input = sine(44_100, 1.0);
        let out = convert(44_100, 48_000, &input, 777);
        assert_eq!(out.len(), 48_000 * 2);
    }

    #[test]
    fn a_short_track_is_not_lost_in_the_delay() {
        let input = sine(44_100, 0.01);
        let out = convert(44_100, 48_000, &input, 4096);
        assert_eq!(out.len(), 480 * 2);
        assert!(out.iter().any(|sample| sample.abs() > 0.5));
    }

    #[test]
    fn conversion_keeps_the_level_and_skips_the_delay() {
        let input = sine(44_100, 0.5);
        let out = convert(44_100, 48_000, &input, 1000);
        // The resampler's delay is silence. Dropping it means the first
        // frames already carry the tone, which starts at zero and rises.
        let early_peak = out[..200].iter().fold(0.0_f32, |peak, s| peak.max(s.abs()));
        assert!(early_peak > 0.5, "early peak {early_peak}");
        let peak = out.iter().fold(0.0_f32, |peak, s| peak.max(s.abs()));
        assert!((0.9..1.1).contains(&peak), "peak {peak}");
    }

    #[test]
    fn a_converter_can_be_reused_after_finishing() {
        let input = sine(44_100, 0.25);
        let mut converter = Converter::new(44_100, 48_000);
        let mut first = Vec::new();
        converter.push(&input, &mut first);
        converter.finish(&mut first);
        let mut second = Vec::new();
        converter.push(&input, &mut second);
        converter.finish(&mut second);
        assert_eq!(first.len(), second.len());
    }

    #[test]
    fn mono_is_copied_to_both_sides() {
        let mut out = Vec::new();
        to_stereo(&[0.1, 0.2], 1, &mut out);
        assert_eq!(out, vec![0.1, 0.1, 0.2, 0.2]);
    }

    #[test]
    fn surround_keeps_front_left_and_right() {
        let mut out = Vec::new();
        to_stereo(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], 3, &mut out);
        assert_eq!(out, vec![1.0, 2.0, 4.0, 5.0]);
    }
}
