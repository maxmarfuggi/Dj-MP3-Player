//! Queue rules shared by the desktop and the wall player.
//!
//! "Previous" restarts the current track once three seconds have passed.
//! Reordering keeps the current entry playing. A track that cannot be played
//! is remembered and skipped.

use std::collections::BTreeSet;
use std::time::Duration;

/// Going "previous" this far into a track restarts it instead.
pub const RESTART_WITHIN: Duration = Duration::from_secs(3);

/// One position in a queue. The same track can appear twice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueItem {
    pub library_id: String,
    pub track_id: String,
    /// Identifies this position. A playlist can repeat a track.
    pub entry_key: String,
}

/// What a queue command asks the audio engine to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    /// Play this entry from the start.
    Start(usize),
    /// Move the current entry back to the start. Leave pause as it is.
    Restart,
    /// The queue order changed. Keep playing the current entry from where it is.
    Reordered,
    /// Nothing left to play. The current entry stays selected.
    Pause,
    /// The command changed nothing.
    Unchanged,
}

/// The queue, which entry is current, and the tracks that failed to play.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Playback {
    items: Vec<QueueItem>,
    index: Option<usize>,
    source: Option<String>,
    failed: BTreeSet<String>,
}

impl Playback {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn current(&self) -> Option<&QueueItem> {
        self.index.and_then(|index| self.items.get(index))
    }

    #[must_use]
    pub fn index(&self) -> Option<usize> {
        self.index
    }

    #[must_use]
    pub fn source(&self) -> Option<&str> {
        self.source.as_deref()
    }

    #[must_use]
    pub fn failed(&self) -> &BTreeSet<String> {
        &self.failed
    }

    /// Replaces the queue and selects `index`. An index past the end changes nothing.
    pub fn play(
        &mut self,
        items: Vec<QueueItem>,
        index: usize,
        source: impl Into<String>,
    ) -> Change {
        if index >= items.len() {
            return Change::Unchanged;
        }
        self.items = items;
        self.index = Some(index);
        self.source = Some(source.into());
        Change::Start(index)
    }

    /// Swaps in a reordered queue from the same view, without changing the current entry.
    ///
    /// Ignored when the view is different or the current entry is not in `items`.
    /// The first matching entry is the one that keeps playing.
    pub fn reorder(&mut self, items: Vec<QueueItem>, source: &str) -> Change {
        let Some(index) = self.index else {
            return Change::Unchanged;
        };
        if self.source.as_deref() != Some(source) {
            return Change::Unchanged;
        }
        let Some(entry_key) = self.items.get(index).map(|item| item.entry_key.clone()) else {
            return Change::Unchanged;
        };
        let Some(new_index) = items.iter().position(|item| item.entry_key == entry_key) else {
            return Change::Unchanged;
        };
        self.items = items;
        self.index = Some(new_index);
        Change::Reordered
    }

    /// The entry after `heard`, or nothing at the end of the queue.
    pub fn next(&mut self, heard: usize) -> Change {
        self.advance(heard, false)
    }

    /// The entry before `heard` during the first three seconds, otherwise a restart.
    pub fn previous(&mut self, heard: usize, position: Duration) -> Change {
        if self.source.is_none() || heard >= self.items.len() {
            return Change::Unchanged;
        }
        if heard > 0 && position < RESTART_WITHIN {
            self.index = Some(heard - 1);
            Change::Start(heard - 1)
        } else {
            self.index = Some(heard);
            Change::Restart
        }
    }

    /// The entry after one that finished, or a pause at the end of the queue.
    pub fn track_ended(&mut self, heard: usize) -> Change {
        self.advance(heard, true)
    }

    /// Remembers `heard` and skips it. The last entry pauses instead.
    pub fn track_failed(&mut self, heard: usize) -> Change {
        let Some(item) = self.items.get(heard) else {
            return Change::Unchanged;
        };
        self.failed
            .insert(failure_key(&item.library_id, &item.track_id));
        self.advance(heard, true)
    }

    fn advance(&mut self, heard: usize, pause_at_end: bool) -> Change {
        if self.source.is_none() || heard >= self.items.len() {
            return Change::Unchanged;
        }
        let next = heard + 1;
        if next >= self.items.len() {
            self.index = Some(heard);
            return if pause_at_end {
                Change::Pause
            } else {
                Change::Unchanged
            };
        }
        self.index = Some(next);
        Change::Start(next)
    }
}

/// `libraryId/trackId`, the key a failed track is remembered under.
#[must_use]
pub fn failure_key(library_id: &str, track_id: &str) -> String {
    format!("{library_id}/{track_id}")
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{failure_key, Change, Playback, QueueItem, RESTART_WITHIN};

    fn item(track: &str, entry: &str) -> QueueItem {
        QueueItem {
            library_id: "rekordbox-857074f2fb38".to_string(),
            track_id: track.to_string(),
            entry_key: entry.to_string(),
        }
    }

    fn queue() -> Vec<QueueItem> {
        vec![item("1", "a"), item("2", "b"), item("1", "c")]
    }

    fn started() -> Playback {
        let mut playback = Playback::new();
        assert_eq!(
            playback.play(queue(), 1, "rekordbox-857074f2fb38/2"),
            Change::Start(1)
        );
        playback
    }

    #[test]
    fn play_selects_the_entry_and_remembers_the_view() {
        let playback = started();
        assert_eq!(playback.index(), Some(1));
        assert_eq!(
            playback.current().map(|item| item.entry_key.as_str()),
            Some("b")
        );
        assert_eq!(playback.source(), Some("rekordbox-857074f2fb38/2"));
    }

    #[test]
    fn play_past_the_end_leaves_the_queue_alone() {
        let mut playback = started();
        assert_eq!(
            playback.play(vec![item("9", "z")], 3, "other"),
            Change::Unchanged
        );
        assert_eq!(playback.index(), Some(1));
        assert_eq!(playback.source(), Some("rekordbox-857074f2fb38/2"));
    }

    #[test]
    fn previous_goes_back_during_the_first_three_seconds() {
        let mut playback = started();
        assert_eq!(
            playback.previous(1, Duration::from_secs(3) - Duration::from_nanos(1)),
            Change::Start(0)
        );
        assert_eq!(playback.index(), Some(0));
    }

    #[test]
    fn previous_restarts_once_three_seconds_have_passed() {
        let mut playback = started();
        assert_eq!(playback.previous(1, RESTART_WITHIN), Change::Restart);
        assert_eq!(playback.index(), Some(1));
        assert_eq!(
            playback.previous(1, Duration::from_secs(30)),
            Change::Restart
        );
    }

    #[test]
    fn previous_restarts_the_first_entry() {
        let mut playback = Playback::new();
        playback.play(queue(), 0, "all");
        assert_eq!(playback.previous(0, Duration::ZERO), Change::Restart);
        assert_eq!(playback.index(), Some(0));
    }

    #[test]
    fn previous_does_nothing_before_a_queue_is_loaded() {
        let mut playback = Playback::new();
        assert_eq!(playback.previous(0, Duration::ZERO), Change::Unchanged);
    }

    #[test]
    fn reorder_keeps_the_current_entry_and_follows_it() {
        let mut playback = started();
        let reordered = vec![item("1", "c"), item("2", "b"), item("1", "a")];
        assert_eq!(
            playback.reorder(reordered, "rekordbox-857074f2fb38/2"),
            Change::Reordered
        );
        assert_eq!(playback.index(), Some(1));
        assert_eq!(
            playback.current().map(|item| item.entry_key.as_str()),
            Some("b")
        );
        assert_eq!(
            playback.current().map(|item| item.track_id.as_str()),
            Some("2")
        );
    }

    #[test]
    fn reorder_finds_the_first_copy_of_the_current_entry() {
        let mut playback = started();
        let reordered = vec![item("2", "b"), item("2", "b"), item("1", "a")];
        playback.reorder(reordered, "rekordbox-857074f2fb38/2");
        assert_eq!(playback.index(), Some(0));
    }

    #[test]
    fn reorder_from_another_view_is_ignored() {
        let mut playback = started();
        assert_eq!(
            playback.reorder(vec![item("1", "a")], "other"),
            Change::Unchanged
        );
        assert_eq!(playback.index(), Some(1));
        assert_eq!(
            playback.current().map(|item| item.entry_key.as_str()),
            Some("b")
        );
    }

    #[test]
    fn reorder_that_drops_the_current_entry_is_ignored() {
        let mut playback = started();
        assert_eq!(
            playback.reorder(
                vec![item("1", "a"), item("1", "c")],
                "rekordbox-857074f2fb38/2"
            ),
            Change::Unchanged
        );
        assert_eq!(playback.index(), Some(1));
    }

    #[test]
    fn next_moves_on_and_does_nothing_after_the_last_entry() {
        let mut playback = started();
        assert_eq!(playback.next(1), Change::Start(2));
        assert_eq!(playback.index(), Some(2));
        assert_eq!(playback.next(2), Change::Unchanged);
        assert_eq!(playback.index(), Some(2));
    }

    #[test]
    fn the_last_entry_pauses_when_it_finishes() {
        let mut playback = started();
        playback.next(1);
        assert_eq!(playback.track_ended(2), Change::Pause);
        assert_eq!(playback.index(), Some(2));
    }

    #[test]
    fn a_failed_entry_is_skipped_and_remembered() {
        let mut playback = started();
        assert_eq!(playback.track_failed(1), Change::Start(2));
        assert_eq!(playback.index(), Some(2));
        assert!(playback
            .failed()
            .contains(&failure_key("rekordbox-857074f2fb38", "2")));
        // The earlier copy of track 1 shares the key when it fails.
        assert_eq!(playback.track_failed(2), Change::Pause);
        assert!(playback
            .failed()
            .contains(&failure_key("rekordbox-857074f2fb38", "1")));
        assert_eq!(playback.failed().len(), 2);
    }

    #[test]
    fn failing_one_copy_remembers_the_track_not_the_position() {
        let mut playback = Playback::new();
        playback.play(queue(), 0, "all");
        playback.track_failed(0);
        assert_eq!(
            playback
                .failed()
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            vec!["rekordbox-857074f2fb38/1"]
        );
    }
}
