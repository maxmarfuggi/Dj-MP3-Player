//! Serato `database V2` and `.crate` files.
//!
//! An empty `_Serato_` folder is not a library. macOS `._*` files are ignored.

mod chunks;

use std::collections::{HashMap, HashSet};
use std::path::Path;

use icu_collator::options::{CollatorOptions, Strength};
use icu_collator::preferences::CollationNumericOrdering;
use icu_collator::{Collator, CollatorBorrowed, CollatorPreferences};
use icu_locale_core::locale;

use crate::paths::{exists, file_name_without_extension, to_relative_path};
use crate::{library_id, Library, LibrarySource, PlaylistKind, PlaylistNode, Track};
use chunks::{decode_text, read_chunks, tag_is};

const DATABASE_PATH: &str = "database V2";
const CRATE_EXTENSION: &str = ".crate";
/// `House%%Deep.crate` is the crate "Deep" nested inside the crate "House".
const CRATE_NESTING_SEPARATOR: &str = "%%";

/// A volume root, or a folder the user picked in its place.
pub struct Volume {
    pub path: String,
    pub name: String,
}

pub struct SeratoCrate {
    /// Full name including parents, for example `House%%Deep`.
    pub name: String,
    pub track_paths: Vec<String>,
}

#[derive(Debug)]
pub enum SeratoError {
    Io(std::io::Error),
    /// The volume path is not valid Unicode, so it cannot be stored on a library.
    Path,
}

impl std::fmt::Display for SeratoError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "{error}"),
            Self::Path => write!(formatter, "volume path is not valid Unicode"),
        }
    }
}

impl std::error::Error for SeratoError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Path => None,
        }
    }
}

impl From<std::io::Error> for SeratoError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

/// Reads a Serato library, or `Ok(None)` when the volume has none.
pub fn load_volume(volume_path: &Path, volume_name: &str) -> Result<Option<Library>, SeratoError> {
    let database_path = volume_path.join("_Serato_").join(DATABASE_PATH);
    let crate_files = list_crate_files(volume_path)?;
    if !exists(&database_path) && crate_files.is_empty() {
        return Ok(None);
    }

    let database_tracks = if exists(&database_path) {
        parse_database(&std::fs::read(&database_path)?)
    } else {
        Vec::new()
    };
    let mut crates = Vec::with_capacity(crate_files.len());
    for file_name in &crate_files {
        let bytes = std::fs::read(
            volume_path
                .join("_Serato_")
                .join("Subcrates")
                .join(file_name),
        )?;
        let name = file_name.strip_suffix(CRATE_EXTENSION).unwrap_or(file_name);
        crates.push(SeratoCrate {
            name: name.to_string(),
            track_paths: parse_crate(&bytes),
        });
    }

    let path = volume_path.to_str().ok_or(SeratoError::Path)?.to_string();
    Ok(Some(build_serato_library(
        database_tracks,
        &crates,
        &Volume {
            path,
            name: volume_name.to_string(),
        },
    )))
}

/// Crate file names, in UTF-16 code-unit order. `._*` files are left out.
fn list_crate_files(volume_path: &Path) -> Result<Vec<String>, SeratoError> {
    let directory = volume_path.join("_Serato_").join("Subcrates");
    let entries = match std::fs::read_dir(&directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let mut names = Vec::new();
    for entry in entries {
        let name = entry?.file_name();
        let Some(name) = name.to_str() else {
            return Err(SeratoError::Path);
        };
        if name.ends_with(CRATE_EXTENSION) && !name.starts_with("._") {
            names.push(name.to_string());
        }
    }
    names.sort_by(|left, right| left.encode_utf16().cmp(right.encode_utf16()));
    Ok(names)
}

/// Reads every track from a `database V2` file.
pub fn parse_database(file: &[u8]) -> Vec<Track> {
    let mut tracks = Vec::new();
    for chunk in read_chunks(file) {
        if !tag_is(chunk.tag, "otrk") {
            continue;
        }
        let mut fields = HashMap::new();
        for field in read_chunks(chunk.data) {
            // Only text (`t…`) and path (`p…`) fields are needed; the rest are binary.
            if field.tag[0] == b't' || field.tag[0] == b'p' {
                fields.insert(latin1(field.tag), decode_text(field.data));
            }
        }
        let Some(file_path) = fields.get("pfil") else {
            continue;
        };
        let relative_path = to_relative_path(file_path).to_string();
        let title = match fields.get("tsng").map(String::as_str) {
            Some(title) if !title.is_empty() => title.to_string(),
            _ => file_name_without_extension(&relative_path).to_string(),
        };
        tracks.push(Track {
            id: relative_path.clone(),
            title,
            artist: fields.get("tart").cloned().unwrap_or_default(),
            album: fields.get("talb").cloned().unwrap_or_default(),
            bpm: parse_positive_number(fields.get("tbpm").map(String::as_str)),
            key: fields
                .get("tkey")
                .map(String::as_str)
                .filter(|key| !key.is_empty())
                .map(str::to_string),
            duration_sec: parse_duration(fields.get("tlen").map(String::as_str)),
            relative_path,
            genre: None,
            label: None,
            color: None,
            artwork_path: None,
        });
    }
    tracks
}

/// Reads the track paths listed in a `.crate` file, in crate order.
pub fn parse_crate(file: &[u8]) -> Vec<String> {
    let mut track_paths = Vec::new();
    for chunk in read_chunks(file) {
        if !tag_is(chunk.tag, "otrk") {
            continue;
        }
        for field in read_chunks(chunk.data) {
            if tag_is(field.tag, "ptrk") {
                track_paths.push(decode_text(field.data));
            }
        }
    }
    track_paths
}

pub fn build_serato_library(
    database_tracks: Vec<Track>,
    crates: &[SeratoCrate],
    volume: &Volume,
) -> Library {
    let mut tracks = Vec::new();
    let mut seen = HashSet::new();
    for track in database_tracks {
        if seen.insert(track.id.clone()) {
            tracks.push(track);
        }
    }

    let mut nodes: HashMap<String, BuiltNode> = HashMap::new();
    let mut roots = Vec::new();
    for crate_file in crates {
        ensure_node(&crate_file.name, &mut nodes, &mut roots);
        let node = nodes.get_mut(&crate_file.name).unwrap();
        for track_path in &crate_file.track_paths {
            let relative_path = to_relative_path(track_path).to_string();
            if seen.insert(relative_path.clone()) {
                tracks.push(track_from_path(relative_path.clone()));
            }
            node.track_ids.push(relative_path);
        }
    }

    let collator = english_collator();
    Library {
        id: library_id(LibrarySource::Serato, &volume.path),
        source: LibrarySource::Serato,
        volume_name: volume.name.clone(),
        volume_path: volume.path.clone(),
        tracks,
        playlists: playlists_in_name_order(&roots, &nodes, &collator),
    }
}

struct BuiltNode {
    name: String,
    track_ids: Vec<String>,
    children: Vec<String>,
}

/// A parent crate can exist only as a name prefix, without a file of its own.
fn ensure_node(full_name: &str, nodes: &mut HashMap<String, BuiltNode>, roots: &mut Vec<String>) {
    if nodes.contains_key(full_name) {
        return;
    }
    let separator_at = full_name.rfind(CRATE_NESTING_SEPARATOR);
    let name = match separator_at {
        Some(at) => &full_name[at + CRATE_NESTING_SEPARATOR.len()..],
        None => full_name,
    };
    if let Some(at) = separator_at {
        let parent = &full_name[..at];
        ensure_node(parent, nodes, roots);
        nodes
            .get_mut(parent)
            .unwrap()
            .children
            .push(full_name.to_string());
    } else {
        roots.push(full_name.to_string());
    }
    nodes.insert(
        full_name.to_string(),
        BuiltNode {
            name: name.to_string(),
            track_ids: Vec::new(),
            children: Vec::new(),
        },
    );
}

fn playlists_in_name_order(
    ids: &[String],
    nodes: &HashMap<String, BuiltNode>,
    collator: &CollatorBorrowed<'static>,
) -> Vec<PlaylistNode> {
    let mut ids = ids.to_vec();
    ids.sort_by(|left, right| collator.compare(&nodes[left].name, &nodes[right].name));
    ids.into_iter()
        .map(|id| {
            let node = &nodes[&id];
            PlaylistNode {
                id: id.clone(),
                name: node.name.clone(),
                kind: PlaylistKind::Playlist,
                track_ids: node.track_ids.clone(),
                children: playlists_in_name_order(&node.children, nodes, collator),
            }
        })
        .collect()
}

fn track_from_path(relative_path: String) -> Track {
    Track {
        id: relative_path.clone(),
        title: file_name_without_extension(&relative_path).to_string(),
        artist: String::new(),
        album: String::new(),
        bpm: None,
        key: None,
        duration_sec: None,
        relative_path,
        genre: None,
        label: None,
        color: None,
        artwork_path: None,
    }
}

/// `en` collation, numeric, base sensitivity: `crate 9` comes before `crate 10`.
fn english_collator() -> CollatorBorrowed<'static> {
    let mut prefs = CollatorPreferences::from(locale!("en"));
    prefs.numeric_ordering = Some(CollationNumericOrdering::True);
    let mut options = CollatorOptions::default();
    options.strength = Some(Strength::Primary);
    Collator::try_new(prefs, options).expect("English collation data is compiled in")
}

fn parse_positive_number(text: Option<&str>) -> Option<f64> {
    parse_float(text?).filter(|value| *value > 0.0)
}

/// Serato stores lengths as text such as `06:22.93`.
fn parse_duration(text: Option<&str>) -> Option<f64> {
    let text = text.filter(|text| !text.is_empty())?;
    let mut seconds = 0.0;
    for part in text.split(':') {
        seconds = seconds * 60.0 + parse_float(part)?;
    }
    (seconds > 0.0).then_some(seconds)
}

/// `Number.parseFloat`: leading whitespace, then a numeric prefix.
fn parse_float(text: &str) -> Option<f64> {
    let text = text.trim_start();
    let bytes = text.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    let mut end = usize::from(bytes[0] == b'+' || bytes[0] == b'-');
    let mut saw_digit = false;
    let mut saw_dot = false;
    while end < bytes.len() {
        match bytes[end] {
            b'0'..=b'9' => saw_digit = true,
            b'.' if !saw_dot => saw_dot = true,
            _ => break,
        }
        end += 1;
    }
    if !saw_digit {
        return None;
    }
    if end < bytes.len() && (bytes[end] == b'e' || bytes[end] == b'E') {
        let exponent = end;
        end += 1;
        if end < bytes.len() && (bytes[end] == b'+' || bytes[end] == b'-') {
            end += 1;
        }
        let digits = end;
        while end < bytes.len() && bytes[end].is_ascii_digit() {
            end += 1;
        }
        if end == digits {
            end = exponent;
        }
    }
    text[..end].parse().ok()
}

fn latin1(bytes: [u8; 4]) -> String {
    bytes.into_iter().map(char::from).collect()
}

#[cfg(test)]
mod tests {
    use super::{
        build_serato_library, load_volume, parse_crate, parse_database, SeratoCrate, Volume,
    };
    use crate::{Library, LibrarySource};
    use serde::Deserialize;
    use std::cmp::Ordering;
    use std::path::PathBuf;

    fn volume() -> Volume {
        Volume {
            path: "/Volumes/GIGS".into(),
            name: "GIGS".into(),
        }
    }

    fn chunk(tag: &str, data: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(8 + data.len());
        out.extend(tag.as_bytes());
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        out.extend_from_slice(data);
        out
    }

    fn text_chunk(tag: &str, text: &str) -> Vec<u8> {
        let mut data = Vec::new();
        for unit in text.encode_utf16() {
            data.extend(unit.to_be_bytes());
        }
        chunk(tag, &data)
    }

    fn database(tracks: &[Vec<(&str, &str)>]) -> Vec<u8> {
        let mut file = text_chunk("vrsn", "2.0/Serato Scratch LIVE Database");
        for fields in tracks {
            let mut body = Vec::new();
            for (tag, text) in fields {
                body.extend(text_chunk(tag, text));
            }
            body.extend(chunk("uadd", &[0x5e, 0x3c, 0x07, 0xea]));
            file.extend(chunk("otrk", &body));
        }
        file
    }

    fn crate_file(track_paths: &[&str]) -> Vec<u8> {
        let mut file = text_chunk("vrsn", "1.0/Serato ScratchLive Crate");
        for track_path in track_paths {
            file.extend(chunk("otrk", &text_chunk("ptrk", track_path)));
        }
        file
    }

    #[test]
    fn parse_database_reads_track_metadata() {
        let tracks = parse_database(&database(&[vec![
            ("pfil", "Music/Björk - Jóga.mp3"),
            ("tsng", "Jóga"),
            ("tart", "Björk"),
            ("talb", "Homogenic"),
            ("tbpm", "126.00"),
            ("tkey", "Bb"),
            ("tlen", "06:22.93"),
        ]]));
        let track = &tracks[0];
        assert_eq!(track.id, "Music/Björk - Jóga.mp3");
        assert_eq!(track.title, "Jóga");
        assert_eq!(track.artist, "Björk");
        assert_eq!(track.album, "Homogenic");
        assert_eq!(track.bpm, Some(126.0));
        assert_eq!(track.key.as_deref(), Some("Bb"));
        assert_eq!(track.duration_sec, Some(382.93));
    }

    #[test]
    fn parse_database_falls_back_to_the_file_name_when_a_track_has_no_tags() {
        let tracks = parse_database(&database(&[vec![("pfil", "Music/Untitled Jam.wav")]]));
        assert_eq!(tracks[0].title, "Untitled Jam");
        assert_eq!(tracks[0].artist, "");
        assert_eq!(tracks[0].bpm, None);
        assert_eq!(tracks[0].key, None);
        assert_eq!(tracks[0].duration_sec, None);
    }

    #[test]
    fn parse_database_ignores_a_truncated_record_at_the_end_of_the_file() {
        let file = database(&[vec![("pfil", "a.mp3")], vec![("pfil", "b.mp3")]]);
        let ids: Vec<_> = parse_database(&file[..file.len() - 3])
            .into_iter()
            .map(|track| track.id)
            .collect();
        assert_eq!(ids, ["a.mp3"]);
    }

    #[test]
    fn parse_crate_lists_track_paths_in_crate_order() {
        assert_eq!(
            parse_crate(&crate_file(&["Music/b.mp3", "Music/a.mp3"])),
            ["Music/b.mp3", "Music/a.mp3"]
        );
    }

    #[test]
    fn build_nests_crates_by_their_separator_names() {
        let tracks = parse_database(&database(&[
            vec![("pfil", "a.mp3")],
            vec![("pfil", "b.mp3")],
        ]));
        let library = build_serato_library(
            tracks,
            &[
                SeratoCrate {
                    name: "House%%Deep".into(),
                    track_paths: vec!["b.mp3".into()],
                },
                SeratoCrate {
                    name: "House".into(),
                    track_paths: vec!["a.mp3".into(), "b.mp3".into()],
                },
                SeratoCrate {
                    name: "Techno%%Hard".into(),
                    track_paths: vec!["a.mp3".into()],
                },
            ],
            &volume(),
        );
        assert_eq!(library.playlists[0].id, "House");
        assert_eq!(library.playlists[0].track_ids, ["a.mp3", "b.mp3"]);
        assert_eq!(library.playlists[0].children[0].name, "Deep");
        assert_eq!(library.playlists[0].children[0].track_ids, ["b.mp3"]);
        assert_eq!(library.playlists[1].name, "Techno");
        assert!(library.playlists[1].track_ids.is_empty());
        assert_eq!(library.playlists[1].children[0].name, "Hard");
        assert_eq!(library.id, "serato-857074f2fb38");
    }

    #[test]
    fn build_adds_tracks_that_only_a_crate_knows_about() {
        let library = build_serato_library(
            Vec::new(),
            &[SeratoCrate {
                name: "New".into(),
                track_paths: vec!["Music/Fresh Tune.flac".into()],
            }],
            &volume(),
        );
        assert_eq!(library.tracks[0].id, "Music/Fresh Tune.flac");
        assert_eq!(library.tracks[0].title, "Fresh Tune");
        assert_eq!(library.playlists[0].track_ids, ["Music/Fresh Tune.flac"]);
    }

    #[test]
    fn crate_names_sort_with_english_numeric_base_collation() {
        let library = build_serato_library(
            Vec::new(),
            &[
                SeratoCrate {
                    name: "House".into(),
                    track_paths: Vec::new(),
                },
                SeratoCrate {
                    name: "crate 10".into(),
                    track_paths: Vec::new(),
                },
                SeratoCrate {
                    name: "夏".into(),
                    track_paths: Vec::new(),
                },
                SeratoCrate {
                    name: "crate 9".into(),
                    track_paths: Vec::new(),
                },
            ],
            &volume(),
        );
        let names: Vec<_> = library
            .playlists
            .iter()
            .map(|node| node.name.as_str())
            .collect();
        assert_eq!(names, ["crate 9", "crate 10", "House", "夏"]);
        assert_eq!(
            super::english_collator().compare("Café", "cafe"),
            Ordering::Equal
        );
    }

    #[test]
    fn serato_fixtures_match_expected_json() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/serato");
        let mut cases: Vec<_> = std::fs::read_dir(&root)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        cases.sort();
        assert!(!cases.is_empty());

        for case in cases {
            let name = case.file_name().unwrap().to_str().unwrap().to_string();
            let volume_path = case.join("volume");
            let expected: ExpectedVolume =
                serde_json::from_str(&std::fs::read_to_string(case.join("expected.json")).unwrap())
                    .unwrap();
            let loaded = load_volume(&volume_path, &name).unwrap();
            let want = expected
                .libraries
                .into_iter()
                .find(|library| library.source == LibrarySource::Serato);
            match (loaded, want) {
                (None, None) => {}
                (Some(mut library), Some(want)) => {
                    let path = volume_path.to_str().unwrap();
                    assert_eq!(
                        library.id,
                        crate::library_id(LibrarySource::Serato, path),
                        "{name}"
                    );
                    library.id = "serato-{volumeHash}".into();
                    library.volume_path = "{volume}".into();
                    assert_eq!(library, want, "{name}");
                }
                (None, Some(_)) => panic!("{name} was not detected as a Serato library"),
                (Some(_), None) => {
                    panic!("{name} was detected but the fixture has no Serato library")
                }
            }
        }
    }

    #[derive(Deserialize)]
    struct ExpectedVolume {
        libraries: Vec<Library>,
    }
}
