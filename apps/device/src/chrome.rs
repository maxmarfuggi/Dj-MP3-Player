//! The wall display: phosphor green on black, as on a radio or a CDJ.
//!
//! The face is VT323. A highlighted row is inverted, black on the current
//! colour, so it reads from across the room. The colour is the illumination
//! setting: a fixed hue, or variable scan. Spacing and type size still come
//! from the shared tokens. The desktop keeps its own palette.

use std::path::PathBuf;
use std::sync::Arc;

use iced::widget::canvas::{self, LineJoin, Path, Stroke, Style};
use iced::widget::image::Image;
use iced::widget::text::Wrapping;
use iced::widget::{column, container, row, text};
use iced::{Alignment, Color, ContentFit, Element, Fill, Font, Padding, Point, Size};

use crate::illumination::Glow;
use crate::input::Row;
use crate::playback::{Playhead, Scope};
use crate::playing::NowHeader;
use crate::screens::{Frame, Line, Mark, Mast, Readout, Tone};
use crate::tokens::{font, space};
use crate::visualizer::{deck, equalizer, milkdrop, waveform, DeckTrack};

/// The name baked into `fonts/VT323-Regular.ttf`.
pub const FACE: Font = Font::with_name("VT323");

/// VT323, The VT323 Project Authors, SIL Open Font License 1.1.
pub const FACE_BYTES: &[u8] = include_bytes!("../fonts/VT323-Regular.ttf");

pub(crate) const BACKGROUND: Color = Color::from_rgb8(0x00, 0x00, 0x00);
pub(crate) const ALERT: Color = Color::from_rgb8(0xff, 0x45, 0x3a);
const READOUT: f32 = 112.0;
const COVER: f32 = 200.0;
const USB_BADGE: f32 = 40.0;
const USB_GLYPH: f32 = 28.0;

/// The sound, the playhead, and the track the deck draws.
pub(crate) struct Visuals {
    pub scope: Arc<Scope>,
    pub playhead: Playhead,
    pub deck: DeckTrack,
}

/// The panel: a lit edge, the screen, and the status line.
///
/// `footer_selected` inverts the playing line, the same way a menu row
/// inverts when the platter lands on it.
pub fn screen<Message: 'static>(
    frame: Frame,
    shown: Vec<Row>,
    header: Option<NowHeader>,
    status: String,
    footer_selected: bool,
    glow: Glow,
    visuals: Visuals,
) -> Element<'static, Message> {
    if frame.fullscreen {
        if let Some(picture) = live_visual(&frame.mast, &visuals, glow) {
            return container(picture)
                .width(Fill)
                .height(Fill)
                .style(|_| container::Style::default().background(BACKGROUND))
                .into();
        }
    }
    let mut rows = Vec::new();
    if matches!(frame.mast, Mast::NowPlaying) {
        rows.push(now_block(header, glow));
    }
    let sort_selected = shown
        .iter()
        .any(|row| matches!(row, Row::Sort { selected: true }));
    let Frame {
        pinned,
        caption,
        sort,
        items,
        mast,
        fullscreen: _,
    } = frame;
    for entry in shown {
        match entry {
            Row::Pinned { selected } => {
                let sort = sort.clone().map(|label| (label, sort_selected));
                rows.push(page_head(
                    pinned.clone(),
                    caption.clone(),
                    sort,
                    selected,
                    glow,
                ));
                match &mast {
                    Mast::Level(readout) => rows.push(level_block(readout.clone(), glow)),
                    Mast::Notice(lines) => rows.push(notice_block(lines.clone(), glow)),
                    Mast::Waveform(_)
                    | Mast::Equalizer(_)
                    | Mast::Deck
                    | Mast::MilkDrop(_)
                    | Mast::NowPlaying
                    | Mast::None => {}
                }
            }
            Row::Sort { .. } => {}
            Row::Item { index, selected } => {
                if let Some(line) = items.get(index) {
                    rows.push(menu_row(line.clone(), selected, false, glow));
                }
            }
        }
    }
    if let Some(picture) = live_visual(&mast, &visuals, glow) {
        rows.push(picture);
    }

    let menu = column(rows).spacing(space::SM).width(Fill).height(Fill);
    let mut stack = vec![rule(glow), menu.into()];
    if footer(
        &status,
        matches!(mast, Mast::NowPlaying | Mast::MilkDrop(_)),
    )
    .is_some()
    {
        stack.push(status_line(status, footer_selected, glow));
    }
    container(column(stack).spacing(space::LG).width(Fill).height(Fill))
        .padding(space::XXL)
        .width(Fill)
        .height(Fill)
        .style(|_| container::Style::default().background(BACKGROUND))
        .into()
}

/// The picture under the menu, or the whole screen while it is hidden.
fn live_visual<Message: 'static>(
    mast: &Mast,
    visuals: &Visuals,
    glow: Glow,
) -> Option<Element<'static, Message>> {
    Some(match mast {
        Mast::Waveform(theme) => waveform(Arc::clone(&visuals.scope), *theme),
        Mast::Equalizer(theme) => equalizer(Arc::clone(&visuals.scope), *theme),
        Mast::Deck => deck(visuals.playhead.clone(), visuals.deck.clone()),
        Mast::MilkDrop(path) => milkdrop(Arc::clone(&visuals.scope), path.clone(), glow.foreground),
        Mast::NowPlaying | Mast::None | Mast::Level(_) | Mast::Notice(_) => return None,
    })
}

fn rule<Message: 'static>(glow: Glow) -> Element<'static, Message> {
    container(text(""))
        .width(Fill)
        .height(3)
        .style(move |_| container::Style::default().background(glow.foreground))
        .into()
}

/// The pinned row and the page name, closed by a lit rule.
///
/// Everything under the rule is a choice on that page. On a track list, Sort
/// sits on the same line as the page name.
fn page_head<Message: 'static>(
    pinned: Line,
    caption: Option<String>,
    sort: Option<(String, bool)>,
    selected: bool,
    glow: Glow,
) -> Element<'static, Message> {
    let mut parts = vec![menu_row(pinned, selected, true, glow)];
    if caption.is_some() || sort.is_some() {
        parts.push(caption_line(caption.unwrap_or_default(), sort, glow));
    }
    parts.push(rule(glow));
    column(parts).spacing(space::XS).width(Fill).into()
}

fn menu_row<Message: 'static>(
    line: Line,
    selected: bool,
    header: bool,
    glow: Glow,
) -> Element<'static, Message> {
    let ink = row_color(selected, line.tone, glow);
    let mut parts: Vec<Element<'static, Message>> = Vec::new();
    if let Some(badge) = mark_badge(line.mark, selected, glow) {
        parts.push(badge);
    }
    parts.push(
        text(line.text)
            .font(FACE)
            .size(font::size::WALL)
            .color(ink)
            .wrapping(Wrapping::None)
            .width(Fill)
            .into(),
    );
    if let Some(detail) = line.detail {
        parts.push(
            text(detail)
                .font(FACE)
                .size(font::size::WALL_DETAIL)
                .color(ink)
                .wrapping(Wrapping::None)
                .into(),
        );
    }
    let nest = if line.mark == Mark::Library {
        // Sit the export under the volume name, not beside the USB glyph.
        USB_BADGE + space::SM
    } else {
        0.0
    };
    let body = row(parts)
        .spacing(space::SM)
        .align_y(Alignment::Center)
        .width(Fill);
    container(body)
        .padding(Padding {
            top: if line.mark == Mark::Usb && !header {
                space::MD
            } else {
                space::ROW
            },
            bottom: space::ROW,
            left: space::LG + nest,
            right: space::LG,
        })
        .width(Fill)
        .clip(true)
        .style(move |_theme| row_fill(selected, line.tone, header, glow))
        .into()
}

/// A glyph in a small lit frame, so the row's kind reads before its words.
fn mark_badge<Message: 'static>(
    mark: Mark,
    selected: bool,
    glow: Glow,
) -> Option<Element<'static, Message>> {
    if mark == Mark::None {
        return None;
    }
    let ink = if selected {
        BACKGROUND
    } else {
        glow.foreground
    };
    let hole = if selected { glow.foreground } else { glow.well };
    let glyph = iced::widget::canvas(Glyph { mark, ink, hole })
        .width(USB_GLYPH)
        .height(USB_GLYPH);
    Some(
        container(glyph)
            .center_x(USB_BADGE)
            .center_y(USB_BADGE)
            .style(move |_| {
                let mut style = container::Style {
                    border: iced::Border {
                        color: ink,
                        width: 2.0,
                        radius: 8.0.into(),
                    },
                    ..container::Style::default()
                };
                if !selected {
                    style.background = Some(glow.well.into());
                }
                style
            })
            .into(),
    )
}

struct Glyph {
    mark: Mark,
    ink: Color,
    hole: Color,
}

impl<Message> canvas::Program<Message> for Glyph {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: iced::Rectangle,
        _cursor: iced::mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        match self.mark {
            Mark::None => {}
            Mark::Usb => paint_usb(&mut frame, self.ink),
            Mark::Library => paint_library(&mut frame, self.ink),
            Mark::Folder => paint_folder(&mut frame, self.ink),
            Mark::Playlist => paint_playlist(&mut frame, self.ink),
            Mark::Settings => paint_gear(&mut frame, self.ink, self.hole),
        }
        vec![frame.into_geometry()]
    }
}

/// A 16-unit square mapped onto the glyph canvas.
struct Plot {
    scale: f32,
    origin_x: f32,
    origin_y: f32,
}

impl Plot {
    fn new(frame: &canvas::Frame) -> Self {
        let size = frame.width().min(frame.height());
        Self {
            scale: size / 16.0,
            origin_x: (frame.width() - size) / 2.0,
            origin_y: (frame.height() - size) / 2.0,
        }
    }

    fn at(&self, x: f32, y: f32) -> Point {
        Point::new(
            self.origin_x + x * self.scale,
            self.origin_y + y * self.scale,
        )
    }

    fn bar(&self, frame: &mut canvas::Frame, x: f32, y: f32, w: f32, h: f32, ink: Color) {
        frame.fill_rectangle(
            self.at(x, y),
            Size::new(w * self.scale, h * self.scale),
            ink,
        );
    }
}

/// The USB trident: circle, arrow, and square on one spine.
fn paint_usb(frame: &mut canvas::Frame, ink: Color) {
    let plot = Plot::new(frame);
    plot.bar(frame, 2.1, 1.3, 1.7, 13.4, ink);
    plot.bar(frame, 2.1, 2.35, 6.7, 1.7, ink);
    frame.stroke(
        &Path::circle(plot.at(10.7, 3.2), 1.65 * plot.scale),
        stroke(ink, 1.35 * plot.scale),
    );
    plot.bar(frame, 2.1, 7.15, 5.6, 1.7, ink);
    frame.fill(
        &Path::new(|path| {
            path.move_to(plot.at(7.9, 5.7));
            path.line_to(plot.at(13.5, 8.0));
            path.line_to(plot.at(7.9, 10.3));
            path.close();
        }),
        ink,
    );
    plot.bar(frame, 2.1, 11.95, 6.2, 1.7, ink);
    frame.stroke(
        &Path::rectangle(
            plot.at(8.5, 11.15),
            Size::new(2.9 * plot.scale, 2.9 * plot.scale),
        ),
        stroke(ink, 1.35 * plot.scale),
    );
}

/// Three books, so an export reads as a library on the USB above it.
fn paint_library(frame: &mut canvas::Frame, ink: Color) {
    let plot = Plot::new(frame);
    let width = 1.55 * plot.scale;
    let books = [(2.2, 5.2, 3.2), (6.2, 2.4, 3.4), (10.4, 4.4, 3.2)];
    for (x, top, book_width) in books {
        frame.stroke(
            &Path::rectangle(
                plot.at(x, top),
                Size::new(book_width * plot.scale, (13.4 - top) * plot.scale),
            ),
            Stroke {
                style: Style::Solid(ink),
                width,
                line_join: LineJoin::Round,
                ..Stroke::default()
            },
        );
    }
}

/// A folder with its tab, drawn as a thick outline.
fn paint_folder(frame: &mut canvas::Frame, ink: Color) {
    let plot = Plot::new(frame);
    frame.stroke(
        &Path::new(|path| {
            path.move_to(plot.at(2.1, 6.0));
            path.line_to(plot.at(2.1, 3.7));
            path.line_to(plot.at(6.6, 3.7));
            path.line_to(plot.at(8.1, 5.5));
            path.line_to(plot.at(13.9, 5.5));
            path.line_to(plot.at(13.9, 12.6));
            path.line_to(plot.at(2.1, 12.6));
            path.close();
        }),
        Stroke {
            style: Style::Solid(ink),
            width: 1.6 * plot.scale,
            line_join: LineJoin::Round,
            ..Stroke::default()
        },
    );
}

/// Three list rules, the last one short, the way a playlist is written.
fn paint_playlist(frame: &mut canvas::Frame, ink: Color) {
    let plot = Plot::new(frame);
    plot.bar(frame, 2.4, 3.4, 11.2, 1.7, ink);
    plot.bar(frame, 2.4, 7.15, 11.2, 1.7, ink);
    plot.bar(frame, 2.4, 10.9, 7.0, 1.7, ink);
}

/// A toothed wheel with a hole, for Settings.
fn paint_gear(frame: &mut canvas::Frame, ink: Color, hole: Color) {
    let plot = Plot::new(frame);
    let center = 8.0;
    let teeth = 8;
    for index in 0..teeth {
        let angle = index as f32 * std::f32::consts::TAU / teeth as f32;
        let (sin, cos) = angle.sin_cos();
        let side_x = -sin;
        let side_y = cos;
        let inner = 4.2;
        let outer = 6.8;
        let half = 0.7;
        frame.fill(
            &Path::new(|path| {
                path.move_to(plot.at(
                    center + cos * inner + side_x * half,
                    center + sin * inner + side_y * half,
                ));
                path.line_to(plot.at(
                    center + cos * outer + side_x * half,
                    center + sin * outer + side_y * half,
                ));
                path.line_to(plot.at(
                    center + cos * outer - side_x * half,
                    center + sin * outer - side_y * half,
                ));
                path.line_to(plot.at(
                    center + cos * inner - side_x * half,
                    center + sin * inner - side_y * half,
                ));
                path.close();
            }),
            ink,
        );
    }
    frame.fill(
        &Path::circle(plot.at(center, center), 4.4 * plot.scale),
        ink,
    );
    frame.fill(
        &Path::circle(plot.at(center, center), 1.85 * plot.scale),
        hole,
    );
}

fn stroke(ink: Color, width: f32) -> Stroke<'static> {
    Stroke {
        style: Style::Solid(ink),
        width,
        ..Stroke::default()
    }
}

fn caption_line<Message: 'static>(
    caption: String,
    sort: Option<(String, bool)>,
    glow: Glow,
) -> Element<'static, Message> {
    let mut parts = vec![text(caption)
        .font(FACE)
        .size(font::size::WALL_DETAIL)
        .color(glow.dim)
        .wrapping(Wrapping::None)
        .width(Fill)
        .into()];
    let has_sort = sort.is_some();
    if let Some((label, selected)) = sort {
        parts.push(sort_chip(label, selected, glow));
    }
    container(
        row(parts)
            .align_y(Alignment::Center)
            .spacing(space::SM)
            .width(Fill),
    )
    .padding(Padding {
        top: 0.0,
        bottom: space::XS,
        left: space::LG,
        right: if has_sort { 0.0 } else { space::LG },
    })
    .width(Fill)
    .clip(true)
    .into()
}

/// Sort, on the title line. Pressing the highlighted chip advances the order.
fn sort_chip<Message: 'static>(
    label: String,
    selected: bool,
    glow: Glow,
) -> Element<'static, Message> {
    let ink = if selected {
        BACKGROUND
    } else {
        glow.foreground
    };
    container(
        text(label)
            .font(FACE)
            .size(font::size::WALL)
            .color(ink)
            .wrapping(Wrapping::None),
    )
    .padding(Padding {
        top: space::ROW,
        bottom: space::ROW,
        left: space::LG,
        right: space::LG,
    })
    .style(move |_| {
        if selected {
            container::Style::default().background(glow.foreground)
        } else {
            container::Style::default()
        }
    })
    .into()
}

fn level_block<Message: 'static>(readout: Readout, glow: Glow) -> Element<'static, Message> {
    let meter = column![
        text(format!("{}%", readout.percent))
            .font(FACE)
            .size(READOUT)
            .color(glow.foreground),
        text(readout.bar)
            .font(FACE)
            .size(font::size::WALL)
            .color(glow.foreground),
    ]
    .spacing(space::LG)
    .width(Fill)
    .align_x(Alignment::Center);
    container(meter)
        .width(Fill)
        .height(Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .into()
}

fn notice_block<Message: 'static>(lines: Vec<String>, glow: Glow) -> Element<'static, Message> {
    column(
        lines
            .into_iter()
            .map(|line| {
                text(line)
                    .font(FACE)
                    .size(font::size::WALL_DETAIL)
                    .color(glow.foreground)
                    .wrapping(Wrapping::Word)
                    .into()
            })
            .collect::<Vec<_>>(),
    )
    .spacing(space::SM)
    .width(Fill)
    .into()
}

fn now_block<Message: 'static>(header: Option<NowHeader>, glow: Glow) -> Element<'static, Message> {
    let Some(header) = header else {
        return text("Nothing playing")
            .font(FACE)
            .size(font::size::WALL)
            .color(glow.dim)
            .into();
    };
    let mut lines: Vec<Element<'static, Message>> = Vec::new();
    if !header.artist.is_empty() {
        lines.push(
            text(header.artist)
                .font(FACE)
                .size(font::size::WALL_DETAIL)
                .color(glow.dim)
                .wrapping(Wrapping::None)
                .into(),
        );
    }
    lines.push(
        text(header.title)
            .font(FACE)
            .size(font::size::WALL)
            .color(glow.foreground)
            .wrapping(Wrapping::Word)
            .into(),
    );
    lines.push(
        text(header.clock)
            .font(FACE)
            .size(font::size::WALL_DETAIL)
            .color(glow.foreground)
            .wrapping(Wrapping::None)
            .into(),
    );
    row![
        cover(&header.artwork, glow),
        column(lines).spacing(space::XS).width(Fill)
    ]
    .spacing(space::XL)
    .align_y(Alignment::Center)
    .into()
}

fn cover<Message: 'static>(path: &Option<PathBuf>, glow: Glow) -> Element<'static, Message> {
    if let Some(path) = path {
        Image::new(path.clone())
            .width(COVER)
            .height(COVER)
            .content_fit(ContentFit::Cover)
            .into()
    } else {
        container(
            text("NO COVER")
                .font(FACE)
                .size(font::size::WALL_DETAIL)
                .color(glow.dim),
        )
        .center_x(COVER)
        .center_y(COVER)
        .style(move |_| container::Style::default().background(glow.well))
        .into()
    }
}

fn footer(status: &str, now_playing: bool) -> Option<&str> {
    if status.is_empty() || (now_playing && !is_problem(status)) {
        None
    } else {
        Some(status)
    }
}

fn status_line<Message: 'static>(
    status: String,
    selected: bool,
    glow: Glow,
) -> Element<'static, Message> {
    let problem = is_problem(&status);
    let ink = if selected {
        BACKGROUND
    } else if problem {
        ALERT
    } else {
        glow.dim
    };
    let body = text(status)
        .font(FACE)
        .size(font::size::WALL_DETAIL)
        .color(ink)
        .wrapping(Wrapping::Word);
    container(body)
        .padding(Padding {
            top: space::ROW,
            bottom: space::ROW,
            left: space::LG,
            right: space::LG,
        })
        .width(Fill)
        .style(move |_theme| {
            if selected {
                let fill = if problem { ALERT } else { glow.foreground };
                container::Style::default().background(fill)
            } else {
                container::Style::default()
            }
        })
        .into()
}

fn is_problem(status: &str) -> bool {
    status.starts_with("No audio") || status.starts_with("Could not")
}

fn row_color(selected: bool, tone: Tone, glow: Glow) -> Color {
    if selected {
        BACKGROUND
    } else {
        match tone {
            Tone::Text => glow.foreground,
            Tone::Dim => glow.dim,
            Tone::Danger => ALERT,
        }
    }
}

fn row_fill(selected: bool, tone: Tone, header: bool, glow: Glow) -> container::Style {
    if selected {
        let fill = if tone == Tone::Danger {
            ALERT
        } else {
            glow.foreground
        };
        container::Style::default().background(fill)
    } else if header {
        container::Style::default().background(glow.well)
    } else {
        container::Style::default()
    }
}
