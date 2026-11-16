//! Uncompressed AIFF presented as WAV.
//!
//! Chromium cannot play AIFF, which is common on DJ USBs. Uncompressed AIFF is
//! the same PCM audio as WAV with a different header and byte order, so it is
//! converted on the fly. The file on the volume is only read.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

const WAV_HEADER_SIZE: u64 = 44;
const CHUNK_HEADER_SIZE: u64 = 8;
const COMM_SIZE: usize = 18;
const COMM_SIZE_WITH_COMPRESSION: usize = 22;
const SSND_HEADER_SIZE: usize = 8;

/// PCM audio described by an AIFF `COMM` and `SSND` chunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiffPcm {
    pub channels: u16,
    pub sample_rate: u32,
    pub bytes_per_sample: u8,
    /// File offset of the first sample.
    pub data_offset: u64,
    pub data_length: u64,
    /// Plain AIFF is big-endian. AIFF-C `sowt` is already little-endian like WAV.
    pub big_endian: bool,
}

/// Describes the audio in an AIFF file.
///
/// Returns `Ok(None)` when the file is not uncompressed PCM. An IO failure
/// while opening or reading the file is returned as `Err`.
pub fn read_aiff_pcm(path: &Path) -> io::Result<Option<AiffPcm>> {
    let mut file = File::open(path)?;
    let file_size = file.seek(SeekFrom::End(0))?;

    let form = read_at(&mut file, 0, 12)?;
    if form.len() < 12 || &form[0..4] != b"FORM" {
        return Ok(None);
    }
    let form_type = &form[8..12];
    if form_type != b"AIFF" && form_type != b"AIFC" {
        return Ok(None);
    }

    let mut format: Option<Format> = None;
    let mut frame_count = 0u64;
    let mut sound: Option<Sound> = None;
    let mut at = 12u64;

    while at + CHUNK_HEADER_SIZE <= file_size && !(format.is_some() && sound.is_some()) {
        let header = read_at(&mut file, at, CHUNK_HEADER_SIZE as usize)?;
        if header.len() < CHUNK_HEADER_SIZE as usize {
            break;
        }
        let id = &header[0..4];
        let size = u32::from_be_bytes([header[4], header[5], header[6], header[7]]);
        let data_at = at + CHUNK_HEADER_SIZE;

        if id == b"COMM" {
            let comm = read_at(&mut file, data_at, COMM_SIZE_WITH_COMPRESSION)?;
            if comm.len() < COMM_SIZE {
                return Ok(None);
            }
            let compression = if comm.len() >= COMM_SIZE_WITH_COMPRESSION
                && size as usize >= COMM_SIZE_WITH_COMPRESSION
            {
                std::str::from_utf8(&comm[18..22]).unwrap_or("")
            } else {
                "NONE"
            };
            if compression != "NONE" && compression != "sowt" {
                return Ok(None);
            }
            frame_count = u32::from_be_bytes([comm[2], comm[3], comm[4], comm[5]]) as u64;
            let Some(parsed) = parse_format(&comm[..COMM_SIZE], compression == "NONE") else {
                return Ok(None);
            };
            format = Some(parsed);
        } else if id == b"SSND" {
            let ssnd = read_at(&mut file, data_at, SSND_HEADER_SIZE)?;
            if ssnd.len() < SSND_HEADER_SIZE {
                return Ok(None);
            }
            let offset = u32::from_be_bytes([ssnd[0], ssnd[1], ssnd[2], ssnd[3]]) as u64;
            let sample_at = data_at + SSND_HEADER_SIZE as u64 + offset;
            let chunk_end = data_at.saturating_add(size as u64).min(file_size);
            sound = Some(Sound {
                offset: sample_at,
                length: chunk_end.saturating_sub(sample_at),
            });
        }

        // Chunks are padded to an even length.
        let next = data_at.saturating_add(size as u64) + u64::from(size % 2);
        if next <= at {
            break;
        }
        at = next;
    }

    let (Some(format), Some(sound)) = (format, sound) else {
        return Ok(None);
    };
    let frame_size = u64::from(format.channels) * u64::from(format.bytes_per_sample);
    let frames = frame_count.min(sound.length / frame_size);
    let data_length = frames * frame_size;
    if data_length == 0 {
        return Ok(None);
    }
    Ok(Some(AiffPcm {
        channels: format.channels,
        sample_rate: format.sample_rate,
        bytes_per_sample: format.bytes_per_sample,
        data_offset: sound.offset,
        data_length,
        big_endian: format.big_endian,
    }))
}

struct Format {
    channels: u16,
    sample_rate: u32,
    bytes_per_sample: u8,
    big_endian: bool,
}

struct Sound {
    offset: u64,
    length: u64,
}

fn parse_format(comm: &[u8], big_endian: bool) -> Option<Format> {
    let channels = i16::from_be_bytes([comm[0], comm[1]]);
    let bits = i16::from_be_bytes([comm[6], comm[7]]);
    if channels < 1 || bits <= 0 {
        return None;
    }
    let bytes_per_sample = u16::try_from(bits).ok()?.div_ceil(8);
    let bytes_per_sample = u8::try_from(bytes_per_sample).ok()?;
    if !(1..=4).contains(&bytes_per_sample) {
        return None;
    }
    let rate = read_extended_float(&comm[8..18]).round();
    if !(1.0..=f64::from(u32::MAX)).contains(&rate) {
        return None;
    }
    Some(Format {
        channels: u16::try_from(channels).ok()?,
        sample_rate: rate as u32,
        bytes_per_sample,
        big_endian,
    })
}

/// AIFF stores the sample rate as an 80-bit IEEE 754 extended float.
fn read_extended_float(bytes: &[u8]) -> f64 {
    let exponent = u16::from_be_bytes([bytes[0], bytes[1]]) & 0x7fff;
    let hi = u32::from_be_bytes([bytes[2], bytes[3], bytes[4], bytes[5]]) as u64;
    let lo = u32::from_be_bytes([bytes[6], bytes[7], bytes[8], bytes[9]]) as u64;
    let mantissa = (hi << 32) | lo;
    let power = i32::from(exponent) - 16383 - 63;
    mantissa as f64 * 2f64.powi(power)
}

#[must_use]
pub fn wav_size(pcm: &AiffPcm) -> u64 {
    WAV_HEADER_SIZE + pcm.data_length
}

/// Bytes `start` to `end` (inclusive) of the WAV file equivalent to `path`.
pub fn read_aiff_as_wav(path: &Path, pcm: &AiffPcm, start: u64, end: u64) -> io::Result<Vec<u8>> {
    let mut out = Vec::new();
    if start < WAV_HEADER_SIZE {
        let header = wav_header(pcm);
        let from = start as usize;
        let to = end.saturating_add(1).min(WAV_HEADER_SIZE) as usize;
        out.extend_from_slice(&header[from..to]);
    }
    if end < WAV_HEADER_SIZE || pcm.data_length == 0 {
        return Ok(out);
    }

    let data_start = start.max(WAV_HEADER_SIZE) - WAV_HEADER_SIZE;
    let data_end = (end - WAV_HEADER_SIZE).min(pcm.data_length - 1);
    if data_end < data_start {
        return Ok(out);
    }

    // Samples can only be converted whole, so read from the sample boundaries
    // around the requested range and trim the excess afterwards.
    let size = u64::from(pcm.bytes_per_sample);
    let aligned_start = data_start - (data_start % size);
    let aligned_end = data_end - (data_end % size) + size - 1;
    let mut file = File::open(path)?;
    let mut samples = read_at(
        &mut file,
        pcm.data_offset + aligned_start,
        (aligned_end - aligned_start + 1) as usize,
    )?;
    let whole = samples.len() - (samples.len() % pcm.bytes_per_sample as usize);
    samples.truncate(whole);
    to_wav_samples(&mut samples, pcm);

    let leading = (data_start - aligned_start) as usize;
    let remaining = (data_end - data_start + 1) as usize;
    if leading < samples.len() {
        let end_index = (leading + remaining).min(samples.len());
        out.extend_from_slice(&samples[leading..end_index]);
    }
    Ok(out)
}

fn wav_header(pcm: &AiffPcm) -> [u8; WAV_HEADER_SIZE as usize] {
    let frame_size = u32::from(pcm.channels) * u32::from(pcm.bytes_per_sample);
    let mut header = [0u8; WAV_HEADER_SIZE as usize];
    header[0..4].copy_from_slice(b"RIFF");
    header[4..8]
        .copy_from_slice(&(WAV_HEADER_SIZE as u32 - 8 + pcm.data_length as u32).to_le_bytes());
    header[8..16].copy_from_slice(b"WAVEfmt ");
    header[16..20].copy_from_slice(&16u32.to_le_bytes());
    header[20..22].copy_from_slice(&1u16.to_le_bytes());
    header[22..24].copy_from_slice(&pcm.channels.to_le_bytes());
    header[24..28].copy_from_slice(&pcm.sample_rate.to_le_bytes());
    header[28..32].copy_from_slice(&(pcm.sample_rate.wrapping_mul(frame_size)).to_le_bytes());
    header[32..34].copy_from_slice(&(frame_size as u16).to_le_bytes());
    header[34..36].copy_from_slice(&(u16::from(pcm.bytes_per_sample) * 8).to_le_bytes());
    header[36..40].copy_from_slice(b"data");
    header[40..44].copy_from_slice(&(pcm.data_length as u32).to_le_bytes());
    header
}

/// Converts whole samples in place from AIFF's representation to WAV's.
fn to_wav_samples(samples: &mut [u8], pcm: &AiffPcm) {
    if pcm.bytes_per_sample == 1 {
        // 8-bit audio is signed in AIFF but unsigned in WAV.
        for sample in samples.iter_mut() {
            *sample ^= 0x80;
        }
        return;
    }
    if !pcm.big_endian {
        return;
    }
    if pcm.bytes_per_sample == 2 {
        for chunk in samples.as_chunks_mut::<2>().0 {
            chunk.swap(0, 1);
        }
    } else if pcm.bytes_per_sample == 4 {
        for chunk in samples.as_chunks_mut::<4>().0 {
            chunk.swap(0, 3);
            chunk.swap(1, 2);
        }
    } else {
        for chunk in samples.as_chunks_mut::<3>().0 {
            chunk.swap(0, 2);
        }
    }
}

fn read_at(file: &mut File, position: u64, length: usize) -> io::Result<Vec<u8>> {
    file.seek(SeekFrom::Start(position))?;
    let mut buffer = vec![0; length];
    let mut filled = 0;
    while filled < length {
        match file.read(&mut buffer[filled..]) {
            Ok(0) => break,
            Ok(read) => filled += read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    buffer.truncate(filled);
    Ok(buffer)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use super::{read_aiff_as_wav, read_aiff_pcm, wav_size, AiffPcm};

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "djusb-aiff-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    struct AiffSpec {
        channels: u16,
        bits_per_sample: u16,
        sample_rate: u64,
        samples: Vec<u8>,
        compression: Option<&'static str>,
        sound_first: bool,
    }

    fn write_aiff(directory: &Path, spec: AiffSpec) -> (PathBuf, AiffPcm) {
        let path = directory.join(format!("{}.aiff", directory.read_dir().unwrap().count()));
        fs::write(&path, build_aiff(&spec)).unwrap();
        let pcm = read_aiff_pcm(&path).unwrap().expect("readable AIFF");
        (path, pcm)
    }

    fn read_wav(path: &Path, pcm: &AiffPcm, start: u64, end: u64) -> Vec<u8> {
        read_aiff_as_wav(path, pcm, start, end).unwrap()
    }

    fn chunk(id: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(8 + data.len() + 1);
        out.extend_from_slice(id);
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        out.extend_from_slice(data);
        if data.len() % 2 == 1 {
            out.push(0);
        }
        out
    }

    /// A positive integer as an 80-bit extended float, matching `aiffBuilder.ts`.
    fn extended_float(value: u64) -> [u8; 10] {
        let exponent = 63 - i32::try_from(value.leading_zeros()).unwrap();
        let mut buffer = [0u8; 10];
        let biased = u16::try_from(16383 + exponent).unwrap();
        buffer[0..2].copy_from_slice(&biased.to_be_bytes());
        buffer[2..10].copy_from_slice(&(value << (63 - exponent)).to_be_bytes());
        buffer
    }

    fn build_aiff(spec: &AiffSpec) -> Vec<u8> {
        let frame_size =
            usize::from(spec.channels) * (usize::from(spec.bits_per_sample)).div_ceil(8);
        let mut comm = Vec::new();
        comm.extend_from_slice(&spec.channels.to_be_bytes());
        let frames = (spec.samples.len() / frame_size) as u32;
        comm.extend_from_slice(&frames.to_be_bytes());
        comm.extend_from_slice(&spec.bits_per_sample.to_be_bytes());
        comm.extend_from_slice(&extended_float(spec.sample_rate));
        if let Some(compression) = spec.compression {
            comm.extend_from_slice(compression.as_bytes());
            comm.extend_from_slice(&[0, 0]);
        }
        let comm_chunk = chunk(b"COMM", &comm);
        let mut sound = vec![0; 8];
        sound.extend_from_slice(&spec.samples);
        let sound_chunk = chunk(b"SSND", &sound);
        let name_chunk = chunk(b"NAME", b"abc");
        let mut body = Vec::new();
        body.extend_from_slice(if spec.compression.is_some() {
            b"AIFC"
        } else {
            b"AIFF"
        });
        if spec.sound_first {
            body.extend_from_slice(&name_chunk);
            body.extend_from_slice(&sound_chunk);
            body.extend_from_slice(&comm_chunk);
        } else {
            body.extend_from_slice(&name_chunk);
            body.extend_from_slice(&comm_chunk);
            body.extend_from_slice(&sound_chunk);
        }
        let mut file = Vec::new();
        file.extend_from_slice(b"FORM");
        file.extend_from_slice(&(body.len() as u32).to_be_bytes());
        file.extend_from_slice(&body);
        file
    }

    #[test]
    fn produces_a_wav_file_with_little_endian_16_bit_samples() {
        let directory = TempDir::new();
        let (path, pcm) = write_aiff(
            &directory.0,
            AiffSpec {
                channels: 2,
                bits_per_sample: 16,
                sample_rate: 44100,
                samples: vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08],
                compression: None,
                sound_first: false,
            },
        );
        let wav = read_wav(&path, &pcm, 0, wav_size(&pcm) - 1);

        assert_eq!(wav.len() as u64, wav_size(&pcm));
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(
            u32::from_le_bytes(wav[4..8].try_into().unwrap()),
            wav.len() as u32 - 8
        );
        assert_eq!(&wav[8..16], b"WAVEfmt ");
        assert_eq!(u16::from_le_bytes(wav[20..22].try_into().unwrap()), 1);
        assert_eq!(u16::from_le_bytes(wav[22..24].try_into().unwrap()), 2);
        assert_eq!(u32::from_le_bytes(wav[24..28].try_into().unwrap()), 44100);
        assert_eq!(
            u32::from_le_bytes(wav[28..32].try_into().unwrap()),
            44100 * 4
        );
        assert_eq!(u16::from_le_bytes(wav[32..34].try_into().unwrap()), 4);
        assert_eq!(u16::from_le_bytes(wav[34..36].try_into().unwrap()), 16);
        assert_eq!(u32::from_le_bytes(wav[40..44].try_into().unwrap()), 8);
        assert_eq!(
            &wav[44..],
            &[0x02, 0x01, 0x04, 0x03, 0x06, 0x05, 0x08, 0x07]
        );
    }

    #[test]
    fn reverses_the_bytes_of_24_bit_samples() {
        let directory = TempDir::new();
        let (path, pcm) = write_aiff(
            &directory.0,
            AiffSpec {
                channels: 1,
                bits_per_sample: 24,
                sample_rate: 48000,
                samples: vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06],
                compression: None,
                sound_first: false,
            },
        );
        let wav = read_wav(&path, &pcm, 0, wav_size(&pcm) - 1);
        assert_eq!(u32::from_le_bytes(wav[24..28].try_into().unwrap()), 48000);
        assert_eq!(u16::from_le_bytes(wav[34..36].try_into().unwrap()), 24);
        assert_eq!(&wav[44..], &[0x03, 0x02, 0x01, 0x06, 0x05, 0x04]);
    }

    #[test]
    fn keeps_aiff_c_sowt_samples_which_are_already_little_endian() {
        let samples = vec![0x01, 0x02, 0x03, 0x04];
        let directory = TempDir::new();
        let (path, pcm) = write_aiff(
            &directory.0,
            AiffSpec {
                channels: 1,
                bits_per_sample: 16,
                sample_rate: 44100,
                samples: samples.clone(),
                compression: Some("sowt"),
                sound_first: false,
            },
        );
        assert_eq!(
            &read_wav(&path, &pcm, 0, wav_size(&pcm) - 1)[44..],
            &samples
        );
    }

    #[test]
    fn makes_signed_8_bit_samples_unsigned() {
        let directory = TempDir::new();
        let (path, pcm) = write_aiff(
            &directory.0,
            AiffSpec {
                channels: 1,
                bits_per_sample: 8,
                sample_rate: 22050,
                samples: vec![0x00, 0x7f, 0x80, 0xff],
                compression: None,
                sound_first: false,
            },
        );
        assert_eq!(
            &read_wav(&path, &pcm, 0, wav_size(&pcm) - 1)[44..],
            &[0x80, 0xff, 0x00, 0x7f]
        );
    }

    #[test]
    fn finds_the_audio_when_the_sound_chunk_comes_before_the_format_chunk() {
        let directory = TempDir::new();
        let (path, pcm) = write_aiff(
            &directory.0,
            AiffSpec {
                channels: 1,
                bits_per_sample: 16,
                sample_rate: 44100,
                samples: vec![0x01, 0x02],
                compression: None,
                sound_first: true,
            },
        );
        assert_eq!(
            &read_wav(&path, &pcm, 0, wav_size(&pcm) - 1)[44..],
            &[0x02, 0x01]
        );
    }

    #[test]
    fn serves_any_byte_range_of_the_wav_file_including_ones_that_split_samples() {
        let samples: Vec<u8> = (0..3 * 2 * 30_000)
            .map(|index| (index % 251) as u8)
            .collect();
        let tail = [
            samples[samples.len() - 1],
            samples[samples.len() - 2],
            samples[samples.len() - 3],
        ];
        let directory = TempDir::new();
        let (path, pcm) = write_aiff(
            &directory.0,
            AiffSpec {
                channels: 2,
                bits_per_sample: 24,
                sample_rate: 44100,
                samples,
                compression: None,
                sound_first: false,
            },
        );
        let last = wav_size(&pcm) - 1;
        let whole = read_wav(&path, &pcm, 0, last);
        assert_eq!(&whole[44..50], &[2, 1, 0, 5, 4, 3]);
        assert_eq!(&whole[whole.len() - 3..], &tail);

        let ranges = [
            (0, 0),
            (0, 43),
            (10, 50),
            (43, 44),
            (44, last),
            (45, 46),
            (46, 100),
            (101, 101),
            (44 + 65_530, 44 + 65_545),
            (100, 150_000),
            (last - 1, last),
            (last, last),
        ];
        for (start, end) in ranges {
            assert_eq!(
                read_wav(&path, &pcm, start, end),
                whole[start as usize..=end as usize],
                "bytes {start}-{end}"
            );
        }
    }

    #[test]
    fn declines_compressed_aiff_c_and_files_that_are_not_aiff() {
        let directory = TempDir::new();
        let compressed = directory.0.join("compressed.aiff");
        fs::write(
            &compressed,
            build_aiff(&AiffSpec {
                channels: 1,
                bits_per_sample: 16,
                sample_rate: 44100,
                samples: vec![0, 0, 0, 0],
                compression: Some("ima4"),
                sound_first: false,
            }),
        )
        .unwrap();
        let not_aiff = directory.0.join("not.aiff");
        fs::write(&not_aiff, b"ID3 this is really an mp3").unwrap();

        assert!(read_aiff_pcm(&compressed).unwrap().is_none());
        assert!(read_aiff_pcm(&not_aiff).unwrap().is_none());
    }

    #[test]
    fn aiff_fixtures_match_expected_wav() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/aiff");
        let mut cases = 0;
        for entry in fs::read_dir(&root).unwrap() {
            let entry = entry.unwrap();
            if !entry.file_type().unwrap().is_dir() {
                continue;
            }
            cases += 1;
            let input = entry.path().join("input.aiff");
            let expected_path = entry.path().join("expected.wav");
            let pcm = read_aiff_pcm(&input).unwrap();
            if !expected_path.exists() {
                assert!(pcm.is_none(), "{}", entry.path().display());
                continue;
            }
            let pcm = pcm.expect("fixture has a wav, so the aiff is pcm");
            let wav = read_aiff_as_wav(&input, &pcm, 0, wav_size(&pcm) - 1).unwrap();
            assert_eq!(
                wav,
                fs::read(expected_path).unwrap(),
                "{}",
                entry.path().display()
            );
        }
        assert!(cases > 0, "fixtures/aiff has cases");
    }
}
