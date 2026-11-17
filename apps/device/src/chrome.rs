//! The wall display: phosphor green on black, as on a radio or a CDJ.
//!
//! The face is VT323. A highlighted row is inverted, black on green, so it
//! reads from across the room. Spacing and type size still come from the
//! shared tokens. The desktop keeps its own palette.

use std::path::PathBuf;
use std::sync::Arc;

use iced::widget::image::Image;
use iced::widget::text::Wrapping;
use iced::widget::{column, container, row, text};
use iced::{Alignment, Color, ContentFit, Element, Fill, Font, Padding};

use crate::input::Row;
use crate::playback::{Playhead, Scope};
use crate::playing::NowHeader;
use crate::screens::{Frame, Line, Mast, Readout, Tone};
use crate::tokens::{font, space};
use crate::visualizer::{deck, equalizer, milkdrop, waveform, DeckTrack};

/// The name baked into `fonts/VT323-Regular.ttf`.
pub const FACE: Font = Font::with_name("VT323");

/// VT323, The VT323 Project Authors, SIL Open Font License 1.1.
pub const FACE_BYTES: &[u8] = include_bytes!("../fonts/VT323-Regular.ttf");

pub(crate) const BACKGROUND: Color = Color::from_rgb8(0x00, 0x00, 0x00);
pub(crate) const FOREGROUND: Color = Color::from_rgb8(0x5c, 0xff, 0x6a);
pub(crate) const ALERT: Color = Color::from_rgb8(0xff, 0x45, 0x3a);

const DIM: Color = Color::from_rgb8(0x1f, 0x9a, 0x40);
const WELL: Color = Color::from_rgb8(0x04, 0x14, 0x08);
const READOUT: f32 = 112.0;
const COVER: f32 = 200.0;

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
    scope: Arc<Scope>,
    playhead: Playhead,
    deck_track: DeckTrack,
) -> Element<'static, Message> {
    if frame.fullscreen {
        if let Some(picture) = live_visual(&frame.mast, &scope, &playhead, &deck_track) {
            return container(picture)
                .width(Fill)
                .height(Fill)
                .style(|_| container::Style::default().background(BACKGROUND))
                .into();
        }
    }
    let mut rows = Vec::new();
    if matches!(frame.mast, Mast::NowPlaying) {
        rows.push(now_block(header));
    }
    let Frame {
        pinned,
        caption,
        items,
        mast,
        fullscreen: _,
    } = frame;
    for entry in shown {
        match entry {
            Row::Pinned { selected } => {
                rows.push(menu_row(pinned.clone(), selected));
                if let Some(caption) = &caption {
                    rows.push(caption_line(caption.clone()));
                }
                match &mast {
                    Mast::Level(readout) => rows.push(level_block(readout.clone())),
                    Mast::Notice(lines) => rows.push(notice_block(lines.clone())),
                    Mast::Waveform
                    | Mast::Equalizer(_)
                    | Mast::Deck
                    | Mast::MilkDrop(_)
                    | Mast::NowPlaying
                    | Mast::None => {}
                }
            }
            Row::Item { index, selected } => {
                if let Some(line) = items.get(index) {
                    rows.push(menu_row(line.clone(), selected));
                }
            }
        }
    }
    if let Some(picture) = live_visual(&mast, &scope, &playhead, &deck_track) {
        rows.push(picture);
    }

    let menu = column(rows).spacing(space::SM).width(Fill).height(Fill);
    let mut stack = vec![rule(), menu.into()];
    if footer(
        &status,
        matches!(mast, Mast::NowPlaying | Mast::MilkDrop(_)),
    )
    .is_some()
    {
        stack.push(status_line(status, footer_selected));
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
    scope: &Arc<Scope>,
    playhead: &Playhead,
    deck_track: &DeckTrack,
) -> Option<Element<'static, Message>> {
    Some(match mast {
        Mast::Waveform => waveform(Arc::clone(scope)),
        Mast::Equalizer(theme) => equalizer(Arc::clone(scope), *theme),
        Mast::Deck => deck(playhead.clone(), deck_track.clone()),
        Mast::MilkDrop(path) => milkdrop(Arc::clone(scope), path.clone()),
        Mast::NowPlaying | Mast::None | Mast::Level(_) | Mast::Notice(_) => return None,
    })
}

fn rule<Message: 'static>() -> Element<'static, Message> {
    container(text(""))
        .width(Fill)
        .height(3)
        .style(|_| container::Style::default().background(FOREGROUND))
        .into()
}

fn menu_row<Message: 'static>(line: Line, selected: bool) -> Element<'static, Message> {
    let ink = ink(selected, line.tone);
    let primary: Element<'static, Message> = text(line.text)
        .font(FACE)
        .size(font::size::WALL)
        .color(ink)
        .wrapping(Wrapping::None)
        .width(Fill)
        .into();
    let body = if let Some(detail) = line.detail {
        let detail: Element<'static, Message> = text(detail)
            .font(FACE)
            .size(font::size::WALL_DETAIL)
            .color(ink)
            .wrapping(Wrapping::None)
            .into();
        row![primary, detail]
            .align_y(Alignment::Center)
            .width(Fill)
            .into()
    } else {
        primary
    };
    container(body)
        .padding(Padding {
            top: space::ROW,
            bottom: space::ROW,
            left: space::LG,
            right: space::LG,
        })
        .width(Fill)
        .clip(true)
        .style(move |_theme| row_fill(selected, line.tone))
        .into()
}

fn caption_line<Message: 'static>(caption: String) -> Element<'static, Message> {
    text(caption)
        .font(FACE)
        .size(font::size::WALL_DETAIL)
        .color(DIM)
        .wrapping(Wrapping::None)
        .into()
}

fn level_block<Message: 'static>(readout: Readout) -> Element<'static, Message> {
    let meter = column![
        text(format!("{}%", readout.percent))
            .font(FACE)
            .size(READOUT)
            .color(FOREGROUND),
        text(readout.bar)
            .font(FACE)
            .size(font::size::WALL)
            .color(FOREGROUND),
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

fn notice_block<Message: 'static>(lines: Vec<String>) -> Element<'static, Message> {
    column(
        lines
            .into_iter()
            .map(|line| {
                text(line)
                    .font(FACE)
                    .size(font::size::WALL_DETAIL)
                    .color(FOREGROUND)
                    .wrapping(Wrapping::Word)
                    .into()
            })
            .collect::<Vec<_>>(),
    )
    .spacing(space::SM)
    .width(Fill)
    .into()
}

fn now_block<Message: 'static>(header: Option<NowHeader>) -> Element<'static, Message> {
    let Some(header) = header else {
        return text("Nothing playing")
            .font(FACE)
            .size(font::size::WALL)
            .color(DIM)
            .into();
    };
    let mut lines: Vec<Element<'static, Message>> = Vec::new();
    if !header.artist.is_empty() {
        lines.push(
            text(header.artist)
                .font(FACE)
                .size(font::size::WALL_DETAIL)
                .color(DIM)
                .wrapping(Wrapping::None)
                .into(),
        );
    }
    lines.push(
        text(header.title)
            .font(FACE)
            .size(font::size::WALL)
            .color(FOREGROUND)
            .wrapping(Wrapping::Word)
            .into(),
    );
    lines.push(
        text(header.clock)
            .font(FACE)
            .size(font::size::WALL_DETAIL)
            .color(FOREGROUND)
            .wrapping(Wrapping::None)
            .into(),
    );
    row![
        cover(&header.artwork),
        column(lines).spacing(space::XS).width(Fill)
    ]
    .spacing(space::XL)
    .align_y(Alignment::Center)
    .into()
}

fn cover<Message: 'static>(path: &Option<PathBuf>) -> Element<'static, Message> {
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
                .color(DIM),
        )
        .center_x(COVER)
        .center_y(COVER)
        .style(|_| container::Style::default().background(WELL))
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

fn status_line<Message: 'static>(status: String, selected: bool) -> Element<'static, Message> {
    let problem = is_problem(&status);
    let ink = if selected {
        BACKGROUND
    } else if problem {
        ALERT
    } else {
        DIM
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
                let fill = if problem { ALERT } else { FOREGROUND };
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

fn ink(selected: bool, tone: Tone) -> Color {
    if selected {
        BACKGROUND
    } else {
        match tone {
            Tone::Text => FOREGROUND,
            Tone::Dim => DIM,
            Tone::Danger => ALERT,
        }
    }
}

fn row_fill(selected: bool, tone: Tone) -> container::Style {
    if selected {
        let fill = if tone == Tone::Danger {
            ALERT
        } else {
            FOREGROUND
        };
        container::Style::default().background(fill)
    } else {
        container::Style::default()
    }
}
