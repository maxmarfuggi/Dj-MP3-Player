//! rekordbox Device Library (`export.pdb`). Device Library Plus has its own reader.

mod pdb;

use std::collections::{HashMap, HashSet};
use std::path::Path;

pub use pdb::{
    parse_pdb, PdbData, PdbError, PdbNamedRow, PdbPlaylistEntry, PdbPlaylistNode, PdbTrack,
};

use crate::paths::{file_name_without_extension, to_relative_path};
use crate::{library_id, Library, LibrarySource, PlaylistKind, PlaylistNode, Track};

const ROOT_PARENT_ID: u32 = 0;

/// A volume root, or a folder the user picked in its place.
pub struct Volume {
    pub path: String,
    pub name: String,
}

#[derive(Debug)]
pub enum RekordboxError {
    Io(std::io::Error),
    Pdb(PdbError),
    /// The volume path is not valid Unicode, so it cannot be stored on a library.
    Path,
}

impl std::fmt::Display for RekordboxError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "{error}"),
            Self::Pdb(error) => write!(formatter, "{error}"),
            Self::Path => write!(formatter, "volume path is not valid Unicode"),
        }
    }
}

impl std::error::Error for RekordboxError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Pdb(error) => Some(error),
            Self::Path => None,
        }
    }
}

impl From<PdbError> for RekordboxError {
    fn from(error: PdbError) -> Self {
        Self::Pdb(error)
    }
}

/// Reads `PIONEER/rekordbox/export.pdb` and builds the library for that volume.
pub fn load_volume(volume_path: &Path, volume_name: &str) -> Result<Library, RekordboxError> {
    let pdb_path = volume_path
        .join("PIONEER")
        .join("rekordbox")
        .join("export.pdb");
    let bytes = std::fs::read(&pdb_path).map_err(RekordboxError::Io)?;
    let pdb = parse_pdb(&bytes)?;
    let path = volume_path
        .to_str()
        .ok_or(RekordboxError::Path)?
        .to_string();
    Ok(build_rekordbox_library(
        &pdb,
        &Volume {
            path,
            name: volume_name.to_string(),
        },
    ))
}

pub fn build_rekordbox_library(pdb: &PdbData, volume: &Volume) -> Library {
    let artists: HashMap<u32, &str> = pdb
        .artists
        .iter()
        .map(|row| (row.id, row.name.as_str()))
        .collect();
    let albums: HashMap<u32, &str> = pdb
        .albums
        .iter()
        .map(|row| (row.id, row.name.as_str()))
        .collect();
    let keys: HashMap<u32, &str> = pdb
        .keys
        .iter()
        .map(|row| (row.id, row.name.as_str()))
        .collect();

    let mut tracks = Vec::new();
    let mut position_of = HashMap::new();
    for row in &pdb.tracks {
        let relative_path = to_relative_path(&row.file_path).to_string();
        let title = if row.title.is_empty() {
            file_name_without_extension(&relative_path).to_string()
        } else {
            row.title.clone()
        };
        let track = Track {
            id: row.id.to_string(),
            title,
            artist: artists
                .get(&row.artist_id)
                .copied()
                .unwrap_or("")
                .to_string(),
            album: albums.get(&row.album_id).copied().unwrap_or("").to_string(),
            bpm: (row.tempo > 0).then_some(f64::from(row.tempo) / 100.0),
            key: keys
                .get(&row.key_id)
                .copied()
                .filter(|name| !name.is_empty())
                .map(str::to_string),
            duration_sec: (row.duration_sec > 0).then_some(f64::from(row.duration_sec)),
            relative_path,
            genre: None,
            label: None,
            color: None,
            artwork_path: None,
        };
        if let Some(&index) = position_of.get(&row.id) {
            tracks[index] = track;
        } else {
            position_of.insert(row.id, tracks.len());
            tracks.push(track);
        }
    }
    let track_ids: HashSet<u32> = position_of.into_keys().collect();

    Library {
        id: library_id(LibrarySource::Rekordbox, &volume.path),
        source: LibrarySource::Rekordbox,
        volume_name: volume.name.clone(),
        volume_path: volume.path.clone(),
        playlists: build_playlist_tree(&pdb.playlist_nodes, &pdb.playlist_entries, &track_ids),
        tracks,
    }
}

/// Both rekordbox exports describe playlists the same way.
pub fn build_playlist_tree(
    nodes: &[PdbPlaylistNode],
    playlist_entries: &[PdbPlaylistEntry],
    track_ids: &HashSet<u32>,
) -> Vec<PlaylistNode> {
    let mut entries_by_playlist: HashMap<u32, Vec<PdbPlaylistEntry>> = HashMap::new();
    for entry in playlist_entries {
        // Entries can outlive the tracks they point to; skip those.
        if !track_ids.contains(&entry.track_id) {
            continue;
        }
        entries_by_playlist
            .entry(entry.playlist_id)
            .or_default()
            .push(*entry);
    }

    // The root's id is reserved; a row using it would be its own ancestor.
    // A repeated id keeps the first position and the last row, matching `Map`.
    let mut order = Vec::new();
    let mut rows_by_id = HashMap::new();
    for row in nodes.iter().filter(|row| row.id != ROOT_PARENT_ID) {
        if rows_by_id.insert(row.id, row).is_none() {
            order.push(row.id);
        }
    }
    let mut rows_by_parent: HashMap<u32, Vec<&PdbPlaylistNode>> = HashMap::new();
    for id in order {
        let row = rows_by_id[&id];
        // A node whose parent is missing would otherwise be unreachable.
        let parent_id = if rows_by_id.contains_key(&row.parent_id) {
            row.parent_id
        } else {
            ROOT_PARENT_ID
        };
        rows_by_parent.entry(parent_id).or_default().push(row);
    }

    fn build(
        parent_id: u32,
        rows_by_parent: &HashMap<u32, Vec<&PdbPlaylistNode>>,
        entries_by_playlist: &HashMap<u32, Vec<PdbPlaylistEntry>>,
    ) -> Vec<PlaylistNode> {
        let mut siblings = rows_by_parent.get(&parent_id).cloned().unwrap_or_default();
        siblings.sort_by_key(|row| row.sort_order);
        siblings
            .into_iter()
            .map(|row| {
                let mut entries = entries_by_playlist
                    .get(&row.id)
                    .cloned()
                    .unwrap_or_default();
                entries.sort_by_key(|entry| entry.entry_index);
                PlaylistNode {
                    id: row.id.to_string(),
                    name: row.name.clone(),
                    kind: if row.is_folder {
                        PlaylistKind::Folder
                    } else {
                        PlaylistKind::Playlist
                    },
                    children: build(row.id, rows_by_parent, entries_by_playlist),
                    track_ids: entries
                        .iter()
                        .map(|entry| entry.track_id.to_string())
                        .collect(),
                }
            })
            .collect()
    }

    build(ROOT_PARENT_ID, &rows_by_parent, &entries_by_playlist)
}

#[cfg(test)]
mod tests {
    use super::{
        build_rekordbox_library, load_volume, PdbData, PdbNamedRow, PdbPlaylistEntry,
        PdbPlaylistNode, PdbTrack, Volume,
    };
    use crate::{Library, LibrarySource};
    use serde::Deserialize;
    use std::path::PathBuf;

    fn volume() -> Volume {
        Volume {
            path: "/Volumes/GIGS".into(),
            name: "GIGS".into(),
        }
    }

    fn pdb_track(id: u32, title: &str, file_path: &str) -> PdbTrack {
        PdbTrack {
            id,
            title: title.into(),
            artist_id: 0,
            album_id: 0,
            key_id: 0,
            tempo: 0,
            duration_sec: 0,
            file_path: file_path.into(),
        }
    }

    fn empty_pdb() -> PdbData {
        PdbData {
            tracks: Vec::new(),
            artists: Vec::new(),
            albums: Vec::new(),
            keys: Vec::new(),
            playlist_nodes: Vec::new(),
            playlist_entries: Vec::new(),
            skipped_rows: 0,
        }
    }

    #[test]
    fn joins_tracks_with_their_artist_album_and_key() {
        let mut pdb = empty_pdb();
        pdb.tracks = vec![
            PdbTrack {
                artist_id: 10,
                album_id: 20,
                key_id: 30,
                tempo: 12850,
                duration_sec: 301,
                ..pdb_track(1, "Track 1", "/Contents/track-1.mp3")
            },
            pdb_track(2, "", "/Contents/Some Artist/Untitled Jam.wav"),
        ];
        pdb.artists = vec![PdbNamedRow {
            id: 10,
            name: "DJ Example".into(),
        }];
        pdb.albums = vec![PdbNamedRow {
            id: 20,
            name: "The Album".into(),
        }];
        pdb.keys = vec![PdbNamedRow {
            id: 30,
            name: "Am".into(),
        }];

        let library = build_rekordbox_library(&pdb, &volume());
        assert_eq!(library.source, LibrarySource::Rekordbox);
        assert_eq!(library.volume_name, "GIGS");
        assert_eq!(library.volume_path, "/Volumes/GIGS");
        assert_eq!(library.id, "rekordbox-857074f2fb38");
        assert_eq!(library.tracks[0].artist, "DJ Example");
        assert_eq!(library.tracks[0].album, "The Album");
        assert_eq!(library.tracks[0].bpm, Some(128.5));
        assert_eq!(library.tracks[0].key.as_deref(), Some("Am"));
        assert_eq!(library.tracks[0].duration_sec, Some(301.0));
        assert_eq!(library.tracks[0].relative_path, "Contents/track-1.mp3");
        assert_eq!(library.tracks[1].title, "Untitled Jam");
        assert_eq!(library.tracks[1].bpm, None);
        assert_eq!(library.tracks[1].key, None);
        assert!(library.tracks[0].genre.is_none());
    }

    #[test]
    fn builds_the_playlist_tree_in_rekordbox_order() {
        let mut pdb = empty_pdb();
        pdb.tracks = vec![
            pdb_track(1, "Track 1", "/Contents/track-1.mp3"),
            pdb_track(2, "Track 2", "/Contents/track-2.mp3"),
            pdb_track(3, "Track 3", "/Contents/track-3.mp3"),
        ];
        pdb.playlist_nodes = vec![
            node(3, 1, 1, false, "Peak time"),
            node(2, 1, 0, false, "Warm up"),
            node(1, 0, 0, true, "Friday"),
        ];
        pdb.playlist_entries = vec![
            PdbPlaylistEntry {
                playlist_id: 2,
                track_id: 3,
                entry_index: 2,
            },
            PdbPlaylistEntry {
                playlist_id: 2,
                track_id: 1,
                entry_index: 1,
            },
            PdbPlaylistEntry {
                playlist_id: 3,
                track_id: 2,
                entry_index: 1,
            },
        ];

        let library = build_rekordbox_library(&pdb, &volume());
        let friday = &library.playlists[0];
        assert_eq!(friday.name, "Friday");
        assert_eq!(friday.kind, crate::PlaylistKind::Folder);
        assert!(friday.track_ids.is_empty());
        assert_eq!(friday.children[0].name, "Warm up");
        assert_eq!(friday.children[0].track_ids, ["1", "3"]);
        assert_eq!(friday.children[1].name, "Peak time");
        assert_eq!(friday.children[1].track_ids, ["2"]);
    }

    #[test]
    fn drops_playlist_entries_whose_track_no_longer_exists() {
        let mut pdb = empty_pdb();
        pdb.tracks = vec![pdb_track(1, "Track 1", "/Contents/track-1.mp3")];
        pdb.playlist_nodes = vec![node(1, 0, 0, false, "Set")];
        pdb.playlist_entries = vec![
            PdbPlaylistEntry {
                playlist_id: 1,
                track_id: 1,
                entry_index: 1,
            },
            PdbPlaylistEntry {
                playlist_id: 1,
                track_id: 99,
                entry_index: 2,
            },
        ];
        let library = build_rekordbox_library(&pdb, &volume());
        assert_eq!(library.playlists[0].track_ids, ["1"]);
    }

    #[test]
    fn keeps_playlists_whose_parent_folder_is_missing() {
        let mut pdb = empty_pdb();
        pdb.playlist_nodes = vec![node(5, 42, 0, false, "Orphan")];
        let library = build_rekordbox_library(&pdb, &volume());
        assert_eq!(
            library
                .playlists
                .iter()
                .map(|node| node.name.as_str())
                .collect::<Vec<_>>(),
            ["Orphan"]
        );
    }

    #[test]
    fn export_pdb_fixtures_match_expected_json() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/rekordbox");
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
            let mut library = load_volume(&volume_path, &name).unwrap();
            let path = volume_path.to_str().unwrap();
            assert_eq!(
                library.id,
                crate::library_id(LibrarySource::Rekordbox, path),
                "{name}"
            );
            library.id = "rekordbox-{volumeHash}".into();
            library.volume_path = "{volume}".into();
            let want = expected
                .libraries
                .into_iter()
                .find(|library| library.source == LibrarySource::Rekordbox)
                .unwrap_or_else(|| panic!("{name} has no rekordbox library"));
            assert_eq!(library, want, "{name}");
        }
    }

    fn node(
        id: u32,
        parent_id: u32,
        sort_order: u32,
        is_folder: bool,
        name: &str,
    ) -> PdbPlaylistNode {
        PdbPlaylistNode {
            id,
            parent_id,
            sort_order,
            is_folder,
            name: name.into(),
        }
    }

    #[derive(Deserialize)]
    struct ExpectedVolume {
        libraries: Vec<Library>,
    }
}
