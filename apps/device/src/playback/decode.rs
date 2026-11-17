//! Reads one audio file as stereo `f32` frames with symphonia.

use std::fs::File;
use std::path::Path;
use std::time::Duration;

use symphonia::core::codecs::audio::{AudioDecoder, AudioDecoderOptions};
use symphonia::core::errors::Error;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo, Track, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::units::{Duration as Ticks, Time, TimeBase};

use super::convert::to_stereo;

/// Used when a file does not state its rate. Every format the players export does.
const FALLBACK_RATE: u32 = 44_100;

pub struct Source {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoder>,
    track_id: u32,
    time_base: Option<TimeBase>,
    rate: u32,
    length: Option<Duration>,
    /// Frames to drop after a seek, between the packet seeked to and the
    /// requested time.
    skip: u64,
    interleaved: Vec<f32>,
}

impl Source {
    /// Opens `path`, retrying it in both Unicode normalization forms.
    pub fn open(path: &Path) -> Result<Self, String> {
        let found = djusb_core::media::find_file(path).ok_or("The file is missing.")?;
        let file = File::open(&found.path).map_err(|error| error.to_string())?;
        let stream = MediaSourceStream::new(Box::new(file), Default::default());
        let mut hint = Hint::new();
        if let Some(extension) = found.path.extension().and_then(|text| text.to_str()) {
            hint.with_extension(extension);
        }
        let format = symphonia::default::get_probe()
            .probe(
                &hint,
                stream,
                FormatOptions::default(),
                MetadataOptions::default(),
            )
            .map_err(describe)?;

        let track = format
            .default_track(TrackType::Audio)
            .ok_or("The file has no audio.")?;
        let params = track
            .codec_params
            .as_ref()
            .and_then(|params| params.audio())
            .ok_or("The file has no audio.")?;
        let decoder = symphonia::default::get_codecs()
            .make_audio_decoder(params, &AudioDecoderOptions::default())
            .map_err(describe)?;
        let rate = params.sample_rate.unwrap_or(FALLBACK_RATE);
        let length = track_length(track, rate);
        let track_id = track.id;
        let time_base = track.time_base;

        Ok(Self {
            format,
            decoder,
            track_id,
            time_base,
            rate,
            length,
            skip: 0,
            interleaved: Vec::new(),
        })
    }

    /// The rate the file states. Decoded frames report their own rate, which
    /// wins if the two differ.
    #[must_use]
    pub fn sample_rate(&self) -> u32 {
        self.rate
    }

    /// The track's length, when the file states it.
    #[must_use]
    pub fn length(&self) -> Option<Duration> {
        self.length
    }

    /// Moves to `to` and returns where playback will start, which is `to`
    /// unless the file cannot say.
    pub fn seek(&mut self, to: Duration) -> Result<Duration, String> {
        let time =
            Time::try_from_secs_f64(to.as_secs_f64()).ok_or("The position is out of range.")?;
        let seeked = self
            .format
            .seek(
                SeekMode::Accurate,
                SeekTo::Time {
                    time,
                    track_id: Some(self.track_id),
                },
            )
            .map_err(describe)?;
        self.decoder.reset();

        let Some(time_base) = self.time_base else {
            self.skip = 0;
            return Ok(to);
        };
        let ahead = seeked
            .required_ts
            .get()
            .saturating_sub(seeked.actual_ts.get())
            .max(0);
        self.skip = time_base
            .calc_duration(Ticks::new(ahead.unsigned_abs()))
            .and_then(to_std)
            .map_or(0, |gap| {
                (gap.as_secs_f64() * f64::from(self.rate)).round() as u64
            });
        Ok(time_base
            .calc_time(seeked.required_ts)
            .and_then(to_std)
            .unwrap_or(to))
    }

    /// Decodes until it has frames, appends them to `out` as stereo, and
    /// returns their rate. Returns `None` at the end of the file.
    pub fn read(&mut self, out: &mut Vec<f32>) -> Result<Option<u32>, String> {
        loop {
            let packet = match self.format.next_packet() {
                Ok(Some(packet)) => packet,
                // A chained Ogg stream starting a new song ends this one.
                Ok(None) | Err(Error::ResetRequired) => return Ok(None),
                Err(error) => return Err(describe(error)),
            };
            if packet.track_id != self.track_id {
                continue;
            }
            let decoded = match self.decoder.decode(&packet) {
                Ok(decoded) => decoded,
                // One damaged packet is a short dropout, not the end of the track.
                Err(Error::DecodeError(_) | Error::IoError(_)) => continue,
                Err(error) => return Err(describe(error)),
            };
            let channels = decoded.spec().channels().count();
            let rate = decoded.spec().rate();
            decoded.copy_to_vec_interleaved(&mut self.interleaved);

            let start = out.len();
            to_stereo(&self.interleaved, channels, out);
            if self.skip > 0 {
                let frames = ((out.len() - start) / 2) as u64;
                let dropped = self.skip.min(frames);
                out.drain(start..start + dropped as usize * 2);
                self.skip -= dropped;
            }
            if out.len() > start {
                self.rate = rate;
                return Ok(Some(rate));
            }
        }
    }
}

fn track_length(track: &Track, rate: u32) -> Option<Duration> {
    if let (Some(time_base), Some(duration)) = (track.time_base, track.duration) {
        if let Some(length) = time_base.calc_duration(duration).and_then(to_std) {
            return Some(length);
        }
    }
    let frames = track.num_frames?;
    Some(Duration::from_secs_f64(frames as f64 / f64::from(rate)))
}

fn to_std(time: Time) -> Option<Duration> {
    let (seconds, nanos) = time.parts();
    u64::try_from(seconds)
        .ok()
        .map(|seconds| Duration::new(seconds, nanos))
}

fn describe(error: Error) -> String {
    match error {
        Error::Unsupported(_) => "The file's format is not supported.".to_string(),
        error => error.to_string(),
    }
}

/// Writes a 16-bit WAV whose left channel counts frames, so a test can tell
/// which frame it is hearing. Shared with the engine's tests.
#[cfg(test)]
pub(super) fn counting_wav(path: &Path, rate: u32, channels: u16, frames: u32) {
    let data_len = frames * u32::from(channels) * 2;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&channels.to_le_bytes());
    bytes.extend_from_slice(&rate.to_le_bytes());
    bytes.extend_from_slice(&(rate * u32::from(channels) * 2).to_le_bytes());
    bytes.extend_from_slice(&(channels * 2).to_le_bytes());
    bytes.extend_from_slice(&16_u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    for frame in 0..frames {
        let count = (frame % 32_768) as i16;
        bytes.extend_from_slice(&count.to_le_bytes());
        for _ in 1..channels {
            bytes.extend_from_slice(&(-count).to_le_bytes());
        }
    }
    std::fs::write(path, bytes).unwrap();
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use super::{counting_wav, Source};

    fn temp_wav(name: &str, rate: u32, channels: u16, frames: u32) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("djusb-decode-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        counting_wav(&path, rate, channels, frames);
        path
    }

    fn read_all(source: &mut Source) -> Vec<f32> {
        let mut out = Vec::new();
        while source.read(&mut out).unwrap().is_some() {}
        out
    }

    fn count(sample: f32) -> i32 {
        (sample * 32_768.0).round() as i32
    }

    #[test]
    fn reads_every_frame_with_its_rate_and_length() {
        let path = temp_wav("whole.wav", 44_100, 2, 22_050);
        let mut source = Source::open(&path).unwrap();
        assert_eq!(source.sample_rate(), 44_100);
        assert_eq!(source.length(), Some(Duration::from_millis(500)));
        let out = read_all(&mut source);
        assert_eq!(out.len(), 22_050 * 2);
        assert_eq!(count(out[2 * 1000]), 1000);
        assert_eq!(count(out[2 * 1000 + 1]), -1000);
    }

    #[test]
    fn mono_files_play_on_both_sides() {
        let path = temp_wav("mono.wav", 48_000, 1, 480);
        let out = read_all(&mut Source::open(&path).unwrap());
        assert_eq!(out.len(), 480 * 2);
        assert_eq!(count(out[2 * 7]), 7);
        assert_eq!(count(out[2 * 7 + 1]), 7);
    }

    #[test]
    fn a_seek_starts_on_the_requested_frame() {
        let path = temp_wav("seek.wav", 44_100, 2, 88_200);
        let mut source = Source::open(&path).unwrap();
        let position = source.seek(Duration::from_millis(1250)).unwrap();
        assert_eq!(position, Duration::from_millis(1250));
        let mut out = Vec::new();
        source.read(&mut out).unwrap();
        assert_eq!(count(out[0]), 55_125 % 32_768);
    }

    #[test]
    fn aiff_fixtures_decode_like_the_wav_the_desktop_serves() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/aiff");
        let mut checked = 0;
        for entry in std::fs::read_dir(&root).unwrap() {
            let case = entry.unwrap().path();
            let expected = case.join("expected.wav");
            if !expected.exists() {
                continue;
            }
            let aiff = read_all(&mut Source::open(&case.join("input.aiff")).unwrap());
            let wav = read_all(&mut Source::open(&expected).unwrap());
            assert_eq!(aiff, wav, "{}", case.display());
            checked += 1;
        }
        assert!(checked >= 6, "only {checked} fixtures");
    }

    #[test]
    fn a_missing_file_says_so() {
        let missing = std::env::temp_dir().join("djusb-no-such-track.mp3");
        assert_eq!(
            Source::open(&missing).err().as_deref(),
            Some("The file is missing.")
        );
    }

    #[test]
    fn a_file_that_is_not_audio_is_refused() {
        let dir = std::env::temp_dir().join(format!("djusb-decode-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("notes.mp3");
        std::fs::write(&path, b"not audio at all").unwrap();
        assert!(Source::open(&path).is_err());
    }
}
