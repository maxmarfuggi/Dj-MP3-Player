//! The engine thread: owns the queue, the decoder, and the output.

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, RecvTimeoutError, TryRecvError};
use std::sync::Arc;
use std::time::{Duration, Instant};

use djusb_core::{Change, Playback, QueueItem};

use super::convert::Converter;
use super::decode::Source;
use super::output::{Marker, Output};
use super::{lock, to_duration, to_frames, Shared, TrackError, NO_TRACK};

/// How long to wait before trying the output again after it failed to open.
const OUTPUT_RETRY: Duration = Duration::from_secs(2);

/// How long to wait for a command when the buffer is full or there is nothing to play.
const IDLE_WAIT: Duration = Duration::from_millis(5);

pub(super) enum Command {
    Load { queue: Vec<PathBuf>, start: usize },
    Jump(usize),
    Next,
    Previous,
    Seek(Duration),
    Quit,
}

pub(super) fn run(commands: &Receiver<Command>, shared: Arc<Shared>) {
    let mut engine = Engine {
        shared,
        queue: Vec::new(),
        playback: Playback::new(),
        output: None,
        next_output_try: Instant::now(),
        resume: None,
        current: None,
        decoded: Vec::new(),
        pending: Vec::new(),
    };
    loop {
        loop {
            match commands.try_recv() {
                Ok(Command::Quit) | Err(TryRecvError::Disconnected) => return,
                Ok(command) => engine.handle(command),
                Err(TryRecvError::Empty) => break,
            }
        }
        engine.keep_output();
        if !engine.feed() {
            match commands.recv_timeout(IDLE_WAIT) {
                Ok(Command::Quit) | Err(RecvTimeoutError::Disconnected) => return,
                Ok(command) => engine.handle(command),
                Err(RecvTimeoutError::Timeout) => {}
            }
        }
    }
}

struct Engine {
    shared: Arc<Shared>,
    queue: Vec<PathBuf>,
    playback: Playback,
    output: Option<Output>,
    next_output_try: Instant,
    /// Where to start once an output opens. Only set while none is open.
    resume: Option<(usize, Duration)>,
    current: Option<Current>,
    /// One packet of stereo frames at the file's rate.
    decoded: Vec<f32>,
    /// Converted samples the ring buffer had no room for.
    pending: Vec<f32>,
}

/// The track being decoded, which can be ahead of the track heard.
struct Current {
    index: usize,
    source: Source,
    converter: Converter,
}

/// What reading one packet led to, decided before the engine changes track.
enum Read {
    More,
    Finished(usize),
    Failed(usize, String),
}

impl Engine {
    fn handle(&mut self, command: Command) {
        match command {
            Command::Load { queue, start } => {
                let items = (0..queue.len())
                    .map(|index| QueueItem {
                        library_id: "queue".to_string(),
                        track_id: index.to_string(),
                        entry_key: format!("entry-{index}"),
                    })
                    .collect();
                if let Change::Start(index) = self.playback.play(items, start, "queue") {
                    self.queue = queue;
                    lock(&self.shared.problems).track = None;
                    self.start(index, Duration::ZERO);
                    self.unpause();
                }
            }
            Command::Jump(index) => {
                self.start(index, Duration::ZERO);
                self.unpause();
            }
            Command::Next => {
                if let Some(index) = self.heard_track() {
                    if let Change::Start(next) = self.playback.next(index) {
                        self.start(next, Duration::ZERO);
                        self.unpause();
                    }
                }
            }
            Command::Previous => {
                if let Some(index) = self.heard_track() {
                    match self.playback.previous(index, self.heard_position()) {
                        Change::Start(previous) => {
                            self.start(previous, Duration::ZERO);
                            self.unpause();
                        }
                        // A restart keeps the pause the listener already chose.
                        Change::Restart => self.start(index, Duration::ZERO),
                        Change::Reordered | Change::Pause | Change::Unchanged => {}
                    }
                }
            }
            Command::Seek(to) => {
                if let Some(index) = self.heard_track() {
                    self.start(index, to);
                }
            }
            Command::Quit => {}
        }
    }

    fn unpause(&self) {
        self.shared.paused.store(false, Ordering::Release);
    }

    /// Where the listener is in the current entry.
    fn heard_position(&self) -> Duration {
        if let Some((_, at)) = self.resume {
            return at;
        }
        let rate = self.shared.output_rate.load(Ordering::Acquire);
        to_duration(self.shared.position.load(Ordering::Acquire), rate)
    }

    /// The queue entry heard, ended or not.
    fn heard_track(&self) -> Option<usize> {
        if let Some((index, _)) = self.resume {
            return Some(index);
        }
        let track = self.shared.track.load(Ordering::Acquire);
        if track != NO_TRACK {
            return Some(track);
        }
        self.current.as_ref().map(|current| current.index)
    }

    /// Where to carry on after the output is lost.
    fn resume_point(&self) -> Option<(usize, Duration)> {
        if self.shared.ended.load(Ordering::Acquire) {
            return None;
        }
        let track = self.shared.track.load(Ordering::Acquire);
        if track == NO_TRACK {
            return self
                .current
                .as_ref()
                .map(|current| (current.index, Duration::ZERO));
        }
        let rate = self.shared.output_rate.load(Ordering::Acquire);
        let position = self.shared.position.load(Ordering::Acquire);
        Some((track, to_duration(position, rate)))
    }

    /// Opens the output when there is none, and reopens it when it is lost.
    fn keep_output(&mut self) {
        if self.output.is_some() {
            if !self.shared.output_lost.load(Ordering::Acquire) {
                return;
            }
            self.resume = self.resume_point();
            self.output = None;
            self.current = None;
            self.pending.clear();
            self.next_output_try = Instant::now();
        }
        if Instant::now() < self.next_output_try {
            return;
        }
        match Output::open(&self.shared) {
            Ok(output) => {
                self.shared
                    .output_rate
                    .store(output.rate(), Ordering::Release);
                lock(&self.shared.problems).output = None;
                self.output = Some(output);
                if let Some((index, at)) = self.resume.take() {
                    self.start(index, at);
                }
            }
            Err(message) => {
                let mut problems = lock(&self.shared.problems);
                if problems.output.as_deref() != Some(message.as_str()) {
                    eprintln!("No audio output: {message}");
                    problems.output = Some(message);
                }
                self.next_output_try = Instant::now() + OUTPUT_RETRY;
            }
        }
    }

    /// Plays entry `index` from `at`, dropping whatever is buffered.
    fn start(&mut self, index: usize, at: Duration) {
        if index >= self.queue.len() {
            return;
        }
        let Some(output) = &mut self.output else {
            self.resume = Some((index, at));
            self.current = None;
            let rate = self.shared.output_rate.load(Ordering::Acquire);
            self.shared.track.store(index, Ordering::Release);
            self.shared
                .position
                .store(to_frames(at, rate), Ordering::Release);
            self.shared.ended.store(false, Ordering::Release);
            return;
        };
        self.pending.clear();
        self.shared
            .discard_until
            .store(output.written(), Ordering::Release);

        // A seek within the track being decoded reuses it; anything else opens the file.
        let source = match self.current.take() {
            Some(current) if current.index == index && !at.is_zero() => Ok(current.source),
            _ => Source::open(&self.queue[index]),
        };
        let mut source = match source {
            Ok(source) => source,
            Err(message) => return self.fail(index, message),
        };
        let mut start = Duration::ZERO;
        if !at.is_zero() {
            match source.seek(at) {
                Ok(position) => start = position,
                // Usually a seek past the end, which is where the track finishes.
                Err(_) => return self.after(index),
            }
        }
        self.begin(index, source, start);
    }

    /// Follows entry `index` with the next one, after what is already buffered.
    fn after(&mut self, index: usize) {
        let next = index + 1;
        if next >= self.queue.len() {
            self.end();
            return;
        }
        match Source::open(&self.queue[next]) {
            Ok(source) => self.begin(next, source, Duration::ZERO),
            Err(message) => self.fail(next, message),
        }
    }

    /// Marks where `source` starts in the sample stream and decodes it next.
    fn begin(&mut self, index: usize, source: Source, start: Duration) {
        let Some(output) = &mut self.output else {
            return;
        };
        let rate = output.rate();
        output.mark(Marker::Track {
            at: output.written() + self.pending.len() as u64,
            index,
            start: to_frames(start, rate),
            length: source.length().map(|length| to_frames(length, rate)),
        });
        let converter = Converter::new(source.sample_rate(), rate);
        self.current = Some(Current {
            index,
            source,
            converter,
        });
        lock(&self.shared.problems).track = None;
    }

    /// Stops after what is already buffered.
    fn end(&mut self) {
        self.current = None;
        match &mut self.output {
            Some(output) => output.mark(Marker::End {
                at: output.written() + self.pending.len() as u64,
            }),
            None => {
                self.resume = None;
                self.shared.ended.store(true, Ordering::Release);
            }
        }
    }

    fn fail(&mut self, mut index: usize, mut message: String) {
        loop {
            eprintln!("Track {} could not play: {message}", index + 1);
            let change = self.playback.track_failed(index);
            let mut problems = lock(&self.shared.problems);
            problems.failed.clone_from(self.playback.failed());
            match change {
                Change::Start(next) => {
                    problems.track = None;
                    drop(problems);
                    if self.output.is_none() {
                        self.start(next, Duration::ZERO);
                        return;
                    }
                    match Source::open(&self.queue[next]) {
                        Ok(source) => {
                            self.begin(next, source, Duration::ZERO);
                            return;
                        }
                        Err(error) => {
                            index = next;
                            message = error;
                        }
                    }
                }
                Change::Restart | Change::Reordered | Change::Pause | Change::Unchanged => {
                    problems.track = Some(TrackError { index, message });
                    drop(problems);
                    self.end();
                    return;
                }
            }
        }
    }

    /// Moves audio towards the output. Returns false when there was nothing to do.
    fn feed(&mut self) -> bool {
        let Some(output) = &mut self.output else {
            return false;
        };
        if !self.pending.is_empty() {
            let written = output.write(&self.pending);
            self.pending.drain(..written);
            if !self.pending.is_empty() {
                return false;
            }
        }
        let Some(current) = &mut self.current else {
            return false;
        };

        self.decoded.clear();
        let read = match current.source.read(&mut self.decoded) {
            Ok(Some(rate)) => {
                if rate != current.converter.input_rate() {
                    current.converter.finish(&mut self.pending);
                    current.converter = Converter::new(rate, output.rate());
                }
                current.converter.push(&self.decoded, &mut self.pending);
                Read::More
            }
            Ok(None) => {
                current.converter.finish(&mut self.pending);
                Read::Finished(current.index)
            }
            Err(message) => Read::Failed(current.index, message),
        };
        match read {
            Read::More => {}
            Read::Finished(index) => {
                self.current = None;
                self.after(index);
            }
            Read::Failed(index, message) => self.fail(index, message),
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::atomic::Ordering;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use djusb_core::Playback;

    use super::super::{lock, Shared};
    use super::{Command, Engine};

    fn engine(shared: Arc<Shared>) -> Engine {
        Engine {
            shared,
            queue: Vec::new(),
            playback: Playback::new(),
            output: None,
            next_output_try: Instant::now(),
            resume: None,
            current: None,
            decoded: Vec::new(),
            pending: Vec::new(),
        }
    }

    fn paths(count: usize) -> Vec<PathBuf> {
        (0..count)
            .map(|index| PathBuf::from(format!("track-{index}.mp3")))
            .collect()
    }

    #[test]
    fn previous_restarts_after_three_seconds_and_steps_back_before_that() {
        let shared = Arc::new(Shared::new());
        let mut engine = engine(Arc::clone(&shared));
        engine.handle(Command::Load {
            queue: paths(3),
            start: 1,
        });
        assert_eq!(shared.track.load(Ordering::Acquire), 1);

        shared.paused.store(true, Ordering::Release);
        engine.handle(Command::Seek(Duration::from_secs(3)));
        engine.handle(Command::Previous);
        assert_eq!(shared.track.load(Ordering::Acquire), 1);
        assert_eq!(shared.position.load(Ordering::Acquire), 0);
        assert!(shared.paused.load(Ordering::Acquire));

        engine.handle(Command::Previous);
        assert_eq!(shared.track.load(Ordering::Acquire), 0);
        assert!(!shared.paused.load(Ordering::Acquire));
    }

    #[test]
    fn next_on_the_last_track_leaves_it_playing() {
        let shared = Arc::new(Shared::new());
        let mut engine = engine(Arc::clone(&shared));
        engine.handle(Command::Load {
            queue: paths(2),
            start: 1,
        });
        engine.handle(Command::Next);
        assert_eq!(shared.track.load(Ordering::Acquire), 1);
        assert!(!shared.ended.load(Ordering::Acquire));
    }

    #[test]
    fn a_failed_track_is_skipped_and_the_last_one_stops() {
        let shared = Arc::new(Shared::new());
        let mut engine = engine(Arc::clone(&shared));
        engine.handle(Command::Load {
            queue: paths(3),
            start: 0,
        });
        engine.fail(0, "missing".to_string());
        assert_eq!(shared.track.load(Ordering::Acquire), 1);
        assert!(!shared.ended.load(Ordering::Acquire));
        assert!(lock(&shared.problems).failed.contains("queue/0"));
        assert!(lock(&shared.problems).track.is_none());

        engine.fail(2, "missing".to_string());
        assert!(shared.ended.load(Ordering::Acquire));
        assert_eq!(
            lock(&shared.problems)
                .track
                .as_ref()
                .map(|error| error.index),
            Some(2)
        );
        assert!(lock(&shared.problems).failed.contains("queue/2"));
    }
}
