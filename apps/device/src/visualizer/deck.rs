//! A CDJ performance screen, laid out like a Pioneer CDJ-2000NXS.
//!
//! The title, key, track number, remaining time, and tempo come from the
//! library and the playhead. The waveform is scanned from the file. Beat
//! marks, the beat and bar counter, and the bar count are counted from the
//! start of the file at the file's BPM. The MEMORY and CUE tags and the zoom
//! panel are only the CDJ's furniture: the library has no cue points and the
//! zoom is fixed. Tempo always reads 0.00% because this player does not
//! pitch the track.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;

use iced::alignment;
use iced::widget::canvas::{self, Fill, LineCap, LineJoin, Path, Stroke, Style};
use iced::widget::text::{self, LineHeight, Shaping};
use iced::{window, Color, Element, Fill as FillLength, Pixels, Point, Size};

use super::deck_math::{
    bars_text, beats, bpm_digits, counter, detail_span, remain_digits, window_start,
};
use super::peaks::{columns, scan_file, Peaks};
use crate::chrome::FACE;
use crate::playback::{Moment, Playhead};

const SCREEN: Color = Color::from_rgb8(0x00, 0x04, 0x0c);
const TEXT: Color = Color::from_rgb8(0xf4, 0xf8, 0xff);
const LABEL: Color = Color::from_rgb8(0xc4, 0xcf, 0xe0);
const MUTED: Color = Color::from_rgb8(0x8a, 0x95, 0xa6);

const TITLE_LEFT: Color = Color::from_rgb8(0x1c, 0x5c, 0xe6);
const TITLE_RIGHT: Color = Color::from_rgb8(0x06, 0x16, 0x46);
const TITLE_EDGE: Color = Color::from_rgb8(0x6c, 0xa4, 0xff);

const WAVE_EDGE: Color = Color::from_rgb8(0x2e, 0x7c, 0xff);
const WAVE_CORE: Color = Color::from_rgb8(0xe6, 0xf2, 0xff);
const MIDLINE: Color = Color::from_rgb8(0x10, 0x2a, 0x52);
const OVER_EDGE: Color = Color::from_rgb8(0x56, 0x80, 0xc4);
const OVER_CORE: Color = Color::from_rgb8(0xc6, 0xd9, 0xf4);
const PLAYED_EDGE: Color = Color::from_rgb8(0x3c, 0x46, 0x56);
const PLAYED_CORE: Color = Color::from_rgb8(0x84, 0x8e, 0x9e);

const BEAT_TICK: Color = Color::from_rgba8(255, 255, 255, 0.85);
const BAR_TICK: Color = Color::from_rgb8(0xff, 0x3b, 0x4e);
const BAR_LINE: Color = Color::from_rgba8(255, 59, 78, 0.35);
const NEEDLE: Color = Color::from_rgb8(0xff, 0x3b, 0x2f);
const POINTER: Color = Color::from_rgb8(0xff, 0x8a, 0x1e);
const OVER_NEEDLE: Color = Color::WHITE;
const RULER: Color = Color::from_rgb8(0xc0, 0xca, 0xd8);

const RED: Color = Color::from_rgb8(0xff, 0x2b, 0x3a);
const ORANGE: Color = Color::from_rgb8(0xff, 0x7a, 0x1a);
const ORANGE_DIM: Color = Color::from_rgb8(0x9a, 0x4a, 0x14);
const BLUE: Color = Color::from_rgb8(0x3a, 0x8c, 0xff);
const BLUE_DIM: Color = Color::from_rgb8(0x24, 0x52, 0x9a);
const TAG_FILL: Color = Color::from_rgb8(0x04, 0x10, 0x26);
const BADGE_FILL: Color = Color::from_rgb8(0x0b, 0x0f, 0x16);

/// VT323 is 0.4 em wide per character, with the baseline at 0.8 em and
/// capitals 0.56 em tall.
const ADVANCE: f32 = 0.4;
const ASCENT: f32 = 0.8;
const CAP: f32 = 0.56;

/// The track the deck is showing.
#[derive(Debug, Clone)]
pub struct DeckTrack {
    pub artist: String,
    pub title: String,
    pub key: String,
    pub bpm: Option<f64>,
    pub path: Option<PathBuf>,
    /// Position in the queue, counting from 1.
    pub number: Option<usize>,
}

impl DeckTrack {
    #[must_use]
    pub fn empty() -> Self {
        Self {
            artist: String::new(),
            title: String::new(),
            key: String::new(),
            bpm: None,
            path: None,
            number: None,
        }
    }
}

/// The deck canvas. It redraws every frame so the playhead stays smooth.
pub fn deck<Message: 'static>(playhead: Playhead, track: DeckTrack) -> Element<'static, Message> {
    iced::widget::canvas(Deck { playhead, track })
        .width(FillLength)
        .height(FillLength)
        .into()
}

struct Deck {
    playhead: Playhead,
    track: DeckTrack,
}

#[derive(Default)]
struct DeckState {
    store: PeakStore,
    shown: Option<Arc<Peaks>>,
    failed: bool,
}

impl<Message> canvas::Program<Message> for Deck {
    type State = DeckState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &canvas::Event,
        _bounds: iced::Rectangle,
        _cursor: iced::mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        let canvas::Event::Window(window::Event::RedrawRequested(_)) = event else {
            return None;
        };
        if let Some(path) = &self.track.path {
            state.store.request(path);
            state.shown = state.store.get(path);
            state.failed = state.store.failed(path);
        } else {
            state.shown = None;
            state.failed = false;
        }
        Some(canvas::Action::request_redraw())
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: iced::Rectangle,
        _cursor: iced::mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        paint(
            &mut frame,
            &self.track,
            self.playhead.moment(),
            state.shown.as_deref(),
            state.failed,
        );
        vec![frame.into_geometry()]
    }
}

/// Peaks for the current file. A second request for the same path does not
/// start another scan.
#[derive(Clone)]
struct PeakStore {
    slot: Arc<Mutex<Slot>>,
}

struct Slot {
    path: Option<PathBuf>,
    peaks: Option<Arc<Peaks>>,
    failed: bool,
}

impl Default for PeakStore {
    fn default() -> Self {
        Self {
            slot: Arc::new(Mutex::new(Slot {
                path: None,
                peaks: None,
                failed: false,
            })),
        }
    }
}

impl PeakStore {
    fn request(&self, path: &std::path::Path) {
        {
            let mut slot = lock(&self.slot);
            if slot.path.as_deref() == Some(path) {
                return;
            }
            slot.path = Some(path.to_path_buf());
            slot.peaks = None;
            slot.failed = false;
        }
        let slot = Arc::clone(&self.slot);
        let path = path.to_path_buf();
        let _ = thread::Builder::new()
            .name("deck-waveform".to_string())
            .spawn(move || {
                let scanned = scan_file(&path);
                let mut slot = lock(&slot);
                if slot.path.as_deref() != Some(path.as_path()) {
                    return;
                }
                match scanned {
                    Ok(peaks) => slot.peaks = Some(Arc::new(peaks)),
                    Err(error) => {
                        eprintln!("The deck waveform could not be read: {error}");
                        slot.failed = true;
                    }
                }
            });
    }

    fn get(&self, path: &std::path::Path) -> Option<Arc<Peaks>> {
        let slot = lock(&self.slot);
        if slot.path.as_deref() == Some(path) {
            slot.peaks.clone()
        } else {
            None
        }
    }

    fn failed(&self, path: &std::path::Path) -> bool {
        let slot = lock(&self.slot);
        slot.path.as_deref() == Some(path) && slot.failed
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

/// The screen's rows, top to bottom, drawn for a 1280×720 panel and scaled
/// down by `s` for anything smaller.
struct Layout {
    s: f32,
    width: f32,
    title: f32,
    counter_top: f32,
    counter: f32,
    detail_top: f32,
    detail: f32,
    readout_top: f32,
    readout: f32,
    overview_top: f32,
    overview: f32,
    ruler: f32,
    /// The tag column on the left of both waveforms.
    left: f32,
    /// The zoom panel on the right of the scrolling waveform.
    right: f32,
}

fn layout(size: Size) -> Layout {
    let s = (size.height / 720.0)
        .min(size.width / 1280.0)
        .clamp(0.4, 1.0);
    let gap = 6.0 * s;
    let title = 56.0 * s;
    let counter = 46.0 * s;
    let readout = 136.0 * s;
    let overview = 84.0 * s;
    let ruler = 16.0 * s;
    let detail = (size.height - title - counter - readout - overview - ruler - gap * 4.0).max(36.0);
    let counter_top = title + gap;
    let detail_top = counter_top + counter;
    let readout_top = detail_top + detail + gap;
    let overview_top = readout_top + readout + gap;
    Layout {
        s,
        width: size.width,
        title,
        counter_top,
        counter,
        detail_top,
        detail,
        readout_top,
        readout,
        overview_top,
        overview,
        ruler,
        left: 92.0 * s,
        right: 72.0 * s,
    }
}

fn paint(
    frame: &mut canvas::Frame,
    track: &DeckTrack,
    moment: Moment,
    peaks: Option<&Peaks>,
    failed: bool,
) {
    frame.fill_rectangle(Point::ORIGIN, frame.size(), SCREEN);
    if track.path.is_none() && track.artist.is_empty() && track.title.is_empty() {
        write(
            frame,
            "Nothing playing",
            frame.center(),
            Glyph {
                size: 36.0,
                color: TEXT,
                align_x: text::Alignment::Center,
                align_y: alignment::Vertical::Center,
            },
            frame.width(),
        );
        return;
    }

    let l = layout(frame.size());
    let length = moment.length.filter(|length| *length > 0.0);
    let position = moment.position.max(0.0);
    paint_title(frame, &l, track);
    paint_counter(frame, &l, position, track.bpm);
    paint_detail(frame, &l, track, peaks, failed, position, length);
    paint_zoom(frame, &l);
    paint_readout(frame, &l, track, position, length, moment.paused);
    paint_overview(frame, &l, peaks, position, length);
}

/// White on a blue bar, with a note at the left and the key at the right.
fn paint_title(frame: &mut canvas::Frame, l: &Layout, track: &DeckTrack) {
    let s = l.s;
    frame.fill_rectangle(
        Point::ORIGIN,
        Size::new(l.width, l.title),
        gradient(
            Point::ORIGIN,
            Point::new(l.width, 0.0),
            &[(0.0, TITLE_LEFT), (0.55, TITLE_RIGHT), (1.0, TITLE_RIGHT)],
        ),
    );
    frame.fill_rectangle(
        Point::new(0.0, l.title - 2.0 * s),
        Size::new(l.width, 2.0 * s),
        TITLE_EDGE,
    );
    let middle = l.title * 0.5;
    note(frame, Point::new(14.0 * s, middle), s);

    let size = 44.0 * s;
    let baseline = middle + size * CAP * 0.5;
    let key_width = if track.key.is_empty() {
        0.0
    } else {
        let width = measure(&track.key, size);
        let x = l.width - 16.0 * s - width;
        key_icon(frame, Point::new(x - 30.0 * s, middle), s);
        run(frame, &track.key, x, baseline, size, TEXT);
        width + 50.0 * s
    };
    let start = 42.0 * s;
    let room = (l.width - start - key_width - 24.0 * s).max(0.0);
    let heading = fit(&title_line(&track.artist, &track.title), size, room);
    run(frame, &heading, start, baseline, size, TEXT);
}

/// Beats of the bar on top in orange, bars of the phrase underneath in blue,
/// and the bar count beside them.
fn paint_counter(frame: &mut canvas::Frame, l: &Layout, position: f64, bpm: Option<f64>) {
    let s = l.s;
    let width = 50.0 * s;
    let height = 14.0 * s;
    let gap = 6.0 * s;
    let between = 8.0 * s;
    let group = width * 4.0 + gap * 3.0;
    let x0 = l.width * 0.5 - group * 0.5 - 70.0 * s;
    let top = l.counter_top + (l.counter - height * 2.0 - between) * 0.5;
    let lower = top + height + between;
    let count = counter(position, bpm);
    for index in 0..4 {
        let x = x0 + index as f32 * (width + gap);
        let beat = count.is_some_and(|count| count.beat == index);
        let bar = count.is_some_and(|count| count.bar == index);
        lamp(
            frame,
            Point::new(x, top),
            Size::new(width, height),
            beat,
            RED,
            ORANGE_DIM,
            s,
        );
        lamp(
            frame,
            Point::new(x, lower),
            Size::new(width, height),
            bar,
            BLUE,
            BLUE_DIM,
            s,
        );
    }

    let x = x0 + group + 20.0 * s;
    let small = 18.0 * s;
    let beat = count.map_or_else(|| "-".to_string(), |count| (count.beat + 1).to_string());
    let after = run(frame, &beat, x, top + height, 24.0 * s, ORANGE);
    run(frame, "Beat", after + 6.0 * s, top + height, small, ORANGE);
    let bars = bars_text(position, bpm).unwrap_or_else(|| "--.-".to_string());
    let after = run(frame, &bars, x, lower + height + 2.0 * s, 30.0 * s, RED);
    run(
        frame,
        "Bars",
        after + 6.0 * s,
        lower + height + 2.0 * s,
        small,
        RED,
    );
}

fn lamp(
    frame: &mut canvas::Frame,
    at: Point,
    size: Size,
    lit: bool,
    on: Color,
    off: Color,
    s: f32,
) {
    if lit {
        let glow = 4.0 * s;
        frame.fill_rectangle(
            Point::new(at.x - glow, at.y - glow),
            Size::new(size.width + glow * 2.0, size.height + glow * 2.0),
            with_alpha(on, 0.3),
        );
        frame.fill_rectangle(at, size, on);
    } else {
        outline(frame, at, size, off, 1.5 * s);
    }
}

/// The scrolling waveform, eight beats wide, with the playhead fixed in the
/// middle.
fn paint_detail(
    frame: &mut canvas::Frame,
    l: &Layout,
    track: &DeckTrack,
    peaks: Option<&Peaks>,
    failed: bool,
    position: f64,
    length: Option<f64>,
) {
    let s = l.s;
    let left = l.left;
    let width = (l.width - l.left - l.right).max(1.0);
    let top = l.detail_top;
    let height = l.detail;
    let mid = top + height * 0.5;
    let span = detail_span(track.bpm);
    let start = window_start(position, span);

    tag(frame, "MEMORY", Point::new(6.0 * s, top + 6.0 * s), BLUE, s);
    tag(
        frame,
        "CUE",
        Point::new(6.0 * s, top + height - 30.0 * s),
        BLUE,
        s,
    );
    frame.fill_rectangle(Point::new(left, mid - 0.5), Size::new(width, 1.0), MIDLINE);

    match peaks {
        Some(peaks) => {
            let end = length.unwrap_or(peaks.seconds).max(peaks.seconds);
            let count = ((width / (3.0 * s)).floor() as usize).clamp(1, 1600);
            let pitch = width / count as f32;
            let bar = (pitch * 0.72).max(1.0);
            let half = height * 0.47;
            for (index, (min, max)) in columns(peaks, start, start + span, count)
                .into_iter()
                .enumerate()
            {
                let time = start + (index as f64 + 0.5) / count as f64 * span;
                if time < 0.0 || time > end {
                    continue;
                }
                let x = left + index as f32 * pitch;
                span_column(frame, x, bar, mid - max * half, mid - min * half, WAVE_EDGE);
                span_column(
                    frame,
                    x,
                    bar,
                    mid - max * half * 0.55,
                    mid - min * half * 0.55,
                    WAVE_CORE,
                );
            }
        }
        None => {
            let message = if failed {
                "The waveform could not be read."
            } else if track.path.is_some() {
                "Reading the waveform…"
            } else {
                ""
            };
            if !message.is_empty() {
                write(
                    frame,
                    message,
                    Point::new(left + width * 0.5, mid),
                    Glyph {
                        size: 24.0 * s,
                        color: LABEL,
                        align_x: text::Alignment::Center,
                        align_y: alignment::Vertical::Center,
                    },
                    width,
                );
            }
        }
    }

    if let Some(bpm) = track.bpm {
        for mark in beats(start, span, bpm) {
            let x = left + ((mark.at - start) / span) as f32 * width;
            let (color, thick, reach) = if mark.bar {
                vertical(frame, x, top, top + height, BAR_LINE, 1.0 * s);
                (BAR_TICK, 3.0 * s, 12.0 * s)
            } else {
                (BEAT_TICK, 2.0 * s, 8.0 * s)
            };
            vertical(frame, x, top, top + reach, color, thick);
            vertical(frame, x, top + height - reach, top + height, color, thick);
        }
    }

    let x = left + width * 0.5;
    vertical(frame, x, top, top + height, NEEDLE, 2.5 * s);
    pointer(frame, Point::new(x, top), 9.0 * s, true);
    pointer(frame, Point::new(x, top + height), 9.0 * s, false);
}

/// The CDJ's zoom and grid panel, to the right of the scrolling waveform.
fn paint_zoom(frame: &mut canvas::Frame, l: &Layout) {
    let s = l.s;
    let x = l.width - l.right * 0.5;
    let top = l.detail_top;
    let bottom = l.detail_top + l.detail;
    let mid = (top + bottom) * 0.5;
    let centered = |size: f32, color: Color| Glyph {
        size,
        color,
        align_x: text::Alignment::Center,
        align_y: alignment::Vertical::Center,
    };
    write(
        frame,
        "+",
        Point::new(x, top + 30.0 * s),
        centered(64.0 * s, TEXT),
        l.right,
    );
    write(
        frame,
        "−",
        Point::new(x, bottom - 30.0 * s),
        centered(64.0 * s, TEXT),
        l.right,
    );
    write(
        frame,
        "ZOOM",
        Point::new(x, mid - 12.0 * s),
        centered(18.0 * s, MUTED),
        l.right,
    );
    frame.fill_rectangle(
        Point::new(x - 18.0 * s, mid - 0.5),
        Size::new(36.0 * s, 1.0),
        MUTED,
    );
    write(
        frame,
        "GRID",
        Point::new(x, mid + 12.0 * s),
        centered(18.0 * s, MUTED),
        l.right,
    );
}

/// Player badge, track number, remaining time, tempo, and the BPM box.
fn paint_readout(
    frame: &mut canvas::Frame,
    l: &Layout,
    track: &DeckTrack,
    position: f64,
    length: Option<f64>,
    paused: bool,
) {
    let s = l.s;
    let top = l.readout_top;
    let height = l.readout;
    let label = 20.0 * s;
    let big = 88.0 * s;
    let unit = 30.0 * s;
    let label_line = top + 28.0 * s;
    let digit_line = top + height - 22.0 * s;

    let mut x = 8.0 * s;
    let badge = Size::new(80.0 * s, height - 24.0 * s);
    let badge_top = top + 10.0 * s;
    frame.fill_rectangle(Point::new(x, badge_top), badge, BADGE_FILL);
    outline(frame, Point::new(x, badge_top), badge, MUTED, 2.0 * s);
    let center = x + badge.width * 0.5;
    centered_run(
        frame,
        "PLAYER",
        center,
        badge_top + 24.0 * s,
        18.0 * s,
        MUTED,
    );
    centered_run(
        frame,
        "1",
        center,
        badge_top + badge.height - 10.0 * s,
        72.0 * s,
        MUTED,
    );
    x += badge.width + 24.0 * s;

    let number = track
        .number
        .map_or_else(|| "--".to_string(), |number| format!("{:02}", number % 100));
    run(frame, "TRACK", x, label_line, label, LABEL);
    run(frame, &number, x, digit_line, big, TEXT);
    x += measure("TRACK", label).max(measure(&number, big)) + 24.0 * s;

    run(frame, "REMAIN", x, top + height * 0.5, label, LABEL);
    if paused {
        let word = "PAUSE";
        let size = 18.0 * s;
        let box_top = top + height * 0.5 + 8.0 * s;
        let pad = 5.0 * s;
        outline(
            frame,
            Point::new(x, box_top),
            Size::new(measure(word, size) + pad * 2.0, size * CAP + pad * 2.0),
            TEXT,
            1.5 * s,
        );
        run(frame, word, x + pad, box_top + pad + size * CAP, size, TEXT);
    }
    x += measure("REMAIN", label) + 18.0 * s;

    let [minutes, seconds, hundredths] = remain_digits(position, length);
    x = run(frame, &minutes, x, digit_line, big, TEXT);
    x = run(frame, "M", x + 2.0 * s, digit_line, unit, TEXT);
    x = run(frame, ":", x, digit_line, big, TEXT);
    x = run(frame, &seconds, x, digit_line, big, TEXT);
    x = run(frame, "S", x + 2.0 * s, digit_line, unit, TEXT);
    x = run(frame, &hundredths, x + 20.0 * s, digit_line, big, TEXT);
    run(frame, "F", x + 2.0 * s, digit_line, unit, TEXT);

    let box_size = Size::new(184.0 * s, height - 24.0 * s);
    let box_at = Point::new(l.width - 10.0 * s - box_size.width, top + 10.0 * s);
    outline(frame, box_at, box_size, TEXT, 2.5 * s);
    let (whole, tenth) = bpm_digits(track.bpm);
    let bpm_size = 80.0 * s;
    let tenth_size = 40.0 * s;
    let inner_right = box_at.x + box_size.width - 12.0 * s;
    let bpm_line = box_at.y + box_size.height - 30.0 * s;
    let tenth_x = inner_right - measure(&tenth, tenth_size);
    run(frame, &tenth, tenth_x, bpm_line, tenth_size, TEXT);
    run(
        frame,
        &whole,
        tenth_x - measure(&whole, bpm_size),
        bpm_line,
        bpm_size,
        TEXT,
    );
    let bpm_label = 18.0 * s;
    run(
        frame,
        "BPM",
        inner_right - measure("BPM", bpm_label),
        box_at.y + box_size.height - 8.0 * s,
        bpm_label,
        TEXT,
    );

    let tempo = 76.0 * s;
    let total = measure("+", 40.0 * s) + 6.0 * s + measure("0.00", tempo) + measure("%", unit);
    let tempo_x = box_at.x - 28.0 * s - total;
    run(frame, "TEMPO", tempo_x, label_line, label, LABEL);
    let after = run(frame, "+", tempo_x, digit_line, 40.0 * s, TEXT);
    let after = run(frame, "0.00", after + 6.0 * s, digit_line, tempo, TEXT);
    run(frame, "%", after, digit_line, unit, TEXT);
}

/// The whole track, standing on a ruler with a tick each minute. The part
/// already played is grey.
fn paint_overview(
    frame: &mut canvas::Frame,
    l: &Layout,
    peaks: Option<&Peaks>,
    position: f64,
    length: Option<f64>,
) {
    let s = l.s;
    let left = l.left;
    let width = (l.width - l.left - 10.0 * s).max(1.0);
    let top = l.overview_top;
    let height = l.overview;
    let floor = top + height;

    tag(frame, "MEMORY", Point::new(6.0 * s, top + 4.0 * s), RED, s);
    tag(frame, "CUE", Point::new(6.0 * s, floor - 26.0 * s), RED, s);

    let end = match (peaks, length) {
        (Some(peaks), length) => length.unwrap_or(peaks.seconds).max(peaks.seconds),
        (None, Some(length)) => length,
        (None, None) => 0.0,
    };
    if let Some(peaks) = peaks.filter(|_| end > 0.0) {
        let count = (width.round() as usize).clamp(1, 1600);
        let slot = width / count as f32;
        for (index, (min, max)) in columns(peaks, 0.0, end, count).into_iter().enumerate() {
            let time = (index as f64 + 0.5) / count as f64 * end;
            let (edge, core) = if time < position {
                (PLAYED_EDGE, PLAYED_CORE)
            } else {
                (OVER_EDGE, OVER_CORE)
            };
            let level = max.max(-min).clamp(0.0, 1.0) * height * 0.95;
            let x = left + index as f32 * slot;
            span_column(frame, x, slot.max(1.0), floor - level, floor, edge);
            span_column(frame, x, slot.max(1.0), floor - level * 0.55, floor, core);
        }
    }

    frame.fill_rectangle(Point::new(left, floor), Size::new(width, 1.5 * s), RULER);
    if end > 0.0 {
        let minutes = (end / 60.0).floor() as usize;
        for minute in 1..=minutes.min(600) {
            let x = left + (minute as f64 * 60.0 / end) as f32 * width;
            vertical(frame, x, floor, floor + 6.0 * s, RULER, 1.5 * s);
        }
        let x = left + ((position / end) as f32).clamp(0.0, 1.0) * width;
        vertical(frame, x, top, floor, OVER_NEEDLE, 2.0 * s);
        pointer(frame, Point::new(x, floor + l.ruler), 8.0 * s, false);
    }
}

/// A column from `top` down to `bottom`, at least a pixel tall.
fn span_column(frame: &mut canvas::Frame, x: f32, width: f32, top: f32, bottom: f32, color: Color) {
    frame.fill_rectangle(
        Point::new(x, top),
        Size::new(width, (bottom - top).max(1.0)),
        color,
    );
}

/// A slanted label, as the CDJ draws MEMORY and CUE.
fn tag(frame: &mut canvas::Frame, word: &str, at: Point, color: Color, s: f32) {
    let width = 78.0 * s;
    let height = 22.0 * s;
    let lean = 6.0 * s;
    let shape = Path::new(|path| {
        path.move_to(Point::new(at.x + lean, at.y));
        path.line_to(Point::new(at.x + width, at.y));
        path.line_to(Point::new(at.x + width - lean, at.y + height));
        path.line_to(Point::new(at.x, at.y + height));
        path.close();
    });
    frame.fill(&shape, TAG_FILL);
    frame.stroke(
        &shape,
        Stroke {
            style: Style::Solid(color),
            width: 1.5 * s,
            line_join: LineJoin::Miter,
            ..Stroke::default()
        },
    );
    let size = 18.0 * s;
    run(
        frame,
        word,
        at.x + lean + 4.0 * s,
        at.y + height * 0.5 + size * CAP * 0.5,
        size,
        TEXT,
    );
}

/// An orange triangle on the needle, pointing into the waveform.
fn pointer(frame: &mut canvas::Frame, tip_base: Point, size: f32, down: bool) {
    let reach = if down { size } else { -size };
    let triangle = Path::new(|path| {
        path.move_to(Point::new(tip_base.x - size, tip_base.y));
        path.line_to(Point::new(tip_base.x + size, tip_base.y));
        path.line_to(Point::new(tip_base.x, tip_base.y + reach));
        path.close();
    });
    frame.fill(&triangle, POINTER);
}

/// A quaver, since VT323 has no music symbols.
fn note(frame: &mut canvas::Frame, center: Point, s: f32) {
    let head = Point::new(center.x, center.y + 8.0 * s);
    frame.fill(&Path::circle(head, 6.0 * s), TEXT);
    let stem = head.x + 5.0 * s;
    vertical(frame, stem, center.y - 14.0 * s, head.y, TEXT, 2.0 * s);
    let flag = Path::line(
        Point::new(stem, center.y - 14.0 * s),
        Point::new(stem + 8.0 * s, center.y - 4.0 * s),
    );
    frame.stroke(
        &flag,
        Stroke {
            style: Style::Solid(TEXT),
            width: 2.5 * s,
            line_cap: LineCap::Round,
            ..Stroke::default()
        },
    );
}

/// The small keyboard the CDJ puts before the key.
fn key_icon(frame: &mut canvas::Frame, center: Point, s: f32) {
    let size = Size::new(20.0 * s, 20.0 * s);
    let at = Point::new(center.x - size.width * 0.5, center.y - size.height * 0.5);
    frame.fill_rectangle(at, size, TEXT);
    for index in 1..4 {
        let x = at.x + index as f32 * size.width / 4.0;
        frame.fill_rectangle(
            Point::new(x - 1.5 * s, at.y),
            Size::new(3.0 * s, size.height * 0.6),
            TITLE_RIGHT,
        );
    }
}

fn outline(frame: &mut canvas::Frame, at: Point, size: Size, color: Color, width: f32) {
    frame.stroke(
        &Path::rectangle(at, size),
        Stroke {
            style: Style::Solid(color),
            width,
            line_join: LineJoin::Miter,
            ..Stroke::default()
        },
    );
}

fn vertical(frame: &mut canvas::Frame, x: f32, top: f32, bottom: f32, color: Color, width: f32) {
    let path = Path::line(Point::new(x, top), Point::new(x, bottom));
    frame.stroke(
        &path,
        Stroke {
            style: Style::Solid(color),
            width,
            line_cap: LineCap::Butt,
            line_join: LineJoin::Miter,
            ..Stroke::default()
        },
    );
}

fn gradient(start: Point, end: Point, stops: &[(f32, Color)]) -> Fill {
    let mut linear = canvas::gradient::Linear::new(start, end);
    for (offset, color) in stops {
        linear = linear.add_stop(*offset, *color);
    }
    Fill {
        style: Style::Gradient(canvas::Gradient::Linear(linear)),
        ..Fill::default()
    }
}

fn with_alpha(color: Color, alpha: f32) -> Color {
    Color { a: alpha, ..color }
}

/// Width of `content` in VT323 at `size`.
fn measure(content: &str, size: f32) -> f32 {
    content.chars().count() as f32 * size * ADVANCE
}

/// Writes `content` with its baseline on `baseline`, so runs of different
/// sizes line up as they do on the CDJ. Returns where the next run starts.
fn run(
    frame: &mut canvas::Frame,
    content: &str,
    x: f32,
    baseline: f32,
    size: f32,
    color: Color,
) -> f32 {
    let width = measure(content, size);
    write(
        frame,
        content,
        Point::new(x, baseline - size * ASCENT),
        Glyph {
            size,
            color,
            align_x: text::Alignment::Left,
            align_y: alignment::Vertical::Top,
        },
        width + size,
    );
    x + width
}

fn centered_run(
    frame: &mut canvas::Frame,
    content: &str,
    center: f32,
    baseline: f32,
    size: f32,
    color: Color,
) {
    run(
        frame,
        content,
        center - measure(content, size) * 0.5,
        baseline,
        size,
        color,
    );
}

struct Glyph {
    size: f32,
    color: Color,
    align_x: text::Alignment,
    align_y: alignment::Vertical,
}

fn write(frame: &mut canvas::Frame, content: &str, at: Point, glyph: Glyph, max_width: f32) {
    frame.fill_text(canvas::Text {
        content: content.to_string(),
        position: at,
        color: glyph.color,
        size: Pixels(glyph.size),
        line_height: LineHeight::Relative(1.0),
        font: FACE,
        align_x: glyph.align_x,
        align_y: glyph.align_y,
        shaping: Shaping::Basic,
        max_width,
    });
}

fn title_line(artist: &str, title: &str) -> String {
    match (artist.is_empty(), title.is_empty()) {
        (true, true) => String::new(),
        (true, false) => title.to_string(),
        (false, true) => artist.to_string(),
        (false, false) => format!("{artist} - {title}"),
    }
}

/// Shortens `text` to fit `width` at `size`.
fn fit(text: &str, size: f32, width: f32) -> String {
    let cell = (size * ADVANCE).max(1.0);
    let limit = (width / cell) as usize;
    let count = text.chars().count();
    if limit == 0 || count <= limit {
        return text.to_string();
    }
    let keep = limit.saturating_sub(1);
    let mut out: String = text.chars().take(keep).collect();
    out.push('…');
    out
}
