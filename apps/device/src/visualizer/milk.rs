//! Full-screen MilkDrop, drawn by projectM at half the panel size.
//!
//! The shared library is opened when this screen appears. Until a frame
//! arrives the panel stays black. If the library or the pack is missing, the
//! message stays up and the pinned row still leaves.

use std::path::PathBuf;
use std::sync::Arc;

use djusb_projectm::{render_size, Show};
use iced::widget::canvas::{self, Image};
use iced::widget::image::Handle;
use iced::widget::text::{self, LineHeight, Shaping};
use iced::{alignment, window, Color, Element, Fill, Pixels, Point, Rectangle};

use crate::chrome::{BACKGROUND, FACE};
use crate::playback::Scope;
use crate::tokens::font;
use crate::{PANEL_HEIGHT, PANEL_WIDTH};

/// The picture, scaled up to the space under the pinned row.
pub fn milkdrop<Message: 'static>(
    scope: Arc<Scope>,
    pack: PathBuf,
    color: Color,
) -> Element<'static, Message> {
    iced::widget::canvas(Milk { scope, pack, color })
        .width(Fill)
        .height(Fill)
        .into()
}

struct Milk {
    scope: Arc<Scope>,
    pack: PathBuf,
    color: Color,
}

#[derive(Default)]
struct MilkState {
    pack: Option<PathBuf>,
    show: Option<Show>,
    fault: Option<String>,
    reported: bool,
    serial: u64,
    image: Option<Image>,
}

impl<Message> canvas::Program<Message> for Milk {
    type State = MilkState;

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
        if state.pack.as_ref() != Some(&self.pack) {
            *state = MilkState {
                pack: Some(self.pack.clone()),
                ..MilkState::default()
            };
        }
        if state.show.is_none() && state.fault.is_none() {
            let scope = Arc::clone(&self.scope);
            let size = render_size(PANEL_WIDTH as u32, PANEL_HEIGHT as u32);
            match Show::start(&library_dirs(), &self.pack, size, move |cursor, samples| {
                scope.read_since(cursor, samples);
            }) {
                Ok(show) => state.show = Some(show),
                Err(message) => state.fault = Some(message),
            }
        }
        if let Some(show) = &state.show {
            if let Some(picture) = show.frame() {
                if picture.serial != state.serial {
                    state.serial = picture.serial;
                    let handle =
                        Handle::from_rgba(picture.width, picture.height, picture.rgba.to_vec());
                    state.image = Some(Image::new(handle));
                }
            }
            if let Some(message) = show.fault() {
                state.fault = Some(message);
            }
        }
        let mut announce = false;
        if let Some(message) = &state.fault {
            if !state.reported {
                eprintln!("MilkDrop: {message}");
                state.reported = true;
                announce = true;
            }
        }
        if (state.show.is_some() && state.fault.is_none()) || announce {
            Some(canvas::Action::request_redraw())
        } else {
            None
        }
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
        if let Some(message) = &state.fault {
            frame.fill_text(canvas::Text {
                content: message.clone(),
                position: frame.center(),
                color: self.color,
                size: Pixels(font::size::WALL_DETAIL),
                line_height: LineHeight::default(),
                font: FACE,
                align_x: text::Alignment::Center,
                align_y: alignment::Vertical::Center,
                shaping: Shaping::Basic,
                max_width: frame.width(),
            });
        } else if let Some(image) = &state.image {
            frame.draw_image(Rectangle::new(Point::ORIGIN, frame.size()), image.clone());
        }
        vec![frame.into_geometry()]
    }
}

fn library_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(dir) = std::env::var_os("DJUSB_PROJECTM") {
        dirs.push(PathBuf::from(dir));
    }
    dirs.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("projectm"));
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            dirs.push(dir.to_path_buf());
        }
    }
    dirs
}
