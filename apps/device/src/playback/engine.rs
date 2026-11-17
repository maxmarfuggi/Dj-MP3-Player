//! The engine thread: owns the queue, the decoder, and the output.

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, RecvTimeoutError, TryRecvError};
use std::sync::Arc;
use std::time::{Duration, Instant};

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
                self.queue = queue;
                lock(&self.shared.problems).track = None;
                self.start(start, Duration::ZERO);
                self.unpause();
            }
            Command::Jump(index) => {
                self.start(index, Duration::ZERO);
                self.unpause();
            }
            Command::Next => match self.heard_track() {
                Some(index) if index + 1 < self.queue.len() => {
                    self.start(index + 1, Duration::ZERO);
                    self.unpause();
                }
                Some(_) => self.stop(),
                None => {}
            },
            Command::Previous => {
                if let Some(index) = self.heard_track() {
                    self.start(index.saturating_sub(1), Duration::ZERO);
                    self.unpause();
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

    /// Stops now, dropping whatever is buffered.
    fn stop(&mut self) {
        self.pending.clear();
        if let Some(output) = &self.output {
            self.shared
                .discard_until
                .store(output.written(), Ordering::Release);
        }
        self.end();
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

    fn fail(&mut self, index: usize, message: String) {
        eprintln!("Track {} could not play: {message}", index + 1);
        lock(&self.shared.problems).track = Some(TrackError { index, message });
        self.end();
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
