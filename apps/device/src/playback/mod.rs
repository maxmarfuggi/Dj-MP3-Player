//! The playback engine: a queue of files played to the default audio output.
//!
//! An engine thread decodes with symphonia, converts to the output's sample
//! rate, and writes stereo samples into a ring buffer. The output callback
//! plays them, applies the volume, and copies what it plays into a
//! [`Scope`] for the visualizers. Pausing and the volume act in the callback,
//! so they are immediate. Seeks and track changes drop the buffered audio,
//! so they are immediate too.
//!
//! On the Pi the default output is ALSA's, which PipeWire serves, so a wired
//! or Bluetooth speaker is whichever PipeWire has chosen. When the output is
//! lost the engine reopens it and carries on from the position last heard.
//!
//! "Previous" and a track that fails follow [`djusb_core::Playback`]: previous
//! restarts the track after three seconds, and a failed track is skipped.
//! [`djusb_core::Playback::reorder`] keeps the current entry when the queue
//! order changes.

mod convert;
pub(crate) mod decode;
mod engine;
mod output;
mod scope;

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::Duration;

pub use scope::{Scope, SCOPE_FRAMES};

use engine::Command;

/// What the player is doing, as heard.
#[derive(Debug, Clone, PartialEq)]
pub struct Status {
    /// The queue entry playing, or the last one played once the queue ends.
    pub track: Option<usize>,
    pub position: Duration,
    /// The track's length, when the file states it.
    pub length: Option<Duration>,
    pub paused: bool,
    /// The queue played to its end, or stopped on a track that failed.
    pub ended: bool,
    /// From 0 to 1.
    pub volume: f32,
    /// The last track that could not be played, when the queue then stopped.
    pub track_error: Option<TrackError>,
    /// `queue/<index>` for entries that could not be played. A skip keeps these.
    pub failed: BTreeSet<String>,
    /// Why no audio output is open, while none is.
    pub output_error: Option<String>,
}

impl Status {
    #[must_use]
    pub fn playing(&self) -> bool {
        self.track.is_some() && !self.paused && !self.ended
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackError {
    /// The queue entry.
    pub index: usize,
    pub message: String,
}

/// Starts the engine thread. Dropping the player stops it.
pub struct Player {
    commands: Sender<Command>,
    shared: Arc<Shared>,
    engine: Option<JoinHandle<()>>,
}

impl Player {
    #[must_use]
    pub fn start() -> Self {
        let shared = Arc::new(Shared::new());
        let (commands, receiver) = mpsc::channel();
        let engine_shared = Arc::clone(&shared);
        let engine = std::thread::Builder::new()
            .name("playback".to_string())
            .spawn(move || engine::run(&receiver, engine_shared))
            .map_err(|error| eprintln!("Playback could not start: {error}"))
            .ok();
        Self {
            commands,
            shared,
            engine,
        }
    }

    /// Replaces the queue and plays entry `start`.
    pub fn load(&self, queue: Vec<PathBuf>, start: usize) {
        self.send(Command::Load { queue, start });
    }

    pub fn play(&self) {
        self.shared.paused.store(false, Ordering::Release);
    }

    pub fn pause(&self) {
        self.shared.paused.store(true, Ordering::Release);
    }

    pub fn toggle(&self) {
        self.shared.paused.fetch_xor(true, Ordering::AcqRel);
    }

    /// Plays queue entry `index` from its start.
    pub fn jump(&self, index: usize) {
        self.send(Command::Jump(index));
    }

    /// Plays the entry after the one heard, or stops after the last.
    pub fn next(&self) {
        self.send(Command::Next);
    }

    /// Plays the entry before the one heard, or the first again.
    pub fn previous(&self) {
        self.send(Command::Previous);
    }

    /// Moves within the track heard. Pausing is kept.
    pub fn seek(&self, to: Duration) {
        self.send(Command::Seek(to));
    }

    /// Sets the level from 0 to 1. Changes ramp over one output buffer.
    pub fn set_volume(&self, level: f32) {
        self.shared.set_volume(level);
    }

    #[must_use]
    pub fn status(&self) -> Status {
        let shared = &self.shared;
        let rate = shared.output_rate.load(Ordering::Acquire);
        let track = shared.track.load(Ordering::Acquire);
        let length = shared.length.load(Ordering::Acquire);
        let problems = lock(&shared.problems);
        Status {
            track: (track != NO_TRACK).then_some(track),
            position: to_duration(shared.position.load(Ordering::Acquire), rate),
            length: (length > 0).then(|| to_duration(length, rate)),
            paused: shared.paused.load(Ordering::Acquire),
            ended: shared.ended.load(Ordering::Acquire),
            volume: shared.volume(),
            track_error: problems.track.clone(),
            failed: problems.failed.clone(),
            output_error: problems.output.clone(),
        }
    }

    /// The audio as it is played, for the visualizers.
    #[must_use]
    pub fn scope(&self) -> Arc<Scope> {
        Arc::clone(&self.shared.scope)
    }

    /// A playhead that reads the atomics directly, so a canvas can follow the
    /// music between status ticks.
    #[must_use]
    pub fn playhead(&self) -> Playhead {
        Playhead {
            shared: Arc::clone(&self.shared),
        }
    }

    fn send(&self, command: Command) {
        if self.commands.send(command).is_err() {
            eprintln!("Playback has stopped.");
        }
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Quit);
        if let Some(engine) = self.engine.take() {
            let _ = engine.join();
        }
    }
}

/// Where the track is, without the problem list the status locks.
#[derive(Clone)]
pub struct Playhead {
    shared: Arc<Shared>,
}

/// One reading of the playhead, in seconds.
#[derive(Debug, Clone, Copy)]
pub struct Moment {
    /// Seconds into the file.
    pub position: f64,
    /// Seconds in the file, when it is known.
    pub length: Option<f64>,
    pub paused: bool,
}

impl Playhead {
    #[must_use]
    pub fn moment(&self) -> Moment {
        let rate = self.shared.output_rate.load(Ordering::Acquire).max(1);
        let length = self.shared.length.load(Ordering::Acquire);
        let frames = |count: u64| count as f64 / f64::from(rate);
        Moment {
            position: frames(self.shared.position.load(Ordering::Acquire)),
            length: (length > 0).then(|| frames(length)),
            paused: self.shared.paused.load(Ordering::Acquire),
        }
    }
}

/// `track` when nothing has played.
const NO_TRACK: usize = usize::MAX;

/// Used for positions before any output has opened.
const DEFAULT_RATE: u32 = 48_000;

/// State shared by the player handle, the engine thread, and the output callback.
struct Shared {
    /// The level, as `f32` bits.
    volume: AtomicU32,
    paused: AtomicBool,
    /// The callback drops samples before this count. Set by seeks and track changes.
    discard_until: AtomicU64,
    /// Published by the callback, or by the engine while no output is open.
    track: AtomicUsize,
    /// Frames at the output's rate.
    position: AtomicU64,
    /// Frames at the output's rate, or 0 when unknown.
    length: AtomicU64,
    ended: AtomicBool,
    output_rate: AtomicU32,
    /// Set by the output's error callback when the device goes away.
    output_lost: AtomicBool,
    scope: Arc<Scope>,
    problems: Mutex<Problems>,
}

#[derive(Default)]
struct Problems {
    track: Option<TrackError>,
    failed: BTreeSet<String>,
    output: Option<String>,
}

impl Shared {
    fn new() -> Self {
        Self {
            volume: AtomicU32::new(1.0_f32.to_bits()),
            paused: AtomicBool::new(false),
            discard_until: AtomicU64::new(0),
            track: AtomicUsize::new(NO_TRACK),
            position: AtomicU64::new(0),
            length: AtomicU64::new(0),
            ended: AtomicBool::new(false),
            output_rate: AtomicU32::new(DEFAULT_RATE),
            output_lost: AtomicBool::new(false),
            scope: Arc::new(Scope::new(SCOPE_FRAMES)),
            problems: Mutex::new(Problems::default()),
        }
    }

    fn volume(&self) -> f32 {
        f32::from_bits(self.volume.load(Ordering::Relaxed))
    }

    fn set_volume(&self, level: f32) {
        let level = if level.is_nan() {
            0.0
        } else {
            level.clamp(0.0, 1.0)
        };
        self.volume.store(level.to_bits(), Ordering::Relaxed);
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

fn to_frames(time: Duration, rate: u32) -> u64 {
    (time.as_secs_f64() * f64::from(rate)).round() as u64
}

fn to_duration(frames: u64, rate: u32) -> Duration {
    Duration::from_secs_f64(frames as f64 / f64::from(rate.max(1)))
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::decode::counting_wav;
    use super::{to_duration, to_frames, Player, Shared, Status};

    fn wait_for(player: &Player, within: Duration, done: impl Fn(&Status) -> bool) -> Status {
        let deadline = Instant::now() + within;
        loop {
            let status = player.status();
            if done(&status) || Instant::now() > deadline {
                return status;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Plays quietly through this computer's default output:
    /// `cargo test -p djusb-device -- --ignored`
    #[test]
    #[ignore = "needs an audio output"]
    fn plays_seeks_pauses_and_moves_through_the_queue() {
        let dir = std::env::temp_dir().join(format!("djusb-player-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let first = dir.join("first.wav");
        let second = dir.join("second.wav");
        counting_wav(&first, 44_100, 2, 44_100 * 3);
        counting_wav(&second, 48_000, 2, 48_000);

        let player = Player::start();
        player.set_volume(0.02);
        player.load(vec![first, second], 0);

        let status = wait_for(&player, Duration::from_secs(3), |status| {
            status.position > Duration::from_millis(300)
        });
        assert_eq!(status.output_error, None);
        assert_eq!(status.track_error, None);
        assert_eq!(status.track, Some(0));
        assert_eq!(status.length, Some(Duration::from_secs(3)));
        assert!(status.playing());

        player.seek(Duration::from_secs(2));
        let status = wait_for(&player, Duration::from_secs(1), |status| {
            status.position >= Duration::from_secs(2)
        });
        assert!(status.position < Duration::from_millis(2300), "{status:?}");

        player.pause();
        std::thread::sleep(Duration::from_millis(100));
        let held = player.status().position;
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(player.status().position, held);
        player.play();

        player.next();
        let status = wait_for(&player, Duration::from_secs(1), |status| {
            status.track == Some(1)
        });
        assert_eq!(status.track, Some(1));
        assert!(status.position < Duration::from_millis(300), "{status:?}");

        let status = wait_for(&player, Duration::from_secs(3), |status| status.ended);
        assert!(status.ended, "{status:?}");
        assert!(player.scope().written() > 0);
    }

    #[test]
    fn positions_round_trip_through_frames() {
        let time = Duration::from_millis(83_250);
        assert_eq!(to_frames(time, 44_100), 3_671_325);
        assert_eq!(to_duration(3_671_325, 44_100), time);
    }

    #[test]
    fn the_volume_stays_between_silent_and_full() {
        let shared = Shared::new();
        shared.set_volume(1.5);
        assert_eq!(shared.volume(), 1.0);
        shared.set_volume(-0.2);
        assert_eq!(shared.volume(), 0.0);
        shared.set_volume(f32::NAN);
        assert_eq!(shared.volume(), 0.0);
    }
}
