//! Device Library Plus (`exportLibrary.db`), the SQLCipher database newer
//! rekordbox versions export.
//!
//! The volume is only read. The database and its `-wal` file, when present, are
//! copied into a temporary directory and opened there with SQLCipher.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use rusqlite::Connection;

use super::pdb::{PdbPlaylistEntry, PdbPlaylistNode};
use super::{build_playlist_tree, Volume};
use crate::paths::{file_name_without_extension, resolve_inside_volume, to_relative_path};
use crate::{library_id, Library, LibrarySource, Track, TrackColor};

const DATABASE_FILE: &str = "exportLibrary.db";
/// Every Device Library Plus export uses this key; players have it built in.
const DEVICE_LIBRARY_PLUS_KEY: &str =
    "r8gddnr4k847830ar6cqzbkk0el6qytmb3trbbx805jm74vez64i5o8fnrqryqls";
const FOLDER_ATTRIBUTE: i64 = 1;

const TRACKS_SQL: &str = "
SELECT c.content_id AS id,
       COALESCE(c.title, '') AS title,
       COALESCE(artist.name, '') AS artist,
       COALESCE(album.name, '') AS album,
       COALESCE(genre.name, '') AS genre,
       COALESCE(label.name, '') AS label,
       COALESCE(k.name, '') AS key,
       COALESCE(c.color_id, 0) AS colorId,
       COALESCE(color.name, '') AS colorName,
       COALESCE(c.bpmx100, 0) AS bpmx100,
       COALESCE(c.length, 0) AS durationSec,
       COALESCE(c.path, '') AS path,
       COALESCE(image.path, '') AS imagePath
  FROM content c
  LEFT JOIN artist ON artist.artist_id = c.artist_id_artist
  LEFT JOIN album ON album.album_id = c.album_id
  LEFT JOIN genre ON genre.genre_id = c.genre_id
  LEFT JOIN label ON label.label_id = c.label_id
  LEFT JOIN \"key\" k ON k.key_id = c.key_id
  LEFT JOIN color ON color.color_id = c.color_id
  LEFT JOIN image ON image.image_id = c.image_id";

const PLAYLISTS_SQL: &str = "
SELECT playlist_id AS id,
       COALESCE(playlist_id_parent, 0) AS parentId,
       COALESCE(sequenceNo, 0) AS sortOrder,
       attribute,
       COALESCE(name, '') AS name
  FROM playlist";

const ENTRIES_SQL: &str = "
SELECT playlist_id AS playlistId, content_id AS trackId, COALESCE(sequenceNo, 0) AS entryIndex
  FROM playlist_content";

/// rekordbox's eight track colours, by `color_id`.
const TRACK_COLORS: [(i64, &str, &str); 8] = [
    (1, "Pink", "#f870c8"),
    (2, "Red", "#f5423c"),
    (3, "Orange", "#ff9a2e"),
    (4, "Yellow", "#f2d93a"),
    (5, "Green", "#3fcf5c"),
    (6, "Aqua", "#2bd0d6"),
    (7, "Blue", "#3d6cff"),
    (8, "Purple", "#a65cff"),
];

#[derive(Debug)]
pub enum DeviceLibraryPlusError {
    Io(std::io::Error),
    Sql(rusqlite::Error),
    /// The volume path is not valid Unicode, so it cannot be stored on a library.
    Path,
    /// An id in the export does not fit the integer width the playlist tree uses.
    Id(i64),
}

impl std::fmt::Display for DeviceLibraryPlusError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "{error}"),
            Self::Sql(error) => write!(formatter, "{error}"),
            Self::Path => write!(formatter, "volume path is not valid Unicode"),
            Self::Id(id) => write!(formatter, "export id {id} is out of range"),
        }
    }
}

impl std::error::Error for DeviceLibraryPlusError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Sql(error) => Some(error),
            Self::Path | Self::Id(_) => None,
        }
    }
}

impl From<std::io::Error> for DeviceLibraryPlusError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<rusqlite::Error> for DeviceLibraryPlusError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Sql(error)
    }
}

struct ContentRow {
    id: i64,
    title: String,
    artist: String,
    album: String,
    genre: String,
    label: String,
    key: String,
    color_id: i64,
    color_name: String,
    bpmx100: i64,
    duration_sec: i64,
    path: String,
    image_path: String,
}

/// Reads `PIONEER/rekordbox/exportLibrary.db` from a volume root.
pub fn load_volume(
    volume_path: &Path,
    volume_name: &str,
) -> Result<Library, DeviceLibraryPlusError> {
    // Declared first so the SQLCipher connection closes before the directory is removed.
    let temporary = TemporaryDir::create()?;
    let database_path = copy_database(volume_path, temporary.path())?;
    let connection = open_encrypted(&database_path)?;
    let data = read_database(&connection)?;
    drop(connection);

    let image_paths: Vec<&str> = data
        .tracks
        .iter()
        .map(|track| track.image_path.as_str())
        .collect();
    let path = volume_path.to_str().ok_or(DeviceLibraryPlusError::Path)?;
    let artwork = find_artwork(path, &image_paths);
    build_library(
        &data,
        &Volume {
            path: path.to_string(),
            name: volume_name.to_string(),
        },
        &artwork,
    )
}

fn copy_database(
    volume_path: &Path,
    destination: &Path,
) -> Result<PathBuf, DeviceLibraryPlusError> {
    let source = volume_path
        .join("PIONEER")
        .join("rekordbox")
        .join(DATABASE_FILE);
    let database_path = destination.join(DATABASE_FILE);
    std::fs::copy(&source, &database_path)?;
    let wal_source = wal_path(&source);
    if wal_source.is_file() {
        std::fs::copy(&wal_source, wal_path(&database_path))?;
    }
    Ok(database_path)
}

fn wal_path(database_path: &Path) -> PathBuf {
    let mut name = database_path.file_name().unwrap_or_default().to_os_string();
    name.push("-wal");
    database_path.with_file_name(name)
}

fn open_encrypted(database_path: &Path) -> Result<Connection, DeviceLibraryPlusError> {
    let connection = Connection::open(database_path)?;
    connection.pragma_update(None, "key", DEVICE_LIBRARY_PLUS_KEY)?;
    // Read one row so a wrong key or a damaged page fails here, before the copy is queried.
    connection.query_row("SELECT count(*) FROM sqlite_master", [], |row| {
        row.get::<_, i64>(0)
    })?;
    Ok(connection)
}

struct LibraryRows {
    tracks: Vec<ContentRow>,
    playlist_nodes: Vec<PdbPlaylistNode>,
    playlist_entries: Vec<PdbPlaylistEntry>,
}

fn read_database(connection: &Connection) -> Result<LibraryRows, DeviceLibraryPlusError> {
    let mut tracks_query = connection.prepare(TRACKS_SQL)?;
    let tracks = tracks_query
        .query_map([], |row| {
            Ok(ContentRow {
                id: row.get(0)?,
                title: row.get(1)?,
                artist: row.get(2)?,
                album: row.get(3)?,
                genre: row.get(4)?,
                label: row.get(5)?,
                key: row.get(6)?,
                color_id: row.get(7)?,
                color_name: row.get(8)?,
                bpmx100: row.get(9)?,
                duration_sec: row.get(10)?,
                path: row.get(11)?,
                image_path: row.get(12)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let mut playlists_query = connection.prepare(PLAYLISTS_SQL)?;
    let playlist_nodes = playlists_query
        .query_map([], |row| {
            let id: i64 = row.get(0)?;
            let parent_id: i64 = row.get(1)?;
            let sort_order: i64 = row.get(2)?;
            let attribute: Option<i64> = row.get(3)?;
            let name: String = row.get(4)?;
            Ok((
                id,
                parent_id,
                sort_order,
                attribute.unwrap_or(0) == FOLDER_ATTRIBUTE,
                name,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let playlist_nodes = playlist_nodes
        .into_iter()
        .map(|(id, parent_id, sort_order, is_folder, name)| {
            Ok(PdbPlaylistNode {
                id: id_of(id)?,
                parent_id: id_of(parent_id)?,
                sort_order: id_of(sort_order)?,
                is_folder,
                name,
            })
        })
        .collect::<Result<Vec<_>, DeviceLibraryPlusError>>()?;

    let mut entries_query = connection.prepare(ENTRIES_SQL)?;
    let playlist_entries = entries_query
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let playlist_entries = playlist_entries
        .into_iter()
        .map(|(playlist_id, track_id, entry_index)| {
            Ok(PdbPlaylistEntry {
                playlist_id: id_of(playlist_id)?,
                track_id: id_of(track_id)?,
                entry_index: id_of(entry_index)?,
            })
        })
        .collect::<Result<Vec<_>, DeviceLibraryPlusError>>()?;

    Ok(LibraryRows {
        tracks,
        playlist_nodes,
        playlist_entries,
    })
}

fn id_of(id: i64) -> Result<u32, DeviceLibraryPlusError> {
    u32::try_from(id).map_err(|_| DeviceLibraryPlusError::Id(id))
}

fn build_library(
    data: &LibraryRows,
    volume: &Volume,
    artwork: &HashMap<String, String>,
) -> Result<Library, DeviceLibraryPlusError> {
    let mut tracks = Vec::new();
    let mut position_of = HashMap::new();
    for row in &data.tracks {
        let id = id_of(row.id)?;
        let relative_path = to_relative_path(&row.path).to_string();
        let title = if row.title.is_empty() {
            file_name_without_extension(&relative_path).to_string()
        } else {
            row.title.clone()
        };
        let stored_artwork = to_relative_path(&row.image_path);
        let track = Track {
            id: id.to_string(),
            title,
            artist: row.artist.clone(),
            album: row.album.clone(),
            bpm: (row.bpmx100 > 0).then_some(row.bpmx100 as f64 / 100.0),
            key: (!row.key.is_empty()).then(|| row.key.clone()),
            duration_sec: (row.duration_sec > 0).then_some(row.duration_sec as f64),
            relative_path,
            genre: Some(row.genre.clone()),
            label: Some(row.label.clone()),
            color: track_color(row.color_id, &row.color_name),
            artwork_path: if stored_artwork.is_empty() {
                None
            } else {
                artwork.get(stored_artwork).cloned()
            },
        };
        if let Some(&index) = position_of.get(&id) {
            tracks[index] = track;
        } else {
            position_of.insert(id, tracks.len());
            tracks.push(track);
        }
    }
    let track_ids: HashSet<u32> = position_of.into_keys().collect();

    Ok(Library {
        id: library_id(LibrarySource::DeviceLibraryPlus, &volume.path),
        source: LibrarySource::DeviceLibraryPlus,
        volume_name: volume.name.clone(),
        volume_path: volume.path.clone(),
        tracks,
        playlists: build_playlist_tree(&data.playlist_nodes, &data.playlist_entries, &track_ids),
    })
}

fn track_color(color_id: i64, color_name: &str) -> Option<TrackColor> {
    let (_, name, value) = TRACK_COLORS.iter().find(|(id, _, _)| *id == color_id)?;
    Some(TrackColor {
        name: if color_name.is_empty() {
            (*name).to_string()
        } else {
            color_name.to_string()
        },
        value: (*value).to_string(),
    })
}

/// Maps each stored artwork path to the volume-relative file to show.
fn find_artwork(volume_path: &str, image_paths: &[&str]) -> HashMap<String, String> {
    let mut by_directory: HashMap<String, Vec<String>> = HashMap::new();
    for image_path in image_paths {
        if image_path.is_empty() {
            continue;
        }
        let relative = to_relative_path(image_path).to_string();
        if relative.is_empty() {
            continue;
        }
        let directory = posix_dirname(&relative).to_string();
        by_directory.entry(directory).or_default().push(relative);
    }

    let mut found = HashMap::new();
    for (directory, paths) in by_directory {
        let names = directory_names(volume_path, &directory);
        let mut by_lower_case = HashMap::new();
        for name in names {
            by_lower_case.insert(name.to_lowercase(), name);
        }
        for relative in paths {
            let file_name = posix_basename(&relative);
            let matched = artwork_candidates(file_name)
                .into_iter()
                .find_map(|candidate| by_lower_case.get(&candidate.to_lowercase()).cloned());
            if let Some(matched) = matched {
                found.insert(relative, format!("{directory}/{matched}"));
            }
        }
    }
    found
}

fn directory_names(volume_path: &str, directory: &str) -> Vec<String> {
    let Some(absolute) = resolve_inside_volume(volume_path, directory) else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(absolute) else {
        return Vec::new();
    };
    entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect()
}

fn artwork_candidates(file_name: &str) -> Vec<String> {
    let (name, extension) = posix_name_and_extension(file_name);
    let mut bases = vec![name.to_string()];
    if let Some(rest) = name.strip_prefix('b') {
        bases.push(format!("a{rest}"));
    }
    bases
        .into_iter()
        .flat_map(|candidate| {
            [
                format!("{candidate}_m{extension}"),
                format!("{candidate}{extension}"),
            ]
        })
        .collect()
}

fn posix_dirname(path: &str) -> &str {
    match path.rfind('/') {
        Some(0) => "/",
        Some(index) => &path[..index],
        None => ".",
    }
}

fn posix_basename(path: &str) -> &str {
    match path.rfind('/') {
        Some(index) => &path[index + 1..],
        None => path,
    }
}

/// `path.posix.parse`: the last dot starts the extension, unless it is the first character.
fn posix_name_and_extension(file_name: &str) -> (&str, &str) {
    if file_name == "." || file_name == ".." {
        return (file_name, "");
    }
    match file_name.rfind('.') {
        Some(0) | None => (file_name, ""),
        Some(index) => (&file_name[..index], &file_name[index..]),
    }
}

/// Removed when this value is dropped, including when reading the database fails.
struct TemporaryDir(PathBuf);

impl TemporaryDir {
    fn create() -> Result<Self, DeviceLibraryPlusError> {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let path = std::env::temp_dir().join(format!("djusb-{nanos}-{}", std::process::id()));
        std::fs::create_dir(&path)?;
        Ok(Self(path))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TemporaryDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::{artwork_candidates, load_volume};
    use crate::{Library, LibrarySource};
    use serde::Deserialize;
    use std::path::PathBuf;

    #[test]
    fn artwork_candidates_prefer_the_medium_file_and_the_a_prefix() {
        assert_eq!(
            artwork_candidates("b1.jpg"),
            ["b1_m.jpg", "b1.jpg", "a1_m.jpg", "a1.jpg"]
        );
        assert_eq!(artwork_candidates("B2.JPG"), ["B2_m.JPG", "B2.JPG"]);
    }

    #[test]
    fn device_library_plus_fixtures_match_expected_json() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
        let mut cases = Vec::new();
        for group in ["deviceLibraryPlus", "rekordbox", "local"] {
            let directory = root.join(group);
            if !directory.is_dir() {
                continue;
            }
            for entry in std::fs::read_dir(directory).unwrap() {
                let case = entry.unwrap().path();
                if case
                    .join("volume")
                    .join("PIONEER")
                    .join("rekordbox")
                    .join("exportLibrary.db")
                    .is_file()
                {
                    cases.push(case);
                }
            }
        }
        cases.sort();
        assert!(cases
            .iter()
            .any(|case| case.ends_with(std::path::Path::new("deviceLibraryPlus").join("basic"))));

        for case in cases {
            let name = case.file_name().unwrap().to_str().unwrap().to_string();
            let volume_path = case.join("volume");
            let expected: ExpectedVolume =
                serde_json::from_str(&std::fs::read_to_string(case.join("expected.json")).unwrap())
                    .unwrap();
            let result = load_volume(&volume_path, &name);
            let want = expected
                .libraries
                .into_iter()
                .find(|library| library.source == LibrarySource::DeviceLibraryPlus);
            let failed = expected.errors.contains(&LibrarySource::DeviceLibraryPlus);
            match (result, want, failed) {
                (Ok(mut library), Some(want), false) => {
                    let path = volume_path.to_str().unwrap();
                    assert_eq!(
                        library.id,
                        crate::library_id(LibrarySource::DeviceLibraryPlus, path),
                        "{name}"
                    );
                    library.id = "deviceLibraryPlus-{volumeHash}".into();
                    library.volume_path = "{volume}".into();
                    assert_eq!(library, want, "{name}");
                }
                (Err(_), None, true) => {}
                (Err(error), _, _) => panic!("{name} failed to read: {error}"),
                (Ok(_), None, _) => panic!("{name} produced a library the fixture does not expect"),
                (Ok(_), Some(_), true) => panic!("{name} loaded, but the fixture records an error"),
            }
        }
    }

    #[derive(Deserialize)]
    struct ExpectedVolume {
        libraries: Vec<Library>,
        errors: Vec<LibrarySource>,
    }
}
