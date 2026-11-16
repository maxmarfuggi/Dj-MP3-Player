//! Playlist order: saved orders, drag moves, and column sorting.

use std::cmp::Ordering;
use std::collections::HashMap;

use icu_collator::options::{CollatorOptions, Strength};
use icu_collator::preferences::CollationNumericOrdering;
use icu_collator::{Collator, CollatorBorrowed, CollatorPreferences};
use icu_locale_core::locale;

use crate::Track;

/// A column the track list can be sorted by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortColumn {
    Title,
    Artist,
    Genre,
    Label,
    Bpm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDirection {
    Asc,
    Desc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SortState {
    pub column: SortColumn,
    pub direction: SortDirection,
}

/// A track as it appears at one position of a list. Playlists can repeat a track.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackEntry {
    /// Unique within its list: the track id plus which repeat of it this is.
    pub key: String,
    pub track: Track,
}

/// `en` collation, numeric, base strength. Sorting does not follow the machine
/// locale, so the desktop and the wall player order titles the same way.
/// `Track 2` comes before `Track 10`, and case is ignored.
fn list_collator() -> CollatorBorrowed<'static> {
    let mut prefs = CollatorPreferences::from(locale!("en"));
    prefs.numeric_ordering = Some(CollationNumericOrdering::True);
    let mut options = CollatorOptions::default();
    options.strength = Some(Strength::Primary);
    Collator::try_new(prefs, options).expect("English collation data is compiled in")
}

#[must_use]
pub fn to_entries(tracks: &[Track]) -> Vec<TrackEntry> {
    let mut seen: HashMap<&str, usize> = HashMap::new();
    let mut entries = Vec::with_capacity(tracks.len());
    for track in tracks {
        let occurrence = seen.get(track.id.as_str()).copied().unwrap_or(0);
        seen.insert(track.id.as_str(), occurrence + 1);
        entries.push(TrackEntry {
            key: format!("{}#{occurrence}", track.id),
            track: track.clone(),
        });
    }
    entries
}

/// Applies a saved custom order to a playlist's current track ids.
///
/// The playlist may have changed since the order was saved. Saved tracks that
/// are gone are dropped, and tracks the order does not know about are appended
/// in their playlist order. Repeated tracks are matched by count.
#[must_use]
pub fn reconcile_order(saved: &[impl AsRef<str>], current: &[impl AsRef<str>]) -> Vec<String> {
    let mut remaining: HashMap<String, usize> = HashMap::new();
    for id in current {
        *remaining.entry(id.as_ref().to_string()).or_default() += 1;
    }
    let mut result = Vec::new();
    let take = |remaining: &mut HashMap<String, usize>, result: &mut Vec<String>, id: &str| {
        let count = remaining.get(id).copied().unwrap_or(0);
        if count == 0 {
            return;
        }
        remaining.insert(id.to_string(), count - 1);
        result.push(id.to_string());
    };
    for id in saved {
        take(&mut remaining, &mut result, id.as_ref());
    }
    for id in current {
        take(&mut remaining, &mut result, id.as_ref());
    }
    result
}

/// Moves the item at `from` so that it ends up at index `to` of the result.
#[must_use]
pub fn move_item<T: Clone>(items: &[T], from: usize, to: usize) -> Vec<T> {
    let mut result = items.to_vec();
    if from >= result.len() {
        return result;
    }
    let item = result.remove(from);
    let index = to.min(result.len());
    result.insert(index, item);
    result
}

/// Sorts entries by a column. Tracks without a BPM, or with a blank artist,
/// genre, or label, always go last. Ties keep list order.
#[must_use]
pub fn sort_entries(entries: &[TrackEntry], sort: SortState) -> Vec<TrackEntry> {
    let collator = list_collator();
    let mut indexed: Vec<(usize, &TrackEntry)> = entries.iter().enumerate().collect();
    indexed.sort_by(|left, right| {
        compare_tracks(&collator, &left.1.track, &right.1.track, sort).then(left.0.cmp(&right.0))
    });
    indexed
        .into_iter()
        .map(|(_, entry)| entry.clone())
        .collect()
}

fn compare_tracks(
    collator: &CollatorBorrowed<'static>,
    left: &Track,
    right: &Track,
    sort: SortState,
) -> Ordering {
    let desc = sort.direction == SortDirection::Desc;
    match sort.column {
        SortColumn::Title => signed(collator.compare(&left.title, &right.title), desc),
        SortColumn::Artist => {
            compare_text(collator, &left.artist, &right.artist, left, right, desc)
        }
        SortColumn::Genre => compare_text(
            collator,
            left.genre.as_deref().unwrap_or(""),
            right.genre.as_deref().unwrap_or(""),
            left,
            right,
            desc,
        ),
        SortColumn::Label => compare_text(
            collator,
            left.label.as_deref().unwrap_or(""),
            right.label.as_deref().unwrap_or(""),
            left,
            right,
            desc,
        ),
        SortColumn::Bpm => compare_bpm(left.bpm, right.bpm, desc),
    }
}

/// A blank value sorts last in either direction. Otherwise the column decides
/// the order and the title breaks ties, still in ascending title order.
fn compare_text(
    collator: &CollatorBorrowed<'static>,
    left_text: &str,
    right_text: &str,
    left: &Track,
    right: &Track,
    desc: bool,
) -> Ordering {
    if left_text.is_empty() || right_text.is_empty() {
        return left_text.is_empty().cmp(&right_text.is_empty());
    }
    signed(collator.compare(left_text, right_text), desc)
        .then_with(|| collator.compare(&left.title, &right.title))
}

fn compare_bpm(left: Option<f64>, right: Option<f64>, desc: bool) -> Ordering {
    match (left, right) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(left), Some(right)) => signed(left.total_cmp(&right), desc),
    }
}

fn signed(ordering: Ordering, desc: bool) -> Ordering {
    if desc {
        ordering.reverse()
    } else {
        ordering
    }
}

#[must_use]
pub fn sort_by_title(tracks: &[Track]) -> Vec<Track> {
    let collator = list_collator();
    let mut sorted = tracks.to_vec();
    sorted.sort_by(|left, right| collator.compare(&left.title, &right.title));
    sorted
}

/// Clicking a column header sorts ascending, then descending, then turns sorting off.
#[must_use]
pub fn next_sort(sort: Option<SortState>, column: SortColumn) -> Option<SortState> {
    match sort {
        Some(sort) if sort.column == column && sort.direction == SortDirection::Asc => {
            Some(SortState {
                column,
                direction: SortDirection::Desc,
            })
        }
        Some(sort) if sort.column == column => None,
        _ => Some(SortState {
            column,
            direction: SortDirection::Asc,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        move_item, next_sort, reconcile_order, sort_entries, to_entries, SortColumn, SortDirection,
        SortState, TrackEntry,
    };
    use crate::Track;

    fn track(
        id: &str,
        artist: &str,
        bpm: Option<f64>,
        genre: Option<&str>,
        label: Option<&str>,
    ) -> Track {
        Track {
            id: id.to_string(),
            title: id.to_string(),
            artist: artist.to_string(),
            album: String::new(),
            bpm,
            key: None,
            duration_sec: None,
            relative_path: format!("{id}.mp3"),
            genre: genre.map(str::to_string),
            label: label.map(str::to_string),
            color: None,
            artwork_path: None,
        }
    }

    fn titles(entries: &[TrackEntry]) -> Vec<String> {
        entries
            .iter()
            .map(|entry| entry.track.title.clone())
            .collect()
    }

    fn sort(column: SortColumn, direction: SortDirection) -> SortState {
        SortState { column, direction }
    }

    #[test]
    fn gives_repeated_tracks_distinct_keys() {
        let a = track("a", "", None, None, None);
        let entries = to_entries(&[a.clone(), track("b", "", None, None, None), a]);
        assert_eq!(
            entries
                .iter()
                .map(|entry| entry.key.as_str())
                .collect::<Vec<_>>(),
            ["a#0", "b#0", "a#1"]
        );
    }

    #[test]
    fn uses_the_saved_order_when_the_playlist_is_unchanged() {
        assert_eq!(
            reconcile_order(&["c", "a", "b"], &["a", "b", "c"]),
            ["c", "a", "b"]
        );
    }

    #[test]
    fn drops_removed_tracks_and_appends_new_ones_in_playlist_order() {
        assert_eq!(
            reconcile_order(&["c", "x", "a"], &["a", "d", "c", "e"]),
            ["c", "a", "d", "e"]
        );
    }

    #[test]
    fn matches_repeated_tracks_by_count() {
        assert_eq!(
            reconcile_order(&["b", "a", "a", "a"], &["a", "b", "a"]),
            ["b", "a", "a"]
        );
        assert_eq!(
            reconcile_order(&["a", "b"], &["a", "b", "a"]),
            ["a", "b", "a"]
        );
    }

    #[test]
    fn moves_an_item_down_or_up() {
        assert_eq!(move_item(&["a", "b", "c", "d"], 0, 2), ["b", "c", "a", "d"]);
        assert_eq!(move_item(&["a", "b", "c", "d"], 3, 0), ["d", "a", "b", "c"]);
    }

    #[test]
    fn clamps_the_target_to_the_list() {
        assert_eq!(move_item(&["a", "b", "c"], 0, 10), ["b", "c", "a"]);
    }

    fn sample_entries() -> Vec<TrackEntry> {
        to_entries(&[
            track("Zeta", "Bravo", Some(128.0), None, None),
            track("alpha", "", None, None, None),
            track("Track 10", "alpha", Some(124.5), None, None),
            track("Track 2", "Alpha", Some(128.0), None, None),
        ])
    }

    #[test]
    fn sorts_titles_case_insensitively_with_natural_number_order() {
        let entries = sample_entries();
        assert_eq!(
            titles(&sort_entries(
                &entries,
                sort(SortColumn::Title, SortDirection::Asc)
            )),
            ["alpha", "Track 2", "Track 10", "Zeta"]
        );
        assert_eq!(
            titles(&sort_entries(
                &entries,
                sort(SortColumn::Title, SortDirection::Desc)
            )),
            ["Zeta", "Track 10", "Track 2", "alpha"]
        );
    }

    #[test]
    fn sorts_by_artist_then_title_with_blank_artists_last_either_way() {
        let entries = sample_entries();
        assert_eq!(
            titles(&sort_entries(
                &entries,
                sort(SortColumn::Artist, SortDirection::Asc)
            )),
            ["Track 2", "Track 10", "Zeta", "alpha"]
        );
        let descending = sort_entries(&entries, sort(SortColumn::Artist, SortDirection::Desc));
        assert_eq!(
            descending.last().map(|entry| entry.track.title.as_str()),
            Some("alpha")
        );
    }

    #[test]
    fn sorts_by_genre_or_label_with_tracks_that_have_none_last() {
        let tagged = to_entries(&[
            track("b", "", None, Some("Techno"), Some("Kompakt")),
            track("a", "", None, None, None),
            track("d", "", None, Some("house"), Some("")),
            track("c", "", None, Some("House"), Some("Defected")),
        ]);
        assert_eq!(
            titles(&sort_entries(
                &tagged,
                sort(SortColumn::Genre, SortDirection::Asc)
            )),
            ["c", "d", "b", "a"]
        );
        assert_eq!(
            titles(&sort_entries(
                &tagged,
                sort(SortColumn::Genre, SortDirection::Desc)
            )),
            ["b", "c", "d", "a"]
        );
        assert_eq!(
            titles(&sort_entries(
                &tagged,
                sort(SortColumn::Label, SortDirection::Asc)
            )),
            ["c", "b", "a", "d"]
        );
    }

    #[test]
    fn sorts_by_bpm_with_missing_bpms_last_and_ties_in_list_order() {
        let entries = sample_entries();
        assert_eq!(
            titles(&sort_entries(
                &entries,
                sort(SortColumn::Bpm, SortDirection::Asc)
            )),
            ["Track 10", "Zeta", "Track 2", "alpha"]
        );
        assert_eq!(
            titles(&sort_entries(
                &entries,
                sort(SortColumn::Bpm, SortDirection::Desc)
            )),
            ["Zeta", "Track 2", "Track 10", "alpha"]
        );
    }

    #[test]
    fn cycles_ascending_descending_off() {
        let asc = next_sort(None, SortColumn::Bpm);
        assert_eq!(asc, Some(sort(SortColumn::Bpm, SortDirection::Asc)));
        let desc = next_sort(asc, SortColumn::Bpm);
        assert_eq!(desc, Some(sort(SortColumn::Bpm, SortDirection::Desc)));
        assert_eq!(next_sort(desc, SortColumn::Bpm), None);
        assert_eq!(
            next_sort(desc, SortColumn::Title),
            Some(sort(SortColumn::Title, SortDirection::Asc))
        );
    }
}
