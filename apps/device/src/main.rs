//! Wall player for a Raspberry Pi, also run on this computer while developing.
//!
//! The window is the size of the 7" panel. A background thread scans volumes
//! through [`djusb_core`], and the window lists the libraries it loaded.
//! Arrow keys turn the highlight and Enter presses it. On the Pi the platter
//! and the round key do the same. Pressing an export plays it.

#[cfg(target_os = "linux")]
mod gpio;
mod home;
mod input;
#[allow(
    dead_code,
    reason = "the now-playing screen and the visualizers use the rest of the player"
)]
mod playback;
mod playing;
#[cfg(test)]
mod token_gen;
mod tokens;

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use djusb_core::{
    settings_path, LibraryManager, LibraryState, ScanHandle, Settings, Volume, SCAN_INTERVAL,
};
use iced::theme::Palette;
use iced::widget::{column, container, text};
use iced::{Element, Fill, Padding, Point, Size, Subscription, Task, Theme};

use home::{home_lines, HomeLine, EMPTY_HOME};
use input::{apply_control, Applied, Highlight};
use playback::{Player, Status};
use playing::{build_queue, status_line, Playing};
use tokens::{color, font, space};

/// Raspberry Pi Touch Display 2, turned on its side. The panel's native
/// pixels are 720×1280, and on the wall the long edge is horizontal.
const PANEL_WIDTH: f32 = 1280.0;
const PANEL_HEIGHT: f32 = 720.0;

/// The home screen's pinned row.
const SETTINGS: &str = "Settings";

/// Item rows under the pinned row. The panel is 720 pixels tall, and the
/// status line takes the bottom.
const VISIBLE_ITEMS: usize = 7;

/// How often the status line follows the player.
const STATUS_INTERVAL: Duration = Duration::from_millis(250);

/// Centers the panel, and keeps its top on screen when the monitor is shorter.
fn place_on_screen(window: Size, monitor: Size) -> Point {
    Point::new(
        ((monitor.width - window.width) / 2.0).max(0.0),
        ((monitor.height - window.height) / 2.0).max(0.0),
    )
}

/// The window uses the shared palette. The toolkit also has a warning
/// colour, which this shell does not, so that slot stays the dark theme's.
fn theme(_app: &App) -> Theme {
    Theme::custom(
        "DJ USB",
        Palette {
            background: color::BG,
            text: color::TEXT,
            primary: color::ACCENT,
            success: color::SERATO,
            danger: color::ERROR,
            ..Palette::DARK
        },
    )
}

fn main() -> iced::Result {
    iced::application(App::boot, App::update, App::view)
        .title("DJ USB")
        .theme(theme)
        .window_size((PANEL_WIDTH, PANEL_HEIGHT))
        .position(iced::window::Position::SpecificWith(place_on_screen))
        .subscription(App::subscription)
        .run()
}

struct App {
    libraries: Arc<LibraryManager>,
    /// Owns the scan thread. Dropping the app stops the loop.
    #[expect(dead_code)]
    scan: ScanHandle,
    state: LibraryState,
    keys: input::DeviceInput,
    key_origin: Instant,
    highlight: Highlight,
    player: Player,
    playing: Option<Playing>,
    status: Status,
    /// Playlist orders and preferences. The settings screen reads this.
    #[expect(dead_code, reason = "the settings screen in phase 4 reads this")]
    settings: Settings,
}

#[derive(Debug, Clone)]
enum Message {
    Refresh,
    Tick,
    Keyboard(iced::keyboard::Event),
    #[cfg(target_os = "linux")]
    Control(input::Control),
}

impl App {
    fn boot() -> Self {
        let libraries = Arc::new(LibraryManager::new(|_| {}));
        let scan = libraries.start_scanning(SCAN_INTERVAL, remembered_volumes());
        let state = libraries.state();
        let highlight = Highlight::new(home_lines(&state).len());
        let player = Player::start();
        let status = player.status();
        Self {
            libraries,
            scan,
            state,
            keys: input::DeviceInput::new(),
            key_origin: Instant::now(),
            highlight,
            player,
            playing: None,
            status,
            settings: load_settings(),
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Refresh => {
                self.state = self.libraries.state();
                self.highlight
                    .fit(home_lines(&self.state).len(), VISIBLE_ITEMS);
            }
            Message::Tick => self.status = self.player.status(),
            Message::Keyboard(event) => {
                if let iced::keyboard::Event::KeyPressed { key, repeat, .. } = event {
                    if let Some(dev) = input::dev_key(&key, repeat) {
                        if let Some(control) = self.keys.key(dev, self.key_origin.elapsed()) {
                            self.follow(control);
                        }
                    }
                }
            }
            #[cfg(target_os = "linux")]
            Message::Control(control) => self.follow(control),
        }
        Task::none()
    }

    fn follow(&mut self, control: input::Control) {
        let lines = home_lines(&self.state);
        let applied = apply_control(&mut self.highlight, control, lines.len(), VISIBLE_ITEMS);
        // Row 0 is the pinned Settings row, which opens nothing yet.
        if let Applied::Pressed(row) = applied {
            if let Some(HomeLine::Export {
                library: Some(id), ..
            }) = row.checked_sub(1).and_then(|index| lines.get(index))
            {
                self.play_library(id);
            }
        }
    }

    /// Plays the export's tracks from the first, or pauses and resumes it.
    fn play_library(&mut self, id: &str) {
        if self
            .playing
            .as_ref()
            .is_some_and(|playing| playing.library == id)
            && !self.status.ended
        {
            self.player.toggle();
        } else if let Some(library) = self.state.libraries.iter().find(|library| library.id == id) {
            let (queue, titles) = build_queue(library, |track| {
                self.libraries.resolve_track_path(&library.id, track)
            });
            self.player.load(queue, 0);
            self.playing = Some(Playing {
                library: id.to_string(),
                titles,
            });
        }
        self.status = self.player.status();
    }

    fn subscription(_app: &Self) -> Subscription<Message> {
        Subscription::batch([
            iced::time::every(SCAN_INTERVAL).map(|_| Message::Refresh),
            iced::time::every(STATUS_INTERVAL).map(|_| Message::Tick),
            iced::keyboard::listen().map(Message::Keyboard),
            #[cfg(target_os = "linux")]
            Subscription::run(platter_stream),
        ])
    }

    fn view(&self) -> Element<'_, Message> {
        let items = home_lines(&self.state);
        let mut rows = Vec::new();
        for entry in self.highlight.rows(items.len(), VISIBLE_ITEMS) {
            rows.push(match entry {
                input::Row::Pinned { selected } => {
                    lined(text(SETTINGS).size(font::size::WALL).into(), selected)
                }
                input::Row::Item { index, selected } => {
                    lined(line_body(items[index].clone()), selected)
                }
            });
        }
        let titles = self
            .playing
            .as_ref()
            .map_or(&[][..], |playing| playing.titles.as_slice());
        let status = status_line(&self.status, titles).unwrap_or_default();
        let list = column(rows).spacing(space::SM).width(Fill).height(Fill);
        container(column![list, text(status).size(font::size::WALL_DETAIL)].spacing(space::LG))
            .padding(space::XXL)
            .width(Fill)
            .height(Fill)
            .into()
    }
}

fn line_body(line: HomeLine) -> Element<'static, Message> {
    match line {
        HomeLine::Volume(name) => text(name).size(font::size::WALL).into(),
        HomeLine::Export { label, detail, .. } => {
            container(text(format!("{label} · {detail}")).size(font::size::WALL_DETAIL))
                .padding(Padding {
                    left: space::INDENT,
                    ..Padding::ZERO
                })
                .into()
        }
        HomeLine::Empty => text(EMPTY_HOME).size(font::size::WALL_DETAIL).into(),
    }
}

fn lined(content: Element<'static, Message>, selected: bool) -> Element<'static, Message> {
    container(content)
        .padding(Padding {
            top: space::ROW,
            bottom: space::ROW,
            left: space::LG,
            right: space::LG,
        })
        .width(Fill)
        .style(move |_theme| row_style(selected))
        .into()
}

fn row_style(selected: bool) -> container::Style {
    if selected {
        container::Style::default()
            .background(color::ACCENT)
            .color(color::TEXT)
    } else {
        container::Style::default()
    }
}

/// Saved orders and preferences. A missing file is the default, and a broken
/// file is left in place.
fn load_settings() -> Settings {
    let path = settings_path();
    match Settings::load(&path) {
        Ok(settings) => settings,
        Err(error) => {
            eprintln!(
                "Settings could not be read from {}: {error}",
                path.display()
            );
            Settings::new()
        }
    }
}

/// The last listing that succeeded, so a failed listing does not drop libraries.
fn remembered_volumes() -> impl Fn() -> Vec<Volume> + Send {
    let previous = Arc::new(Mutex::new(Vec::new()));
    move || match djusb_core::list_volumes() {
        Ok(volumes) => {
            *lock(&previous) = volumes.clone();
            volumes
        }
        Err(error) => {
            eprintln!("Scanning for volumes failed: {error}");
            lock(&previous).clone()
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

#[cfg(target_os = "linux")]
fn platter_stream() -> impl iced::futures::Stream<Item = Message> {
    use std::sync::atomic::AtomicBool;

    use iced::futures::channel::mpsc;
    use iced::futures::SinkExt;
    use iced::futures::StreamExt;

    iced::stream::channel(32, async |mut output| {
        let (tx, mut rx) = mpsc::unbounded();
        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = Arc::clone(&stop);
        std::thread::spawn(move || {
            gpio::run(
                move |control| {
                    let _ = tx.unbounded_send(control);
                },
                &stop_thread,
            );
        });
        let _release = StopOnDrop(stop);
        while let Some(control) = rx.next().await {
            if output.send(Message::Control(control)).await.is_err() {
                break;
            }
        }
    })
}

/// Releases the GPIO lines when the window closes.
#[cfg(target_os = "linux")]
struct StopOnDrop(Arc<std::sync::atomic::AtomicBool>);

#[cfg(target_os = "linux")]
impl Drop for StopOnDrop {
    fn drop(&mut self) {
        self.0.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::{place_on_screen, PANEL_HEIGHT, PANEL_WIDTH};
    use iced::{Point, Size};

    #[test]
    fn keeps_the_title_bar_on_a_shorter_monitor() {
        let placed = place_on_screen(
            Size::new(PANEL_WIDTH, PANEL_HEIGHT),
            Size::new(PANEL_WIDTH, 600.0),
        );
        assert_eq!(placed, Point::new(0.0, 0.0));
    }

    #[test]
    fn centers_the_panel_when_the_monitor_is_larger() {
        let placed = place_on_screen(
            Size::new(PANEL_WIDTH, PANEL_HEIGHT),
            Size::new(1920.0, 1080.0),
        );
        assert_eq!(placed, Point::new(320.0, 180.0));
    }
}
