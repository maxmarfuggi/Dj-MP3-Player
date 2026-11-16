//! Which library, playlist, and rows are on screen.
//!
//! A selection that names a library still mounted wins. Otherwise the first
//! library is used, which is what happens when a USB is removed.

use std::collections::HashMap;

use crate::logic::order::{sort_by_title, sort_entries, to_entries, SortState, TrackEntry};
use crate::{Library, PlaylistNode, Track};

/// One row of the queue built from the list on screen.
#[derive(Debug, Clone, PartialEq)]
pub struct QueueEntry {
    pub library_id: String,
    pub track: Track,
    /// Identifies this position. A playlist can repeat a track.
    pub entry_key: String,
}

/// The playlist the user selected, or all tracks when `playlist_id` is `None`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaylistSelection {
    pub library_id: String,
    pub playlist_id: Option<String>,
}

/// `<libraryId>/<playlistId>`, the key a custom playlist order is saved under.
#[must_use]
pub fn order_key(library_id: &str, playlist_id: &str) -> String {
    format!("{library_id}/{playlist_id}")
}

/// The library to show.
///
/// A selection that names a library still mounted wins. Otherwise the first
/// library is used.
#[must_use]
pub fn visible_library<'a>(
    libraries: &'a [Library],
    selected_library_id: Option<&str>,
) -> Option<&'a Library> {
    selected_library_id
        .and_then(|id| libraries.iter().find(|library| library.id == id))
        .or_else(|| libraries.first())
}

#[must_use]
pub fn find_playlist<'a>(nodes: &'a [PlaylistNode], id: &str) -> Option<&'a PlaylistNode> {
    for node in nodes {
        if node.id == id {
            return Some(node);
        }
        if let Some(found) = find_playlist(&node.children, id) {
            return Some(found);
        }
    }
    None
}

/// The playlist named by the selection, when that selection belongs to `library`.
///
/// All tracks, a selection for another library, and a playlist that no longer
/// exists all mean there is no playlist to show.
#[must_use]
pub fn visible_playlist<'a>(
    library: Option<&'a Library>,
    selection: Option<&PlaylistSelection>,
) -> Option<&'a PlaylistNode> {
    let library = library?;
    let selection = selection?;
    if selection.library_id != library.id {
        return None;
    }
    let playlist_id = selection.playlist_id.as_deref()?;
    find_playlist(&library.playlists, playlist_id)
}

/// Identifies the list on screen. The player uses it to tell one queue from another.
#[must_use]
pub fn list_view_key(library: Option<&Library>, playlist: Option<&PlaylistNode>) -> String {
    match (library, playlist) {
        (Some(library), Some(playlist)) => order_key(&library.id, &playlist.id),
        (Some(library), None) => format!("{}:all", library.id),
        (None, _) => "undefined:all".to_string(),
    }
}

/// The rows of the list on screen, in custom order when there is one, then sorted.
#[must_use]
pub fn list_entries(
    library: Option<&Library>,
    playlist: Option<&PlaylistNode>,
    custom_order: Option<&[String]>,
    sort: Option<SortState>,
) -> Vec<TrackEntry> {
    let Some(library) = library else {
        return Vec::new();
    };
    let tracks = match playlist {
        Some(playlist) => tracks_in_playlist(library, playlist, custom_order),
        None => sort_by_title(&library.tracks),
    };
    let listed = to_entries(&tracks);
    match sort {
        Some(sort) => sort_entries(&listed, sort),
        None => listed,
    }
}

fn tracks_in_playlist(
    library: &Library,
    playlist: &PlaylistNode,
    custom_order: Option<&[String]>,
) -> Vec<Track> {
    let mut tracks_by_id: HashMap<&str, &Track> = HashMap::new();
    for track in &library.tracks {
        tracks_by_id.insert(track.id.as_str(), track);
    }
    let owned;
    let track_ids: &[String] = if let Some(custom_order) = custom_order {
        owned = super::reconcile_order(custom_order, &playlist.track_ids);
        &owned
    } else {
        &playlist.track_ids
    };
    track_ids
        .iter()
        .filter_map(|id| tracks_by_id.get(id.as_str()).map(|track| (*track).clone()))
        .collect()
}

#[must_use]
pub fn queue_from_entries(library_id: Option<&str>, entries: &[TrackEntry]) -> Vec<QueueEntry> {
    let Some(library_id) = library_id else {
        return Vec::new();
    };
    entries
        .iter()
        .map(|entry| QueueEntry {
            library_id: library_id.to_string(),
            track: entry.track.clone(),
            entry_key: entry.key.clone(),
        })
        .collect()
}

/// The track that is playing lives on a library that is no longer mounted.
#[must_use]
pub fn library_was_removed(libraries: &[Library], playing_library_id: Option<&str>) -> bool {
    match playing_library_id {
        Some(id) => !libraries.iter().any(|library| library.id == id),
        None => false,
    }
}

/// Which row is the one playing.
///
/// A list prefers its own queue position. A track started from another list of
/// the same library is matched by track id. `None` is a row that is not in the list.
#[must_use]
pub fn current_entry_index(
    entries: &[TrackEntry],
    playing_entry_key: Option<&str>,
    playing_track_id: Option<&str>,
) -> Option<usize> {
    if let Some(key) = playing_entry_key {
        entries.iter().position(|entry| entry.key == key)
    } else {
        let track_id = playing_track_id?;
        entries.iter().position(|entry| entry.track.id == track_id)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        current_entry_index, find_playlist, library_was_removed, list_entries, list_view_key,
        queue_from_entries, visible_library, visible_playlist, PlaylistSelection,
    };
    use crate::logic::{SortColumn, SortDirection, SortState};
    use crate::{Library, LibrarySource, PlaylistKind, PlaylistNode, Track};

    fn track(id: &str, title: &str) -> Track {
        Track {
            id: id.to_string(),
            title: title.to_string(),
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

    fn library(id: &str, tracks: Vec<Track>, playlists: Vec<PlaylistNode>) -> Library {
        Library {
            id: id.to_string(),
            source: LibrarySource::Rekordbox,
            volume_name: id.to_string(),
            volume_path: format!("/{id}"),
            tracks,
            playlists,
        }
    }

    fn nested() -> PlaylistNode {
        PlaylistNode {
            id: "folder".to_string(),
            name: "Folder".to_string(),
            kind: PlaylistKind::Folder,
            track_ids: Vec::new(),
            children: vec![PlaylistNode {
                id: "set".to_string(),
                name: "Set".to_string(),
                kind: PlaylistKind::Playlist,
                track_ids: vec!["b".to_string(), "a".to_string(), "gone".to_string()],
                children: Vec::new(),
            }],
        }
    }

    #[test]
    fn uses_the_selected_library() {
        let libraries = [
            library("first", Vec::new(), Vec::new()),
            library("second", Vec::new(), Vec::new()),
        ];
        assert_eq!(
            visible_library(&libraries, Some("second")).unwrap().id,
            "second"
        );
    }

    #[test]
    fn falls_back_to_the_first_library_when_nothing_is_selected_or_the_usb_was_removed() {
        let libraries = [
            library("first", Vec::new(), Vec::new()),
            library("second", Vec::new(), Vec::new()),
        ];
        assert_eq!(visible_library(&libraries, None).unwrap().id, "first");
        assert_eq!(
            visible_library(&libraries, Some("unplugged")).unwrap().id,
            "first"
        );
        assert!(visible_library(&[], Some("first")).is_none());
    }

    #[test]
    fn finds_a_playlist_nested_in_a_folder() {
        let tree = [nested()];
        assert_eq!(find_playlist(&tree, "set").unwrap().name, "Set");
        assert!(find_playlist(&tree, "missing").is_none());
    }

    #[test]
    fn returns_the_selected_playlist_of_the_library_on_screen() {
        let shown = library("lib", Vec::new(), vec![nested()]);
        let selection = PlaylistSelection {
            library_id: "lib".to_string(),
            playlist_id: Some("set".to_string()),
        };
        assert_eq!(
            visible_playlist(Some(&shown), Some(&selection)).unwrap().id,
            "set"
        );
    }

    #[test]
    fn shows_all_tracks_when_the_selection_is_not_a_playlist_of_this_library() {
        let shown = library("lib", Vec::new(), vec![nested()]);
        let all = PlaylistSelection {
            library_id: "lib".to_string(),
            playlist_id: None,
        };
        let other = PlaylistSelection {
            library_id: "other".to_string(),
            playlist_id: Some("set".to_string()),
        };
        let missing = PlaylistSelection {
            library_id: "lib".to_string(),
            playlist_id: Some("missing".to_string()),
        };
        let set = PlaylistSelection {
            library_id: "lib".to_string(),
            playlist_id: Some("set".to_string()),
        };
        assert!(visible_playlist(Some(&shown), Some(&all)).is_none());
        assert!(visible_playlist(Some(&shown), Some(&other)).is_none());
        assert!(visible_playlist(Some(&shown), Some(&missing)).is_none());
        assert!(visible_playlist(None, Some(&set)).is_none());
    }

    #[test]
    fn sorts_all_tracks_by_title() {
        let shown = library(
            "lib",
            vec![track("b", "Beta"), track("a", "Alpha")],
            vec![nested()],
        );
        let entries = list_entries(Some(&shown), None, None, None);
        let ids: Vec<_> = entries
            .iter()
            .map(|entry| entry.track.id.as_str())
            .collect();
        assert_eq!(ids, ["a", "b"]);
    }

    #[test]
    fn lists_a_playlist_in_its_order_and_skips_tracks_the_library_no_longer_has() {
        let shown = library(
            "lib",
            vec![track("b", "Beta"), track("a", "Alpha")],
            vec![nested()],
        );
        let playlist = &shown.playlists[0].children[0];
        let entries = list_entries(Some(&shown), Some(playlist), None, None);
        let ids: Vec<_> = entries
            .iter()
            .map(|entry| entry.track.id.as_str())
            .collect();
        assert_eq!(ids, ["b", "a"]);
    }

    #[test]
    fn applies_a_saved_custom_order() {
        let shown = library(
            "lib",
            vec![track("b", "Beta"), track("a", "Alpha")],
            vec![nested()],
        );
        let playlist = &shown.playlists[0].children[0];
        let order = vec!["a".to_string(), "b".to_string()];
        let entries = list_entries(Some(&shown), Some(playlist), Some(&order), None);
        let ids: Vec<_> = entries
            .iter()
            .map(|entry| entry.track.id.as_str())
            .collect();
        assert_eq!(ids, ["a", "b"]);
    }

    #[test]
    fn sorts_the_rows_when_a_column_sort_is_active() {
        let shown = library(
            "lib",
            vec![track("b", "Beta"), track("a", "Alpha")],
            vec![nested()],
        );
        let playlist = &shown.playlists[0].children[0];
        let entries = list_entries(
            Some(&shown),
            Some(playlist),
            None,
            Some(SortState {
                column: SortColumn::Title,
                direction: SortDirection::Asc,
            }),
        );
        let titles: Vec<_> = entries
            .iter()
            .map(|entry| entry.track.title.as_str())
            .collect();
        assert_eq!(titles, ["Alpha", "Beta"]);
    }

    #[test]
    fn returns_nothing_when_no_library_is_mounted() {
        assert!(list_entries(None, None, None, None).is_empty());
    }

    #[test]
    fn builds_a_queue_from_the_rows_on_screen() {
        let shown = library("lib", vec![track("a", "a")], Vec::new());
        let entries = list_entries(Some(&shown), None, None, None);
        let queue = queue_from_entries(Some("lib"), &entries);
        assert_eq!(queue.len(), 1);
        assert_eq!(queue[0].library_id, "lib");
        assert_eq!(queue[0].entry_key, "a#0");
        assert_eq!(queue[0].track.id, entries[0].track.id);
    }

    #[test]
    fn queue_is_empty_until_a_library_is_showing() {
        assert!(queue_from_entries(None, &[]).is_empty());
    }

    #[test]
    fn names_a_playlist_list_and_an_all_tracks_list_differently() {
        let shown = library("lib", Vec::new(), Vec::new());
        let playlist = nested().children.remove(0);
        assert_eq!(list_view_key(Some(&shown), Some(&playlist)), "lib/set");
        assert_eq!(list_view_key(Some(&shown), None), "lib:all");
    }

    #[test]
    fn library_was_removed_is_true_only_when_the_playing_library_is_gone() {
        let libraries = [library("still", Vec::new(), Vec::new())];
        assert!(!library_was_removed(&libraries, Some("still")));
        assert!(library_was_removed(&libraries, Some("unplugged")));
        assert!(!library_was_removed(&libraries, None));
    }

    #[test]
    fn prefers_the_queue_position_and_otherwise_matches_the_track() {
        let shown = library("lib", vec![track("a", "a"), track("a", "a")], Vec::new());
        let entries = list_entries(Some(&shown), None, None, None);
        assert_eq!(
            current_entry_index(&entries, Some("a#1"), Some("a")),
            Some(1)
        );
        assert_eq!(current_entry_index(&entries, None, Some("a")), Some(0));
        assert_eq!(current_entry_index(&entries, None, Some("missing")), None);
    }
}
