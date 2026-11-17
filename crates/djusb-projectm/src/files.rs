//! Preset packs and the projectM shared library file.
//!
//! A pack is a directory of `.milk` presets. Child directories of a root are
//! packs. A root that itself holds presets, not only in children, is a pack
//! too. The library is a file next to the player or on the system search
//! path, and it is never linked into this crate.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use libloading::Library;

/// One preset pack the platter can open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pack {
    pub name: String,
    pub path: PathBuf,
}

/// File names projectM 4 is installed under. The first one that exists is used.
#[must_use]
pub fn library_names() -> &'static [&'static str] {
    #[cfg(windows)]
    {
        &["projectM-4.dll", "libprojectM-4.dll"]
    }
    #[cfg(target_os = "linux")]
    {
        &["libprojectM-4.so.4", "libprojectM-4.so"]
    }
    #[cfg(target_os = "macos")]
    {
        &["libprojectM-4.dylib"]
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        &[]
    }
}

/// Packs under `roots`, in name order. A missing root is skipped.
#[must_use]
pub fn discover_packs(roots: &[PathBuf]) -> Vec<Pack> {
    let mut found = Vec::new();
    let mut seen = BTreeSet::new();
    for root in roots {
        if !root.is_dir() {
            continue;
        }
        if has_loose_milk(root) {
            remember(&mut found, &mut seen, root);
        }
        let Ok(entries) = fs::read_dir(root) else {
            continue;
        };
        let mut children: Vec<PathBuf> = entries
            .flatten()
            .filter(is_directory)
            .map(|entry| entry.path())
            .filter(|path| !is_hidden(path))
            .collect();
        children.sort();
        for child in children {
            if contains_milk(&child) {
                remember(&mut found, &mut seen, &child);
            }
        }
    }
    label(found)
}

/// `.milk` files inside `pack`, including nested directories, in path order.
/// Symbolic links are not followed.
#[must_use]
pub fn preset_files(pack: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_milk(pack, &mut files);
    files.sort();
    files
}

/// A library file in one of `dirs`, if any of the known names is there.
#[must_use]
pub fn find_library_file(dirs: &[PathBuf]) -> Option<PathBuf> {
    for dir in dirs {
        for name in library_names() {
            let path = dir.join(name);
            if path.is_file() {
                return Some(path);
            }
        }
    }
    None
}

/// The library file in `dirs`, or a name the system loader can open.
pub fn locate_library(dirs: &[PathBuf]) -> Result<PathBuf, String> {
    if let Some(path) = find_library_file(dirs) {
        return Ok(path);
    }
    for name in library_names() {
        if open_library(Path::new(name)).is_ok() {
            return Ok(PathBuf::from(name));
        }
    }
    Err(crate::MISSING_LIBRARY.to_string())
}

pub(crate) fn open_library(path: &Path) -> Result<Library, String> {
    unsafe { Library::new(path) }.map_err(|error| {
        format!(
            "projectM could not be loaded from {}: {error}",
            path.display()
        )
    })
}

fn remember(found: &mut Vec<PathBuf>, seen: &mut BTreeSet<PathBuf>, path: &Path) {
    if seen.insert(path.to_path_buf()) {
        found.push(path.to_path_buf());
    }
}

fn label(paths: Vec<PathBuf>) -> Vec<Pack> {
    let mut packs: Vec<Pack> = paths
        .into_iter()
        .filter_map(|path| {
            let name = path.file_name()?.to_string_lossy().into_owned();
            Some(Pack { name, path })
        })
        .collect();
    let mut counts = BTreeSet::new();
    let mut duplicated = BTreeSet::new();
    for pack in &packs {
        let key = pack.name.to_lowercase();
        if !counts.insert(key.clone()) {
            duplicated.insert(key);
        }
    }
    for pack in &mut packs {
        if duplicated.contains(&pack.name.to_lowercase()) {
            if let Some(parent) = pack
                .path
                .parent()
                .and_then(|parent| parent.file_name())
                .map(|name| name.to_string_lossy().into_owned())
            {
                pack.name = format!("{} · {parent}", pack.name);
            }
        }
    }
    packs.sort_by(|left, right| {
        left.name
            .to_lowercase()
            .cmp(&right.name.to_lowercase())
            .then_with(|| left.path.cmp(&right.path))
    });
    disambiguate(packs)
}

/// A second pass, for two packs that still share a name after the parent is added.
fn disambiguate(mut packs: Vec<Pack>) -> Vec<Pack> {
    let mut used = BTreeSet::new();
    for pack in &mut packs {
        let base = pack.name.clone();
        if used.insert(base.to_lowercase()) {
            continue;
        }
        let mut suffix = 2;
        loop {
            let name = format!("{base} {suffix}");
            if used.insert(name.to_lowercase()) {
                pack.name = name;
                break;
            }
            suffix += 1;
        }
    }
    packs
}

fn has_loose_milk(dir: &Path) -> bool {
    let Ok(entries) = fs::read_dir(dir) else {
        return false;
    };
    entries
        .flatten()
        .any(|entry| is_file(&entry) && is_milk(&entry.path()))
}

fn contains_milk(dir: &Path) -> bool {
    let mut pending = vec![dir.to_path_buf()];
    while let Some(current) = pending.pop() {
        let Ok(entries) = fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if is_file(&entry) && is_milk(&path) {
                return true;
            }
            if is_directory(&entry) && !is_hidden(&path) {
                pending.push(path);
            }
        }
    }
    false
}

fn collect_milk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut children = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if is_file(&entry) && is_milk(&path) {
            out.push(path);
        } else if is_directory(&entry) && !is_hidden(&path) {
            children.push(path);
        }
    }
    children.sort();
    for child in children {
        collect_milk(&child, out);
    }
}

fn is_milk(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("milk"))
}

fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with('.'))
}

fn is_directory(entry: &fs::DirEntry) -> bool {
    entry.file_type().is_ok_and(|kind| kind.is_dir())
}

fn is_file(entry: &fs::DirEntry) -> bool {
    entry.file_type().is_ok_and(|kind| kind.is_file())
}

#[cfg(test)]
mod tests {
    use super::{discover_packs, find_library_file, preset_files};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    fn scratch(name: &str) -> PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "djusb-milk-{name}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn touch(path: &Path) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, b"milk").unwrap();
    }

    #[test]
    fn child_directories_with_presets_are_packs_and_empty_ones_are_not() {
        let root = scratch("packs");
        touch(&root.join("cream").join("a.milk"));
        touch(&root.join("cream").join("nested").join("b.MILK"));
        fs::create_dir_all(root.join("empty")).unwrap();
        fs::write(root.join("notes.txt"), b"no").unwrap();
        let packs = discover_packs(std::slice::from_ref(&root));
        assert_eq!(packs.len(), 1);
        assert_eq!(packs[0].name, "cream");
        assert_eq!(packs[0].path, root.join("cream"));
        let files = preset_files(&packs[0].path);
        assert_eq!(files.len(), 2);
        assert!(files[0].ends_with("a.milk"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn loose_presets_make_the_root_itself_a_pack() {
        let root = scratch("loose");
        touch(&root.join("idle.milk"));
        let packs = discover_packs(std::slice::from_ref(&root));
        assert_eq!(packs.len(), 1);
        assert_eq!(packs[0].path, root);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn two_packs_with_the_same_name_keep_their_parent() {
        let root = scratch("same");
        let left = root.join("usb");
        let right = root.join("card");
        touch(&left.join("cream").join("a.milk"));
        touch(&right.join("cream").join("b.milk"));
        let packs = discover_packs(&[left, right]);
        let names: Vec<_> = packs.iter().map(|pack| pack.name.as_str()).collect();
        assert_eq!(names, vec!["cream · card", "cream · usb"]);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_missing_root_adds_no_packs() {
        let root = scratch("missing");
        let _ = fs::remove_dir_all(&root);
        assert!(discover_packs(&[root]).is_empty());
    }

    #[test]
    fn the_library_file_is_the_known_name_in_the_directory() {
        let root = scratch("lib");
        let name = super::library_names()[0];
        touch(&root.join(name));
        assert_eq!(
            find_library_file(std::slice::from_ref(&root)).as_deref(),
            Some(root.join(name).as_path())
        );
        assert!(find_library_file(&[root.join("nope")]).is_none());
        let _ = fs::remove_dir_all(root);
    }
}
