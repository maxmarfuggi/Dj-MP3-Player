//! Settings saved off the USB, in the contract's JSON object.
//!
//! A missing file is the default. Unknown fields are kept, so a newer version
//! of the file still loads. The volume is never written.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

/// Custom playlist orders and the other saved preferences.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    orders: BTreeMap<String, Vec<String>>,
    energy_flow_open: bool,
    /// Fields this version does not use, kept for the next save.
    extra: Map<String, Value>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            orders: BTreeMap::new(),
            energy_flow_open: false,
            extra: Map::new(),
        }
    }
}

impl Settings {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Reads the file. A missing file is the default settings.
    pub fn load(path: &Path) -> Result<Self, SettingsError> {
        match fs::read_to_string(path) {
            Ok(text) => Self::from_json(&text),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Self::new()),
            Err(error) => Err(SettingsError::Io(error)),
        }
    }

    /// Writes the file, creating the directory if it is missing.
    pub fn save(&self, path: &Path) -> Result<(), SettingsError> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        write_replacing(path, self.to_json().as_bytes())?;
        Ok(())
    }

    /// Reads one JSON object. Anything else is an error, so a broken file is not replaced.
    pub fn from_json(text: &str) -> Result<Self, SettingsError> {
        let value: Value =
            serde_json::from_str(text).map_err(|error| SettingsError::Format(error.to_string()))?;
        let Value::Object(mut map) = value else {
            return Err(SettingsError::Format(
                "settings are not a JSON object".to_string(),
            ));
        };
        let energy_flow_open = matches!(map.remove("energyFlowOpen"), Some(Value::Bool(true)));
        let orders = match map.remove("customPlaylistOrders") {
            Some(Value::Object(entries)) => parse_orders(&entries),
            _ => BTreeMap::new(),
        };
        Ok(Self {
            orders,
            energy_flow_open,
            extra: map,
        })
    }

    #[must_use]
    pub fn to_json(&self) -> String {
        let mut orders = Map::new();
        for (key, ids) in &self.orders {
            orders.insert(
                key.clone(),
                Value::Array(ids.iter().cloned().map(Value::String).collect()),
            );
        }
        let mut map = Map::new();
        map.insert("customPlaylistOrders".to_string(), Value::Object(orders));
        map.insert(
            "energyFlowOpen".to_string(),
            Value::Bool(self.energy_flow_open),
        );
        for (key, value) in &self.extra {
            map.insert(key.clone(), value.clone());
        }
        let mut text =
            serde_json::to_string_pretty(&Value::Object(map)).unwrap_or_else(|_| "{}".to_string());
        text.push('\n');
        text
    }

    #[must_use]
    pub fn orders(&self) -> &BTreeMap<String, Vec<String>> {
        &self.orders
    }

    #[must_use]
    pub fn order(&self, key: &str) -> Option<&[String]> {
        self.orders.get(key).map(Vec::as_slice)
    }

    pub fn set_order(&mut self, key: impl Into<String>, track_ids: Vec<String>) {
        self.orders.insert(key.into(), track_ids);
    }

    pub fn reset_order(&mut self, key: &str) {
        self.orders.remove(key);
    }

    #[must_use]
    pub fn energy_flow_open(&self) -> bool {
        self.energy_flow_open
    }

    pub fn set_energy_flow_open(&mut self, open: bool) {
        self.energy_flow_open = open;
    }
}

#[derive(Debug)]
pub enum SettingsError {
    Io(std::io::Error),
    Format(String),
}

impl std::fmt::Display for SettingsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::Format(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for SettingsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Format(_) => None,
        }
    }
}

/// Where the device keeps `settings.json`.
#[must_use]
pub fn settings_path() -> PathBuf {
    config_dir(
        current_host(),
        std::env::var("XDG_CONFIG_HOME").ok().as_deref(),
        std::env::var("APPDATA").ok().as_deref(),
        home_dir().as_deref(),
    )
    .join("djusb")
    .join("settings.json")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Host {
    Windows,
    Mac,
    Other,
}

fn parse_orders(entries: &Map<String, Value>) -> BTreeMap<String, Vec<String>> {
    let mut orders = BTreeMap::new();
    for (key, value) in entries {
        let Value::Array(values) = value else {
            continue;
        };
        let mut ids = Vec::with_capacity(values.len());
        let mut strings = true;
        for value in values {
            match value {
                Value::String(id) => ids.push(id.clone()),
                _ => strings = false,
            }
        }
        if strings {
            orders.insert(key.clone(), ids);
        }
    }
    orders
}

/// Replaces `path`. Windows cannot rename a file onto one that already exists.
fn write_replacing(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut temporary = path.as_os_str().to_os_string();
    temporary.push(".tmp");
    let temporary = PathBuf::from(temporary);
    fs::write(&temporary, bytes)?;
    match fs::rename(&temporary, path) {
        Ok(()) => Ok(()),
        Err(_) if path.exists() => {
            fs::remove_file(path)?;
            fs::rename(&temporary, path).inspect_err(|_| {
                let _ = fs::remove_file(&temporary);
            })
        }
        Err(error) => {
            let _ = fs::remove_file(&temporary);
            Err(error)
        }
    }
}

fn current_host() -> Host {
    if cfg!(windows) {
        Host::Windows
    } else if cfg!(target_os = "macos") {
        Host::Mac
    } else {
        Host::Other
    }
}

fn home_dir() -> Option<PathBuf> {
    let name = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(name).map(PathBuf::from)
}

fn config_dir(
    host: Host,
    xdg_config_home: Option<&str>,
    app_data: Option<&str>,
    home: Option<&Path>,
) -> PathBuf {
    if let Some(xdg) = xdg_config_home
        .map(str::trim)
        .filter(|path| !path.is_empty())
    {
        return PathBuf::from(xdg);
    }
    match host {
        Host::Windows => {
            if let Some(app_data) = app_data.map(str::trim).filter(|path| !path.is_empty()) {
                return PathBuf::from(app_data);
            }
            home.map_or_else(
                || PathBuf::from("."),
                |path| path.join("AppData").join("Roaming"),
            )
        }
        Host::Mac => home.map_or_else(
            || PathBuf::from("."),
            |path| path.join("Library").join("Application Support"),
        ),
        Host::Other => home.map_or_else(|| PathBuf::from("."), |path| path.join(".config")),
    }
}

impl From<io::Error> for SettingsError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::{config_dir, Host, Settings, SettingsError};

    fn scratch() -> PathBuf {
        static N: AtomicU64 = AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("djusb-settings-{}-{n}.json", std::process::id()))
    }

    #[test]
    fn the_contract_object_round_trips() {
        let text = r#"{
  "customPlaylistOrders": {
    "rekordbox-857074f2fb38/2": ["7", "8", "1"]
  },
  "energyFlowOpen": false
}
"#;
        let settings = Settings::from_json(text).unwrap();
        assert_eq!(
            settings.order("rekordbox-857074f2fb38/2"),
            Some(["7".to_string(), "8".to_string(), "1".to_string()].as_slice())
        );
        assert!(!settings.energy_flow_open());
        let again = Settings::from_json(&settings.to_json()).unwrap();
        assert_eq!(again, settings);
    }

    #[test]
    fn only_true_opens_the_energy_flow_chart() {
        let open = Settings::from_json(r#"{"energyFlowOpen": true}"#).unwrap();
        assert!(open.energy_flow_open());
        for text in [
            r#"{}"#,
            r#"{"energyFlowOpen": false}"#,
            r#"{"energyFlowOpen": "true"}"#,
            r#"{"energyFlowOpen": 1}"#,
            r#"{"energyFlowOpen": null}"#,
        ] {
            assert!(
                !Settings::from_json(text).unwrap().energy_flow_open(),
                "{text}"
            );
        }
    }

    #[test]
    fn a_missing_or_unusable_order_map_means_there_are_no_orders() {
        for text in [
            r#"{}"#,
            r#"{"customPlaylistOrders": null}"#,
            r#"{"customPlaylistOrders": ["7"]}"#,
            r#"{"customPlaylistOrders": "rekordbox-1/2"}"#,
        ] {
            assert!(
                Settings::from_json(text).unwrap().orders().is_empty(),
                "{text}"
            );
        }
    }

    #[test]
    fn an_order_that_is_not_an_array_of_strings_is_ignored() {
        let settings = Settings::from_json(
            r#"{
                "customPlaylistOrders": {
                    "lib/bad": ["7", 8],
                    "lib/empty": [],
                    "lib/kept": ["3", "3", "1"],
                    "lib/number": 4,
                    "lib/nested": ["1", ["2"]]
                }
            }"#,
        )
        .unwrap();
        assert_eq!(settings.orders().len(), 2);
        assert_eq!(
            settings.order("lib/kept"),
            Some(["3".to_string(), "3".to_string(), "1".to_string()].as_slice())
        );
        assert_eq!(settings.order("lib/empty"), Some([].as_slice()));
    }

    #[test]
    fn unknown_fields_are_kept() {
        let mut settings = Settings::from_json(
            r#"{"volumeLimit": 0.8, "energyFlowOpen": true, "customPlaylistOrders": {}, "name": "wall"}"#,
        )
        .unwrap();
        settings.set_energy_flow_open(false);
        let text = settings.to_json();
        assert!(text.contains("\"volumeLimit\": 0.8"), "{text}");
        assert!(text.contains("\"name\": \"wall\""), "{text}");
        let again = Settings::from_json(&text).unwrap();
        assert_eq!(again.to_json(), text);
    }

    #[test]
    fn a_file_that_is_not_an_object_is_left_unread() {
        let error = Settings::from_json("[1, 2]").unwrap_err();
        assert!(matches!(error, SettingsError::Format(_)));
        assert!(!Settings::from_json("").unwrap_err().to_string().is_empty());
    }

    #[test]
    fn saving_creates_the_file_and_replaces_it() {
        let path = scratch();
        let _ = fs::remove_file(&path);
        let mut settings = Settings::new();
        settings.set_order("rekordbox-1/2", vec!["8".to_string(), "1".to_string()]);
        settings.set_energy_flow_open(true);
        settings.save(&path).unwrap();
        let mut loaded = Settings::load(&path).unwrap();
        assert_eq!(
            loaded.order("rekordbox-1/2"),
            Some(["8".to_string(), "1".to_string()].as_slice())
        );
        assert!(loaded.energy_flow_open());

        loaded.reset_order("rekordbox-1/2");
        loaded.save(&path).unwrap();
        let cleared = Settings::load(&path).unwrap();
        assert!(cleared.order("rekordbox-1/2").is_none());
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn a_missing_file_is_the_default() {
        let path = scratch();
        let _ = fs::remove_file(&path);
        let settings = Settings::load(&path).unwrap();
        assert!(settings.orders().is_empty());
        assert!(!settings.energy_flow_open());
    }

    #[test]
    fn save_creates_missing_directories() {
        let path = scratch();
        let nested = path
            .with_extension("dir")
            .join("djusb")
            .join("settings.json");
        let _ = fs::remove_dir_all(path.with_extension("dir"));
        Settings::new().save(&nested).unwrap();
        assert!(nested.is_file());
        let _ = fs::remove_dir_all(path.with_extension("dir"));
    }

    #[test]
    fn the_config_directory_follows_the_host() {
        let home = PathBuf::from("/home/dj");
        assert_eq!(
            config_dir(Host::Other, None, None, Some(&home)),
            PathBuf::from("/home/dj/.config")
        );
        assert_eq!(
            config_dir(Host::Other, Some("/var/config"), None, Some(&home)),
            PathBuf::from("/var/config")
        );
        assert_eq!(
            config_dir(Host::Other, Some("  "), None, Some(&home)),
            PathBuf::from("/home/dj/.config")
        );
        assert_eq!(
            config_dir(Host::Mac, None, None, Some(&home)),
            home.join("Library").join("Application Support")
        );
        assert_eq!(
            config_dir(
                Host::Windows,
                None,
                Some(r"C:\Users\dj\AppData\Roaming"),
                Some(&home)
            ),
            PathBuf::from(r"C:\Users\dj\AppData\Roaming")
        );
        assert_eq!(
            config_dir(Host::Windows, None, None, Some(&home)),
            home.join("AppData").join("Roaming")
        );
        assert_eq!(
            config_dir(
                Host::Windows,
                Some("/xdg"),
                Some(r"C:\AppData"),
                Some(&home)
            ),
            PathBuf::from("/xdg")
        );
    }
}
