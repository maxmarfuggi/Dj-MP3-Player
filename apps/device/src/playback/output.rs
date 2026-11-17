//! The audio output and the callback that feeds it.
//!
//! The engine writes stereo samples into a ring buffer and, alongside them,
//! markers saying where each track starts. The callback plays the samples and
//! publishes the track and position it reached, so the status matches what
//! is heard rather than what was decoded. A seek or a track change sets
//! `discard_until`, and the callback drops everything buffered before it.
//!
//! The callback never locks or allocates once its scratch buffer has grown to
//! the device's buffer size.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, StreamConfig};
use rtrb::{Consumer, Producer, RingBuffer};

use super::Shared;

/// Seconds of decoded audio buffered ahead of the output. Seeks and track
/// changes drop the buffer, so this only absorbs slow reads from the USB.
const BUFFER_SECONDS: usize = 2;

/// Markers waiting for the callback. One is written per track change or seek.
const MARKERS: usize = 256;

/// Where a track starts in the sample stream.
#[derive(Debug, Clone, Copy)]
pub(super) enum Marker {
    /// Samples from `at` on are queue entry `index`, starting `start` frames in.
    Track {
        at: u64,
        index: usize,
        start: u64,
        length: Option<u64>,
    },
    /// Nothing follows the samples before `at`.
    End { at: u64 },
}

impl Marker {
    fn at(self) -> u64 {
        match self {
            Self::Track { at, .. } | Self::End { at } => at,
        }
    }
}

/// An open output. Dropping it stops the stream.
pub(super) struct Output {
    _stream: cpal::Stream,
    samples: Producer<f32>,
    markers: Producer<Marker>,
    rate: u32,
    /// Samples written since the output opened.
    written: u64,
}

impl Output {
    /// Opens the default output device.
    pub fn open(shared: &Arc<Shared>) -> Result<Self, String> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or("No audio output is connected.")?;
        let config = choose_config(&device)?;
        let rate = config.sample_rate();
        let format = config.sample_format();
        let stream_config = config.config();

        let (samples, sample_reader) = RingBuffer::new(rate as usize * 2 * BUFFER_SECONDS);
        let (markers, marker_reader) = RingBuffer::new(MARKERS);
        shared.discard_until.store(0, Ordering::Release);
        shared.output_lost.store(false, Ordering::Release);
        shared.scope.set_sample_rate(rate);
        let mixer = Mixer::new(sample_reader, marker_reader, Arc::clone(shared));

        let stream = match format {
            SampleFormat::F32 => build::<f32>(&device, stream_config, mixer, shared),
            SampleFormat::I16 => build::<i16>(&device, stream_config, mixer, shared),
            SampleFormat::I32 => build::<i32>(&device, stream_config, mixer, shared),
            SampleFormat::U16 => build::<u16>(&device, stream_config, mixer, shared),
            other => return Err(format!("The audio output wants {other} samples.")),
        }
        .map_err(|error| error.to_string())?;
        stream.play().map_err(|error| error.to_string())?;

        Ok(Self {
            _stream: stream,
            samples,
            markers,
            rate,
            written: 0,
        })
    }

    #[must_use]
    pub fn rate(&self) -> u32 {
        self.rate
    }

    /// Samples written so far. A marker at this count applies to the next sample.
    #[must_use]
    pub fn written(&self) -> u64 {
        self.written
    }

    /// Writes as much of `samples` as fits and returns how many were written.
    pub fn write(&mut self, samples: &[f32]) -> usize {
        let (written, _) = self.samples.push_partial_slice(samples);
        let count = written.len();
        self.written += count as u64;
        count
    }

    pub fn mark(&mut self, marker: Marker) {
        if self.markers.push(marker).is_err() {
            eprintln!("Playback status is behind: the marker queue is full.");
        }
    }
}

fn choose_config(device: &cpal::Device) -> Result<cpal::SupportedStreamConfig, String> {
    let default = device
        .default_output_config()
        .map_err(|error| error.to_string())?;
    let rate = default.sample_rate();
    // Floats avoid a conversion when the device takes them at its usual rate.
    let float = device
        .supported_output_configs()
        .ok()
        .and_then(|mut configs| {
            configs.find(|range| {
                range.sample_format() == SampleFormat::F32
                    && range.channels() == default.channels()
                    && range.min_sample_rate() <= rate
                    && rate <= range.max_sample_rate()
            })
        });
    Ok(float.map_or(default, |range| range.with_sample_rate(rate)))
}

fn build<T>(
    device: &cpal::Device,
    config: StreamConfig,
    mut mixer: Mixer,
    shared: &Arc<Shared>,
) -> Result<cpal::Stream, cpal::Error>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = usize::from(config.channels);
    let mut mixed: Vec<f32> = Vec::with_capacity(8192);
    let lost = Arc::clone(shared);
    device.build_output_stream(
        config,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            mixed.resize(data.len(), 0.0);
            mixer.fill(&mut mixed, channels);
            for (out, sample) in data.iter_mut().zip(&mixed) {
                *out = T::from_sample(*sample);
            }
        },
        move |error: cpal::Error| match error.kind() {
            cpal::ErrorKind::DeviceNotAvailable
            | cpal::ErrorKind::StreamInvalidated
            | cpal::ErrorKind::HostUnavailable => {
                eprintln!("Audio output was lost: {error}");
                lost.output_lost.store(true, Ordering::Release);
            }
            _ => eprintln!("Audio output: {error}"),
        },
        None,
    )
}

/// The callback's side of the ring buffer.
pub(super) struct Mixer {
    samples: Consumer<f32>,
    markers: Consumer<Marker>,
    shared: Arc<Shared>,
    /// Samples read since the output opened.
    read: u64,
    /// Whether a track marker has been reached, so there is a position to publish.
    active: bool,
    /// Frames into the track at the last marker.
    start: u64,
    /// Frames played since the last marker.
    played: u64,
    /// The gain at the end of the last buffer, so volume changes ramp.
    gain: f32,
    stereo: Vec<f32>,
}

impl Mixer {
    pub fn new(samples: Consumer<f32>, markers: Consumer<Marker>, shared: Arc<Shared>) -> Self {
        let gain = shared.volume();
        Self {
            samples,
            markers,
            shared,
            read: 0,
            active: false,
            start: 0,
            played: 0,
            gain,
            stereo: Vec::with_capacity(8192),
        }
    }

    /// Fills one device buffer of `channels` interleaved channels.
    pub fn fill(&mut self, out: &mut [f32], channels: usize) {
        let channels = channels.max(1);
        let frames = out.len() / channels;
        self.drop_discarded();
        self.apply_markers();

        let mut filled = 0;
        if !self.shared.paused.load(Ordering::Relaxed) {
            let target = self.shared.volume();
            let step = (target - self.gain) / frames.max(1) as f32;
            while filled < frames {
                // A marker takes effect when the first sample after it plays.
                self.apply_markers();
                let until_marker = match self.markers.peek() {
                    Ok(marker) => {
                        usize::try_from((marker.at() - self.read) / 2).unwrap_or(usize::MAX)
                    }
                    Err(_) => usize::MAX,
                };
                let ready = self.samples.slots() / 2;
                let count = (frames - filled).min(until_marker).min(ready);
                if count == 0 {
                    break;
                }
                self.stereo.resize(count * 2, 0.0);
                let _ = self.samples.pop_partial_slice(&mut self.stereo);
                for (offset, [left, right]) in self.stereo.as_chunks::<2>().0.iter().enumerate() {
                    let gain = self.gain + step * (filled + offset + 1) as f32;
                    let frame = &mut out[(filled + offset) * channels..][..channels];
                    write_frame(frame, left * gain, right * gain);
                }
                self.shared.scope.push(&self.stereo);
                self.read += (count * 2) as u64;
                self.played += count as u64;
                filled += count;
            }
            self.gain = target;
        }
        out[filled * channels..].fill(0.0);

        if self.active {
            self.shared
                .position
                .store(self.start + self.played, Ordering::Release);
        }
    }

    fn drop_discarded(&mut self) {
        let until = self.shared.discard_until.load(Ordering::Acquire);
        while self.read < until {
            let wanted = usize::try_from(until - self.read).unwrap_or(usize::MAX);
            let count = wanted.min(self.samples.slots());
            if count == 0 {
                break;
            }
            if let Ok(chunk) = self.samples.read_chunk(count) {
                chunk.commit_all();
            }
            self.read += count as u64;
            // Markers inside the dropped audio take effect as they are passed.
            self.apply_markers();
        }
    }

    fn apply_markers(&mut self) {
        while let Ok(marker) = self.markers.peek() {
            if marker.at() > self.read {
                break;
            }
            let Ok(marker) = self.markers.pop() else {
                break;
            };
            match marker {
                Marker::Track {
                    index,
                    start,
                    length,
                    ..
                } => {
                    self.active = true;
                    self.start = start;
                    self.played = 0;
                    self.shared.track.store(index, Ordering::Release);
                    self.shared
                        .length
                        .store(length.unwrap_or(0), Ordering::Release);
                    self.shared.position.store(start, Ordering::Release);
                    self.shared.ended.store(false, Ordering::Release);
                }
                Marker::End { .. } => {
                    self.shared.ended.store(true, Ordering::Release);
                }
            }
        }
    }
}

/// Writes one stereo frame to a device frame. Mono devices get the average;
/// channels past the first two stay silent.
fn write_frame(frame: &mut [f32], left: f32, right: f32) {
    match frame {
        [] => {}
        [mono] => *mono = (left + right) * 0.5,
        [first, second, rest @ ..] => {
            *first = left;
            *second = right;
            rest.fill(0.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;
    use std::sync::Arc;

    use rtrb::RingBuffer;

    use super::{write_frame, Marker, Mixer};
    use crate::playback::{Shared, NO_TRACK};

    struct Rig {
        shared: Arc<Shared>,
        samples: rtrb::Producer<f32>,
        markers: rtrb::Producer<Marker>,
        mixer: Mixer,
        written: u64,
    }

    impl Rig {
        fn new() -> Self {
            Self::with_volume(1.0)
        }

        fn with_volume(volume: f32) -> Self {
            let shared = Arc::new(Shared::new());
            shared.set_volume(volume);
            let (samples, sample_reader) = RingBuffer::new(1024);
            let (markers, marker_reader) = RingBuffer::new(16);
            let mixer = Mixer::new(sample_reader, marker_reader, Arc::clone(&shared));
            Self {
                shared,
                samples,
                markers,
                mixer,
                written: 0,
            }
        }

        fn track(&mut self, index: usize, start: u64) {
            self.markers
                .push(Marker::Track {
                    at: self.written,
                    index,
                    start,
                    length: Some(1000),
                })
                .unwrap();
        }

        fn write(&mut self, samples: &[f32]) {
            for sample in samples {
                self.samples.push(*sample).unwrap();
            }
            self.written += samples.len() as u64;
        }

        fn fill(&mut self, frames: usize) -> Vec<f32> {
            let mut out = vec![9.0; frames * 2];
            self.mixer.fill(&mut out, 2);
            out
        }

        fn track_index(&self) -> usize {
            self.shared.track.load(Ordering::Acquire)
        }

        fn position(&self) -> u64 {
            self.shared.position.load(Ordering::Acquire)
        }
    }

    #[test]
    fn plays_buffered_frames_and_reports_the_position() {
        let mut rig = Rig::new();
        rig.track(0, 0);
        rig.write(&[0.5, -0.5, 0.25, -0.25]);
        assert_eq!(rig.fill(3), vec![0.5, -0.5, 0.25, -0.25, 0.0, 0.0]);
        assert_eq!(rig.track_index(), 0);
        assert_eq!(rig.position(), 2);
    }

    #[test]
    fn nothing_is_reported_before_the_first_track() {
        let mut rig = Rig::new();
        rig.fill(4);
        assert_eq!(rig.track_index(), NO_TRACK);
    }

    #[test]
    fn the_next_track_is_reported_when_it_is_heard() {
        let mut rig = Rig::new();
        rig.track(0, 0);
        rig.write(&[0.1; 4]);
        rig.track(1, 0);
        rig.write(&[0.2; 4]);
        rig.fill(2);
        assert_eq!(rig.track_index(), 0);
        assert_eq!(rig.position(), 2);
        rig.fill(1);
        assert_eq!(rig.track_index(), 1);
        assert_eq!(rig.position(), 1);
    }

    #[test]
    fn a_seek_drops_the_buffered_audio() {
        let mut rig = Rig::new();
        rig.track(0, 0);
        rig.write(&[0.1; 8]);
        rig.shared
            .discard_until
            .store(rig.written, Ordering::Release);
        rig.track(0, 48_000);
        rig.write(&[0.7, 0.7]);
        assert_eq!(rig.fill(2), vec![0.7, 0.7, 0.0, 0.0]);
        assert_eq!(rig.position(), 48_001);
    }

    #[test]
    fn a_seek_while_paused_reports_the_new_position() {
        let mut rig = Rig::new();
        rig.track(0, 0);
        rig.write(&[0.1; 8]);
        rig.fill(1);
        rig.shared.paused.store(true, Ordering::Release);
        rig.shared
            .discard_until
            .store(rig.written, Ordering::Release);
        rig.track(0, 9_000);
        rig.write(&[0.7, 0.7]);
        assert_eq!(rig.fill(2), vec![0.0; 4]);
        assert_eq!(rig.position(), 9_000);
    }

    #[test]
    fn pausing_holds_the_audio() {
        let mut rig = Rig::new();
        rig.track(0, 0);
        rig.write(&[0.5, 0.5]);
        rig.shared.paused.store(true, Ordering::Release);
        assert_eq!(rig.fill(1), vec![0.0, 0.0]);
        rig.shared.paused.store(false, Ordering::Release);
        assert_eq!(rig.fill(1), vec![0.5, 0.5]);
    }

    #[test]
    fn the_end_marker_reports_the_queue_ended() {
        let mut rig = Rig::new();
        rig.track(0, 0);
        rig.write(&[0.1, 0.1]);
        rig.markers.push(Marker::End { at: rig.written }).unwrap();
        rig.fill(1);
        assert!(!rig.shared.ended.load(Ordering::Acquire));
        rig.fill(1);
        assert!(rig.shared.ended.load(Ordering::Acquire));
    }

    #[test]
    fn a_volume_change_ramps_across_the_buffer() {
        let mut rig = Rig::new();
        rig.track(0, 0);
        rig.write(&[1.0; 8]);
        rig.shared.set_volume(0.0);
        let out = rig.fill(4);
        assert_eq!(out, vec![0.75, 0.75, 0.5, 0.5, 0.25, 0.25, 0.0, 0.0]);
    }

    #[test]
    fn the_visualizers_get_the_audio_before_the_volume() {
        let mut rig = Rig::with_volume(0.5);
        rig.track(0, 0);
        rig.write(&[0.8, 0.6]);
        assert_eq!(rig.fill(1), vec![0.4, 0.3]);
        let mut seen = vec![0.0; 2];
        rig.shared.scope.latest(&mut seen);
        assert_eq!(seen, vec![0.8, 0.6]);
    }

    #[test]
    fn device_frames_follow_the_channel_count() {
        let mut mono = [9.0];
        write_frame(&mut mono, 0.2, 0.4);
        assert!((mono[0] - 0.3).abs() < 1e-6);
        let mut quad = [9.0; 4];
        write_frame(&mut quad, 0.2, 0.4);
        assert_eq!(quad, [0.2, 0.4, 0.0, 0.0]);
    }
}
