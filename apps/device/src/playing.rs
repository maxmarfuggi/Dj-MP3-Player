//! The line under the home list that says what is playing.
//!
//! Pressing an export plays its tracks in order, and pressing it again
//! pauses. The now-playing screen replaces this line later.

use std::path::PathBuf;
use std::time::Duration;

use djusb_core::logic::format_time;
use djusb_core::Library;

use crate::playback::Status;

/// The queue loaded from one export.
pub struct Playing {
    pub library: String,
    pub titles: Vec<String>,
}

/// The name shown for a track: the artist and title, or the title alone.
#[must_use]
pub fn track_name(artist: &str, title: &str) -> String {
    if artist.is_empty() {
        title.to_string()
    } else {
        format!("{artist} – {title}")
    }
}

/// The library's tracks in order, with the names to show beside them.
/// Tracks whose file `resolve` cannot place are left out.
#[must_use]
pub fn build_queue(
    library: &Library,
    resolve: impl Fn(&str) -> Option<PathBuf>,
) -> (Vec<PathBuf>, Vec<String>) {
    library
        .tracks
        .iter()
        .filter_map(|track| {
            let path = resolve(&track.id)?;
            Some((path, track_name(&track.artist, &track.title)))
        })
        .unzip()
}

/// The status line, or `None` when nothing has played.
#[must_use]
pub fn status_line(status: &Status, titles: &[String]) -> Option<String> {
    if let Some(problem) = &status.output_error {
        return Some(format!("No audio output: {problem}"));
    }
    if let (true, Some(error)) = (status.ended, &status.track_error) {
        let title = titles.get(error.index).map_or("a track", String::as_str);
        return Some(format!("Could not play {title}: {}", error.message));
    }
    let title = titles.get(status.track?)?;
    if status.ended {
        return Some(format!("Finished · {title}"));
    }
    let state = if status.paused { "Paused" } else { "Playing" };
    let time = match status.length {
        Some(length) => format!("{} / {}", clock(status.position), clock(length)),
        None => clock(status.position),
    };
    Some(format!("{state} · {title} · {time}"))
}

fn clock(time: Duration) -> String {
    format_time(time.as_secs_f64())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::PathBuf;
    use std::time::Duration;

    use djusb_core::{Library, LibrarySource, Track};

    use super::{build_queue, status_line, track_name};
    use crate::playback::{Status, TrackError};

    fn status() -> Status {
        Status {
            track: Some(1),
            position: Duration::from_secs(83),
            length: Some(Duration::from_secs(300)),
            paused: false,
            ended: false,
            volume: 1.0,
            track_error: None,
            failed: BTreeSet::new(),
            output_error: None,
        }
    }

    fn titles() -> Vec<String> {
        vec!["One".to_string(), "Two".to_string()]
    }

    #[test]
    fn the_queue_leaves_out_tracks_without_a_file() {
        let library = Library {
            id: "rekordbox-1".to_string(),
            source: LibrarySource::Rekordbox,
            volume_name: "USB".to_string(),
            volume_path: "/media/usb".to_string(),
            tracks: vec![track("a", "One"), track("b", "Two"), track("c", "Three")],
            playlists: Vec::new(),
        };
        let (paths, names) = build_queue(&library, |id| {
            (id != "b").then(|| PathBuf::from(format!("/media/usb/{id}.mp3")))
        });
        assert_eq!(
            paths,
            vec![
                PathBuf::from("/media/usb/a.mp3"),
                PathBuf::from("/media/usb/c.mp3")
            ]
        );
        assert_eq!(names, vec!["Artist – One", "Artist – Three"]);
    }

    fn track(id: &str, title: &str) -> Track {
        Track {
            id: id.to_string(),
            title: title.to_string(),
            artist: "Artist".to_string(),
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

    #[test]
    fn names_join_the_artist_and_title() {
        assert_eq!(track_name("Moby", "Porcelain"), "Moby – Porcelain");
        assert_eq!(track_name("", "Porcelain"), "Porcelain");
    }

    #[test]
    fn shows_the_track_and_time() {
        assert_eq!(
            status_line(&status(), &titles()).as_deref(),
            Some("Playing · Two · 1:23 / 5:00")
        );
    }

    #[test]
    fn shows_a_pause_and_an_unknown_length() {
        let paused = Status {
            paused: true,
            length: None,
            ..status()
        };
        assert_eq!(
            status_line(&paused, &titles()).as_deref(),
            Some("Paused · Two · 1:23")
        );
    }

    #[test]
    fn nothing_before_the_first_track() {
        let idle = Status {
            track: None,
            ..status()
        };
        assert_eq!(status_line(&idle, &titles()), None);
    }

    #[test]
    fn a_failed_track_is_named() {
        let failed = Status {
            ended: true,
            track_error: Some(TrackError {
                index: 0,
                message: "The file is missing.".to_string(),
            }),
            ..status()
        };
        assert_eq!(
            status_line(&failed, &titles()).as_deref(),
            Some("Could not play One: The file is missing.")
        );
    }

    #[test]
    fn a_missing_output_comes_first() {
        let silent = Status {
            output_error: Some("No audio output is connected.".to_string()),
            ..status()
        };
        assert_eq!(
            status_line(&silent, &titles()).as_deref(),
            Some("No audio output: No audio output is connected.")
        );
    }
}
