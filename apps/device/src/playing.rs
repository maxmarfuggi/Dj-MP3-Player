//! The track on screen, and the line under the list while you browse.
//!
//! Now playing shows the artist, title, and artwork. The line at the bottom
//! of the other screens uses the same names.

use std::path::PathBuf;
use std::time::Duration;

use djusb_core::logic::{format_time, TrackEntry};

use crate::playback::Status;

/// The queue loaded from one list.
pub struct Playing {
    /// "Artist – Title", in queue order. The status line uses these.
    pub titles: Vec<String>,
    pub artists: Vec<String>,
    pub track_titles: Vec<String>,
    pub artworks: Vec<Option<PathBuf>>,
    /// Musical key, empty when the file does not state one.
    pub keys: Vec<String>,
    pub bpms: Vec<Option<f64>>,
    /// The audio files, in queue order. The deck scans these for waveforms.
    pub paths: Vec<PathBuf>,
}

/// One list ready to play. `start` is the pressed row, after tracks with no
/// file have been left out.
pub struct Cue {
    pub paths: Vec<PathBuf>,
    pub titles: Vec<String>,
    pub artists: Vec<String>,
    pub track_titles: Vec<String>,
    pub artworks: Vec<Option<PathBuf>>,
    pub keys: Vec<String>,
    pub bpms: Vec<Option<f64>>,
    pub start: usize,
}

/// The block above the now-playing rows.
pub struct NowHeader {
    pub artist: String,
    pub title: String,
    pub clock: String,
    pub artwork: Option<PathBuf>,
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

/// The list on screen, in that order, without tracks whose file is missing.
///
/// `None` means the pressed row has no file, so nothing should load.
#[must_use]
pub fn cue_from_entries(
    entries: &[TrackEntry],
    index: usize,
    resolve: impl Fn(&str) -> Option<PathBuf>,
    artwork: impl Fn(&str) -> Option<PathBuf>,
) -> Option<Cue> {
    let chosen = entries.get(index)?;
    resolve(&chosen.track.id)?;
    let mut cue = Cue {
        paths: Vec::new(),
        titles: Vec::new(),
        artists: Vec::new(),
        track_titles: Vec::new(),
        artworks: Vec::new(),
        keys: Vec::new(),
        bpms: Vec::new(),
        start: 0,
    };
    for (at, entry) in entries.iter().enumerate() {
        let Some(path) = resolve(&entry.track.id) else {
            continue;
        };
        if at == index {
            cue.start = cue.paths.len();
        }
        cue.paths.push(path);
        cue.titles
            .push(track_name(&entry.track.artist, &entry.track.title));
        cue.artists.push(entry.track.artist.clone());
        cue.track_titles.push(entry.track.title.clone());
        cue.artworks.push(artwork(&entry.track.id));
        cue.keys.push(entry.track.key.clone().unwrap_or_default());
        cue.bpms.push(entry.track.bpm);
    }
    Some(cue)
}

/// The track heard, for the block above the now-playing rows.
#[must_use]
pub fn now_header(status: &Status, playing: &Playing) -> Option<NowHeader> {
    let index = status.track?;
    Some(NowHeader {
        artist: playing.artists.get(index).cloned().unwrap_or_default(),
        title: playing.track_titles.get(index)?.clone(),
        clock: header_clock(status),
        artwork: playing.artworks.get(index).cloned().flatten(),
    })
}

fn header_clock(status: &Status) -> String {
    if status.ended {
        return "Finished".to_string();
    }
    match status.length {
        Some(length) => format!("{} / {}", clock(status.position), clock(length)),
        None => clock(status.position),
    }
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

    use djusb_core::Track;

    use super::{cue_from_entries, now_header, status_line, track_name, Playing};
    use crate::playback::{Status, TrackError};
    use djusb_core::logic::TrackEntry;

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

    fn entry(id: &str, title: &str) -> TrackEntry {
        let track = track(id, title);
        TrackEntry {
            key: track.id.clone(),
            track,
        }
    }

    #[test]
    fn the_queue_leaves_out_tracks_without_a_file() {
        let entries = vec![entry("a", "One"), entry("b", "Two"), entry("c", "Three")];
        let cue = cue_from_entries(
            &entries,
            2,
            |id| (id != "b").then(|| PathBuf::from(format!("/media/usb/{id}.mp3"))),
            |id| (id == "c").then(|| PathBuf::from("/media/usb/c.jpg")),
        )
        .unwrap();
        assert_eq!(
            cue.paths,
            vec![
                PathBuf::from("/media/usb/a.mp3"),
                PathBuf::from("/media/usb/c.mp3")
            ]
        );
        assert_eq!(cue.titles, vec!["Artist – One", "Artist – Three"]);
        assert_eq!(cue.start, 1);
        assert_eq!(
            cue.artworks[1].as_deref(),
            Some(PathBuf::from("/media/usb/c.jpg").as_path())
        );
        assert_eq!(cue.bpms, vec![None, None]);
        assert_eq!(cue.keys, vec![String::new(), String::new()]);
    }

    #[test]
    fn a_row_with_no_file_does_not_load() {
        let entries = vec![entry("a", "One")];
        assert!(cue_from_entries(&entries, 0, |_| None, |_| None).is_none());
        assert!(
            cue_from_entries(&entries, 3, |_| Some(PathBuf::from("a.mp3")), |_| None).is_none()
        );
    }

    #[test]
    fn the_header_names_the_track_that_is_heard() {
        let playing = Playing {
            titles: vec!["Artist – Two".to_string()],
            artists: vec!["Artist".to_string()],
            track_titles: vec!["Two".to_string()],
            artworks: vec![Some(PathBuf::from("two.jpg"))],
            keys: vec!["F#m".to_string()],
            bpms: vec![Some(128.0)],
            paths: vec![PathBuf::from("two.mp3")],
        };
        let heard = Status {
            track: Some(0),
            ..status()
        };
        let header = now_header(&heard, &playing).unwrap();
        assert_eq!(header.artist, "Artist");
        assert_eq!(header.title, "Two");
        assert_eq!(header.clock, "1:23 / 5:00");
        assert_eq!(
            header.artwork.as_deref(),
            Some(PathBuf::from("two.jpg").as_path())
        );

        let ended = Status {
            track: Some(0),
            ended: true,
            ..status()
        };
        assert_eq!(now_header(&ended, &playing).unwrap().clock, "Finished");
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
