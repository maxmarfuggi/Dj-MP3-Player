//! Turns `cargo deny list --format json` into the wall player's licence rows.
//!
//! The checked-in report is `docs/third-party-licences.json`. It is the same
//! graph `cargo deny` checks, without this repo's own crates and without
//! machine-specific paths. The licences screen is generated from that file.
//! Regenerate it with `UPDATE_LICENCE_REPORT=1` and the freshness test below.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use serde_json::{json, Value};

/// One third-party crate and the licences cargo-deny found for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrateLicence {
    pub name: String,
    pub version: String,
    pub licences: Vec<String>,
}

/// A row on the licences screen: the crate, and its licences.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoticeRow {
    pub label: String,
    pub licence: String,
}

/// Reads the `licenses` groups from `cargo deny list --format json`.
///
/// A crate that sits under several groups becomes one entry. Crates from a
/// path (this workspace) are left out, so the report does not contain a
/// machine-specific path.
#[cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "the freshness test reads cargo-deny; the build uses the checked-in report"
    )
)]
pub fn from_cargo_deny(json: &str) -> Result<Vec<CrateLicence>, String> {
    let value: Value = serde_json::from_str(json).map_err(|error| error.to_string())?;
    if value.get("licenses").is_none() && value.get("unlicensed").is_none() {
        return Err("cargo-deny report has no licences".to_string());
    }
    let mut by_crate: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();
    if let Some(groups) = value.get("licenses").and_then(Value::as_array) {
        for group in groups {
            let Some(pair) = group.as_array() else {
                return Err("a licence group is not a pair".to_string());
            };
            if pair.len() != 2 {
                return Err("a licence group is not a pair".to_string());
            }
            let Some(licence) = pair[0].as_str() else {
                return Err("a licence name is not text".to_string());
            };
            let Some(ids) = pair[1].as_array() else {
                return Err("a licence group has no crates".to_string());
            };
            for id in ids {
                let Some(id) = id.as_str() else {
                    return Err("a crate id is not text".to_string());
                };
                let Some((name, version)) = published_crate(id) else {
                    continue;
                };
                by_crate
                    .entry((name, version))
                    .or_default()
                    .insert(licence.to_string());
            }
        }
    }
    if let Some(ids) = value.get("unlicensed").and_then(Value::as_array) {
        for id in ids {
            let Some(id) = id.as_str() else {
                return Err("a crate id is not text".to_string());
            };
            let Some((name, version)) = published_crate(id) else {
                continue;
            };
            by_crate.entry((name, version)).or_default();
        }
    }
    Ok(sorted(by_crate))
}

/// The screen label is the crate name. A second version of the same crate
/// keeps its version, so the two rows can be told apart.
#[cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "the build emits labels; the tests check how versions are shown"
    )
)]
pub fn notice_rows(crates: &[CrateLicence]) -> Vec<NoticeRow> {
    let mut copies: BTreeMap<&str, usize> = BTreeMap::new();
    for crate_ in crates {
        *copies.entry(&crate_.name).or_default() += 1;
    }
    crates
        .iter()
        .map(|crate_| {
            let label = if copies[crate_.name.as_str()] > 1 {
                format!("{} {}", crate_.name, crate_.version)
            } else {
                crate_.name.clone()
            };
            let licence = if crate_.licences.is_empty() {
                "unlicensed".to_string()
            } else {
                crate_.licences.join(", ")
            };
            NoticeRow { label, licence }
        })
        .collect()
}

/// Stable JSON for `docs/third-party-licences.json`.
#[cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "the freshness test writes this text; the build reads the file"
    )
)]
pub fn canonical_report(crates: &[CrateLicence]) -> Result<String, String> {
    let mut crates = crates.to_vec();
    sort_crates(&mut crates);
    let list: Vec<Value> = crates
        .iter()
        .map(|crate_| {
            let mut licences = crate_.licences.clone();
            licences.sort();
            licences.dedup();
            json!({
                "name": crate_.name,
                "version": crate_.version,
                "licences": licences,
            })
        })
        .collect();
    let mut text = serde_json::to_string_pretty(&list).map_err(|error| error.to_string())?;
    text.push('\n');
    Ok(text)
}

/// Reads [`canonical_report`] output.
pub fn from_report(json: &str) -> Result<Vec<CrateLicence>, String> {
    let value: Value = serde_json::from_str(json).map_err(|error| error.to_string())?;
    let Some(list) = value.as_array() else {
        return Err("the licence report is not a list".to_string());
    };
    let mut crates = Vec::with_capacity(list.len());
    for item in list {
        let Some(name) = item.get("name").and_then(Value::as_str) else {
            return Err("a crate is missing its name".to_string());
        };
        let Some(version) = item.get("version").and_then(Value::as_str) else {
            return Err("a crate is missing its version".to_string());
        };
        let Some(listed) = item.get("licences").and_then(Value::as_array) else {
            return Err("a crate is missing its licences".to_string());
        };
        let mut licences = Vec::with_capacity(listed.len());
        for licence in listed {
            let Some(licence) = licence.as_str() else {
                return Err("a licence is not text".to_string());
            };
            licences.push(licence.to_string());
        }
        licences.sort();
        licences.dedup();
        crates.push(CrateLicence {
            name: name.to_string(),
            version: version.to_string(),
            licences,
        });
    }
    sort_crates(&mut crates);
    Ok(crates)
}

/// Rust source the device build includes. One static row per crate.
pub fn rust_module(report_json: &str) -> Result<String, String> {
    let rows = notice_rows(&from_report(report_json)?);
    let mut out = String::from(
        "// Generated from docs/third-party-licences.json. Do not edit.\n\
         pub struct Notice {\n    \
         pub label: &'static str,\n    \
         pub licence: &'static str,\n\
         }\n\n\
         pub static NOTICES: &[Notice] = &[\n",
    );
    for row in rows {
        out.push_str("    Notice { label: ");
        push_rust_string(&mut out, &row.label);
        out.push_str(", licence: ");
        push_rust_string(&mut out, &row.licence);
        out.push_str(" },\n");
    }
    out.push_str("];\n");
    Ok(out)
}

/// `name version source` from cargo-deny. Path crates are this workspace.
fn published_crate(id: &str) -> Option<(String, String)> {
    let mut parts = id.splitn(3, ' ');
    let name = parts.next()?;
    let version = parts.next()?;
    let source = parts.next().unwrap_or("");
    if name.is_empty() || version.is_empty() || source.starts_with("path+") {
        return None;
    }
    Some((name.to_string(), version.to_string()))
}

fn sorted(by_crate: BTreeMap<(String, String), BTreeSet<String>>) -> Vec<CrateLicence> {
    let mut crates: Vec<CrateLicence> = by_crate
        .into_iter()
        .map(|((name, version), licences)| CrateLicence {
            name,
            version,
            licences: licences.into_iter().collect(),
        })
        .collect();
    sort_crates(&mut crates);
    crates
}

fn sort_crates(crates: &mut [CrateLicence]) {
    crates.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| cmp_version(&left.version, &right.version))
    });
}

fn cmp_version(left: &str, right: &str) -> Ordering {
    version_numbers(left)
        .cmp(&version_numbers(right))
        .then_with(|| left.cmp(right))
}

fn version_numbers(version: &str) -> Vec<u64> {
    let head = version.split(['+', '-']).next().unwrap_or(version);
    head.split('.')
        .map(|part| part.parse().unwrap_or(0))
        .collect()
}

fn push_rust_string(out: &mut String, text: &str) {
    out.push('"');
    for character in text.chars() {
        match character {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            other => out.push(other),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::process::Command;

    use super::{canonical_report, from_cargo_deny, from_report, notice_rows, rust_module};

    const DENY: &str = r#"{
        "licenses": [
            ["Apache-2.0", [
                "ab_glyph 0.2.32 registry+https://github.com/rust-lang/crates.io-index",
                "bitflags 1.3.2 registry+https://github.com/rust-lang/crates.io-index",
                "bitflags 2.13.2 registry+https://github.com/rust-lang/crates.io-index"
            ]],
            ["MIT", [
                "bitflags 1.3.2 registry+https://github.com/rust-lang/crates.io-index",
                "bitflags 2.13.2 registry+https://github.com/rust-lang/crates.io-index"
            ]]
        ],
        "unlicensed": [
            "djusb-device 0.1.0 path+file:///C:/Projects/Dj-MP3-Player/apps/device"
        ]
    }"#;

    #[test]
    fn a_crate_keeps_its_name_version_and_licence() {
        let crates = from_cargo_deny(DENY).unwrap();
        let glyph = crates
            .iter()
            .find(|crate_| crate_.name == "ab_glyph")
            .unwrap();
        assert_eq!(glyph.version, "0.2.32");
        assert_eq!(glyph.licences, vec!["Apache-2.0".to_string()]);
    }

    #[test]
    fn a_crate_listed_under_several_licences_is_one_entry() {
        let crates = from_cargo_deny(DENY).unwrap();
        let flags: Vec<_> = crates
            .iter()
            .filter(|crate_| crate_.name == "bitflags")
            .collect();
        assert_eq!(flags.len(), 2);
        assert_eq!(
            flags[0].licences,
            vec!["Apache-2.0".to_string(), "MIT".to_string()]
        );
    }

    #[test]
    fn workspace_crates_are_left_out() {
        let crates = from_cargo_deny(DENY).unwrap();
        assert!(crates.iter().all(|crate_| crate_.name != "djusb-device"));
        let report = canonical_report(&crates).unwrap();
        assert!(!report.contains("file:///"));
        assert!(!report.contains("appliance image"));
    }

    #[test]
    fn a_second_version_is_labelled_with_that_version() {
        let rows = notice_rows(&from_cargo_deny(DENY).unwrap());
        assert!(rows
            .iter()
            .any(|row| row.label == "ab_glyph" && row.licence == "Apache-2.0"));
        assert!(rows.iter().any(|row| row.label == "bitflags 1.3.2"));
        assert!(rows.iter().any(|row| row.label == "bitflags 2.13.2"));
    }

    #[test]
    fn the_report_round_trips_into_the_generated_module() {
        let crates = from_cargo_deny(DENY).unwrap();
        let report = canonical_report(&crates).unwrap();
        assert_eq!(from_report(&report).unwrap(), crates);
        let rust = rust_module(&report).unwrap();
        assert!(rust.contains("ab_glyph"));
        assert!(rust.contains("bitflags 1.3.2"));
        assert!(rust.contains("Apache-2.0, MIT"));
    }

    #[test]
    fn the_checked_in_report_matches_cargo_deny() {
        let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let output = match Command::new("cargo")
            .args(["deny", "list", "--format", "json"])
            .current_dir(&workspace)
            .output()
        {
            Ok(output) if output.status.success() => output,
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                if std::env::var_os("CI").is_some() {
                    panic!("cargo deny failed: {stderr}");
                }
                eprintln!("skipping licence report check: cargo deny failed: {stderr}");
                return;
            }
            Err(error) => {
                if std::env::var_os("CI").is_some() {
                    panic!("cargo deny is not installed: {error}");
                }
                eprintln!("skipping licence report check: cargo deny is not installed");
                return;
            }
        };
        let stdout = String::from_utf8(output.stdout).expect("cargo deny speaks utf-8");
        let live = canonical_report(&from_cargo_deny(&stdout).expect("cargo deny json")).unwrap();
        let path = workspace.join("docs/third-party-licences.json");
        if std::env::var_os("UPDATE_LICENCE_REPORT").is_some() {
            std::fs::write(&path, &live).unwrap_or_else(|error| {
                panic!("writing {}: {error}", path.display());
            });
        }
        let saved = std::fs::read_to_string(&path)
            .unwrap_or_default()
            .replace("\r\n", "\n");
        assert_reports_match(&saved, &live);
    }

    fn assert_reports_match(saved: &str, live: &str) {
        if saved == live {
            return;
        }
        let saved_crates = from_report(saved).unwrap_or_default();
        let live_crates = from_report(live).unwrap_or_default();
        let saved_keys: std::collections::BTreeSet<_> =
            saved_crates.iter().map(crate_key).collect();
        let live_keys: std::collections::BTreeSet<_> = live_crates.iter().map(crate_key).collect();
        let missing: Vec<_> = live_keys.difference(&saved_keys).cloned().collect();
        let extra: Vec<_> = saved_keys.difference(&live_keys).cloned().collect();
        let relicensed: Vec<_> = saved_crates
            .iter()
            .filter(|saved| {
                live_crates.iter().any(|live| {
                    live.name == saved.name
                        && live.version == saved.version
                        && live.licences != saved.licences
                })
            })
            .map(crate_key)
            .collect();
        panic!(
            "docs/third-party-licences.json is stale; rerun with UPDATE_LICENCE_REPORT=1\n\
             missing from the file: {missing:?}\n\
             no longer reported: {extra:?}\n\
             licences changed: {relicensed:?}"
        );
    }

    fn crate_key(crate_: &super::CrateLicence) -> String {
        format!("{} {}", crate_.name, crate_.version)
    }
}
