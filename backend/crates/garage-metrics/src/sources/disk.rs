//! Disk: which block device is "the disk", and what its sysfs stat line says.

use crate::files::{as_float, read_int, read_text, sorted_children};
use garage_core::process;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// The patterns the fallback sweep tries, in order. Real disks first, then the names a
/// VM or an SD-card boot uses.
const DISK_PATTERNS: [&str; 4] = ["nvme", "sd", "vd", "mmcblk"];

/// `WAYBAR_DISK_DEVICE`, for a root this cannot follow on its own.
///
/// LVM, LUKS, bcachefs across two drives -- findmnt names a mapper device that has no
/// sysfs stat of its own. Kept separate from detection because the override has to beat
/// the cached device in the state file, and a caller that only consulted
/// [`detect_block_device`] would keep using yesterday's answer.
pub(crate) fn device_override() -> Option<String> {
    let raw = std::env::var("WAYBAR_DISK_DEVICE").unwrap_or_default();
    let trimmed = raw.trim();
    let name = trimmed.strip_prefix("/dev/").unwrap_or(trimmed);
    (!name.is_empty()).then(|| name.to_string())
}

/// The whole disk backing `/`, not the partition.
///
/// `/sys/class/block/<part>/stat` counts only that partition's traffic, so a graph built
/// on it misses everything on `/home` or a swap partition on the same drive. findmnt
/// gives the partition; the sysfs symlink's parent gives the disk it belongs to.
pub(crate) fn detect_block_device() -> Option<String> {
    if let Some(override_name) = device_override() {
        return Some(override_name);
    }
    if let Some(found) = from_findmnt() {
        return Some(found);
    }
    first_whole_disk()
}

/// What findmnt says, resolved from a partition to the disk it sits on.
fn from_findmnt() -> Option<String> {
    let output = process::run(
        &["findmnt", "--noheadings", "--output", "SOURCE", "/"],
        Duration::from_secs(2),
    )
    .ok()?;
    if output.status != 0 {
        return None;
    }
    let source = output.stdout;
    if source.is_empty() {
        return None;
    }
    // btrfs reports "/dev/nvme0n1p2[/@]" -- the subvolume is not part of the device
    // path.
    let trimmed = source.trim();
    let without_subvolume = trimmed.split_once('[').map_or(trimmed, |(head, _)| head);
    let name = Path::new(without_subvolume)
        .file_name()?
        .to_str()?
        .to_string();
    let sys_path = block_path(&name);
    if sys_path.join("partition").exists() {
        // resolve() follows the symlink into /sys/devices, where the partition's parent
        // directory is the disk it belongs to.
        return sys_path
            .canonicalize()
            .ok()?
            .parent()?
            .file_name()?
            .to_str()
            .map(ToString::to_string);
    }
    sys_path.exists().then_some(name)
}

/// The first whole disk sysfs knows about, for a root findmnt could not name.
///
/// Pattern order is preference order, and within a pattern the names are path-sorted.
/// A directory with a `partition` file is a partition rather than a disk and is skipped.
fn first_whole_disk() -> Option<String> {
    let children = sorted_children(Path::new("/sys/class/block"));
    for pattern in DISK_PATTERNS {
        for path in &children {
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if !matches_pattern(name, pattern) || path.join("partition").exists() {
                continue;
            }
            return Some(name.to_string());
        }
    }
    None
}

/// The four globs the Python sweeps with, as tests rather than as glob syntax.
///
/// `nvme*n*` needs an `n` somewhere after the prefix, which is what separates
/// `nvme0n1` (a namespace, and a whole disk) from `nvme0` (the controller, which has no
/// stat file). The other three are plain prefixes.
fn matches_pattern(name: &str, pattern: &str) -> bool {
    let Some(rest) = name.strip_prefix(pattern) else {
        return false;
    };
    if pattern == "nvme" {
        return rest.contains('n');
    }
    true
}

fn block_path(device: &str) -> PathBuf {
    PathBuf::from(format!("/sys/class/block/{device}"))
}

/// Whether the device still has a stat file, which is how a cached name is revalidated
/// against a re-plugged drive.
pub(crate) fn has_stat(device: &str) -> bool {
    block_path(device).join("stat").exists()
}

/// Sectors read and written. Fields 3 and 7 of the sysfs stat line.
pub(crate) fn counters(device: &str) -> (Option<i64>, Option<i64>) {
    let Some(text) = read_text(&block_path(device).join("stat")) else {
        return (None, None);
    };
    let fields: Vec<&str> = text.split_whitespace().collect();
    if fields.len() < 7 {
        return (None, None);
    }
    match (
        fields.get(2).and_then(|value| value.parse().ok()),
        fields.get(6).and_then(|value| value.parse().ok()),
    ) {
        (Some(read), Some(written)) => (Some(read), Some(written)),
        // The Python's try/except covers both int() calls together, so one unparseable
        // field discards the pair rather than half of it.
        _ => (None, None),
    }
}

/// The device's sector size, defaulting to the 512 bytes every stat line is quoted in
/// when sysfs does not say.
pub(crate) fn sector_size(device: &str) -> i64 {
    let size = read_int(&block_path(device).join("queue/hw_sector_size"));
    // `or 512` in the Python, which also replaces a zero -- a sector size of nothing
    // would make every throughput figure zero.
    match size {
        Some(size) if size != 0 => size,
        _ => 512,
    }
}

/// The drive's own temperature, where its controller publishes one.
pub(crate) fn temperature(device: &str) -> Option<f64> {
    let device_root = block_path(device).join("device");
    for entry in sorted_children(&device_root) {
        let name = entry
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        if !name.starts_with("hwmon") {
            continue;
        }
        if let Some(milli) = read_int(&entry.join("temp1_input")) {
            return Some(as_float(milli) / 1000.0);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{matches_pattern, DISK_PATTERNS};

    #[test]
    fn the_pattern_order_prefers_real_disks_over_virtual_ones() {
        assert_eq!(DISK_PATTERNS, ["nvme", "sd", "vd", "mmcblk"]);
    }

    #[test]
    fn the_nvme_pattern_wants_a_namespace_not_a_controller() {
        assert!(matches_pattern("nvme0n1", "nvme"));
        assert!(matches_pattern("nvme1n1", "nvme"));
        assert!(!matches_pattern("nvme0", "nvme"));
    }

    #[test]
    fn the_other_three_patterns_are_plain_prefixes() {
        assert!(matches_pattern("sda", "sd"));
        assert!(matches_pattern("vda", "vd"));
        assert!(matches_pattern("mmcblk0", "mmcblk"));
        assert!(!matches_pattern("loop0", "sd"));
        assert!(!matches_pattern("dm-0", "vd"));
    }
}
