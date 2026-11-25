//! Checks the wall player's USB automount files.
//!
//! udisks2 mounts the stick. The config forces that mount to be read-only,
//! and the udev rule starts it for a FAT32, exFAT, or HFS+ USB filesystem.

const CONF: &str = include_str!("mount_options.conf");
const RULES: &str = include_str!("99-djusb-automount.rules");
const UNIT: &str = include_str!("djusb-mount@.service");
const SCRIPT: &str = include_str!("djusb-mount");
const INSTALL: &str = include_str!("install.sh");

fn option_name(token: &str) -> &str {
    token.split_once('=').map(|(name, _)| name).unwrap_or(token)
}

fn option_names(list: &str) -> Vec<&str> {
    list.split(',')
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(option_name)
        .collect()
}

fn option_tokens(list: &str) -> Vec<&str> {
    list.split(',')
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .collect()
}

/// Keys in the `[defaults]` group. A later group would be a per-disk
/// override, which this appliance does not use.
fn defaults_entries(conf: &str) -> Vec<(&str, &str)> {
    let mut in_defaults = false;
    let mut entries = Vec::new();
    for line in conf.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(group) = line
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
        {
            in_defaults = group == "defaults";
            continue;
        }
        if !in_defaults {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        entries.push((key.trim(), value.trim()));
    }
    entries
}

fn entry<'a>(entries: &[(&'a str, &'a str)], key: &str) -> Option<&'a str> {
    entries
        .iter()
        .rev()
        .find(|(candidate, _)| *candidate == key)
        .map(|(_, value)| *value)
}

fn effective_tokens(conf: &str, filesystem: &str, suffix: &str) -> Vec<String> {
    let entries = defaults_entries(conf);
    let mut tokens = Vec::new();
    if let Some(global) = entry(&entries, suffix) {
        tokens.extend(option_tokens(global).into_iter().map(str::to_string));
    }
    let specific = format!("{filesystem}_{suffix}");
    if let Some(value) = entry(&entries, &specific) {
        tokens.extend(option_tokens(value).into_iter().map(str::to_string));
    }
    tokens
}

/// `options` is a findmnt OPTIONS field. Read-only means `ro` is set and
/// `rw` is not. A flag such as `errors=remount-ro` does not count as `ro`.
fn mount_options_are_read_only(options: &str) -> bool {
    let mut read_only = false;
    let mut read_write = false;
    for token in option_tokens(options) {
        match option_name(token) {
            "ro" => read_only = true,
            "rw" => read_write = true,
            _ => {}
        }
    }
    read_only && !read_write
}

fn allows(token: &str, allow: &[String]) -> bool {
    let name = option_name(token);
    allow
        .iter()
        .any(|candidate| candidate == token || candidate == name)
}

#[cfg(test)]
mod tests {
    use super::{
        allows, defaults_entries, effective_tokens, entry, mount_options_are_read_only,
        option_name, option_names, CONF, INSTALL, RULES, SCRIPT, UNIT,
    };

    #[test]
    fn fat32_exfat_and_hfs_default_to_read_only() {
        let entries = defaults_entries(CONF);
        let allow = entry(&entries, "allow").expect("common allow list");
        assert!(
            !option_names(allow).contains(&"rw"),
            "rw must not be an allowed udisks option"
        );
        for filesystem in ["vfat", "exfat", "hfsplus"] {
            let defaults = effective_tokens(CONF, filesystem, "defaults");
            let names: Vec<_> = defaults.iter().map(|token| option_name(token)).collect();
            assert!(
                names.contains(&"ro"),
                "{filesystem} must mount read-only, got {defaults:?}"
            );
            assert!(
                !names.contains(&"rw"),
                "{filesystem} must not mount read-write"
            );
            assert!(
                defaults.iter().any(|token| token == "umask=022"),
                "{filesystem} must be world-readable, got {defaults:?}"
            );
            let allowed = effective_tokens(CONF, filesystem, "allow");
            for token in &defaults {
                assert!(
                    allows(token, &allowed),
                    "{filesystem} default {token} is not allowed"
                );
            }
        }
    }

    #[test]
    fn filesystem_names_stay_readable() {
        let vfat = effective_tokens(CONF, "vfat", "defaults");
        let exfat = effective_tokens(CONF, "exfat", "defaults");
        let hfs = effective_tokens(CONF, "hfsplus", "defaults");
        assert!(vfat.iter().any(|token| token == "utf8=1"), "{vfat:?}");
        assert!(
            exfat.iter().any(|token| token == "iocharset=utf8"),
            "{exfat:?}"
        );
        assert!(hfs.iter().any(|token| token == "nls=utf8"), "{hfs:?}");
        assert!(
            !vfat.iter().any(|token| token.starts_with("flush")),
            "{vfat:?}"
        );
        assert!(
            !hfs.iter()
                .any(|token| token == "force" || token.starts_with("force=")),
            "{hfs:?}"
        );
        let hfs_allow = effective_tokens(CONF, "hfsplus", "allow");
        assert!(
            !hfs_allow.iter().any(|token| token == "force"),
            "force can write a journaled HFS+ volume"
        );
    }

    #[test]
    fn only_those_usb_filesystems_are_automounted() {
        assert!(RULES.contains(r#"ACTION=="add|change""#), "{RULES}");
        assert!(RULES.contains(r#"ENV{ID_BUS}=="usb""#), "{RULES}");
        assert!(
            RULES.contains(r#"ENV{ID_FS_USAGE}=="filesystem""#),
            "{RULES}"
        );
        assert!(
            RULES.contains(r#"ENV{ID_FS_TYPE}=="vfat|exfat|hfsplus""#),
            "{RULES}"
        );
        assert!(RULES.contains("djusb-mount@%k.service"), "{RULES}");
        assert!(
            RULES.contains(r#"ENV{UDISKS_FILESYSTEM_SHARED}="1""#),
            "a root mount must be reachable by the player user"
        );
        assert!(!RULES.contains("udisksctl"), "{RULES}");
        assert!(!RULES.contains("rw"), "{RULES}");
    }

    #[test]
    fn the_unit_mounts_through_the_script_and_leaves_with_the_device() {
        assert!(
            UNIT.contains("ExecStart=/usr/local/bin/djusb-mount %i"),
            "{UNIT}"
        );
        assert!(UNIT.contains("BindsTo=dev-%i.device"), "{UNIT}");
        assert!(UNIT.contains("udisks2.service"), "{UNIT}");
        assert!(!UNIT.contains("rw"), "{UNIT}");
    }

    #[test]
    fn the_script_mounts_with_udisks_and_rejects_a_writable_stick() {
        assert!(
            SCRIPT.contains("udisksctl mount --no-user-interaction --block-device"),
            "{SCRIPT}"
        );
        assert!(SCRIPT.contains("remount,ro"), "{SCRIPT}");
        assert!(SCRIPT.contains("*[!A-Za-z0-9]*"), "{SCRIPT}");
        assert!(SCRIPT.contains("*,rw,*"), "{SCRIPT}");
        assert!(SCRIPT.contains("*,ro,*"), "{SCRIPT}");
        for line in SCRIPT.lines() {
            if line.contains("udisksctl mount") {
                assert!(!line.contains("rw"), "{line}");
            }
        }
        assert!(!mount_options_are_read_only("rw,noatime"));
        assert!(!mount_options_are_read_only("ro,rw"));
        assert!(!mount_options_are_read_only("noatime"));
        assert!(mount_options_are_read_only("ro,noatime,nosuid"));
    }

    #[test]
    fn the_installer_puts_the_policy_where_udisks_and_udev_read_it() {
        for package in ["udisks2", "dosfstools", "exfatprogs", "hfsprogs"] {
            assert!(INSTALL.contains(package), "{package}");
        }
        assert!(
            INSTALL.contains("/etc/udisks2/mount_options.conf"),
            "{INSTALL}"
        );
        assert!(
            INSTALL.contains("/etc/udev/rules.d/99-djusb-automount.rules"),
            "{INSTALL}"
        );
        assert!(
            INSTALL.contains("/etc/systemd/system/djusb-mount@.service"),
            "{INSTALL}"
        );
        assert!(INSTALL.contains("/usr/local/bin/djusb-mount"), "{INSTALL}");
    }
}
