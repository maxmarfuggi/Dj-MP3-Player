//! Serving one track or artwork file.
//!
//! File types, ranges, the AIFF to WAV conversion, and the NFC/NFD retry live
//! here. The volume is only read. The desktop's protocol handler and the wall
//! player both call this instead of naming a path themselves.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use unicode_normalization::UnicodeNormalization;

use crate::aiff::{read_aiff_as_wav, read_aiff_pcm, wav_size};
use crate::library::LibraryManager;
use crate::range::{parse_range_header, ParsedRange};

/// Looks up a file the loaded libraries list. The caller never names a path.
pub trait MediaLibrary {
    fn resolve_track_path(&self, library_id: &str, track_id: &str) -> Option<PathBuf>;
    fn resolve_artwork_path(&self, library_id: &str, track_id: &str) -> Option<PathBuf>;
}

impl MediaLibrary for LibraryManager {
    fn resolve_track_path(&self, library_id: &str, track_id: &str) -> Option<PathBuf> {
        LibraryManager::resolve_track_path(self, library_id, track_id)
    }

    fn resolve_artwork_path(&self, library_id: &str, track_id: &str) -> Option<PathBuf> {
        LibraryManager::resolve_artwork_path(self, library_id, track_id)
    }
}

/// One response to a `media://track/…` or `media://artwork/…` request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaResponse {
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    /// Bytes to send, or `None` when the response has no body.
    pub body: Option<Vec<u8>>,
}

/// Serves one `media://track/<libraryId>/<trackId>` or
/// `media://artwork/<libraryId>/<trackId>` request.
///
/// A file that cannot be read after it was found is an `Err`. A request that
/// does not name a listed file is a 404 in the response.
pub fn serve_media(
    url: &str,
    range_header: Option<&str>,
    libraries: &impl MediaLibrary,
) -> io::Result<MediaResponse> {
    let Some(request) = parse_media_url(url) else {
        return Ok(not_found());
    };
    let resolved = match request.kind {
        MediaKind::Track => libraries.resolve_track_path(&request.library_id, &request.track_id),
        MediaKind::Artwork => {
            libraries.resolve_artwork_path(&request.library_id, &request.track_id)
        }
        MediaKind::Other => None,
    };
    let Some(file) = resolved.as_deref().and_then(find_file) else {
        return Ok(not_found());
    };
    let source = open_source(&file)?;

    let range = parse_range_header(range_header, source.size);
    if matches!(range, ParsedRange::Unsatisfiable) {
        let mut headers = BTreeMap::new();
        headers.insert(
            "Content-Range".to_string(),
            format!("bytes */{}", source.size),
        );
        return Ok(MediaResponse {
            status: 416,
            headers,
            body: None,
        });
    }

    let mut headers = BTreeMap::new();
    headers.insert("Content-Type".to_string(), source.content_type.to_string());
    headers.insert("Accept-Ranges".to_string(), "bytes".to_string());
    headers.insert("Access-Control-Allow-Origin".to_string(), "*".to_string());
    if source.size == 0 {
        return Ok(MediaResponse {
            status: 200,
            headers,
            body: None,
        });
    }

    let (start, end) = match range {
        ParsedRange::Satisfiable(range) => (range.start, range.end),
        ParsedRange::Absent => (0, source.size - 1),
        ParsedRange::Unsatisfiable => unreachable!("handled above"),
    };
    headers.insert("Content-Length".to_string(), (end - start + 1).to_string());
    let status = if matches!(range, ParsedRange::Satisfiable(_)) {
        headers.insert(
            "Content-Range".to_string(),
            format!("bytes {start}-{end}/{}", source.size),
        );
        206
    } else {
        200
    };
    Ok(MediaResponse {
        status,
        headers,
        body: Some(source.read(start, end)?),
    })
}

enum MediaKind {
    Track,
    Artwork,
    Other,
}

struct MediaRequest {
    kind: MediaKind,
    library_id: String,
    track_id: String,
}

/// `media://<host>/<libraryId>/<trackId>`. The host is compared case-insensitively,
/// matching URL hostname rules. Extra path segments are ignored.
fn parse_media_url(url: &str) -> Option<MediaRequest> {
    let rest = url.strip_prefix("media://")?;
    let (host, path) = rest.split_once('/')?;
    let mut parts = path.split('/').filter(|part| !part.is_empty());
    let library_id = decode_component(parts.next()?)?;
    let track_id = decode_component(parts.next()?)?;
    let kind = if host.eq_ignore_ascii_case("track") {
        MediaKind::Track
    } else if host.eq_ignore_ascii_case("artwork") {
        MediaKind::Artwork
    } else {
        MediaKind::Other
    };
    Some(MediaRequest {
        kind,
        library_id,
        track_id,
    })
}

fn decode_component(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return None;
            }
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).ok()?;
            decoded.push(u8::from_str_radix(hex, 16).ok()?);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).ok()
}

fn not_found() -> MediaResponse {
    MediaResponse {
        status: 404,
        headers: BTreeMap::new(),
        body: None,
    }
}

/// A file that exists, in the spelling the filesystem accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundFile {
    pub path: PathBuf,
    pub size: u64,
}

/// Libraries and file systems do not always agree on how accented characters
/// are composed, so a path that fails as written is retried in both Unicode
/// normalization forms.
#[must_use]
pub fn find_file(path: &Path) -> Option<FoundFile> {
    let Some(text) = path.to_str() else {
        return file_info(path);
    };
    let nfc: String = text.nfc().collect();
    let nfd: String = text.nfd().collect();
    let mut tried: Vec<&str> = Vec::new();
    for candidate in [text, nfc.as_str(), nfd.as_str()] {
        if tried.contains(&candidate) {
            continue;
        }
        tried.push(candidate);
        if let Some(found) = file_info(Path::new(candidate)) {
            return Some(found);
        }
    }
    None
}

fn file_info(path: &Path) -> Option<FoundFile> {
    let metadata = std::fs::metadata(path).ok()?;
    if metadata.is_file() {
        Some(FoundFile {
            path: path.to_path_buf(),
            size: metadata.len(),
        })
    } else {
        None
    }
}

/// MIME type for a file extension, or `application/octet-stream`.
#[must_use]
pub fn content_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("mp3") => "audio/mpeg",
        Some("m4a" | "mp4") => "audio/mp4",
        Some("aac") => "audio/aac",
        Some("wav") => "audio/wav",
        Some("flac") => "audio/flac",
        Some("ogg" | "opus") => "audio/ogg",
        Some("aif" | "aiff") => "audio/aiff",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("png") => "image/png",
        _ => "application/octet-stream",
    }
}

struct MediaSource {
    size: u64,
    content_type: &'static str,
    read: ReadBytes,
}

enum ReadBytes {
    File(PathBuf),
    Aiff {
        path: PathBuf,
        pcm: crate::aiff::AiffPcm,
    },
}

impl MediaSource {
    fn read(&self, start: u64, end: u64) -> io::Result<Vec<u8>> {
        match &self.read {
            ReadBytes::File(path) => read_file_range(path, start, end),
            ReadBytes::Aiff { path, pcm } => read_aiff_as_wav(path, pcm, start, end),
        }
    }
}

fn open_source(file: &FoundFile) -> io::Result<MediaSource> {
    let extension = file
        .path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    if extension == "aif" || extension == "aiff" {
        if let Some(pcm) = read_aiff_pcm(&file.path)? {
            return Ok(MediaSource {
                size: wav_size(&pcm),
                content_type: "audio/wav",
                read: ReadBytes::Aiff {
                    path: file.path.clone(),
                    pcm,
                },
            });
        }
    }
    Ok(MediaSource {
        size: file.size,
        content_type: content_type(&file.path),
        read: ReadBytes::File(file.path.clone()),
    })
}

fn read_file_range(path: &Path, start: u64, end: u64) -> io::Result<Vec<u8>> {
    if end < start {
        return Ok(Vec::new());
    }
    let mut file = File::open(path)?;
    file.seek(SeekFrom::Start(start))?;
    let length = (end - start + 1) as usize;
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

    use super::{content_type, serve_media, MediaLibrary, MediaResponse};

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "djusb-media-{}-{}",
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

    struct DirLibrary {
        directory: PathBuf,
        track: Option<PathBuf>,
    }

    impl MediaLibrary for DirLibrary {
        fn resolve_track_path(&self, _library_id: &str, track_id: &str) -> Option<PathBuf> {
            self.track
                .clone()
                .or_else(|| Some(self.directory.join(track_id)))
        }

        fn resolve_artwork_path(&self, _library_id: &str, _track_id: &str) -> Option<PathBuf> {
            None
        }
    }

    fn body_of(url: &str, range: Option<&str>, libraries: &impl MediaLibrary) -> Vec<u8> {
        let result = serve_media(url, range, libraries).unwrap();
        result.body.expect("response has a body")
    }

    fn header<'a>(response: &'a MediaResponse, name: &str) -> Option<&'a str> {
        response.headers.get(name).map(String::as_str)
    }

    /// A tiny uncompressed AIFF: two big-endian 16-bit samples, 0x0102 then 0x0304.
    fn stereo_frame() -> Vec<u8> {
        let samples = [0x01, 0x02, 0x03, 0x04];
        let mut comm = vec![0; 18];
        comm[0..2].copy_from_slice(&1i16.to_be_bytes());
        comm[2..6].copy_from_slice(&2u32.to_be_bytes());
        comm[6..8].copy_from_slice(&16i16.to_be_bytes());
        comm[8..10].copy_from_slice(&0x400eu16.to_be_bytes());
        comm[10..18].copy_from_slice(&(1u64 << 63).to_be_bytes());
        let mut body = Vec::new();
        body.extend_from_slice(b"AIFF");
        body.extend(chunk(b"COMM", &comm));
        let mut sound = vec![0; 8];
        sound.extend_from_slice(&samples);
        body.extend(chunk(b"SSND", &sound));
        let mut file = Vec::new();
        file.extend_from_slice(b"FORM");
        file.extend_from_slice(&(body.len() as u32).to_be_bytes());
        file.extend_from_slice(&body);
        file
    }

    fn chunk(id: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(id);
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        out.extend_from_slice(data);
        if data.len() % 2 == 1 {
            out.push(0);
        }
        out
    }

    #[test]
    fn rejects_a_request_that_does_not_name_a_listed_file() {
        let directory = TempDir::new();
        let libraries = DirLibrary {
            directory: directory.0.clone(),
            track: None,
        };
        for url in [
            "media://track/lib/nope.mp3",
            "media://artwork/lib/nope.mp3",
            "media://other/lib/nope.mp3",
        ] {
            let response = serve_media(url, None, &libraries).unwrap();
            assert_eq!(response.status, 404, "{url}");
            assert!(response.headers.is_empty(), "{url}");
            assert!(response.body.is_none(), "{url}");
        }
    }

    #[test]
    fn streams_a_byte_range_of_the_file() {
        let directory = TempDir::new();
        fs::write(directory.0.join("song.mp3"), b"abcdefghij").unwrap();
        let libraries = DirLibrary {
            directory: directory.0.clone(),
            track: None,
        };
        let result =
            serve_media("media://track/lib/song.mp3", Some("bytes=2-5"), &libraries).unwrap();
        assert_eq!(result.status, 206);
        assert_eq!(header(&result, "Content-Type"), Some("audio/mpeg"));
        assert_eq!(header(&result, "Content-Range"), Some("bytes 2-5/10"));
        assert_eq!(header(&result, "Content-Length"), Some("4"));
        assert_eq!(header(&result, "Access-Control-Allow-Origin"), Some("*"));
        assert_eq!(result.body.as_deref(), Some(&b"cdef"[..]));
    }

    #[test]
    fn reports_a_range_outside_the_file() {
        let directory = TempDir::new();
        fs::write(directory.0.join("song.mp3"), b"abcdefghij").unwrap();
        let libraries = DirLibrary {
            directory: directory.0.clone(),
            track: None,
        };
        let result = serve_media(
            "media://track/lib/song.mp3",
            Some("bytes=20-30"),
            &libraries,
        )
        .unwrap();
        assert_eq!(result.status, 416);
        assert_eq!(result.headers.len(), 1);
        assert_eq!(header(&result, "Content-Range"), Some("bytes */10"));
        assert!(result.body.is_none());
    }

    #[test]
    fn sends_no_body_for_an_empty_file() {
        let directory = TempDir::new();
        fs::write(directory.0.join("song.mp3"), b"").unwrap();
        let libraries = DirLibrary {
            directory: directory.0.clone(),
            track: None,
        };
        let result = serve_media("media://track/lib/song.mp3", None, &libraries).unwrap();
        assert_eq!(result.status, 200);
        assert!(header(&result, "Content-Length").is_none());
        assert!(result.body.is_none());
    }

    #[test]
    fn serves_uncompressed_aiff_as_wav() {
        let directory = TempDir::new();
        fs::write(directory.0.join("song.aiff"), stereo_frame()).unwrap();
        let libraries = DirLibrary {
            directory: directory.0.clone(),
            track: None,
        };
        let result = serve_media("media://track/lib/song.aiff", None, &libraries).unwrap();
        let wav = body_of("media://track/lib/song.aiff", None, &libraries);
        assert_eq!(result.status, 200);
        assert_eq!(header(&result, "Content-Type"), Some("audio/wav"));
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[44..], &[0x02, 0x01, 0x04, 0x03]);
    }

    #[test]
    fn finds_a_file_stored_in_the_other_unicode_normalization() {
        use unicode_normalization::UnicodeNormalization;

        let directory = TempDir::new();
        let composed: String = "café.mp3".nfc().collect();
        let decomposed: String = composed.nfd().collect();
        fs::write(directory.0.join(&decomposed), b"accented").unwrap();
        let libraries = DirLibrary {
            directory: directory.0.clone(),
            track: Some(directory.0.join(&composed)),
        };
        let url = format!("media://track/lib/{}", encode_component(&composed));
        let wav = body_of(&url, None, &libraries);
        assert_eq!(wav, b"accented");
    }

    fn encode_component(text: &str) -> String {
        let mut out = String::new();
        for byte in text.as_bytes() {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
                out.push(*byte as char);
            } else {
                out.push_str(&format!("%{byte:02X}"));
            }
        }
        out
    }

    #[test]
    fn content_type_covers_audio_and_artwork() {
        assert_eq!(content_type(Path::new("song.MP3")), "audio/mpeg");
        assert_eq!(content_type(Path::new("cover.JPEG")), "image/jpeg");
        assert_eq!(
            content_type(Path::new("notes.bin")),
            "application/octet-stream"
        );
    }
}
