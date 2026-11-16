//! Mounted volumes that could hold a DJ library.
//!
//! The system volume is left out. On Windows the display name is the drive
//! label from `GetVolumeInformationW`, cached until the drive disappears. On
//! macOS the list is `/Volumes`, without the boot volume. On Linux it is the
//! user's `/media` and `/run/media` directories plus the mounts udisks2 reports.

use std::collections::HashMap;
use std::path::Path;
#[cfg(windows)]
use std::sync::Mutex;

#[cfg(windows)]
use crate::paths::exists;
use crate::Volume;

/// Lists mounted volumes that could be a DJ USB, excluding the system volume.
///
/// A missing media directory is an empty list. Any other failure to read one
/// is returned. A missing udisks2 service is ignored.
pub fn list_volumes() -> std::io::Result<Vec<Volume>> {
    #[cfg(windows)]
    {
        list_windows()
    }
    #[cfg(target_os = "macos")]
    {
        list_macos()
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        list_linux()
    }
}

#[cfg(windows)]
fn list_windows() -> std::io::Result<Vec<Volume>> {
    let system = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".to_string());
    let system = system.to_uppercase();
    let mut present = Vec::new();
    for code in b'A'..=b'Z' {
        let drive = format!("{}:", char::from(code));
        if drive == system {
            continue;
        }
        if exists(format!("{drive}\\")) {
            present.push(drive);
        }
    }
    static LABELS: std::sync::LazyLock<Mutex<HashMap<String, String>>> =
        std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));
    let mut labels = LABELS.lock().unwrap_or_else(|error| error.into_inner());
    Ok(windows_volumes(&present, &mut labels, |drive| {
        djusb_sys::volume_label(Path::new(&format!("{drive}\\")).as_os_str())
    }))
}

#[cfg(target_os = "macos")]
fn list_macos() -> std::io::Result<Vec<Volume>> {
    directory_volumes(Path::new("/Volumes"), Some(Path::new("/")))
}

#[cfg(not(any(windows, target_os = "macos")))]
fn list_linux() -> std::io::Result<Vec<Volume>> {
    let user = std::env::var("USER").unwrap_or_default();
    let mut volumes = Vec::new();
    if !user.is_empty() {
        volumes.extend(directory_volumes(&Path::new("/media").join(&user), None)?);
        volumes.extend(directory_volumes(
            &Path::new("/run/media").join(&user),
            None,
        )?);
    }
    Ok(merge_volumes(volumes, udisks_volumes(&udisks_dump())))
}

/// Drives that are present, with labels filled in and forgotten once they go away.
#[cfg_attr(not(windows), allow(dead_code))]
fn windows_volumes(
    present: &[String],
    labels: &mut HashMap<String, String>,
    mut read_label: impl FnMut(&str) -> String,
) -> Vec<Volume> {
    labels.retain(|drive, _| present.iter().any(|candidate| candidate == drive));
    for drive in present {
        if !labels.contains_key(drive) {
            labels.insert(drive.clone(), read_label(drive));
        }
    }
    present
        .iter()
        .map(|drive| {
            let cached = labels.get(drive).map(String::as_str).unwrap_or("");
            Volume {
                path: format!("{drive}\\"),
                name: if cached.is_empty() {
                    drive.clone()
                } else {
                    cached.to_string()
                },
            }
        })
        .collect()
}

/// Child entries of `parent`. `boot` entries, once resolved, are skipped.
///
/// A missing directory is empty. Resolving one entry and failing skips that
/// entry, matching a volume that disappeared while it was being listed.
#[cfg_attr(windows, allow(dead_code))]
fn directory_volumes(parent: &Path, boot: Option<&Path>) -> std::io::Result<Vec<Volume>> {
    let entries = match std::fs::read_dir(parent) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut volumes = Vec::new();
    for entry in entries {
        let entry = entry?;
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        let path = parent.join(&name);
        if let Some(boot) = boot {
            match std::fs::canonicalize(&path) {
                Ok(real) if real == boot => continue,
                Ok(_) => {}
                Err(_) => continue,
            }
        }
        let Some(path) = path.to_str().map(str::to_string) else {
            continue;
        };
        volumes.push(Volume { path, name });
    }
    Ok(volumes)
}

#[cfg_attr(any(windows, target_os = "macos"), allow(dead_code))]
fn merge_volumes(mut volumes: Vec<Volume>, extra: Vec<Volume>) -> Vec<Volume> {
    for volume in extra {
        if volumes.iter().any(|existing| existing.path == volume.path) {
            continue;
        }
        volumes.push(volume);
    }
    volumes
}

#[cfg_attr(any(windows, target_os = "macos"), allow(dead_code))]
fn udisks_dump() -> String {
    let output = std::process::Command::new("udisksctl").arg("dump").output();
    let Ok(output) = output else {
        return String::new();
    };
    if !output.status.success() {
        return String::new();
    }
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Mounted filesystems reported by `udisksctl dump`, skipping system disks.
#[cfg_attr(any(windows, target_os = "macos"), allow(dead_code))]
fn udisks_volumes(dump: &str) -> Vec<Volume> {
    let mut volumes = Vec::new();
    let mut device: Option<UDisksDevice> = None;
    let mut reading_mounts = false;

    let finish = |device: &mut Option<UDisksDevice>, volumes: &mut Vec<Volume>| {
        let Some(device) = device.take() else {
            return;
        };
        if device.system {
            return;
        }
        for mount in device.mounts {
            if mount == "/" {
                continue;
            }
            let name = if device.label.is_empty() {
                Path::new(&mount)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .filter(|name| !name.is_empty())
                    .unwrap_or(&mount)
                    .to_string()
            } else {
                device.label.clone()
            };
            volumes.push(Volume { path: mount, name });
        }
    };

    for line in dump.lines() {
        if line.is_empty() || line.starts_with(|character: char| !character.is_whitespace()) {
            finish(&mut device, &mut volumes);
            reading_mounts = false;
            if line.starts_with("/org/freedesktop/UDisks2/block_devices/") {
                device = Some(UDisksDevice::default());
            }
            continue;
        }
        let Some(device) = device.as_mut() else {
            continue;
        };
        let trimmed = line.trim();
        let Some((key, value)) = trimmed.split_once(':') else {
            if reading_mounts && trimmed.starts_with('/') {
                device.mounts.push(trimmed.to_string());
            }
            continue;
        };
        match key.trim() {
            "HintSystem" => {
                reading_mounts = false;
                device.system = value.trim() == "true";
            }
            "IdLabel" => {
                reading_mounts = false;
                device.label = value.trim().to_string();
            }
            "MountPoints" => {
                reading_mounts = true;
                let value = value.trim();
                if value.starts_with('/') {
                    device.mounts.push(value.to_string());
                }
            }
            _ => reading_mounts = false,
        }
    }
    finish(&mut device, &mut volumes);
    volumes.sort_by(|left, right| left.path.cmp(&right.path));
    volumes
}

#[cfg_attr(any(windows, target_os = "macos"), allow(dead_code))]
#[derive(Default)]
struct UDisksDevice {
    system: bool,
    label: String,
    mounts: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::{directory_volumes, merge_volumes, udisks_volumes, windows_volumes};
    use crate::Volume;
    use std::collections::HashMap;
    use std::path::PathBuf;

    #[test]
    fn windows_labels_are_cached_until_the_drive_goes_away() {
        let mut labels = HashMap::new();
        let mut reads = Vec::new();
        let first = windows_volumes(&["D:".into(), "E:".into()], &mut labels, |drive| {
            reads.push(drive.to_string());
            if drive == "D:" {
                "ETHER".into()
            } else {
                String::new()
            }
        });
        assert_eq!(
            first,
            [
                Volume {
                    path: "D:\\".into(),
                    name: "ETHER".into(),
                },
                Volume {
                    path: "E:\\".into(),
                    name: "E:".into(),
                },
            ]
        );
        assert_eq!(reads, ["D:", "E:"]);

        reads.clear();
        let second = windows_volumes(&["D:".into()], &mut labels, |drive| {
            reads.push(drive.to_string());
            "CHANGED".into()
        });
        assert!(reads.is_empty(), "a mounted drive keeps its label");
        assert_eq!(second[0].name, "ETHER");
        assert!(!labels.contains_key("E:"));
    }

    #[test]
    fn a_missing_media_directory_adds_no_volumes() {
        let missing = std::env::temp_dir().join(format!("djusb-no-volumes-{}", std::process::id()));
        let volumes = directory_volumes(&missing, None).unwrap();
        assert!(volumes.is_empty());
    }

    #[test]
    fn directory_entries_become_volumes_and_the_boot_volume_is_skipped() {
        let root = temp_dir("volumes");
        std::fs::create_dir(root.join("ETHER")).unwrap();
        std::fs::write(root.join("note.txt"), []).unwrap();
        let volumes = directory_volumes(&root, None).unwrap();
        let mut names: Vec<_> = volumes.iter().map(|volume| volume.name.as_str()).collect();
        names.sort_unstable();
        assert_eq!(names, ["ETHER", "note.txt"]);

        let ether = std::fs::canonicalize(root.join("ETHER")).unwrap();
        let skipped = directory_volumes(&root, Some(&ether)).unwrap();
        assert_eq!(skipped.len(), 1);
        assert_eq!(skipped[0].name, "note.txt");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn udisks_lists_mounted_labels_and_skips_the_system_disk() {
        let dump = "
/org/freedesktop/UDisks2/drives/drive_a:
  org.freedesktop.UDisks2.Drive:
    Id:                         ignored

/org/freedesktop/UDisks2/block_devices/sda2:
  org.freedesktop.UDisks2.Block:
    HintSystem:                 true
    IdLabel:                    root
  org.freedesktop.UDisks2.Filesystem:
    MountPoints:                /

/org/freedesktop/UDisks2/block_devices/sdb1:
  org.freedesktop.UDisks2.Block:
    HintSystem:                 false
    IdLabel:                    ETHER
  org.freedesktop.UDisks2.Filesystem:
    MountPoints:                /run/media/pi/ETHER
                                /mnt/also

/org/freedesktop/UDisks2/block_devices/sdc1:
  org.freedesktop.UDisks2.Block:
    HintSystem:                 false
    IdLabel:
  org.freedesktop.UDisks2.Filesystem:
    MountPoints:                /mnt/stick
";
        assert_eq!(
            udisks_volumes(dump),
            [
                Volume {
                    path: "/mnt/also".into(),
                    name: "ETHER".into(),
                },
                Volume {
                    path: "/mnt/stick".into(),
                    name: "stick".into(),
                },
                Volume {
                    path: "/run/media/pi/ETHER".into(),
                    name: "ETHER".into(),
                },
            ]
        );
    }

    #[test]
    fn a_directory_mount_is_not_listed_twice_when_udisks_reports_it() {
        let from_directory = vec![Volume {
            path: "/run/media/pi/ETHER".into(),
            name: "ETHER".into(),
        }];
        let from_udisks = vec![
            Volume {
                path: "/run/media/pi/ETHER".into(),
                name: "ETHER".into(),
            },
            Volume {
                path: "/mnt/stick".into(),
                name: "stick".into(),
            },
        ];
        let merged = merge_volumes(from_directory, from_udisks);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[1].path, "/mnt/stick");
    }

    #[cfg(windows)]
    #[test]
    fn the_running_system_lists_no_system_drive() {
        let volumes = super::list_volumes().unwrap();
        let system = std::env::var("SystemDrive")
            .unwrap_or_else(|_| "C:".to_string())
            .to_uppercase();
        for volume in &volumes {
            assert!(
                volume.path.ends_with('\\') && volume.path.len() == 3,
                "{}",
                volume.path
            );
            assert!(
                !volume.path.to_uppercase().starts_with(&system),
                "{}",
                volume.path
            );
            assert!(!volume.name.is_empty(), "{}", volume.path);
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("djusb-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        path
    }
}
