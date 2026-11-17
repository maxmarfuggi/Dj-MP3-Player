//! The waveform and equalizer canvases.
//!
//! Each one redraws itself from the audio copy. The waveform is the desktop
//! oscilloscope: a rising-zero lock, a soft trail, and a white, red, blue, or
//! pulsing line on the app's black. The equalizer is the desktop's mirror, LED, and
//! fire styles, fed by the log bands and a smoothed FFT.

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
use crate::chrome::{BACKGROUND, FACE};
use crate::playback::Scope;
use crate::tokens::{color, font};

const LINE_WIDTH: f32 = 3.0;
const GLOW_WIDTH: f32 = 14.0;

/// White, red, blue, and the colour pulse. The same four as the desktop waveform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaveformTheme {
    White,
    Red,
    Blue,
    /// The line walks the colour wheel, the way the first waveform did.
    Pulse,
}

impl WaveformTheme {
    pub fn label(self) -> &'static str {
        match self {
            Self::White => "White",
            Self::Red => "Red",
            Self::Blue => "Blue",
            Self::Pulse => "Pulse",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::White => Self::Red,
            Self::Red => Self::Blue,
            Self::Blue => Self::Pulse,
            Self::Pulse => Self::White,
        }
    }

    /// The line colour. Pulse has no single colour, so it is absent.
    fn color(self) -> Option<Color> {
        match self {
            Self::White => Some(Color::WHITE),
            Self::Red => Some(Color::from_rgb8(0xff, 0x40, 0x40)),
            Self::Blue => Some(color::ACCENT),
            Self::Pulse => None,
        }
    }
}

/// The live oscilloscope.
pub fn waveform<Message: 'static>(
    scope: Arc<Scope>,
    theme: WaveformTheme,
) -> Element<'static, Message> {
    iced::widget::canvas(Waveform { scope, theme })
        .width(FillLength)
        .height(FillLength)
        .into()
}

/// Mirror, LED, and Fire. The same three styles as the desktop equalizer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EqualizerTheme {
    Mirror,
    Led,
    Fire,
}

impl EqualizerTheme {
    pub fn label(self) -> &'static str {
        match self {
            Self::Mirror => "Mirror",
            Self::Led => "LED",
            Self::Fire => "Fire",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Mirror => Self::Led,
            Self::Led => Self::Fire,
            Self::Fire => Self::Mirror,
        }
    }
}

/// The live spectrum, bars above a reflection.
pub fn equalizer<Message: 'static>(
    scope: Arc<Scope>,
    theme: EqualizerTheme,
) -> Element<'static, Message> {
    iced::widget::canvas(Equalizer { scope, theme })
        .width(FillLength)
        .height(FillLength)
        .into()
}

struct Waveform {
    scope: Arc<Scope>,
    theme: WaveformTheme,
}

struct WaveTrace {
    points: Vec<(f32, f32)>,
    /// Hue of this frame. Solid themes ignore it.
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
        frame.fill_rectangle(Point::ORIGIN, frame.size(), BACKGROUND);
        let kept = state.trail.len();
        for (index, trace) in state.trail.iter().enumerate() {
            let age = kept - index;
            let alpha = TRAIL_KEEP.powi(i32::try_from(age).unwrap_or(i32::MAX));
            paint_wave(&mut frame, self.theme, trace, alpha, false);
        }
        paint_wave(&mut frame, self.theme, &state.current, 1.0, true);
        vec![frame.into_geometry()]
    }
}

fn paint_wave(
    frame: &mut canvas::Frame,
    theme: WaveformTheme,
    trace: &WaveTrace,
    alpha: f32,
    glow: bool,
) {
    if let Some(ink) = theme.color() {
        stroke_wave(frame, &trace.points, ink, alpha, glow);
    } else {
        stroke_pulse(frame, &trace.points, trace.hue, alpha, glow);
    }
}

fn stroke_wave(
    frame: &mut canvas::Frame,
    points: &[(f32, f32)],
    ink: Color,
    alpha: f32,
    glow: bool,
) {
    let Some(path) = line(points) else {
        return;
    };
    if glow {
        frame.stroke(&path, solid_stroke(with_alpha(ink, 0.35), GLOW_WIDTH));
    }
    frame.stroke(&path, solid_stroke(with_alpha(ink, alpha), LINE_WIDTH));
}

fn stroke_pulse(
    frame: &mut canvas::Frame,
    points: &[(f32, f32)],
    hue: f32,
    alpha: f32,
    glow: bool,
) {
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

fn spectrum_color(hue: f32) -> Color {
    let (red, green, blue) = hsl(hue, 1.0, 0.68);
    Color::from_rgb(red, green, blue)
}

fn horizontal(width: f32, colors: &[Color]) -> canvas::Gradient {
    linear(Point::ORIGIN, Point::new(width, 0.0), &stops(colors))
}

struct Equalizer {
    scope: Arc<Scope>,
    theme: EqualizerTheme,
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
        paint_equalizer(&mut frame, state, self.theme);
        if state.idle {
            idle_message(&mut frame, "Press play to start the equalizer.");
        }
        vec![frame.into_geometry()]
    }
}

struct MirrorPalette {
    edge: Color,
    middle: Color,
    bars: &'static [Color],
    reflection: [Color; 2],
    baseline: Color,
    peak: Color,
}

const MIRROR: MirrorPalette = MirrorPalette {
    edge: Color::from_rgb8(0x0c, 0x0a, 0x1f),
    middle: Color::from_rgb8(0x2a, 0x1a, 0x55),
    bars: &[
        Color::from_rgb8(0x7b, 0x4d, 0xff),
        Color::from_rgb8(0x3f, 0x8c, 0xff),
        Color::from_rgb8(0x43, 0xe6, 0xff),
        Color::from_rgb8(0xb8, 0xff, 0xf4),
    ],
    reflection: [
        Color::from_rgba8(67, 230, 255, 0.45),
        Color::from_rgba8(123, 77, 255, 0.0),
    ],
    baseline: Color::from_rgba8(184, 255, 244, 0.6),
    peak: Color::WHITE,
};

const FIRE: MirrorPalette = MirrorPalette {
    edge: Color::from_rgb8(0x05, 0x03, 0x03),
    middle: Color::from_rgb8(0x2a, 0x0d, 0x06),
    bars: &[
        Color::from_rgb8(0xb3, 0x14, 0x0c),
        Color::from_rgb8(0xf0, 0x5a, 0x14),
        Color::from_rgb8(0xff, 0xb8, 0x1f),
        Color::from_rgb8(0xff, 0xe4, 0x4a),
        Color::from_rgb8(0xff, 0xf7, 0xa8),
    ],
    reflection: [
        Color::from_rgba8(255, 140, 30, 0.45),
        Color::from_rgba8(179, 20, 12, 0.0),
    ],
    baseline: Color::from_rgba8(255, 220, 140, 0.6),
    peak: Color::from_rgb8(0xff, 0xf8, 0xd6),
};

fn paint_equalizer(frame: &mut canvas::Frame, state: &EqualizerState, theme: EqualizerTheme) {
    match theme {
        EqualizerTheme::Led => paint_led(frame, state),
        EqualizerTheme::Mirror => paint_mirror(frame, state, &MIRROR),
        EqualizerTheme::Fire => paint_mirror(frame, state, &FIRE),
    }
}

fn paint_mirror(frame: &mut canvas::Frame, state: &EqualizerState, palette: &MirrorPalette) {
    let width = frame.width();
    let height = frame.height();
    frame.fill_rectangle(
        Point::ORIGIN,
        frame.size(),
        fill_gradient(vertical(
            0.0,
            height,
            &[
                (0.0, palette.edge),
                (0.64, palette.middle),
                (1.0, palette.edge),
            ],
        )),
    );

    let top = height * 0.05;
    let base = height * 0.64;
    let span = (base - top).max(0.0);
    let columns = BAR_COUNT.max(1) as f32;
    let slot = width / columns;
    let bar_width = (slot - (slot * 0.22).max(1.0)).max(1.0);
    let cap_height = (span * 0.008).round().max(2.0);
    let rising = fill_gradient(vertical(base, top, &stops(palette.bars)));
    let reflection = fill_gradient(vertical(
        base,
        base + span * 0.45,
        &[(0.0, palette.reflection[0]), (1.0, palette.reflection[1])],
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
                palette.peak,
            );
        }
    }
    frame.fill_rectangle(
        Point::new(0.0, base),
        Size::new(width, 1.0),
        palette.baseline,
    );
}

/// Desktop LED: each light covers two bars, green then yellow then red.
fn paint_led(frame: &mut canvas::Frame, state: &EqualizerState) {
    let width = frame.width();
    let height = frame.height();
    if width < 1.0 || height < 1.0 {
        return;
    }
    frame.fill_rectangle(Point::ORIGIN, frame.size(), Color::BLACK);
    let columns = (BAR_COUNT / 2).max(1);
    let slot = width / columns as f32;
    let top = height * 0.05;
    let base = height * 0.96;
    let rows = ((base - top) / slot).floor().max(4.0) as usize;
    let row_pitch = (base - top) / rows as f32;
    let radius = slot.min(row_pitch) * 0.34;
    if radius < 0.5 || rows == 0 {
        return;
    }
    for column in 0..columns {
        for row in 0..rows {
            let center = led_center(column, row, slot, base, row_pitch);
            frame.fill(
                &Path::circle(center, radius),
                with_alpha(led_color(row, rows), 0.09),
            );
        }
    }
    for column in 0..columns {
        let left = column * 2;
        let right = left + 1;
        let level = state
            .levels
            .get(left)
            .copied()
            .unwrap_or(0.0)
            .max(state.levels.get(right).copied().unwrap_or(0.0));
        let peak = state
            .peaks
            .get(left)
            .map_or(0.0, |peak| peak.level)
            .max(state.peaks.get(right).map_or(0.0, |peak| peak.level));
        let lit = (level * rows as f32).round() as usize;
        let peak_row = (peak * rows as f32).round() as i32;
        let peak_row = peak_row.min(i32::try_from(rows).unwrap_or(i32::MAX)) - 1;
        for row in 0..rows {
            if row >= lit && i32::try_from(row).unwrap_or(i32::MAX) != peak_row {
                continue;
            }
            // Same circle as the unlit lamp. Canvas fills have no blur, so a glow reads as a second ring.
            let center = led_center(column, row, slot, base, row_pitch);
            frame.fill(&Path::circle(center, radius), led_color(row, rows));
        }
    }
}

fn led_center(column: usize, row: usize, slot: f32, base: f32, row_pitch: f32) -> Point {
    Point::new(
        (column as f32 + 0.5) * slot,
        base - (row as f32 + 0.5) * row_pitch,
    )
}

fn led_color(row: usize, rows: usize) -> Color {
    let position = (row as f32 + 0.5) / rows.max(1) as f32;
    if position >= 0.85 {
        Color::from_rgb8(0xff, 0x3b, 0x30)
    } else if position >= 0.6 {
        Color::from_rgb8(0xff, 0xd8, 0x3a)
    } else {
        Color::from_rgb8(0x3d, 0xff, 0x6e)
    }
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
