//! The waveform and equalizer canvases.
//!
//! Each one redraws itself from the audio copy. The waveform is the desktop
//! oscilloscope: a rising-zero lock, a twenty-second colour cycle, and a
//! trail. The equalizer is the desktop's default mirror style, fed by the
//! log bands and a smoothed FFT.

use std::collections::VecDeque;
use std::sync::Arc;

use iced::alignment;
use iced::widget::canvas::{self, Fill, LineCap, LineJoin, Path, Stroke, Style};
use iced::widget::text::{self, LineHeight, Shaping};
use iced::{window, Color, Element, Fill as FillLength, Pixels, Point, Size};

use super::bands::log_band_ranges;
use super::meter::{step_bars, Peak, BAR_COUNT, MAX_HZ, MIN_HZ};
use super::spectrum::{Spectrum, FFT_SIZE, SMOOTHING};
use super::waveform::{
    downmix, hsl, hue_at, trace, GRADIENT_HUES, TRAIL_FRAMES, TRAIL_KEEP, WAVE_FRAMES,
};
use crate::chrome::FACE;
use crate::playback::Scope;
use crate::tokens::font;

const WAVE_BACKGROUND: Color = Color::from_rgb8(0x0b, 0x0c, 0x10);
const LINE_WIDTH: f32 = 3.0;
const GLOW_WIDTH: f32 = 14.0;

/// The live oscilloscope.
pub fn waveform<Message: 'static>(scope: Arc<Scope>) -> Element<'static, Message> {
    iced::widget::canvas(Waveform { scope })
        .width(FillLength)
        .height(FillLength)
        .into()
}

/// The live spectrum, bars above a reflection.
pub fn equalizer<Message: 'static>(scope: Arc<Scope>) -> Element<'static, Message> {
    iced::widget::canvas(Equalizer { scope })
        .width(FillLength)
        .height(FillLength)
        .into()
}

struct Waveform {
    scope: Arc<Scope>,
}

struct WaveTrace {
    points: Vec<(f32, f32)>,
    hue: f32,
}

struct WaveformState {
    stereo: Vec<f32>,
    mono: Vec<f32>,
    current: WaveTrace,
    trail: VecDeque<WaveTrace>,
    origin: Option<iced::time::Instant>,
}

impl Default for WaveformState {
    fn default() -> Self {
        Self {
            stereo: vec![0.0; WAVE_FRAMES * 2],
            mono: vec![0.0; WAVE_FRAMES],
            current: WaveTrace {
                points: Vec::new(),
                hue: 0.0,
            },
            trail: VecDeque::new(),
            origin: None,
        }
    }
}

impl<Message> canvas::Program<Message> for Waveform {
    type State = WaveformState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &canvas::Event,
        bounds: iced::Rectangle,
        _cursor: iced::mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        let canvas::Event::Window(window::Event::RedrawRequested(now)) = event else {
            return None;
        };
        let origin = *state.origin.get_or_insert(*now);
        let seconds = now
            .checked_duration_since(origin)
            .map_or(0.0, |elapsed| elapsed.as_secs_f32());
        if !state.current.points.is_empty() {
            state.trail.push_back(WaveTrace {
                points: std::mem::take(&mut state.current.points),
                hue: state.current.hue,
            });
            while state.trail.len() > TRAIL_FRAMES {
                state.trail.pop_front();
            }
        }
        self.scope.latest(&mut state.stereo);
        downmix(&state.stereo, &mut state.mono);
        state.current = WaveTrace {
            points: trace(&state.mono, bounds.width, bounds.height),
            hue: hue_at(seconds),
        };
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
        frame.fill_rectangle(Point::ORIGIN, frame.size(), WAVE_BACKGROUND);
        let kept = state.trail.len();
        for (index, trace) in state.trail.iter().enumerate() {
            let age = kept - index;
            let alpha = TRAIL_KEEP.powi(i32::try_from(age).unwrap_or(i32::MAX));
            stroke_wave(&mut frame, &trace.points, trace.hue, alpha, false);
        }
        stroke_wave(
            &mut frame,
            &state.current.points,
            state.current.hue,
            1.0,
            true,
        );
        vec![frame.into_geometry()]
    }
}

fn stroke_wave(frame: &mut canvas::Frame, points: &[(f32, f32)], hue: f32, alpha: f32, glow: bool) {
    let Some(path) = line(points) else {
        return;
    };
    if glow {
        let middle = with_alpha(spectrum_color(hue + GRADIENT_HUES[1]), 0.35);
        frame.stroke(&path, solid_stroke(middle, GLOW_WIDTH));
    }
    let gradient = horizontal(
        frame.width(),
        &GRADIENT_HUES.map(|offset| with_alpha(spectrum_color(hue + offset), alpha)),
    );
    frame.stroke(
        &path,
        Stroke {
            style: Style::Gradient(gradient),
            width: LINE_WIDTH,
            line_cap: LineCap::Round,
            line_join: LineJoin::Round,
            ..Stroke::default()
        },
    );
}

struct Equalizer {
    scope: Arc<Scope>,
}

struct EqualizerState {
    spectrum: Option<Spectrum>,
    ranges: Vec<super::bands::BinRange>,
    rate: u32,
    levels: Vec<f32>,
    peaks: Vec<Peak>,
    stereo: Vec<f32>,
    mono: Vec<f32>,
    decibels: Vec<f32>,
    last: Option<iced::time::Instant>,
    idle: bool,
}

impl Default for EqualizerState {
    fn default() -> Self {
        Self {
            spectrum: None,
            ranges: Vec::new(),
            rate: 0,
            levels: vec![0.0; BAR_COUNT],
            peaks: vec![Peak::default(); BAR_COUNT],
            stereo: vec![0.0; FFT_SIZE * 2],
            mono: vec![0.0; FFT_SIZE],
            decibels: vec![f32::NEG_INFINITY; FFT_SIZE / 2],
            last: None,
            idle: true,
        }
    }
}

impl<Message> canvas::Program<Message> for Equalizer {
    type State = EqualizerState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &canvas::Event,
        _bounds: iced::Rectangle,
        _cursor: iced::mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        let canvas::Event::Window(window::Event::RedrawRequested(now)) = event else {
            return None;
        };
        if state.spectrum.is_none() {
            state.spectrum = Some(Spectrum::new(FFT_SIZE));
        }
        // Before the output opens the rate is 0. The desktop falls back to 48 kHz.
        let heard = self.scope.sample_rate();
        let rate = if heard == 0 { 48_000 } else { heard };
        if state.ranges.is_empty() || state.rate != rate {
            state.ranges = log_band_ranges(f64::from(rate), FFT_SIZE, BAR_COUNT, MIN_HZ, MAX_HZ);
            state.rate = rate;
        }
        state.idle = self.scope.written() == 0;
        if state.idle {
            state.decibels.fill(f32::NEG_INFINITY);
        } else if let Some(spectrum) = state.spectrum.as_mut() {
            self.scope.latest(&mut state.stereo);
            downmix(&state.stereo, &mut state.mono);
            spectrum.read(&state.mono, &mut state.decibels, SMOOTHING);
        }
        let seconds = state.last.map_or(0.0, |last| {
            now.checked_duration_since(last)
                .map_or(0.0, |elapsed| elapsed.as_secs_f32())
        });
        state.last = Some(*now);
        step_bars(
            &state.decibels,
            &state.ranges,
            &mut state.levels,
            &mut state.peaks,
            seconds,
        );
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
        paint_equalizer(&mut frame, state);
        if state.idle {
            idle_message(&mut frame, "Press play to start the equalizer.");
        }
        vec![frame.into_geometry()]
    }
}

fn paint_equalizer(frame: &mut canvas::Frame, state: &EqualizerState) {
    let width = frame.width();
    let height = frame.height();
    let edge = Color::from_rgb8(0x0c, 0x0a, 0x1f);
    let middle = Color::from_rgb8(0x2a, 0x1a, 0x55);
    frame.fill_rectangle(
        Point::ORIGIN,
        frame.size(),
        fill_gradient(vertical(
            0.0,
            height,
            &[(0.0, edge), (0.64, middle), (1.0, edge)],
        )),
    );

    let top = height * 0.05;
    let base = height * 0.64;
    let span = (base - top).max(0.0);
    let columns = BAR_COUNT.max(1) as f32;
    let slot = width / columns;
    let bar_width = (slot - (slot * 0.22).max(1.0)).max(1.0);
    let cap_height = (span * 0.008).round().max(2.0);
    let bars = [
        Color::from_rgb8(0x7b, 0x4d, 0xff),
        Color::from_rgb8(0x3f, 0x8c, 0xff),
        Color::from_rgb8(0x43, 0xe6, 0xff),
        Color::from_rgb8(0xb8, 0xff, 0xf4),
    ];
    let rising = fill_gradient(vertical(base, top, &stops(&bars)));
    let reflection = fill_gradient(vertical(
        base,
        base + span * 0.45,
        &[
            (0.0, Color::from_rgba8(67, 230, 255, 0.45)),
            (1.0, Color::from_rgba8(123, 77, 255, 0.0)),
        ],
    ));
    for (index, level) in state.levels.iter().enumerate() {
        let x = index as f32 * slot;
        let bar_height = level * span;
        if bar_height > 0.0 {
            frame.fill_rectangle(
                Point::new(x, base - bar_height),
                Size::new(bar_width, bar_height),
                rising,
            );
            frame.fill_rectangle(
                Point::new(x, base + 1.0),
                Size::new(bar_width, bar_height * 0.45),
                reflection,
            );
        }
        let peak = state.peaks.get(index).map_or(0.0, |peak| peak.level);
        if peak > 0.01 {
            frame.fill_rectangle(
                Point::new(x, base - peak * span - cap_height * 2.0),
                Size::new(bar_width, cap_height),
                Color::WHITE,
            );
        }
    }
    frame.fill_rectangle(
        Point::new(0.0, base),
        Size::new(width, 1.0),
        Color::from_rgba8(184, 255, 244, 0.6),
    );
}

fn idle_message(frame: &mut canvas::Frame, message: &str) {
    frame.fill_text(canvas::Text {
        content: message.to_string(),
        position: frame.center(),
        color: Color::from_rgb(0.82, 0.86, 0.94),
        size: Pixels(font::size::WALL_DETAIL),
        line_height: LineHeight::default(),
        font: FACE,
        align_x: text::Alignment::Center,
        align_y: alignment::Vertical::Center,
        shaping: Shaping::Basic,
        max_width: frame.width(),
    });
}

fn line(points: &[(f32, f32)]) -> Option<Path> {
    let (first, rest) = points.split_first()?;
    if rest.is_empty() {
        return None;
    }
    Some(Path::new(|builder| {
        builder.move_to(Point::new(first.0, first.1));
        for point in rest {
            builder.line_to(Point::new(point.0, point.1));
        }
    }))
}

fn spectrum_color(hue: f32) -> Color {
    let (red, green, blue) = hsl(hue, 1.0, 0.68);
    Color::from_rgb(red, green, blue)
}

fn with_alpha(color: Color, alpha: f32) -> Color {
    Color { a: alpha, ..color }
}

fn solid_stroke(color: Color, width: f32) -> Stroke<'static> {
    Stroke {
        style: Style::Solid(color),
        width,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Stroke::default()
    }
}

fn stops(colors: &[Color]) -> Vec<(f32, Color)> {
    let last = colors.len().saturating_sub(1).max(1) as f32;
    colors
        .iter()
        .enumerate()
        .map(|(index, color)| (index as f32 / last, *color))
        .collect()
}

fn horizontal(width: f32, stops: &[Color]) -> canvas::Gradient {
    let placed: Vec<(f32, Color)> = {
        let last = stops.len().saturating_sub(1).max(1) as f32;
        stops
            .iter()
            .enumerate()
            .map(|(index, color)| (index as f32 / last, *color))
            .collect()
    };
    linear(Point::ORIGIN, Point::new(width, 0.0), &placed)
}

fn vertical(top: f32, bottom: f32, stops: &[(f32, Color)]) -> canvas::Gradient {
    linear(Point::new(0.0, top), Point::new(0.0, bottom), stops)
}

fn linear(start: Point, end: Point, stops: &[(f32, Color)]) -> canvas::Gradient {
    let mut gradient = canvas::gradient::Linear::new(start, end);
    for (offset, color) in stops {
        gradient = gradient.add_stop(*offset, *color);
    }
    canvas::Gradient::Linear(gradient)
}

fn fill_gradient(gradient: canvas::Gradient) -> Fill {
    Fill {
        style: Style::Gradient(gradient),
        ..Fill::default()
    }
}
