//! A CDJ performance screen.
//!
//! The title, key, remaining time, and tempo come from the library and the
//! playhead. The waveform is scanned from the file. Beat marks are counted
//! from the start of the file at the file's BPM. Cue pads, a pitch
//! percentage, and zoom controls are left off: the library has no cue
//! points, and this player does not pitch the track.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;

use iced::alignment;
use iced::widget::canvas::{self, LineCap, LineJoin, Path, Stroke, Style};
use iced::widget::text::{self, LineHeight, Shaping};
use iced::{window, Color, Element, Fill as FillLength, Pixels, Point, Size};

use super::deck_math::{bars_text, beats, detail_span, remain_text, tempo_text, window_start};
use super::peaks::{columns, scan_file, Peaks};
use crate::chrome::FACE;
use crate::playback::{Moment, Playhead};

const SCREEN: Color = Color::from_rgb8(0x00, 0x06, 0x10);
const WAVE: Color = Color::from_rgb8(0x7e, 0xcb, 0xff);
const WAVE_DIM: Color = Color::from_rgb8(0x2c, 0x5c, 0x82);
const BEAT: Color = Color::from_rgba8(255, 59, 78, 0.75);
const BAR: Color = Color::from_rgba8(255, 106, 50, 0.9);
const NEEDLE: Color = Color::from_rgb8(0xff, 0xf4, 0xe8);
const TEXT: Color = Color::from_rgb8(0xf4, 0xfa, 0xff);
const CYAN: Color = Color::from_rgb8(0x7e, 0xc8, 0xff);

/// The track the deck is showing.
#[derive(Debug, Clone)]
pub struct DeckTrack {
    pub artist: String,
    pub title: String,
    pub key: String,
    pub bpm: Option<f64>,
    pub path: Option<PathBuf>,
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

struct Band {
    title: f32,
    detail_top: f32,
    detail: f32,
    readout_top: f32,
    overview_top: f32,
    overview: f32,
    title_size: f32,
    remain_size: f32,
    tempo_size: f32,
    label_size: f32,
}

fn bands(height: f32) -> Band {
    let scale = if height < 420.0 {
        (height / 420.0).clamp(0.55, 1.0)
    } else {
        1.0
    };
    let title = 48.0 * scale;
    let readout = 112.0 * scale;
    let overview = 72.0 * scale;
    let gap = 8.0 * scale;
    let detail = (height - title - readout - overview - gap * 2.0).max(36.0);
    Band {
        title,
        detail_top: title,
        detail,
        readout_top: title + detail + gap,
        overview_top: title + detail + gap + readout + gap,
        overview,
        title_size: 36.0 * scale,
        remain_size: 64.0 * scale,
        tempo_size: 52.0 * scale,
        label_size: 22.0 * scale,
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

    let band = bands(frame.height());
    let width = frame.width();
    paint_heading(frame, track, moment.position, &band, width);

    let length = moment.length.filter(|length| *length > 0.0);
    let position = moment.position.max(0.0);
    let span = detail_span(track.bpm);
    let start = window_start(position, span, length.unwrap_or(f64::MAX));
    paint_detail(
        frame,
        &band,
        width,
        track,
        peaks,
        failed,
        Window {
            start,
            span,
            position,
        },
    );
    paint_readout(
        frame,
        &band,
        width,
        position,
        length,
        track.bpm,
        moment.paused,
    );
    paint_overview(frame, &band, width, peaks, position, length);
}

fn paint_heading(
    frame: &mut canvas::Frame,
    track: &DeckTrack,
    position: f64,
    band: &Band,
    width: f32,
) {
    let heading = fit(
        &title_line(&track.artist, &track.title),
        band.title_size,
        width * 0.62,
    );
    write(
        frame,
        &heading,
        Point::new(0.0, band.title * 0.5),
        Glyph {
            size: band.title_size,
            color: TEXT,
            align_x: text::Alignment::Left,
            align_y: alignment::Vertical::Center,
        },
        width * 0.62,
    );
    if let Some(bars) = bars_text(position, track.bpm) {
        write(
            frame,
            &bars,
            Point::new(width * 0.5, band.title * 0.5),
            Glyph {
                size: band.label_size,
                color: CYAN,
                align_x: text::Alignment::Center,
                align_y: alignment::Vertical::Center,
            },
            width * 0.3,
        );
    }
    if !track.key.is_empty() {
        write(
            frame,
            &track.key,
            Point::new(width, band.title * 0.5),
            Glyph {
                size: band.title_size,
                color: CYAN,
                align_x: text::Alignment::Right,
                align_y: alignment::Vertical::Center,
            },
            width * 0.24,
        );
    }
}

struct Window {
    start: f64,
    span: f64,
    position: f64,
}

fn paint_detail(
    frame: &mut canvas::Frame,
    band: &Band,
    width: f32,
    track: &DeckTrack,
    peaks: Option<&Peaks>,
    failed: bool,
    window: Window,
) {
    let top = band.detail_top;
    let height = band.detail;
    let Window {
        start,
        span,
        position,
    } = window;
    match peaks {
        Some(peaks) => paint_wave(
            frame,
            peaks,
            start,
            start + span,
            WaveBox { top, width, height },
            None,
        ),
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
                    Point::new(width * 0.5, top + height * 0.5),
                    Glyph {
                        size: band.label_size,
                        color: CYAN,
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
            let x = ((mark.at - start) / span) as f32 * width;
            vertical(
                frame,
                x,
                top,
                top + height,
                if mark.bar { BAR } else { BEAT },
                if mark.bar { 2.0 } else { 1.0 },
            );
        }
    }
    let x = (((position - start) / span) as f32 * width).clamp(0.0, width);
    vertical(frame, x, top, top + height, NEEDLE, 2.0);
}

fn paint_readout(
    frame: &mut canvas::Frame,
    band: &Band,
    width: f32,
    position: f64,
    length: Option<f64>,
    bpm: Option<f64>,
    paused: bool,
) {
    let top = band.readout_top;
    write(
        frame,
        if paused { "REMAIN  PAUSED" } else { "REMAIN" },
        Point::new(0.0, top),
        Glyph {
            size: band.label_size,
            color: CYAN,
            align_x: text::Alignment::Left,
            align_y: alignment::Vertical::Top,
        },
        width * 0.5,
    );
    write(
        frame,
        &remain_text(position, length),
        Point::new(0.0, top + band.label_size + 2.0),
        Glyph {
            size: band.remain_size,
            color: TEXT,
            align_x: text::Alignment::Left,
            align_y: alignment::Vertical::Top,
        },
        width * 0.7,
    );
    write(
        frame,
        "TEMPO",
        Point::new(width, top),
        Glyph {
            size: band.label_size,
            color: CYAN,
            align_x: text::Alignment::Right,
            align_y: alignment::Vertical::Top,
        },
        width * 0.4,
    );
    write(
        frame,
        &tempo_text(bpm),
        Point::new(width, top + band.label_size + 2.0),
        Glyph {
            size: band.tempo_size,
            color: TEXT,
            align_x: text::Alignment::Right,
            align_y: alignment::Vertical::Top,
        },
        width * 0.4,
    );
    write(
        frame,
        "BPM",
        Point::new(width, top + band.label_size + band.tempo_size),
        Glyph {
            size: band.label_size,
            color: CYAN,
            align_x: text::Alignment::Right,
            align_y: alignment::Vertical::Top,
        },
        width * 0.4,
    );
}

fn paint_overview(
    frame: &mut canvas::Frame,
    band: &Band,
    width: f32,
    peaks: Option<&Peaks>,
    position: f64,
    length: Option<f64>,
) {
    let top = band.overview_top;
    let height = band.overview;
    let Some(peaks) = peaks else {
        return;
    };
    let end = length.unwrap_or(peaks.seconds).max(peaks.seconds);
    if end <= 0.0 {
        return;
    }
    paint_wave(
        frame,
        peaks,
        0.0,
        end,
        WaveBox { top, width, height },
        Some(position),
    );
    let x = ((position / end) as f32 * width).clamp(0.0, width);
    vertical(frame, x, top, top + height, NEEDLE, 2.0);
}

struct WaveBox {
    top: f32,
    width: f32,
    height: f32,
}

/// `dim_after` colours columns at and after that time in the quieter blue.
/// `None` paints the whole window bright.
fn paint_wave(
    frame: &mut canvas::Frame,
    peaks: &Peaks,
    from: f64,
    to: f64,
    area: WaveBox,
    dim_after: Option<f64>,
) {
    let count = (area.width.round() as usize).clamp(1, 1600);
    let shape = columns(peaks, from, to, count);
    let slot = area.width / count as f32;
    let mid = area.top + area.height * 0.5;
    let half = area.height * 0.46;
    let span = (to - from).max(1e-6);
    for (index, (min, max)) in shape.iter().enumerate() {
        let column_time = from + (index as f64 + 0.5) / count as f64 * span;
        let color = match dim_after {
            Some(at) if column_time >= at => WAVE_DIM,
            _ => WAVE,
        };
        let y = mid - max * half;
        let bottom = mid - min * half;
        frame.fill_rectangle(
            Point::new(index as f32 * slot, y),
            Size::new(slot.max(1.0), (bottom - y).max(1.0)),
            color,
        );
    }
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

/// Shortens `text` to about `width`, in VT323's half-width cells.
fn fit(text: &str, size: f32, width: f32) -> String {
    let cell = (size * 0.5).max(1.0);
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
