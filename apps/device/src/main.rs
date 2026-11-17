//! Wall player for a Raspberry Pi, also run on this computer while developing.
//!
//! The window is the size of the 7" panel and is drawn as a radio display:
//! a chosen colour on black. A background thread scans volumes through
//! [`djusb_core`]. Arrow keys turn the highlight and Enter presses it. On
//! the Pi the platter and the round key do the same.

mod chrome;
#[cfg(target_os = "linux")]
mod gpio;
mod home;
mod illumination;
mod input;
#[allow(
    dead_code,
    reason = "pause, seek and the queue stay on the player until the screens call them"
)]
mod playback;
mod playing;
mod presets;
mod screens;
#[cfg(test)]
mod token_gen;
mod tokens;
mod visualizer;

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use djusb_core::logic::{find_playlist, list_entries, order_key};
use djusb_core::{
    settings_path, LibraryManager, LibraryState, ScanHandle, Settings, Volume, SCAN_INTERVAL,
};
use djusb_projectm::{discover_packs, Pack};
use iced::theme::Palette;
use iced::{Element, Point, Size, Subscription, Task, Theme};

use playback::{Player, Status};
use playing::{cue_from_entries, now_header, status_line, Playing};
use screens::{Browser, Ctx, Effect, PlaySnap};
use visualizer::DeckTrack;

/// Raspberry Pi Touch Display 2, turned on its side. The panel's native
/// pixels are 720×1280, and on the wall the long edge is horizontal.
pub(crate) const PANEL_WIDTH: f32 = 1280.0;
pub(crate) const PANEL_HEIGHT: f32 = 720.0;

/// How often the status line follows the player.
const STATUS_INTERVAL: Duration = Duration::from_millis(250);

/// Centers the panel, and keeps its top on screen when the monitor is shorter.
fn place_on_screen(window: Size, monitor: Size) -> Point {
    Point::new(
        ((monitor.width - window.width) / 2.0).max(0.0),
        ((monitor.height - window.height) / 2.0).max(0.0),
    )
}

/// Directories that hold preset packs: the override, beside settings, and
/// beside the player. The pack that ships with the player is written beside
/// settings before the scan.
fn preset_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(dir) = std::env::var_os("DJUSB_PRESETS") {
        roots.push(PathBuf::from(dir));
    }
    if let Some(dir) = settings_path().parent() {
        let presets = dir.join("presets");
        presets::install_builtin(&presets);
        roots.push(presets);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            roots.push(dir.join("presets"));
        }
    }
    roots
}

/// Phosphor on black. The toolkit's warning colour is unused, so that slot
/// stays the dark theme's.
fn theme(app: &App) -> Theme {
    let glow = illumination::glow(app.settings.display_color(), app.key_origin.elapsed());
    Theme::custom(
        "DJ USB",
        Palette {
            background: chrome::BACKGROUND,
            text: glow.foreground,
            primary: glow.foreground,
            success: glow.foreground,
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
    packs: Vec<Pack>,
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
        let packs = discover_packs(&preset_roots());
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
            packs,
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Refresh => {
                self.state = self.libraries.state();
                self.packs = discover_packs(&preset_roots());
                let play = self.play_snap();
                let status = self.status_text();
                self.browser.refresh(&Ctx {
                    state: &self.state,
                    settings: &self.settings,
                    play,
                    packs: &self.packs,
                    status: &status,
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
        let status = self.status_text();
        let effect = self.browser.apply(
            control,
            &Ctx {
                state: &self.state,
                settings: &self.settings,
                play,
                packs: &self.packs,
                status: &status,
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
            Effect::DisplayColor(id) => {
                if self.settings.display_color() != id {
                    self.settings.set_display_color(id);
                    if let Err(error) = self.settings.save(&settings_path()) {
                        eprintln!("Settings could not be saved: {error}");
                    }
                }
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
                    let status = self.status_text();
                    self.browser.confirm_play(&Ctx {
                        state: &self.state,
                        settings: &self.settings,
                        play,
                        packs: &self.packs,
                        status: &status,
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
        self.player.load(cue.paths.clone(), cue.start);
        self.playing = Some(Playing {
            titles: cue.titles,
            artists: cue.artists,
            track_titles: cue.track_titles,
            artworks: cue.artworks,
            keys: cue.keys,
            bpms: cue.bpms,
            paths: cue.paths,
        });
        self.status = self.player.status();
        true
    }

    fn status_text(&self) -> String {
        self.playing
            .as_ref()
            .and_then(|playing| status_line(&self.status, &playing.titles))
            .unwrap_or_default()
    }

    fn play_snap(&self) -> PlaySnap {
        PlaySnap {
            loaded: self.playing.is_some(),
            running: self.status.playing(),
            ended: self.status.ended,
        }
    }

    /// The track the deck draws. Empty when nothing has been played.
    fn deck_track(&self) -> DeckTrack {
        let Some(playing) = &self.playing else {
            return DeckTrack::empty();
        };
        let Some(index) = self.status.track else {
            return DeckTrack::empty();
        };
        DeckTrack {
            artist: playing.artists.get(index).cloned().unwrap_or_default(),
            title: playing.track_titles.get(index).cloned().unwrap_or_default(),
            key: playing.keys.get(index).cloned().unwrap_or_default(),
            bpm: playing.bpms.get(index).copied().flatten(),
            path: playing.paths.get(index).cloned(),
            next: index
                .checked_add(1)
                .and_then(|next| playing.paths.get(next).cloned()),
            number: Some(index + 1),
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
        let status = self.status_text();
        let ctx = Ctx {
            state: &self.state,
            settings: &self.settings,
            play,
            packs: &self.packs,
            status: &status,
        };
        let frame = self.browser.frame(&ctx);
        let shown = self.browser.shown(&ctx);
        let footer_selected = self.browser.footer_selected(&ctx);
        let glow = illumination::glow(self.settings.display_color(), self.key_origin.elapsed());
        let header = self
            .playing
            .as_ref()
            .and_then(|playing| now_header(&self.status, playing));
        chrome::screen(
            frame,
            shown,
            header,
            status,
            footer_selected,
            glow,
            chrome::Visuals {
                scope: self.player.scope(),
                playhead: self.player.playhead(),
                deck: self.deck_track(),
            },
        )
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
    use djusb_projectm::render_size;
    use iced::{Point, Size};

    #[test]
    fn milkdrop_draws_at_half_the_panel() {
        assert_eq!(
            render_size(PANEL_WIDTH as u32, PANEL_HEIGHT as u32),
            (640, 360)
        );
    }

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
