//! The wall player's screens, driven by the platter and the select key.
//!
//! Every screen has one highlighted row. The top row is pinned: on the USB
//! list it opens settings, and on every other screen it returns. A level
//! screen is the exception the controls describe: the platter changes the
//! value, and the pinned row is what leaves, keeping the value.
//!
//! While a queue is loaded, the playing line under the list is the last
//! stop. Pressing it opens now playing. A level screen keeps that line as
//! information, because the platter is already spoken for.
//!
//! A visualizer keeps that menu above the picture. Full screen clears it,
//! and the next turn of the platter brings it back. On the equalizer, Theme
//! cycles Mirror, LED, and Fire.

use std::path::PathBuf;

use djusb_core::logic::{find_playlist, format_bpm, format_time, list_entries, order_key};
use djusb_core::{
    settings_path, Library, LibraryState, PlaylistKind, PlaylistNode, Settings, Track,
};
use djusb_projectm::{licence_lines, Pack};

use crate::home::{home_lines, source_label, tracks_label, volume_name, HomeLine, EMPTY_HOME};
use crate::input::{Control, Highlight, Row};
use crate::playing::track_name;
use crate::visualizer::EqualizerTheme;

/// Item rows under the pinned row. The panel is 720 pixels tall.
pub const VISIBLE_ROWS: usize = 7;

/// Two percent for one detent. A fast turn arrives as several steps.
const PERCENT_STEP: i32 = 2;

/// Where the platter is, and the playback level it is set to.
#[derive(Debug)]
pub struct Browser {
    place: Place,
    highlight: Highlight,
    origin: Option<Origin>,
    volume: u8,
    limit: u8,
    /// The menu is hidden and the picture fills the screen.
    fullscreen: bool,
    theme: EqualizerTheme,
}

/// What the screen needs from the player and the libraries.
pub struct Ctx<'a> {
    pub state: &'a LibraryState,
    pub settings: &'a Settings,
    pub play: PlaySnap,
    pub packs: &'a [Pack],
    /// The line under the list. Empty when nothing is playing.
    pub status: &'a str,
}

/// Enough of the player to label Play and Pause and to offer Now playing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlaySnap {
    /// A queue has been loaded.
    pub loaded: bool,
    /// Audio is moving.
    pub running: bool,
    /// The queue reached its end.
    pub ended: bool,
}

impl PlaySnap {
    #[cfg(test)]
    #[must_use]
    pub fn idle() -> Self {
        Self {
            loaded: false,
            running: false,
            ended: false,
        }
    }
}

/// What to draw. The pinned row is always first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub pinned: Line,
    pub caption: Option<String>,
    pub items: Vec<Line>,
    pub mast: Mast,
    /// The picture fills the screen and the menu is hidden.
    pub fullscreen: bool,
}

/// One row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub text: String,
    pub detail: Option<String>,
    pub tone: Tone,
}

/// How a row is drawn when it is not the highlight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Text,
    Dim,
    Danger,
}

/// The block above or below the pinned row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mast {
    None,
    /// The track, drawn by the window from the player.
    NowPlaying,
    Level(Readout),
    Notice(Vec<String>),
    /// The oscilloscope, drawn by the window from the audio copy.
    Waveform,
    /// The spectrum, drawn by the window from the audio copy.
    Equalizer(EqualizerTheme),
    /// The CDJ deck: title, key, time, tempo, and the file's waveform.
    Deck,
    /// Full-screen MilkDrop for one preset pack, drawn by the window.
    MilkDrop(PathBuf),
}

/// A big percentage and a meter, for volume and the volume limit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Readout {
    pub percent: u8,
    pub bar: String,
}

/// What a press or a turn should do to the player and the saved settings.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    None,
    /// Play `index` in the list on screen. The window opens now playing
    /// after the file loads, via [`Browser::confirm_play`].
    Play {
        library_id: String,
        playlist_id: Option<String>,
        index: usize,
    },
    Toggle,
    Restart,
    Volume(f32),
    Limit {
        limit: f32,
        volume: f32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Place {
    Home,
    Playlists {
        library_id: String,
        path: Vec<String>,
    },
    Tracks {
        library_id: String,
        playlist_id: Option<String>,
        parent: Vec<String>,
    },
    NowPlaying,
    Volume,
    Visualizers,
    Visualizer(Vis),
    /// MilkDrop, showing one preset pack.
    Milk {
        path: PathBuf,
    },
    Settings,
    Bluetooth,
    VolumeLimit,
    About,
    Licences,
    /// The projectM notice and the written source offer.
    ProjectM,
}

/// The list now playing returns to.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Origin {
    library_id: String,
    playlist_id: Option<String>,
    parent: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Vis {
    Waveform,
    Equalizer,
    Deck,
    MilkDrop,
}

impl Vis {
    fn label(self) -> &'static str {
        match self {
            Self::Waveform => "Waveform",
            Self::Equalizer => "Equalizer",
            Self::Deck => "Deck",
            Self::MilkDrop => "MilkDrop",
        }
    }
}

#[derive(Debug, Clone)]
struct Choice {
    line: Line,
    act: Act,
}

#[derive(Debug, Clone)]
enum Act {
    Nothing,
    NowPlaying,
    Library(String),
    Folder(String),
    Tracks { playlist_id: Option<String> },
    Play(usize),
    Transport,
    Volume,
    Visualizers,
    Mode(Vis),
    Bluetooth,
    VolumeLimit,
    About,
    Licences,
    Pack(PathBuf),
    ProjectM,
    FullScreen,
    Theme,
}

impl Choice {
    fn row(text: impl Into<String>, detail: Option<String>, tone: Tone, act: Act) -> Self {
        Self {
            line: Line {
                text: text.into(),
                detail,
                tone,
            },
            act,
        }
    }

    fn act(text: impl Into<String>, detail: Option<String>, act: Act) -> Self {
        Self::row(text, detail, Tone::Text, act)
    }

    fn dead(text: impl Into<String>, tone: Tone) -> Self {
        Self::row(text, None, tone, Act::Nothing)
    }
}

impl Browser {
    /// Full volume, or the saved limit when that is lower. The highlight
    /// starts on the first USB row.
    #[must_use]
    pub fn new(limit: f32, state: &LibraryState) -> Self {
        let limit = to_percent(limit);
        Self {
            place: Place::Home,
            highlight: Highlight::new(home_lines(state).len()),
            origin: None,
            volume: limit,
            limit,
            fullscreen: false,
            theme: EqualizerTheme::Mirror,
        }
    }

    /// The level the player should use, from 0 to 1.
    #[must_use]
    pub fn volume(&self) -> f32 {
        unit(self.volume)
    }

    #[cfg(test)]
    #[must_use]
    pub fn cursor(&self) -> usize {
        self.highlight.index()
    }

    /// The rows to draw, and the rows the highlight can land on.
    #[must_use]
    pub fn frame(&self, ctx: &Ctx<'_>) -> Frame {
        let choices = self.choices(ctx);
        Frame {
            pinned: line(self.pinned_text(ctx), None, Tone::Text),
            caption: self.caption(ctx),
            items: choices.into_iter().map(|choice| choice.line).collect(),
            mast: self.mast(ctx),
            fullscreen: self.fullscreen && self.showing_visual(),
        }
    }

    #[must_use]
    pub fn shown(&self, ctx: &Ctx<'_>) -> Vec<Row> {
        self.highlight.rows(self.choices(ctx).len(), VISIBLE_ROWS)
    }

    /// Keeps the highlight and the screen inside the libraries still mounted.
    pub fn refresh(&mut self, ctx: &Ctx<'_>) {
        let before = self.place.clone();
        self.repair(ctx);
        if self.place == before {
            self.highlight
                .fit(self.choices(ctx).len(), VISIBLE_ROWS, self.footer(ctx));
        } else {
            self.fullscreen = false;
            self.park(ctx);
        }
    }

    #[must_use]
    pub fn apply(&mut self, control: Control, ctx: &Ctx<'_>) -> Effect {
        self.refresh(ctx);
        // The turn that brings the menu back does not also move the highlight.
        if self.fullscreen {
            if matches!(control, Control::Turn(_)) {
                self.fullscreen = false;
            }
            return Effect::None;
        }
        match control {
            Control::Turn(steps) if self.on_level() => self.nudge(steps),
            Control::Turn(steps) => {
                self.highlight.move_by(
                    steps,
                    self.choices(ctx).len(),
                    VISIBLE_ROWS,
                    self.footer(ctx),
                );
                Effect::None
            }
            Control::Press => self.press(ctx),
        }
    }

    /// Playback of the list on screen started. Now playing opens on top of it.
    pub fn confirm_play(&mut self, ctx: &Ctx<'_>) {
        if let Place::Tracks {
            library_id,
            playlist_id,
            parent,
        } = self.place.clone()
        {
            self.origin = Some(Origin {
                library_id,
                playlist_id,
                parent,
            });
        }
        self.go(Place::NowPlaying, ctx);
    }

    fn on_level(&self) -> bool {
        matches!(self.place, Place::Volume | Place::VolumeLimit)
    }

    /// The playing line can take the highlight. Now playing and MilkDrop
    /// already fill the screen, and a level screen uses the platter for the
    /// value.
    fn footer(&self, ctx: &Ctx<'_>) -> bool {
        !ctx.status.is_empty()
            && !self.on_level()
            && !matches!(self.place, Place::NowPlaying | Place::Milk { .. })
    }

    /// The highlight is on the playing line.
    #[must_use]
    pub fn footer_selected(&self, ctx: &Ctx<'_>) -> bool {
        self.footer(ctx) && self.highlight.on_footer(self.choices(ctx).len())
    }

    fn go(&mut self, place: Place, ctx: &Ctx<'_>) {
        self.place = place;
        self.fullscreen = false;
        self.repair(ctx);
        self.park(ctx);
    }

    /// Waveform, equalizer, deck, or a MilkDrop pack, with the picture up.
    fn showing_visual(&self) -> bool {
        matches!(
            self.place,
            Place::Visualizer(Vis::Waveform | Vis::Equalizer | Vis::Deck) | Place::Milk { .. }
        )
    }

    fn park(&mut self, ctx: &Ctx<'_>) {
        if self.on_level() {
            self.highlight = Highlight::pinned();
            return;
        }
        self.highlight = Highlight::new(self.choices(ctx).len());
    }

    fn nudge(&mut self, steps: i32) -> Effect {
        match self.place {
            Place::Volume => {
                let next = add_percent(self.volume, steps, self.limit);
                if next == self.volume {
                    return Effect::None;
                }
                self.volume = next;
                Effect::Volume(unit(next))
            }
            Place::VolumeLimit => {
                let next = add_percent(self.limit, steps, 100);
                if next == self.limit {
                    return Effect::None;
                }
                self.limit = next;
                if self.volume > next {
                    self.volume = next;
                }
                Effect::Limit {
                    limit: unit(self.limit),
                    volume: unit(self.volume),
                }
            }
            _ => Effect::None,
        }
    }

    fn press(&mut self, ctx: &Ctx<'_>) -> Effect {
        let index = self.highlight.index();
        if index == 0 {
            self.press_pinned(ctx);
            return Effect::None;
        }
        if self.highlight.on_footer(self.choices(ctx).len()) {
            if self.footer(ctx) {
                self.go(Place::NowPlaying, ctx);
            }
            return Effect::None;
        }
        let act = self
            .choices(ctx)
            .get(index - 1)
            .map(|choice| choice.act.clone());
        match act {
            Some(act) => self.act(act, ctx),
            None => Effect::None,
        }
    }

    fn press_pinned(&mut self, ctx: &Ctx<'_>) {
        let place = self.place.clone();
        match place {
            Place::Home => self.go(Place::Settings, ctx),
            Place::Playlists { library_id, path } => {
                if path.is_empty() {
                    self.go(Place::Home, ctx);
                } else {
                    let mut path = path;
                    path.pop();
                    self.go(Place::Playlists { library_id, path }, ctx);
                }
            }
            Place::Tracks {
                library_id, parent, ..
            } => self.go(
                Place::Playlists {
                    library_id,
                    path: parent,
                },
                ctx,
            ),
            Place::NowPlaying => self.back_from_now(ctx),
            Place::Volume | Place::Visualizers => self.go(Place::NowPlaying, ctx),
            Place::Visualizer(_) => self.go(Place::Visualizers, ctx),
            Place::Milk { .. } => self.go(Place::Visualizer(Vis::MilkDrop), ctx),
            Place::Settings => self.go(Place::Home, ctx),
            Place::Bluetooth | Place::VolumeLimit | Place::About | Place::Licences => {
                self.go(Place::Settings, ctx)
            }
            Place::ProjectM => self.go(Place::Licences, ctx),
        }
    }

    fn back_from_now(&mut self, ctx: &Ctx<'_>) {
        let destination = self.origin.clone().and_then(|origin| {
            if !origin_is_open(ctx, &origin) {
                return None;
            }
            Some(Place::Tracks {
                library_id: origin.library_id,
                playlist_id: origin.playlist_id,
                parent: origin.parent,
            })
        });
        self.go(destination.unwrap_or(Place::Home), ctx);
    }

    fn act(&mut self, act: Act, ctx: &Ctx<'_>) -> Effect {
        match act {
            Act::Nothing => Effect::None,
            Act::NowPlaying => {
                self.go(Place::NowPlaying, ctx);
                Effect::None
            }
            Act::Library(id) => {
                self.go(
                    Place::Playlists {
                        library_id: id,
                        path: Vec::new(),
                    },
                    ctx,
                );
                Effect::None
            }
            Act::Folder(id) => {
                let Place::Playlists {
                    library_id,
                    mut path,
                } = self.place.clone()
                else {
                    return Effect::None;
                };
                path.push(id);
                self.go(Place::Playlists { library_id, path }, ctx);
                Effect::None
            }
            Act::Tracks { playlist_id } => {
                let Place::Playlists { library_id, path } = self.place.clone() else {
                    return Effect::None;
                };
                self.go(
                    Place::Tracks {
                        library_id,
                        playlist_id,
                        parent: path,
                    },
                    ctx,
                );
                Effect::None
            }
            Act::Play(index) => {
                let Place::Tracks {
                    library_id,
                    playlist_id,
                    ..
                } = &self.place
                else {
                    return Effect::None;
                };
                Effect::Play {
                    library_id: library_id.clone(),
                    playlist_id: playlist_id.clone(),
                    index,
                }
            }
            Act::Transport => {
                if !ctx.play.loaded {
                    Effect::None
                } else if ctx.play.ended {
                    Effect::Restart
                } else {
                    Effect::Toggle
                }
            }
            Act::Volume => {
                self.go(Place::Volume, ctx);
                Effect::None
            }
            Act::Visualizers => {
                self.go(Place::Visualizers, ctx);
                Effect::None
            }
            Act::Mode(Vis::MilkDrop) if ctx.packs.len() == 1 => {
                let path = ctx.packs[0].path.clone();
                self.go(Place::Milk { path }, ctx);
                Effect::None
            }
            Act::Mode(mode) => {
                self.go(Place::Visualizer(mode), ctx);
                Effect::None
            }
            Act::Bluetooth => {
                self.go(Place::Bluetooth, ctx);
                Effect::None
            }
            Act::VolumeLimit => {
                self.go(Place::VolumeLimit, ctx);
                Effect::None
            }
            Act::About => {
                self.go(Place::About, ctx);
                Effect::None
            }
            Act::Licences => {
                self.go(Place::Licences, ctx);
                Effect::None
            }
            Act::Pack(path) => {
                self.go(Place::Milk { path }, ctx);
                Effect::None
            }
            Act::ProjectM => {
                self.go(Place::ProjectM, ctx);
                Effect::None
            }
            Act::FullScreen => {
                self.fullscreen = true;
                Effect::None
            }
            Act::Theme => {
                self.theme = self.theme.next();
                Effect::None
            }
        }
    }

    fn repair(&mut self, ctx: &Ctx<'_>) {
        match self.place.clone() {
            Place::Playlists { library_id, path } => {
                let Some(library) = find_library(ctx, &library_id) else {
                    self.place = Place::Home;
                    return;
                };
                self.place = Place::Playlists {
                    library_id,
                    path: valid_path(library, &path),
                };
            }
            Place::Tracks {
                library_id,
                playlist_id,
                parent,
            } => {
                let Some(library) = find_library(ctx, &library_id) else {
                    self.place = Place::Home;
                    return;
                };
                if let Some(id) = &playlist_id {
                    if find_playlist(&library.playlists, id).is_none() {
                        self.place = Place::Playlists {
                            library_id,
                            path: valid_path(library, &parent),
                        };
                        return;
                    }
                }
                self.place = Place::Tracks {
                    library_id,
                    playlist_id,
                    parent: valid_path(library, &parent),
                };
            }
            Place::Milk { path } if !ctx.packs.iter().any(|pack| pack.path == path) => {
                self.place = Place::Visualizer(Vis::MilkDrop);
            }
            _ => {}
        }
        if let Some(origin) = &self.origin {
            if !origin_is_open(ctx, origin) {
                self.origin = None;
            }
        }
    }

    fn choices(&self, ctx: &Ctx<'_>) -> Vec<Choice> {
        match &self.place {
            Place::Home => home_choices(ctx),
            Place::Playlists { library_id, path } => playlist_choices(ctx, library_id, path),
            Place::Tracks {
                library_id,
                playlist_id,
                ..
            } => track_choices(ctx, library_id, playlist_id.as_deref()),
            Place::NowPlaying => self.transport_choices(ctx),
            Place::Visualizers => visualizer_choices(),
            Place::Visualizer(Vis::MilkDrop) => milk_choices(ctx.packs),
            Place::Visualizer(mode) => show_choices(self.theme, *mode),
            Place::Milk { .. } => show_choices(self.theme, Vis::MilkDrop),
            Place::Settings => self.settings_choices(),
            Place::Licences => licence_choices(),
            Place::Volume
            | Place::VolumeLimit
            | Place::Bluetooth
            | Place::About
            | Place::ProjectM => Vec::new(),
        }
    }

    fn transport_choices(&self, ctx: &Ctx<'_>) -> Vec<Choice> {
        let label = if ctx.play.running { "Pause" } else { "Play" };
        vec![
            Choice::act(label, None, Act::Transport),
            Choice::act("Volume", Some(percent_label(self.volume)), Act::Volume),
            Choice::act("Visualizer", None, Act::Visualizers),
        ]
    }

    fn settings_choices(&self) -> Vec<Choice> {
        vec![
            Choice::act("Bluetooth", None, Act::Bluetooth),
            Choice::act(
                "Volume limit",
                Some(percent_label(self.limit)),
                Act::VolumeLimit,
            ),
            Choice::act("About", None, Act::About),
            Choice::act("Licences", None, Act::Licences),
        ]
    }

    fn pinned_text(&self, ctx: &Ctx<'_>) -> String {
        match &self.place {
            Place::Home => "Settings".to_string(),
            Place::Playlists { library_id, path } => find_library(ctx, library_id)
                .map(|library| playlist_back(library, path))
                .unwrap_or_else(|| back("USB")),
            Place::Tracks {
                library_id, parent, ..
            } => find_library(ctx, library_id)
                .map(|library| tracks_back(library, parent))
                .unwrap_or_else(|| back("USB")),
            Place::NowPlaying => {
                if self
                    .origin
                    .as_ref()
                    .is_some_and(|origin| origin_is_open(ctx, origin))
                {
                    back("Tracks")
                } else {
                    back("USB")
                }
            }
            Place::Volume | Place::Visualizers => back("Now playing"),
            Place::Visualizer(_) => back("Visualizer"),
            Place::Milk { .. } => back("MilkDrop"),
            Place::Settings => back("USB"),
            Place::Bluetooth | Place::VolumeLimit | Place::About | Place::Licences => {
                back("Settings")
            }
            Place::ProjectM => back("Licences"),
        }
    }

    fn caption(&self, ctx: &Ctx<'_>) -> Option<String> {
        match &self.place {
            Place::Playlists { library_id, path } => {
                let library = find_library(ctx, library_id)?;
                Some(playlist_caption(library, path))
            }
            Place::Tracks {
                library_id,
                playlist_id,
                ..
            } => {
                let library = find_library(ctx, library_id)?;
                Some(tracks_caption(library, playlist_id.as_deref()))
            }
            Place::Volume if self.limit < 100 => {
                Some(format!("Limit {}", percent_label(self.limit)))
            }
            Place::VolumeLimit => Some("Loudest the player will go".to_string()),
            Place::Visualizer(Vis::MilkDrop) if !ctx.packs.is_empty() => {
                Some("Preset pack".to_string())
            }
            _ => None,
        }
    }

    fn mast(&self, ctx: &Ctx<'_>) -> Mast {
        match &self.place {
            Place::NowPlaying => Mast::NowPlaying,
            Place::Volume => Mast::Level(readout(self.volume)),
            Place::VolumeLimit => Mast::Level(readout(self.limit)),
            Place::About => Mast::Notice(about_lines()),
            Place::Bluetooth => Mast::Notice(bluetooth_lines()),
            Place::Visualizer(Vis::Waveform) => Mast::Waveform,
            Place::Visualizer(Vis::Equalizer) => Mast::Equalizer(self.theme),
            Place::Visualizer(Vis::Deck) => Mast::Deck,
            Place::Visualizer(Vis::MilkDrop) if ctx.packs.is_empty() => {
                Mast::Notice(no_pack_lines())
            }
            Place::Milk { path } => Mast::MilkDrop(path.clone()),
            Place::ProjectM => Mast::Notice(
                licence_lines()
                    .iter()
                    .map(|line| (*line).to_string())
                    .collect(),
            ),
            _ => Mast::None,
        }
    }
}

fn home_choices(ctx: &Ctx<'_>) -> Vec<Choice> {
    let mut choices = Vec::new();
    if ctx.play.loaded {
        choices.push(Choice::act("Now playing", None, Act::NowPlaying));
    }
    for line in home_lines(ctx.state) {
        choices.push(match line {
            HomeLine::Volume(name) => Choice::dead(name, Tone::Dim),
            HomeLine::Export {
                label,
                detail,
                library: Some(id),
            } => Choice::act(label, Some(detail), Act::Library(id)),
            HomeLine::Export {
                label,
                detail,
                library: None,
            } => Choice::row(label, Some(detail), Tone::Danger, Act::Nothing),
            HomeLine::Empty => Choice::dead(EMPTY_HOME, Tone::Dim),
        });
    }
    choices
}

fn playlist_choices(ctx: &Ctx<'_>, library_id: &str, path: &[String]) -> Vec<Choice> {
    let Some(library) = find_library(ctx, library_id) else {
        return Vec::new();
    };
    let Some((nodes, current)) = nodes_at(library, path) else {
        return Vec::new();
    };
    let mut choices = Vec::new();
    if let Some(node) = current {
        if !node.track_ids.is_empty() {
            choices.push(Choice::act(
                "Tracks",
                Some(tracks_label(node.track_ids.len())),
                Act::Tracks {
                    playlist_id: Some(node.id.clone()),
                },
            ));
        }
    } else {
        choices.push(Choice::act(
            "All tracks",
            Some(tracks_label(library.tracks.len())),
            Act::Tracks { playlist_id: None },
        ));
    }
    for node in nodes {
        choices.push(node_choice(node));
    }
    if choices.is_empty() {
        choices.push(Choice::dead("Empty", Tone::Dim));
    }
    choices
}

fn node_choice(node: &PlaylistNode) -> Choice {
    let act = if node.kind == PlaylistKind::Folder || !node.children.is_empty() {
        Act::Folder(node.id.clone())
    } else {
        Act::Tracks {
            playlist_id: Some(node.id.clone()),
        }
    };
    let detail = if node.kind == PlaylistKind::Folder && node.track_ids.is_empty() {
        None
    } else {
        Some(tracks_label(node.track_ids.len()))
    };
    Choice::act(node.name.clone(), detail, act)
}

fn track_choices(ctx: &Ctx<'_>, library_id: &str, playlist_id: Option<&str>) -> Vec<Choice> {
    let Some(entries) = track_entries(ctx, library_id, playlist_id) else {
        return Vec::new();
    };
    if entries.is_empty() {
        return vec![Choice::dead("No tracks", Tone::Dim)];
    }
    entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            Choice::act(
                track_name(&entry.track.artist, &entry.track.title),
                track_detail(&entry.track),
                Act::Play(index),
            )
        })
        .collect()
}

fn track_entries(
    ctx: &Ctx<'_>,
    library_id: &str,
    playlist_id: Option<&str>,
) -> Option<Vec<djusb_core::logic::TrackEntry>> {
    let library = find_library(ctx, library_id)?;
    let playlist = match playlist_id {
        Some(id) => Some(find_playlist(&library.playlists, id)?),
        None => None,
    };
    let order = playlist_id.and_then(|id| ctx.settings.order(&order_key(library_id, id)));
    Some(list_entries(Some(library), playlist, order, None))
}

fn track_detail(track: &Track) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(bpm) = track.bpm {
        parts.push(format_bpm(bpm));
    }
    if let Some(key) = track.key.as_deref().filter(|key| !key.is_empty()) {
        parts.push(key.to_string());
    }
    if let Some(seconds) = track.duration_sec {
        parts.push(format_time(seconds));
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("  "))
    }
}

/// Full screen sits under the back row. The equalizer adds Theme under that.
fn show_choices(theme: EqualizerTheme, mode: Vis) -> Vec<Choice> {
    let mut choices = vec![Choice::act("Full screen", None, Act::FullScreen)];
    if mode == Vis::Equalizer {
        choices.push(Choice::act(
            "Theme",
            Some(theme.label().to_string()),
            Act::Theme,
        ));
    }
    choices
}

fn visualizer_choices() -> Vec<Choice> {
    [Vis::Waveform, Vis::Equalizer, Vis::Deck, Vis::MilkDrop]
        .into_iter()
        .map(|mode| Choice::act(mode.label(), None, Act::Mode(mode)))
        .collect()
}

fn milk_choices(packs: &[Pack]) -> Vec<Choice> {
    if packs.is_empty() {
        return vec![Choice::dead("No preset packs", Tone::Dim)];
    }
    packs
        .iter()
        .map(|pack| Choice::act(pack.name.clone(), None, Act::Pack(pack.path.clone())))
        .collect()
}

fn no_pack_lines() -> Vec<String> {
    let directory = settings_path()
        .parent()
        .map(|dir| dir.join("presets"))
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "presets".to_string());
    vec![
        "MilkDrop".to_string(),
        "Add a folder of .milk presets.".to_string(),
        directory,
    ]
}

fn licence_choices() -> Vec<Choice> {
    let rows = [
        ("iced", "MIT"),
        ("symphonia", "MPL-2.0"),
        ("cpal", "Apache-2.0"),
        ("rubato", "MIT or Apache-2.0"),
        ("rtrb", "MIT or Apache-2.0"),
        ("rustfft", "MIT or Apache-2.0"),
        ("rusqlite", "MIT"),
        ("SQLCipher", "BSD"),
        ("gpiocdev", "Apache-2.0 or MIT"),
        ("VT323", "SIL Open Font License"),
    ];
    let mut choices = rows
        .into_iter()
        .map(|(name, licence)| Choice::act(name, Some(licence.to_string()), Act::Nothing))
        .collect::<Vec<_>>();
    choices.push(Choice::act(
        "projectM",
        Some("LGPL-2.1".to_string()),
        Act::ProjectM,
    ));
    choices.push(Choice::dead(
        "The full list comes with the appliance image.",
        Tone::Dim,
    ));
    choices
}

fn about_lines() -> Vec<String> {
    vec![
        "DJ USB".to_string(),
        "Wall player for rekordbox and Serato USBs.".to_string(),
        "The platter moves the highlight. The key presses it.".to_string(),
        format!("Version {}", env!("CARGO_PKG_VERSION")),
    ]
}

fn bluetooth_lines() -> Vec<String> {
    vec![
        "Bluetooth".to_string(),
        "Speakers are paired here when the player runs on the Pi.".to_string(),
        "The player never goes online.".to_string(),
    ]
}

fn playlist_caption(library: &Library, path: &[String]) -> String {
    match path.last() {
        None => format!(
            "{} · {}",
            volume_name(&library.volume_name, &library.volume_path),
            source_label(library.source)
        ),
        Some(id) => find_playlist(&library.playlists, id)
            .map(|node| node.name.clone())
            .unwrap_or_else(|| "Folder".to_string()),
    }
}

fn tracks_caption(library: &Library, playlist_id: Option<&str>) -> String {
    match playlist_id {
        None => "All tracks".to_string(),
        Some(id) => find_playlist(&library.playlists, id)
            .map(|node| node.name.clone())
            .unwrap_or_else(|| "Tracks".to_string()),
    }
}

fn playlist_back(library: &Library, path: &[String]) -> String {
    if path.is_empty() {
        return back("USB");
    }
    if path.len() == 1 {
        return back(&volume_name(&library.volume_name, &library.volume_path));
    }
    let parent = &path[path.len() - 2];
    named_back(library, parent)
}

fn tracks_back(library: &Library, parent: &[String]) -> String {
    match parent.last() {
        None => back(&volume_name(&library.volume_name, &library.volume_path)),
        Some(id) => named_back(library, id),
    }
}

fn named_back(library: &Library, id: &str) -> String {
    let name = find_playlist(&library.playlists, id)
        .map(|node| node.name.clone())
        .unwrap_or_else(|| volume_name(&library.volume_name, &library.volume_path));
    back(&name)
}

fn back(name: &str) -> String {
    format!("‹ {name}")
}

fn nodes_at<'a>(
    library: &'a Library,
    path: &[String],
) -> Option<(&'a [PlaylistNode], Option<&'a PlaylistNode>)> {
    let mut nodes = library.playlists.as_slice();
    let mut current = None;
    for id in path {
        let node = nodes.iter().find(|node| &node.id == id)?;
        current = Some(node);
        nodes = &node.children;
    }
    Some((nodes, current))
}

fn valid_path(library: &Library, path: &[String]) -> Vec<String> {
    let mut nodes = library.playlists.as_slice();
    let mut kept = Vec::new();
    for id in path {
        let Some(node) = nodes.iter().find(|node| &node.id == id) else {
            break;
        };
        kept.push(id.clone());
        nodes = &node.children;
    }
    kept
}

fn origin_is_open(ctx: &Ctx<'_>, origin: &Origin) -> bool {
    let Some(library) = find_library(ctx, &origin.library_id) else {
        return false;
    };
    match &origin.playlist_id {
        Some(id) => find_playlist(&library.playlists, id).is_some(),
        None => true,
    }
}

fn find_library<'a>(ctx: &'a Ctx<'_>, id: &str) -> Option<&'a Library> {
    ctx.state.libraries.iter().find(|library| library.id == id)
}

fn line(text: impl Into<String>, detail: Option<String>, tone: Tone) -> Line {
    Line {
        text: text.into(),
        detail,
        tone,
    }
}

fn percent_label(percent: u8) -> String {
    format!("{percent}%")
}

fn readout(percent: u8) -> Readout {
    Readout {
        percent,
        bar: meter(percent),
    }
}

fn meter(percent: u8) -> String {
    const CELLS: usize = 16;
    let mut filled = (usize::from(percent) * CELLS + 50) / 100;
    if percent > 0 && filled == 0 {
        filled = 1;
    }
    if percent == 100 {
        filled = CELLS;
    }
    let filled = filled.min(CELLS);
    format!("{}{}", "█".repeat(filled), "░".repeat(CELLS - filled))
}

fn add_percent(value: u8, steps: i32, max: u8) -> u8 {
    let next = i32::from(value).saturating_add(steps.saturating_mul(PERCENT_STEP));
    let max = i32::from(max);
    u8::try_from(next.clamp(0, max)).unwrap_or(0)
}

fn to_percent(level: f32) -> u8 {
    if !level.is_finite() {
        return 100;
    }
    let scaled = (level.clamp(0.0, 1.0) * 100.0).round();
    if scaled <= 0.0 {
        0
    } else if scaled >= 100.0 {
        100
    } else {
        scaled as u8
    }
}

fn unit(percent: u8) -> f32 {
    f32::from(percent) / 100.0
}

#[cfg(test)]
mod tests {
    use super::{back, Browser, Ctx, Effect, Mast, PlaySnap};
    use crate::input::Control;
    use djusb_core::{
        Library, LibraryLoadError, LibrarySource, LibraryState, PlaylistKind, PlaylistNode,
        Settings, Track,
    };
    use djusb_projectm::Pack;
    use std::path::PathBuf;

    fn track(id: &str, title: &str) -> Track {
        Track {
            id: id.to_string(),
            title: title.to_string(),
            artist: "Artist".to_string(),
            album: String::new(),
            bpm: Some(128.0),
            key: Some("8A".to_string()),
            duration_sec: Some(180.0),
            relative_path: format!("{id}.mp3"),
            genre: None,
            label: None,
            color: None,
            artwork_path: None,
        }
    }

    fn node(
        id: &str,
        name: &str,
        kind: PlaylistKind,
        children: Vec<PlaylistNode>,
        track_ids: &[&str],
    ) -> PlaylistNode {
        PlaylistNode {
            id: id.to_string(),
            name: name.to_string(),
            kind,
            children,
            track_ids: track_ids.iter().map(|id| (*id).to_string()).collect(),
        }
    }

    fn gig() -> Library {
        Library {
            id: "rekordbox-1".to_string(),
            source: LibrarySource::Rekordbox,
            volume_name: "GIGS".to_string(),
            volume_path: "D:\\".to_string(),
            tracks: vec![track("1", "One"), track("2", "Two"), track("3", "Three")],
            playlists: vec![
                node(
                    "house",
                    "House",
                    PlaylistKind::Folder,
                    vec![node(
                        "late",
                        "Late",
                        PlaylistKind::Playlist,
                        vec![],
                        &["2", "1"],
                    )],
                    &[],
                ),
                node(
                    "techno",
                    "Techno",
                    PlaylistKind::Playlist,
                    vec![node("peak", "Peak", PlaylistKind::Playlist, vec![], &["3"])],
                    &["1", "3"],
                ),
            ],
        }
    }

    fn state(libraries: Vec<Library>) -> LibraryState {
        LibraryState {
            libraries,
            errors: Vec::new(),
        }
    }

    fn context<'a>(state: &'a LibraryState, settings: &'a Settings, play: PlaySnap) -> Ctx<'a> {
        Ctx {
            state,
            settings,
            play,
            packs: &[],
            status: "",
        }
    }

    fn press_text(browser: &mut Browser, ctx: &Ctx<'_>, label: &str) -> Effect {
        let frame = browser.frame(ctx);
        let index = frame
            .items
            .iter()
            .position(|item| item.text == label)
            .unwrap_or_else(|| panic!("no row {label}"));
        let target = i32::try_from(index + 1).unwrap();
        let cursor = i32::try_from(browser.cursor()).unwrap();
        let _ = browser.apply(Control::Turn(target - cursor), ctx);
        browser.apply(Control::Press, ctx)
    }

    fn press_pinned(browser: &mut Browser, ctx: &Ctx<'_>) -> Effect {
        let cursor = i32::try_from(browser.cursor()).unwrap_or(0);
        let _ = browser.apply(Control::Turn(-cursor), ctx);
        browser.apply(Control::Press, ctx)
    }

    #[test]
    fn settings_open_from_the_pinned_row_and_return_to_the_usb_list() {
        let libraries = state(vec![gig()]);
        let settings = Settings::new();
        let ctx = context(&libraries, &settings, PlaySnap::idle());
        let mut browser = Browser::new(1.0, &libraries);
        assert_eq!(browser.frame(&ctx).pinned.text, "Settings");
        press_pinned(&mut browser, &ctx);
        let frame = browser.frame(&ctx);
        assert_eq!(frame.pinned.text, back("USB"));
        assert_eq!(
            frame
                .items
                .iter()
                .map(|item| item.text.as_str())
                .collect::<Vec<_>>(),
            vec!["Bluetooth", "Volume limit", "About", "Licences"]
        );
        press_pinned(&mut browser, &ctx);
        assert_eq!(browser.frame(&ctx).pinned.text, "Settings");
    }

    #[test]
    fn an_export_drills_into_playlists_and_tracks() {
        let libraries = state(vec![gig()]);
        let settings = Settings::new();
        let ctx = context(&libraries, &settings, PlaySnap::idle());
        let mut browser = Browser::new(1.0, &libraries);

        press_text(&mut browser, &ctx, "Device Library");
        let root = browser.frame(&ctx);
        assert_eq!(root.pinned.text, back("USB"));
        assert_eq!(root.caption.as_deref(), Some("GIGS · Device Library"));
        assert_eq!(root.items[0].text, "All tracks");

        press_text(&mut browser, &ctx, "House");
        assert_eq!(browser.frame(&ctx).pinned.text, back("GIGS"));
        assert_eq!(browser.frame(&ctx).caption.as_deref(), Some("House"));

        press_text(&mut browser, &ctx, "Late");
        let tracks = browser.frame(&ctx);
        assert_eq!(tracks.pinned.text, back("House"));
        assert_eq!(tracks.caption.as_deref(), Some("Late"));
        assert_eq!(tracks.items[0].text, "Artist – Two");
        assert_eq!(tracks.items[1].text, "Artist – One");
        assert_eq!(tracks.items[0].detail.as_deref(), Some("128  8A  3:00"));

        let effect = press_text(&mut browser, &ctx, "Artist – Two");
        assert!(matches!(
            effect,
            Effect::Play {
                index: 0,
                playlist_id: Some(ref id),
                ..
            } if id == "late"
        ));
        browser.confirm_play(&ctx);
        assert_eq!(browser.frame(&ctx).pinned.text, back("Tracks"));
        assert!(matches!(browser.frame(&ctx).mast, Mast::NowPlaying));
    }

    #[test]
    fn a_playlist_with_children_offers_its_tracks_and_the_children() {
        let libraries = state(vec![gig()]);
        let settings = Settings::new();
        let ctx = context(&libraries, &settings, PlaySnap::idle());
        let mut browser = Browser::new(1.0, &libraries);
        press_text(&mut browser, &ctx, "Device Library");
        press_text(&mut browser, &ctx, "Techno");
        let frame = browser.frame(&ctx);
        assert_eq!(frame.items[0].text, "Tracks");
        assert_eq!(frame.items[1].text, "Peak");
        press_text(&mut browser, &ctx, "Tracks");
        assert_eq!(browser.frame(&ctx).pinned.text, back("Techno"));
        assert_eq!(browser.frame(&ctx).items[0].text, "Artist – One");
    }

    #[test]
    fn a_saved_order_is_the_track_order() {
        let libraries = state(vec![gig()]);
        let mut settings = Settings::new();
        settings.set_order("rekordbox-1/late", vec!["1".to_string(), "2".to_string()]);
        let ctx = context(&libraries, &settings, PlaySnap::idle());
        let mut browser = Browser::new(1.0, &libraries);
        press_text(&mut browser, &ctx, "Device Library");
        press_text(&mut browser, &ctx, "House");
        press_text(&mut browser, &ctx, "Late");
        let frame = browser.frame(&ctx);
        assert_eq!(frame.items[0].text, "Artist – One");
        assert_eq!(frame.items[1].text, "Artist – Two");
    }

    #[test]
    fn the_platter_changes_volume_and_the_top_row_keeps_it() {
        let libraries = state(Vec::new());
        let settings = Settings::new();
        let playing = PlaySnap {
            loaded: true,
            running: true,
            ended: false,
        };
        let ctx = context(&libraries, &settings, playing);
        let mut browser = Browser::new(1.0, &libraries);
        press_text(&mut browser, &ctx, "Now playing");
        assert_eq!(browser.frame(&ctx).items[0].text, "Pause");
        press_text(&mut browser, &ctx, "Volume");
        assert_eq!(browser.cursor(), 0);
        let effect = browser.apply(Control::Turn(-1), &ctx);
        assert!(matches!(effect, Effect::Volume(level) if (level - 0.98).abs() < 0.001));
        assert_eq!(browser.cursor(), 0);
        let Mast::Level(readout) = browser.frame(&ctx).mast else {
            panic!("volume screen has a readout");
        };
        assert_eq!(readout.percent, 98);
        press_pinned(&mut browser, &ctx);
        assert_eq!(browser.frame(&ctx).items[1].detail.as_deref(), Some("98%"));
    }

    #[test]
    fn the_volume_limit_caps_the_level_and_lowers_a_louder_volume() {
        let libraries = state(Vec::new());
        let settings = Settings::new();
        let playing = PlaySnap {
            loaded: true,
            running: false,
            ended: false,
        };
        let ctx = context(&libraries, &settings, playing);
        let mut browser = Browser::new(1.0, &libraries);
        press_text(&mut browser, &ctx, "Now playing");
        press_text(&mut browser, &ctx, "Volume");
        let effect = browser.apply(Control::Turn(-25), &ctx);
        assert!(matches!(effect, Effect::Volume(level) if (level - 0.50).abs() < 0.001));
        press_pinned(&mut browser, &ctx);
        press_pinned(&mut browser, &ctx);
        assert_eq!(browser.frame(&ctx).pinned.text, "Settings");
        press_pinned(&mut browser, &ctx);
        press_text(&mut browser, &ctx, "Volume limit");
        let effect = browser.apply(Control::Turn(-1), &ctx);
        match effect {
            Effect::Limit { limit, volume } => {
                assert!((limit - 0.98).abs() < 0.001);
                assert!((volume - 0.50).abs() < 0.001);
            }
            other => panic!("expected a limit change, got {other:?}"),
        }
        let effect = browser.apply(Control::Turn(-30), &ctx);
        match effect {
            Effect::Limit { limit, volume } => {
                assert!((limit - 0.38).abs() < 0.001, "{limit}");
                assert!((volume - 0.38).abs() < 0.001, "{volume}");
            }
            other => panic!("expected the cap to pull the volume down, got {other:?}"),
        }
        let effect = browser.apply(Control::Turn(5), &ctx);
        match effect {
            Effect::Limit { limit, volume } => {
                assert!((limit - 0.48).abs() < 0.001, "{limit}");
                assert!((volume - 0.38).abs() < 0.001, "{volume}");
            }
            other => panic!("raising the limit leaves the volume, got {other:?}"),
        }
    }

    #[test]
    fn play_pauses_and_a_finished_queue_restarts() {
        let libraries = state(Vec::new());
        let settings = Settings::new();
        let mut browser = Browser::new(1.0, &libraries);
        let running = PlaySnap {
            loaded: true,
            running: true,
            ended: false,
        };
        let ctx = context(&libraries, &settings, running);
        press_text(&mut browser, &ctx, "Now playing");
        assert!(matches!(
            press_text(&mut browser, &ctx, "Pause"),
            Effect::Toggle
        ));
        let ended = PlaySnap {
            loaded: true,
            running: false,
            ended: true,
        };
        let ctx = context(&libraries, &settings, ended);
        assert_eq!(browser.frame(&ctx).items[0].text, "Play");
        assert!(matches!(
            press_text(&mut browser, &ctx, "Play"),
            Effect::Restart
        ));
    }

    #[test]
    fn a_failed_export_does_not_open() {
        let libraries = LibraryState {
            libraries: Vec::new(),
            errors: vec![LibraryLoadError {
                volume_path: "E:\\".to_string(),
                volume_name: "GIGS".to_string(),
                source: LibrarySource::Serato,
                message: "could not read the crates".to_string(),
            }],
        };
        let settings = Settings::new();
        let ctx = context(&libraries, &settings, PlaySnap::idle());
        let mut browser = Browser::new(1.0, &libraries);
        let frame = browser.frame(&ctx);
        let row = frame
            .items
            .iter()
            .find(|item| item.text == "Serato")
            .unwrap();
        assert_eq!(row.tone, super::Tone::Danger);
        press_text(&mut browser, &ctx, "Serato");
        assert_eq!(browser.frame(&ctx).pinned.text, "Settings");
    }

    #[test]
    fn a_removed_usb_returns_to_the_usb_list() {
        let mounted = state(vec![gig()]);
        let settings = Settings::new();
        let ctx = context(&mounted, &settings, PlaySnap::idle());
        let mut browser = Browser::new(1.0, &mounted);
        press_text(&mut browser, &ctx, "Device Library");
        press_text(&mut browser, &ctx, "House");
        let empty = state(Vec::new());
        let ctx = context(&empty, &settings, PlaySnap::idle());
        browser.refresh(&ctx);
        assert_eq!(browser.frame(&ctx).pinned.text, "Settings");
        assert_eq!(browser.frame(&ctx).items[0].text, crate::home::EMPTY_HOME);
    }

    #[test]
    #[test]
    fn the_playing_line_is_the_last_stop_and_opens_now_playing() {
        let libraries = state(vec![gig()]);
        let settings = Settings::new();
        let idle = context(&libraries, &settings, PlaySnap::idle());
        let mut browser = Browser::new(1.0, &libraries);
        press_text(&mut browser, &idle, "Device Library");
        press_text(&mut browser, &idle, "House");
        press_text(&mut browser, &idle, "Late");
        press_text(&mut browser, &idle, "Artist – Two");
        browser.confirm_play(&idle);
        press_pinned(&mut browser, &idle);
        let loaded = PlaySnap {
            loaded: true,
            running: true,
            ended: false,
        };
        let mut ctx = context(&libraries, &settings, loaded);
        ctx.status = "Playing · Artist – Two · 0:10 / 3:00";
        assert_eq!(browser.frame(&ctx).pinned.text, back("House"));

        let count = browser.frame(&ctx).items.len();
        let cursor = i32::try_from(browser.cursor()).unwrap();
        let last = i32::try_from(count).unwrap();
        let _ = browser.apply(Control::Turn(last - cursor), &ctx);
        assert_eq!(browser.cursor(), count);
        assert!(!browser.footer_selected(&ctx));
        let _ = browser.apply(Control::Turn(1), &ctx);
        assert!(browser.footer_selected(&ctx));
        let _ = browser.apply(Control::Turn(3), &ctx);
        assert!(browser.footer_selected(&ctx));
        assert_eq!(browser.apply(Control::Press, &ctx), Effect::None);
        assert_eq!(browser.frame(&ctx).pinned.text, back("Tracks"));
        assert!(!browser.footer_selected(&ctx));
    }

    #[test]
    fn the_playing_line_opens_now_playing_from_settings() {
        let libraries = state(vec![gig()]);
        let settings = Settings::new();
        let mut ctx = context(
            &libraries,
            &settings,
            PlaySnap {
                loaded: true,
                running: true,
                ended: false,
            },
        );
        ctx.status = "Paused · Artist – One · 1:00 / 4:00";
        let mut browser = Browser::new(1.0, &libraries);
        press_pinned(&mut browser, &ctx);
        assert_eq!(browser.frame(&ctx).pinned.text, back("USB"));
        let count = browser.frame(&ctx).items.len();
        let _ = browser.apply(Control::Turn(i32::try_from(count + 1).unwrap()), &ctx);
        assert!(browser.footer_selected(&ctx));
        let _ = browser.apply(Control::Press, &ctx);
        assert_eq!(browser.frame(&ctx).pinned.text, back("USB"));
        assert_eq!(browser.frame(&ctx).items[0].text, "Pause");
    }

    #[test]
    fn a_level_screen_keeps_the_playing_line_as_information() {
        let libraries = state(Vec::new());
        let settings = Settings::new();
        let mut ctx = context(
            &libraries,
            &settings,
            PlaySnap {
                loaded: true,
                running: true,
                ended: false,
            },
        );
        ctx.status = "Playing · Artist – One · 0:01 / 3:00";
        let mut browser = Browser::new(1.0, &libraries);
        press_text(&mut browser, &ctx, "Now playing");
        press_text(&mut browser, &ctx, "Volume");
        assert_eq!(browser.cursor(), 0);
        let effect = browser.apply(Control::Turn(-1), &ctx);
        assert!(matches!(effect, Effect::Volume(_)));
        assert_eq!(browser.cursor(), 0);
        assert!(!browser.footer_selected(&ctx));
    }

    #[test]
    fn now_playing_is_a_row_on_the_usb_list_once_a_queue_is_loaded() {
        let libraries = state(vec![gig()]);
        let settings = Settings::new();
        let ctx = context(&libraries, &settings, PlaySnap::idle());
        let mut browser = Browser::new(1.0, &libraries);
        press_text(&mut browser, &ctx, "Device Library");
        press_text(&mut browser, &ctx, "All tracks");
        let effect = press_text(&mut browser, &ctx, "Artist – One");
        assert!(matches!(
            effect,
            Effect::Play {
                playlist_id: None,
                index: 0,
                ..
            }
        ));
        browser.confirm_play(&ctx);
        let loaded = PlaySnap {
            loaded: true,
            running: true,
            ended: false,
        };
        let ctx = context(&libraries, &settings, loaded);
        press_pinned(&mut browser, &ctx);
        assert_eq!(browser.frame(&ctx).pinned.text, back("GIGS"));
        press_pinned(&mut browser, &ctx);
        assert_eq!(browser.frame(&ctx).pinned.text, back("USB"));
        press_pinned(&mut browser, &ctx);
        assert_eq!(browser.frame(&ctx).items[0].text, "Now playing");
        press_text(&mut browser, &ctx, "Now playing");
        assert_eq!(browser.frame(&ctx).pinned.text, back("Tracks"));
        press_pinned(&mut browser, &ctx);
        assert_eq!(browser.frame(&ctx).caption.as_deref(), Some("All tracks"));
    }

    #[test]
    fn every_screen_leaves_through_its_top_row() {
        let libraries = state(vec![gig()]);
        let settings = Settings::new();
        let ctx = context(&libraries, &settings, PlaySnap::idle());
        let mut browser = Browser::new(1.0, &libraries);

        press_pinned(&mut browser, &ctx);
        for label in ["Bluetooth", "Volume limit", "About", "Licences"] {
            press_text(&mut browser, &ctx, label);
            let frame = browser.frame(&ctx);
            assert_eq!(frame.pinned.text, back("Settings"), "{label}");
            if label == "About" {
                assert!(
                    matches!(frame.mast, Mast::Notice(ref lines) if lines.iter().any(|line| line.starts_with("Version")))
                );
            }
            if label == "Bluetooth" {
                assert!(
                    matches!(frame.mast, Mast::Notice(ref lines) if lines.iter().any(|line| line.contains("never goes online")))
                );
            }
            if label == "Licences" {
                assert!(frame.items.iter().any(|item| item.text == "symphonia"));
                assert!(frame.items.iter().any(|item| item.text == "VT323"));
            }
            press_pinned(&mut browser, &ctx);
            assert_eq!(browser.frame(&ctx).pinned.text, back("USB"));
        }
        press_pinned(&mut browser, &ctx);
        assert_eq!(browser.frame(&ctx).pinned.text, "Settings");

        press_text(&mut browser, &ctx, "Device Library");
        press_text(&mut browser, &ctx, "House");
        press_text(&mut browser, &ctx, "Late");
        press_text(&mut browser, &ctx, "Artist – Two");
        browser.confirm_play(&ctx);
        press_text(&mut browser, &ctx, "Visualizer");
        press_text(&mut browser, &ctx, "Waveform");
        assert!(matches!(browser.frame(&ctx).mast, Mast::Waveform));
        press_pinned(&mut browser, &ctx);
        press_text(&mut browser, &ctx, "Equalizer");
        assert!(matches!(
            browser.frame(&ctx).mast,
            Mast::Equalizer(crate::visualizer::EqualizerTheme::Mirror)
        ));
        press_pinned(&mut browser, &ctx);
        press_text(&mut browser, &ctx, "Deck");
        assert!(matches!(browser.frame(&ctx).mast, Mast::Deck));
        press_pinned(&mut browser, &ctx);
        press_text(&mut browser, &ctx, "MilkDrop");
        assert_eq!(browser.frame(&ctx).pinned.text, back("Visualizer"));
        press_pinned(&mut browser, &ctx);
        assert_eq!(browser.frame(&ctx).pinned.text, back("Now playing"));
        press_pinned(&mut browser, &ctx);
        assert_eq!(browser.frame(&ctx).pinned.text, back("Tracks"));
        press_pinned(&mut browser, &ctx);
        assert_eq!(browser.frame(&ctx).pinned.text, back("House"));
        press_pinned(&mut browser, &ctx);
        assert_eq!(browser.frame(&ctx).pinned.text, back("GIGS"));
        press_pinned(&mut browser, &ctx);
        assert_eq!(browser.frame(&ctx).pinned.text, back("USB"));
        press_pinned(&mut browser, &ctx);
        assert_eq!(browser.frame(&ctx).pinned.text, "Settings");
    }

    fn playing() -> PlaySnap {
        PlaySnap {
            loaded: true,
            running: true,
            ended: false,
        }
    }

    fn open_visualizers(browser: &mut Browser, ctx: &Ctx<'_>) {
        press_text(browser, ctx, "Device Library");
        press_text(browser, ctx, "House");
        press_text(browser, ctx, "Late");
        press_text(browser, ctx, "Artist – Two");
        browser.confirm_play(ctx);
        press_text(browser, ctx, "Visualizer");
    }

    #[test]
    fn the_only_preset_pack_opens_immediately() {
        let libraries = state(vec![gig()]);
        let settings = Settings::new();
        let mut browser = Browser::new(settings.volume_limit(), &libraries);
        let packs = [Pack {
            name: "Built-in".to_string(),
            path: PathBuf::from("built-in"),
        }];
        let mut ctx = context(&libraries, &settings, playing());
        ctx.packs = &packs;
        open_visualizers(&mut browser, &ctx);
        press_text(&mut browser, &ctx, "MilkDrop");
        assert_eq!(
            browser.frame(&ctx).mast,
            Mast::MilkDrop(PathBuf::from("built-in"))
        );
        assert_eq!(browser.frame(&ctx).pinned.text, back("MilkDrop"));
        press_pinned(&mut browser, &ctx);
        assert_eq!(browser.frame(&ctx).items[0].text, "Built-in");
    }

    #[test]
    fn milkdrop_lists_packs_and_opens_full_screen() {
        let libraries = state(vec![gig()]);
        let settings = Settings::new();
        let mut browser = Browser::new(settings.volume_limit(), &libraries);
        let packs = [
            Pack {
                name: "Built-in".to_string(),
                path: PathBuf::from("built-in"),
            },
            Pack {
                name: "Cream".to_string(),
                path: PathBuf::from("cream"),
            },
        ];
        let mut ctx = context(&libraries, &settings, playing());
        ctx.packs = &packs;
        open_visualizers(&mut browser, &ctx);
        press_text(&mut browser, &ctx, "MilkDrop");
        assert_eq!(browser.frame(&ctx).items[0].text, "Built-in");
        assert_eq!(browser.frame(&ctx).items[1].text, "Cream");
        assert_eq!(browser.frame(&ctx).caption.as_deref(), Some("Preset pack"));
        press_text(&mut browser, &ctx, "Cream");
        assert_eq!(
            browser.frame(&ctx).mast,
            Mast::MilkDrop(PathBuf::from("cream"))
        );
        assert_eq!(browser.frame(&ctx).pinned.text, back("MilkDrop"));
        assert_eq!(browser.frame(&ctx).items[0].text, "Full screen");
        assert_eq!(browser.frame(&ctx).items.len(), 1);
        press_pinned(&mut browser, &ctx);
        assert_eq!(browser.frame(&ctx).items[0].text, "Built-in");
        assert_eq!(browser.frame(&ctx).items[1].text, "Cream");
    }

    #[test]
    fn full_screen_hides_the_menu_until_the_platter_moves() {
        let libraries = state(vec![gig()]);
        let settings = Settings::new();
        let mut browser = Browser::new(settings.volume_limit(), &libraries);
        let ctx = context(&libraries, &settings, playing());
        open_visualizers(&mut browser, &ctx);
        for label in ["Waveform", "Equalizer", "Deck"] {
            press_text(&mut browser, &ctx, label);
            assert_eq!(browser.frame(&ctx).items[0].text, "Full screen");
            assert!(!browser.frame(&ctx).fullscreen);
            press_text(&mut browser, &ctx, "Full screen");
            assert!(browser.frame(&ctx).fullscreen);
            let _ = browser.apply(Control::Press, &ctx);
            assert!(browser.frame(&ctx).fullscreen);
            let cursor = browser.cursor();
            let _ = browser.apply(Control::Turn(4), &ctx);
            assert!(!browser.frame(&ctx).fullscreen);
            assert_eq!(browser.cursor(), cursor);
            press_pinned(&mut browser, &ctx);
        }
    }

    #[test]
    fn the_equalizer_theme_cycles_from_its_row() {
        use crate::visualizer::EqualizerTheme;

        let libraries = state(vec![gig()]);
        let settings = Settings::new();
        let mut browser = Browser::new(settings.volume_limit(), &libraries);
        let ctx = context(&libraries, &settings, playing());
        open_visualizers(&mut browser, &ctx);
        press_text(&mut browser, &ctx, "Waveform");
        assert!(browser
            .frame(&ctx)
            .items
            .iter()
            .all(|item| item.text != "Theme"));
        press_pinned(&mut browser, &ctx);
        press_text(&mut browser, &ctx, "Equalizer");
        assert_eq!(browser.frame(&ctx).items[1].text, "Theme");
        assert_eq!(
            browser.frame(&ctx).items[1].detail.as_deref(),
            Some("Mirror")
        );
        for theme in [
            EqualizerTheme::Led,
            EqualizerTheme::Fire,
            EqualizerTheme::Mirror,
        ] {
            press_text(&mut browser, &ctx, "Theme");
            assert_eq!(
                browser.frame(&ctx).items[1].detail.as_deref(),
                Some(theme.label())
            );
            assert_eq!(browser.frame(&ctx).mast, Mast::Equalizer(theme));
        }
        press_pinned(&mut browser, &ctx);
        press_text(&mut browser, &ctx, "Deck");
        assert!(browser
            .frame(&ctx)
            .items
            .iter()
            .all(|item| item.text != "Theme"));
    }

    #[test]
    fn milkdrop_with_no_packs_says_where_to_put_them() {
        let libraries = state(vec![gig()]);
        let settings = Settings::new();
        let mut browser = Browser::new(settings.volume_limit(), &libraries);
        let ctx = context(&libraries, &settings, playing());
        open_visualizers(&mut browser, &ctx);
        press_text(&mut browser, &ctx, "MilkDrop");
        let frame = browser.frame(&ctx);
        let Mast::Notice(lines) = &frame.mast else {
            panic!("expected a notice");
        };
        assert!(lines.iter().any(|line| line.contains(".milk")));
        assert_eq!(frame.items[0].text, "No preset packs");
    }

    #[test]
    fn a_missing_pack_returns_to_the_list() {
        let libraries = state(vec![gig()]);
        let settings = Settings::new();
        let mut browser = Browser::new(settings.volume_limit(), &libraries);
        let packs = [Pack {
            name: "Cream".to_string(),
            path: PathBuf::from("cream"),
        }];
        let mut ctx = context(&libraries, &settings, playing());
        ctx.packs = &packs;
        open_visualizers(&mut browser, &ctx);
        press_text(&mut browser, &ctx, "MilkDrop");
        ctx.packs = &[];
        browser.refresh(&ctx);
        let frame = browser.frame(&ctx);
        assert!(matches!(frame.mast, Mast::Notice(_)));
        assert_eq!(frame.pinned.text, back("Visualizer"));
    }

    #[test]
    fn projectm_licence_leaves_through_licences() {
        let libraries = state(vec![gig()]);
        let settings = Settings::new();
        let mut browser = Browser::new(settings.volume_limit(), &libraries);
        let ctx = context(&libraries, &settings, PlaySnap::idle());
        press_pinned(&mut browser, &ctx);
        press_text(&mut browser, &ctx, "Licences");
        press_text(&mut browser, &ctx, "projectM");
        let frame = browser.frame(&ctx);
        let Mast::Notice(lines) = &frame.mast else {
            panic!("expected a notice");
        };
        let notice = lines.join("\n");
        assert!(notice.contains("shared library"));
        assert!(notice.contains("three years"));
        assert_eq!(frame.pinned.text, back("Licences"));
        press_pinned(&mut browser, &ctx);
        assert_eq!(browser.frame(&ctx).pinned.text, back("Settings"));
    }
}
