//! The preset pack that ships with the player.
//!
//! The files live in the binary. On startup they are written under the
//! presets directory, next to anything the listener adds. A file is rewritten
//! when the player carries a newer copy.

use std::fs;
use std::path::{Path, PathBuf};

const PACK: &str = "Built-in";

const FILES: &[(&str, &str)] = &[
    (
        "phosphor.milk",
        include_str!("../presets/built-in/phosphor.milk"),
    ),
    ("spin.milk", include_str!("../presets/built-in/spin.milk")),
    (
        "tunnel.milk",
        include_str!("../presets/built-in/tunnel.milk"),
    ),
    ("drift.milk", include_str!("../presets/built-in/drift.milk")),
    (
        "sparks.milk",
        include_str!("../presets/built-in/sparks.milk"),
    ),
    ("swell.milk", include_str!("../presets/built-in/swell.milk")),
];

/// Writes the built-in pack under `root` and returns that pack's directory.
pub fn install_builtin(root: &Path) -> PathBuf {
    let pack = root.join(PACK);
    for (name, body) in FILES {
        let path = pack.join(name);
        if fs::read_to_string(&path).ok().as_deref() == Some(*body) {
            continue;
        }
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(&path, body);
    }
    pack
}

#[cfg(test)]
mod tests {
    use super::{install_builtin, FILES, PACK};

    #[test]
    fn the_builtin_pack_is_discovered_as_a_preset_pack() {
        let root = std::env::temp_dir().join(format!("djusb-builtin-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        install_builtin(&root);
        let packs = djusb_projectm::discover_packs(std::slice::from_ref(&root));
        assert_eq!(packs.len(), 1);
        assert_eq!(packs[0].name, PACK);
        let written = std::fs::read_dir(&packs[0].path)
            .unwrap()
            .flatten()
            .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "milk"))
            .count();
        assert_eq!(written, FILES.len());
        install_builtin(&root);
        assert_eq!(
            djusb_projectm::discover_packs(std::slice::from_ref(&root)).len(),
            1
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn every_builtin_preset_has_frame_code() {
        assert!(FILES.len() >= 4);
        for (name, body) in FILES {
            assert!(name.ends_with(".milk"), "{name}");
            assert!(body.contains("per_frame_1="), "{name}");
            assert!(body.contains("wave_g="), "{name}");
        }
    }
}
