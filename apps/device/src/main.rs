//! Wall player for a Raspberry Pi, also run on this computer while developing.
//!
//! The window is the size of the 7" panel and is drawn as a radio display:
//! phosphor green on black. A background thread scans volumes through
//! [`djusb_core`]. Arrow keys turn the highlight and Enter presses it. On
//! the Pi the platter and the round key do the same.

mod chrome;
#[cfg(target_os = "linux")]
mod gpio;
mod home;
mod input;
#[allow(
    dead_code,
    reason = "the visualizers use the audio copy the player keeps for them"
)]
mod playback;
mod playing;
mod screens;
#[cfg(test)]
mod token_gen;
mod tokens;

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use djusb_core::logic::{find_playlist, list_entries, order_key};
use djusb_core::{
    settings_path, LibraryManager, LibraryState, ScanHandle, Settings, Volume, SCAN_INTERVAL,
};
use iced::theme::Palette;
use iced::{Element, Point, Size, Subscription, Task, Theme};

use playback::{Player, Status};
use playing::{cue_from_entries, now_header, status_line, Playing};
use screens::{Browser, Ctx, Effect, PlaySnap};

/// Raspberry Pi Touch Display 2, turned on its side. The panel's native
/// pixels are 720×1280, and on the wall the long edge is horizontal.
const PANEL_WIDTH: f32 = 1280.0;
const PANEL_HEIGHT: f32 = 720.0;

/// How often the status line follows the player.
const STATUS_INTERVAL: Duration = Duration::from_millis(250);

/// Centers the panel, and keeps its top on screen when the monitor is shorter.
fn place_on_screen(window: Size, monitor: Size) -> Point {
    Point::new(
        ((monitor.width - window.width) / 2.0).max(0.0),
        ((monitor.height - window.height) / 2.0).max(0.0),
    )
}

/// Phosphor on black. The toolkit's warning colour is unused, so that slot
/// stays the dark theme's.
fn theme(_app: &App) -> Theme {
    Theme::custom(
        "DJ USB",
        Palette {
            background: chrome::BACKGROUND,
            text: chrome::FOREGROUND,
            primary: chrome::FOREGROUND,
            success: chrome::FOREGROUND,
            danger: chrome::ALERT,
            ..Palette::DARK
        },
    )
}

fn main() -> iced::Result {
    iced::application(App::boot, App::update, App::view)
        .title("DJ USB")
        .theme(theme)
        .font(chrome::FACE_BYTES)
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
    browser: Browser,
    player: Player,
    playing: Option<Playing>,
    status: Status,
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
        let settings = load_settings();
        let browser = Browser::new(settings.volume_limit(), &state);
        let player = Player::start();
        player.set_volume(browser.volume());
        let status = player.status();
        Self {
            libraries,
            scan,
            state,
            keys: input::DeviceInput::new(),
            key_origin: Instant::now(),
            browser,
            player,
            playing: None,
            status,
            settings,
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Refresh => {
                self.state = self.libraries.state();
                let play = self.play_snap();
                self.browser.refresh(&Ctx {
                    state: &self.state,
                    settings: &self.settings,
                    play,
                });
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
        let play = self.play_snap();
        let effect = self.browser.apply(
            control,
            &Ctx {
                state: &self.state,
                settings: &self.settings,
                play,
            },
        );
        self.perform(effect);
    }

    fn perform(&mut self, effect: Effect) {
        match effect {
            Effect::None => {}
            Effect::Toggle => {
                self.player.toggle();
                self.status = self.player.status();
            }
            Effect::Restart => {
                self.player.jump(0);
                self.player.play();
                self.status = self.player.status();
            }
            Effect::Volume(level) => {
                self.player.set_volume(level);
                self.status = self.player.status();
            }
            Effect::Limit { limit, volume } => {
                self.settings.set_volume_limit(limit);
                if let Err(error) = self.settings.save(&settings_path()) {
                    eprintln!("Settings could not be saved: {error}");
                }
                self.player.set_volume(volume);
                self.status = self.player.status();
            }
            Effect::Play {
                library_id,
                playlist_id,
                index,
            } => {
                if self.load_cue(&library_id, playlist_id.as_deref(), index) {
                    let play = self.play_snap();
                    self.browser.confirm_play(&Ctx {
                        state: &self.state,
                        settings: &self.settings,
                        play,
                    });
                }
            }
        }
    }

    /// Loads the list on screen at `index`. A missing file loads nothing.
    fn load_cue(&mut self, library_id: &str, playlist_id: Option<&str>, index: usize) -> bool {
        let entries = {
            let Some(library) = self
                .state
                .libraries
                .iter()
                .find(|library| library.id == library_id)
            else {
                return false;
            };
            let playlist = match playlist_id {
                Some(id) => match find_playlist(&library.playlists, id) {
                    Some(node) => Some(node),
                    None => return false,
                },
                None => None,
            };
            let order = playlist_id.and_then(|id| self.settings.order(&order_key(library_id, id)));
            list_entries(Some(library), playlist, order, None)
        };
        let Some(cue) = cue_from_entries(
            &entries,
            index,
            |id| self.libraries.resolve_track_path(library_id, id),
            |id| self.libraries.resolve_artwork_path(library_id, id),
        ) else {
            return false;
        };
        self.player.load(cue.paths, cue.start);
        self.playing = Some(Playing {
            titles: cue.titles,
            artists: cue.artists,
            track_titles: cue.track_titles,
            artworks: cue.artworks,
        });
        self.status = self.player.status();
        true
    }

    fn play_snap(&self) -> PlaySnap {
        PlaySnap {
            loaded: self.playing.is_some(),
            running: self.status.playing(),
            ended: self.status.ended,
        }
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
        let play = self.play_snap();
        let ctx = Ctx {
            state: &self.state,
            settings: &self.settings,
            play,
        };
        let frame = self.browser.frame(&ctx);
        let shown = self.browser.shown(&ctx);
        let header = self
            .playing
            .as_ref()
            .and_then(|playing| now_header(&self.status, playing));
        let status = self
            .playing
            .as_ref()
            .and_then(|playing| status_line(&self.status, &playing.titles))
            .unwrap_or_default();
        chrome::screen(frame, shown, header, status)
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
