//! The library contract: the values a reader produces, in the same shape the
//! desktop app already uses.

use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use ts_rs::TS;

/// Which export a library was read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum LibrarySource {
    Rekordbox,
    DeviceLibraryPlus,
    Serato,
}

impl LibrarySource {
    /// The `source` string stored on a library and in its id.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rekordbox => "rekordbox",
            Self::DeviceLibraryPlus => "deviceLibraryPlus",
            Self::Serato => "serato",
        }
    }
}

/// A Device Library Plus track colour. `value` is a CSS colour.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TrackColor {
    pub name: String,
    pub value: String,
}

/// One audio file listed by an export.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub bpm: Option<f64>,
    pub key: Option<String>,
    pub duration_sec: Option<f64>,
    /// Path of the audio file relative to the volume root, using `/` separators.
    pub relative_path: String,
    // Only Device Library Plus provides the fields below. They are left out of
    // the JSON when the export does not have them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub genre: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub color: Option<TrackColor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub artwork_path: Option<String>,
}

/// A playlist tree node. rekordbox folders hold other nodes. Serato crates are
/// always playlists, and a crate can hold both tracks and child crates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum PlaylistKind {
    Folder,
    Playlist,
}

/// One playlist or folder in a library.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistNode {
    pub id: String,
    pub name: String,
    #[ts(inline)]
    pub kind: PlaylistKind,
    pub children: Vec<PlaylistNode>,
    pub track_ids: Vec<String>,
}

/// One export on a volume.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Library {
    pub id: String,
    pub source: LibrarySource,
    /// Display name of the volume, for example the USB stick's label.
    pub volume_name: String,
    pub volume_path: String,
    pub tracks: Vec<Track>,
    pub playlists: Vec<PlaylistNode>,
}

/// An export was present but could not be read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LibraryLoadError {
    pub volume_path: String,
    pub volume_name: String,
    pub source: LibrarySource,
    pub message: String,
}

/// Libraries currently loaded, and the exports that failed to read.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LibraryState {
    pub libraries: Vec<Library>,
    pub errors: Vec<LibraryLoadError>,
}

/// Result of the user picking a folder that might contain a library.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum OpenFolderResult {
    Opened,
    Cancelled,
    #[serde(rename = "no-library", rename_all = "camelCase")]
    NoLibrary {
        folder_path: String,
    },
}

/// `source` plus the first 12 lowercase hex digits of SHA-1(`volume_path`).
///
/// The path is hashed as UTF-8 with no trimming and no change of slash or case.
/// rekordbox on `/Volumes/GIGS` is `rekordbox-857074f2fb38`.
#[must_use]
pub fn library_id(source: LibrarySource, volume_path: &str) -> String {
    let digest = Sha1::digest(volume_path.as_bytes());
    let hex = hex_encode(digest.as_ref());
    format!("{}-{}", source.as_str(), &hex[..12])
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{
        library_id, Library, LibraryLoadError, LibrarySource, LibraryState, OpenFolderResult,
        PlaylistNode, Track, TrackColor,
    };
    use ts_rs::{Config, TS};

    #[test]
    fn library_id_matches_the_contract_example() {
        assert_eq!(
            library_id(LibrarySource::Rekordbox, "/Volumes/GIGS"),
            "rekordbox-857074f2fb38"
        );
        assert_eq!(
            library_id(LibrarySource::DeviceLibraryPlus, "/Volumes/GIGS"),
            "deviceLibraryPlus-857074f2fb38"
        );
        assert_eq!(
            library_id(LibrarySource::Serato, "/Volumes/GIGS"),
            "serato-857074f2fb38"
        );
    }

    #[test]
    fn source_json_uses_the_same_name_as_the_library_id() {
        for source in [
            LibrarySource::Rekordbox,
            LibrarySource::DeviceLibraryPlus,
            LibrarySource::Serato,
        ] {
            assert_eq!(serde_json::to_value(source).unwrap(), source.as_str());
        }
    }

    #[test]
    fn open_folder_result_is_tagged_by_status() {
        assert_eq!(
            serde_json::to_value(OpenFolderResult::Opened).unwrap(),
            serde_json::json!({ "status": "opened" })
        );
        assert_eq!(
            serde_json::to_value(OpenFolderResult::Cancelled).unwrap(),
            serde_json::json!({ "status": "cancelled" })
        );
        assert_eq!(
            serde_json::to_value(OpenFolderResult::NoLibrary {
                folder_path: "D:/copy".into(),
            })
            .unwrap(),
            serde_json::json!({ "status": "no-library", "folderPath": "D:/copy" })
        );
    }

    #[test]
    fn rekordbox_fixture_round_trips_and_omits_device_library_plus_fields() {
        let library = first_library("rekordbox/basic");
        assert_eq!(library.source, LibrarySource::Rekordbox);
        assert_eq!(library.tracks[0].title, "Mädchen");
        assert_eq!(library.tracks[0].bpm, Some(128.5));
        assert!(library.tracks[0].genre.is_none());
        assert_eq!(library.playlists[0].kind, super::PlaylistKind::Folder);

        let json = serde_json::to_value(&library).unwrap();
        assert!(json["tracks"][0].get("genre").is_none());
        assert!(json["tracks"][0].get("artworkPath").is_none());
        let again: Library = serde_json::from_value(json).unwrap();
        assert_eq!(library, again);
    }

    #[test]
    fn device_library_plus_fixture_keeps_optional_fields() {
        let library = first_library("deviceLibraryPlus/basic");
        let opener = &library.tracks[0];
        assert_eq!(opener.genre.as_deref(), Some("House"));
        assert_eq!(
            opener.color.as_ref().map(|color| color.value.as_str()),
            Some("#3fcf5c")
        );
        assert_eq!(
            opener.artwork_path.as_deref(),
            Some("PIONEER/Artwork/00001/a1_m.jpg")
        );
        let untitled = &library.tracks[1];
        assert_eq!(untitled.genre.as_deref(), Some(""));
        assert!(untitled.color.is_none());

        let json = serde_json::to_value(&library).unwrap();
        assert_eq!(json["tracks"][1]["genre"], "");
        assert!(json["tracks"][1].get("color").is_none());
        let again: Library = serde_json::from_value(json).unwrap();
        assert_eq!(library, again);
    }

    #[test]
    fn generated_typescript_matches_types_ts() {
        let source = include_str!("../../../apps/desktop/src/shared/types.ts");
        let exports = parse_exports(source);
        let names: Vec<&str> = exports.iter().map(|(name, _)| name.as_str()).collect();

        // The preload API and the media scheme are desktop-only and are not data.
        let handwritten = ["DjUsbApi", "MEDIA_SCHEME"];
        let generated = rust_declarations();
        let generated_names: Vec<&str> = generated.iter().map(|(name, _)| *name).collect();

        for (name, _) in &exports {
            assert!(
                generated_names.contains(&name.as_str()) || handwritten.contains(&name.as_str()),
                "{name} is exported from types.ts and has no Rust type"
            );
        }
        for (name, _) in &generated {
            assert!(names.contains(name), "{name} is missing from types.ts");
        }

        for (name, rust_decl) in &generated {
            let ts_decl = exports
                .iter()
                .find(|(found, _)| found == name)
                .map(|(_, decl)| decl.as_str())
                .unwrap();
            let ts = canonical_declaration(ts_decl);
            let rust = canonical_declaration(&format!("export {rust_decl}"));
            assert_eq!(ts, rust, "{name} differs from the type ts-rs generates");
        }
    }

    fn first_library(case: &str) -> Library {
        let path = format!(
            "{}/../../fixtures/{case}/expected.json",
            env!("CARGO_MANIFEST_DIR")
        );
        let file = std::fs::read_to_string(path).unwrap();
        let value: serde_json::Value = serde_json::from_str(&file).unwrap();
        serde_json::from_value(value["libraries"][0].clone()).unwrap()
    }

    fn rust_declarations() -> Vec<(&'static str, String)> {
        let cfg = Config::from_env();
        vec![
            ("LibrarySource", LibrarySource::decl(&cfg)),
            ("TrackColor", TrackColor::decl(&cfg)),
            ("Track", Track::decl(&cfg)),
            ("PlaylistNode", PlaylistNode::decl(&cfg)),
            ("Library", Library::decl(&cfg)),
            ("LibraryLoadError", LibraryLoadError::decl(&cfg)),
            ("LibraryState", LibraryState::decl(&cfg)),
            ("OpenFolderResult", OpenFolderResult::decl(&cfg)),
        ]
    }

    /// Pull each `export` out of `types.ts`.
    fn parse_exports(source: &str) -> Vec<(String, String)> {
        let mut exports = Vec::new();
        let mut index = 0;
        while index < source.len() {
            let at_export = source[index..].starts_with("export ")
                && (index == 0 || source.as_bytes()[index - 1] == b'\n');
            if !at_export {
                index += source[index..].chars().next().unwrap().len_utf8();
                continue;
            }
            let after = &source[index + "export ".len()..];
            let (name, end) = end_of_export(after);
            exports.push((
                name,
                source[index..index + "export ".len() + end].to_string(),
            ));
            index += "export ".len() + end;
        }
        exports
    }

    fn end_of_export(after_export: &str) -> (String, usize) {
        let kind_end = after_export.find(char::is_whitespace).unwrap();
        let kind = &after_export[..kind_end];
        let rest = after_export[kind_end..].trim_start();
        let name_end = rest
            .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .unwrap_or(rest.len());
        let name = rest[..name_end].to_string();

        let mut depth = 0i32;
        for (offset, ch) in after_export.char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if kind == "interface" && depth == 0 {
                        return (name, offset + ch.len_utf8());
                    }
                }
                ';' if depth == 0 && kind != "interface" => {
                    return (name, offset + ch.len_utf8());
                }
                _ => {}
            }
        }
        panic!("unterminated export {name}");
    }

    /// Compare type structure, not quote style, comments, or `T[]` versus `Array<T>`.
    fn canonical_declaration(declaration: &str) -> String {
        let stripped = strip_comments(declaration);
        let as_type = interface_to_type_alias(&stripped);
        let mut compact = String::new();
        for ch in as_type.chars() {
            if ch.is_whitespace() || ch == ',' || ch == ';' {
                continue;
            }
            compact.push(if ch == '"' { '\'' } else { ch });
        }
        // A leading `|` on a union is optional in TypeScript.
        unquote_identifiers(&arrays_to_brackets(&compact)).replace("=|", "=")
    }

    fn interface_to_type_alias(declaration: &str) -> String {
        let trimmed = declaration.trim();
        let Some(after_export) = trimmed.strip_prefix("export") else {
            return trimmed.to_string();
        };
        let after_export = after_export.trim_start();
        let Some(after_interface) = after_export.strip_prefix("interface") else {
            return trimmed.to_string();
        };
        let after_interface = after_interface.trim_start();
        let name_end = after_interface
            .find(|c: char| c.is_whitespace() || c == '{' || c == '<')
            .unwrap();
        let name = &after_interface[..name_end];
        let body = &after_interface[after_interface.find('{').unwrap()..];
        format!("export type {name} = {body};")
    }

    fn strip_comments(source: &str) -> String {
        let mut out = String::with_capacity(source.len());
        let mut chars = source.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '/' && chars.peek() == Some(&'/') {
                chars.next();
                for next in chars.by_ref() {
                    if next == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            } else if ch == '/' && chars.peek() == Some(&'*') {
                chars.next();
                while let Some(next) = chars.next() {
                    if next == '*' && chars.peek() == Some(&'/') {
                        chars.next();
                        break;
                    }
                }
            } else {
                out.push(ch);
            }
        }
        out
    }

    fn arrays_to_brackets(source: &str) -> String {
        let mut out = source.to_string();
        while let Some(start) = out.find("Array<") {
            let inner_start = start + "Array<".len();
            let mut depth = 1;
            let mut end = inner_start;
            for ch in out[inner_start..].chars() {
                match ch {
                    '<' => depth += 1,
                    '>' => depth -= 1,
                    _ => {}
                }
                end += ch.len_utf8();
                if depth == 0 {
                    break;
                }
            }
            let inner = out[inner_start..end - '>'.len_utf8()].to_string();
            out.replace_range(start..end, &format!("{inner}[]"));
        }
        out
    }

    fn unquote_identifiers(source: &str) -> String {
        let mut out = String::new();
        let mut chars = source.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '\'' {
                let mut content = String::new();
                let mut closed = false;
                for next in chars.by_ref() {
                    if next == '\'' {
                        closed = true;
                        break;
                    }
                    content.push(next);
                }
                if closed && is_identifier(&content) {
                    out.push_str(&content);
                } else if closed {
                    out.push('\'');
                    out.push_str(&content);
                    out.push('\'');
                } else {
                    out.push('\'');
                    out.push_str(&content);
                }
            } else {
                out.push(ch);
            }
        }
        out
    }

    fn is_identifier(text: &str) -> bool {
        let mut chars = text.chars();
        match chars.next() {
            Some(first) if first.is_ascii_alphabetic() || first == '_' => {}
            _ => return false,
        }
        chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    }
}
