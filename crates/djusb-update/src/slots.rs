//! Two system slots. An update lands on the one that is not the last
//! known-good system, and a trial that never confirms boots that system again.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::bundle::{copy_file, read_bundle, Manifest};
use crate::{is_within, sha256_file, write_replacing, Error, VerifyingKey};

/// How many boots a new system gets before the previous one is selected.
pub const TRIAL_BOOTS: u32 = 1;

/// Where the appliance keeps the slot record, on the data partition.
pub const STATE_PATH: &str = "/var/lib/djusb/slots.json";

const FACTORY: &str = "factory";

/// Which system partition an update occupies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Slot {
    /// The first system partition.
    A,
    /// The second system partition.
    B,
}

impl Slot {
    fn other(self) -> Self {
        match self {
            Self::A => Self::B,
            Self::B => Self::A,
        }
    }
}

/// What [`install`] wrote, before the next boot tries it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    /// Version from the signed manifest.
    pub version: String,
    /// The slot that received the payload. Never the last known-good slot.
    pub slot: Slot,
    /// Path the payload was copied to.
    pub image: PathBuf,
}

/// The system [`boot`] selected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootChoice {
    /// Slot to mount as the system.
    pub slot: Slot,
    /// Version recorded for that slot.
    pub version: String,
    /// Image or partition path recorded for that slot.
    pub image: PathBuf,
    /// The trial ended without a confirm, so the previous system was selected.
    pub rolled_back: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct Record {
    committed: Slot,
    preferred: Slot,
    booted: Slot,
    #[serde(rename = "trialBootId")]
    trial_boot_id: String,
    slots: Pair,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct Pair {
    a: SlotRecord,
    b: SlotRecord,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct SlotRecord {
    version: String,
    image: String,
    #[serde(rename = "bootsLeft")]
    boots_left: u32,
}

impl Record {
    fn get(&self, slot: Slot) -> &SlotRecord {
        match slot {
            Slot::A => &self.slots.a,
            Slot::B => &self.slots.b,
        }
    }

    fn get_mut(&mut self, slot: Slot) -> &mut SlotRecord {
        match slot {
            Slot::A => &mut self.slots.a,
            Slot::B => &mut self.slots.b,
        }
    }

    fn choice(&self, slot: Slot, rolled_back: bool) -> BootChoice {
        let record = self.get(slot);
        BootChoice {
            slot,
            version: record.version.clone(),
            image: PathBuf::from(&record.image),
            rolled_back,
        }
    }
}

/// Records slot A as the running factory system and slot B as empty.
///
/// A record that already names these two images is left as it is.
pub fn init(state: &Path, slot_a: &Path, slot_b: &Path) -> Result<(), Error> {
    let image_a = path_text(slot_a)?;
    let image_b = path_text(slot_b)?;
    if state.is_file() {
        let record = load(state)?;
        if record.slots.a.image == image_a && record.slots.b.image == image_b {
            return Ok(());
        }
        return Err(Error::Format(
            "the slot record already names different system images".to_string(),
        ));
    }
    let record = Record {
        committed: Slot::A,
        preferred: Slot::A,
        booted: Slot::A,
        trial_boot_id: String::new(),
        slots: Pair {
            a: SlotRecord {
                version: FACTORY.to_string(),
                image: image_a,
                boots_left: 0,
            },
            b: SlotRecord {
                version: String::new(),
                image: image_b,
                boots_left: 0,
            },
        },
    };
    save(state, &record)
}

/// Checks the bundle and copies it onto the slot that is not the last known-good one.
pub fn install(bundle: &Path, state: &Path, key: &VerifyingKey) -> Result<Installed, Error> {
    let manifest = read_bundle(bundle, key)?;
    let mut record = load(state)?;
    let slot = record.committed.other();
    let image = PathBuf::from(&record.get(slot).image);
    refuse_stick(bundle, state)?;
    refuse_stick(bundle, &image)?;
    copy_verified(&manifest, &image)?;
    let target = record.get_mut(slot);
    target.version = manifest.version.clone();
    target.boots_left = TRIAL_BOOTS;
    record.preferred = slot;
    record.trial_boot_id.clear();
    save(state, &record)?;
    Ok(Installed {
        version: manifest.version,
        slot,
        image,
    })
}

/// Installs the first signed bundle under `roots`. No bundle is [`None`].
pub fn install_mounted(
    roots: &[PathBuf],
    state: &Path,
    key: &VerifyingKey,
) -> Result<Option<Installed>, Error> {
    let Some(bundle) = crate::find_bundles(roots).into_iter().next() else {
        return Ok(None);
    };
    install(&bundle, state, key).map(Some)
}

/// Selects the system for this boot.
///
/// `boot_id` stays the same for one power-on, so calling this twice does not
/// spend the trial twice. A new id after an unconfirmed trial selects the
/// previous system.
pub fn boot(state: &Path, boot_id: &str) -> Result<BootChoice, Error> {
    if boot_id.is_empty() || boot_id.len() > 128 {
        return Err(Error::Format("the boot id is empty".to_string()));
    }
    let mut record = load(state)?;
    if record.preferred == record.committed {
        record.booted = record.preferred;
        record.trial_boot_id.clear();
        let choice = record.choice(record.booted, false);
        save(state, &record)?;
        return Ok(choice);
    }
    if record.trial_boot_id == boot_id {
        record.booted = record.preferred;
        let choice = record.choice(record.booted, false);
        save(state, &record)?;
        return Ok(choice);
    }
    let fresh = {
        let trial = record.get(record.preferred);
        !trial.version.is_empty() && trial.boots_left > 0
    };
    if !fresh {
        record.preferred = record.committed;
        record.booted = record.committed;
        record.trial_boot_id.clear();
        let choice = record.choice(record.booted, true);
        save(state, &record)?;
        return Ok(choice);
    }
    record.get_mut(record.preferred).boots_left -= 1;
    record.trial_boot_id = boot_id.to_string();
    record.booted = record.preferred;
    let choice = record.choice(record.booted, false);
    save(state, &record)?;
    Ok(choice)
}

/// Marks the booted system known-good when it is the one this boot selected.
///
/// Called on the system that is still running, before the trial has booted,
/// this leaves the pending update armed.
pub fn confirm(state: &Path) -> Result<BootChoice, Error> {
    let mut record = load(state)?;
    if record.booted != record.preferred {
        return Ok(record.choice(record.booted, false));
    }
    if record.get(record.booted).version.is_empty() {
        return Err(Error::Format(
            "the booted system has no version".to_string(),
        ));
    }
    let booted = record.booted;
    record.committed = booted;
    record.preferred = booted;
    record.trial_boot_id.clear();
    record.get_mut(booted).boots_left = 0;
    let choice = record.choice(booted, false);
    save(state, &record)?;
    Ok(choice)
}

fn copy_verified(manifest: &Manifest, image: &Path) -> Result<(), Error> {
    let source = PathBuf::from(&manifest.payload);
    copy_file(&source, image)?;
    let digest = sha256_file(image)?;
    if digest != manifest.sha256 {
        return Err(Error::Format(
            "the payload hash does not match the signed manifest".to_string(),
        ));
    }
    Ok(())
}

fn refuse_stick(bundle: &Path, path: &Path) -> Result<(), Error> {
    let stick = match bundle.file_name() {
        Some(name) if name == crate::BUNDLE_DIR => bundle.parent().unwrap_or(bundle),
        _ => bundle,
    };
    if is_within(path, stick) {
        return Err(Error::Format(
            "refusing to write the update onto the stick".to_string(),
        ));
    }
    Ok(())
}

fn path_text(path: &Path) -> Result<String, Error> {
    path.to_str()
        .map(str::to_string)
        .ok_or_else(|| Error::Format("a slot path is not text".to_string()))
}

fn load(state: &Path) -> Result<Record, Error> {
    let text = fs::read_to_string(state)?;
    serde_json::from_str(&text)
        .map_err(|_| Error::Format("the slot record is not a slot record".to_string()))
}

fn save(state: &Path, record: &Record) -> Result<(), Error> {
    let bytes =
        serde_json::to_vec_pretty(record).map_err(|error| Error::Format(error.to_string()))?;
    let mut bytes = bytes;
    bytes.push(b'\n');
    write_replacing(state, &bytes)
}
