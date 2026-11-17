//! Lines on the home screen: one row per volume, then each export on it.
//!
//! The names match the desktop sidebar. The highlight and the platter live
//! in the app; this module only decides the text.

use djusb_core::{LibrarySource, LibraryState};

/// Shown when no export is loaded and none has failed.
pub const EMPTY_HOME: &str = "Plug in a rekordbox or Serato USB.";

/// One row of the home screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HomeLine {
    /// The volume label, shown once above its exports.
    Volume(String),
    /// One export on the volume above it. `library` is `None` when it failed to load.
    Export {
        label: String,
        detail: String,
        library: Option<String>,
    },
    /// [`EMPTY_HOME`], when the list has nothing to show.
    Empty,
}

/// The rows for `state`, volumes in the order they first appear.
#[must_use]
pub fn home_lines(state: &LibraryState) -> Vec<HomeLine> {
    if state.libraries.is_empty() && state.errors.is_empty() {
        return vec![HomeLine::Empty];
    }

    let mut groups: Vec<Group> = Vec::new();
    for library in &state.libraries {
        let group = group_mut(&mut groups, &library.volume_path, &library.volume_name);
        group.lines.push(HomeLine::Export {
            label: source_label(library.source).to_string(),
            detail: tracks_label(library.tracks.len()),
            library: Some(library.id.clone()),
        });
    }
    for error in &state.errors {
        let group = group_mut(&mut groups, &error.volume_path, &error.volume_name);
        group.lines.push(HomeLine::Export {
            label: source_label(error.source).to_string(),
            detail: error.message.clone(),
            library: None,
        });
    }

    let mut lines = Vec::new();
    for group in groups {
        lines.push(HomeLine::Volume(group.name));
        lines.extend(group.lines);
    }
    lines
}

/// The names the desktop sidebar uses for each export.
pub(crate) fn source_label(source: LibrarySource) -> &'static str {
    match source {
        LibrarySource::Rekordbox => "Device Library",
        LibrarySource::DeviceLibraryPlus => "Device Library Plus",
        LibrarySource::Serato => "Serato",
    }
}

pub(crate) fn tracks_label(count: usize) -> String {
    match count {
        1 => "1 track".to_string(),
        n => format!("{n} tracks"),
    }
}

struct Group {
    path: String,
    name: String,
    lines: Vec<HomeLine>,
}

fn group_mut<'a>(groups: &'a mut Vec<Group>, path: &str, name: &str) -> &'a mut Group {
    if let Some(index) = groups.iter().position(|group| group.path == path) {
        return &mut groups[index];
    }
    groups.push(Group {
        path: path.to_string(),
        name: volume_name(name, path),
        lines: Vec::new(),
    });
    let last = groups.len() - 1;
    &mut groups[last]
}

/// A volume with no label is shown by its path, so the row is never blank.
pub(crate) fn volume_name(name: &str, path: &str) -> String {
    if name.is_empty() {
        path.to_string()
    } else {
        name.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{home_lines, HomeLine, EMPTY_HOME};
    use djusb_core::{Library, LibraryLoadError, LibrarySource, LibraryState, Track};

    fn track(id: &str) -> Track {
        Track {
            id: id.to_string(),
            title: id.to_string(),
            artist: String::new(),
            album: String::new(),
            bpm: None,
            key: None,
            duration_sec: None,
            relative_path: format!("{id}.mp3"),
            genre: None,
            label: None,
            color: None,
            artwork_path: None,
        }
    }

    fn id(source: LibrarySource, path: &str) -> String {
        format!("{}-{path}", source.as_str())
    }

    fn library(source: LibrarySource, name: &str, path: &str, tracks: usize) -> Library {
        Library {
            id: id(source, path),
            source,
            volume_name: name.to_string(),
            volume_path: path.to_string(),
            tracks: (0..tracks).map(|index| track(&index.to_string())).collect(),
            playlists: Vec::new(),
        }
    }

    fn error(source: LibrarySource, name: &str, path: &str, message: &str) -> LibraryLoadError {
        LibraryLoadError {
            volume_path: path.to_string(),
            volume_name: name.to_string(),
            source,
            message: message.to_string(),
        }
    }

    #[test]
    fn empty_state_asks_for_a_usb() {
        let state = LibraryState {
            libraries: Vec::new(),
            errors: Vec::new(),
        };
        assert_eq!(home_lines(&state), vec![HomeLine::Empty]);
        assert_eq!(EMPTY_HOME, "Plug in a rekordbox or Serato USB.");
    }

    #[test]
    fn groups_exports_under_their_volume() {
        let state = LibraryState {
            libraries: vec![
                library(LibrarySource::Rekordbox, "GIGS", "D:\\", 1),
                library(LibrarySource::DeviceLibraryPlus, "GIGS", "D:\\", 2),
                library(LibrarySource::Serato, "HOUSE", "E:\\", 0),
            ],
            errors: vec![error(
                LibrarySource::Serato,
                "GIGS",
                "D:\\",
                "could not read the crates",
            )],
        };
        assert_eq!(
            home_lines(&state),
            vec![
                HomeLine::Volume("GIGS".to_string()),
                HomeLine::Export {
                    label: "Device Library".to_string(),
                    detail: "1 track".to_string(),
                    library: Some(id(LibrarySource::Rekordbox, "D:\\")),
                },
                HomeLine::Export {
                    label: "Device Library Plus".to_string(),
                    detail: "2 tracks".to_string(),
                    library: Some(id(LibrarySource::DeviceLibraryPlus, "D:\\")),
                },
                HomeLine::Export {
                    label: "Serato".to_string(),
                    detail: "could not read the crates".to_string(),
                    library: None,
                },
                HomeLine::Volume("HOUSE".to_string()),
                HomeLine::Export {
                    label: "Serato".to_string(),
                    detail: "0 tracks".to_string(),
                    library: Some(id(LibrarySource::Serato, "E:\\")),
                },
            ]
        );
    }

    #[test]
    fn a_blank_volume_name_uses_the_path() {
        let state = LibraryState {
            libraries: vec![library(LibrarySource::Rekordbox, "", "/media/usb", 3)],
            errors: Vec::new(),
        };
        assert_eq!(
            home_lines(&state),
            vec![
                HomeLine::Volume("/media/usb".to_string()),
                HomeLine::Export {
                    label: "Device Library".to_string(),
                    detail: "3 tracks".to_string(),
                    library: Some(id(LibrarySource::Rekordbox, "/media/usb")),
                },
            ]
        );
    }
}
