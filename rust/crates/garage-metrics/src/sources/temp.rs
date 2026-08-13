//! CPU temperature: found by chip name rather than by index.

use crate::files::{as_float, read_int, read_text, sorted_children};
use std::path::{Path, PathBuf};

/// Preference order, not discovery order. A box can expose several of these at once
/// (k10temp plus an nct6xxx superio that also claims a CPU reading) and k10temp/coretemp
/// are the ones reading the on-die sensor.
const CPU_TEMP_CHIPS: [&str; 3] = ["k10temp", "coretemp", "zenpower"];

/// The package or control temperature, whichever the chip calls it. `temp1_input` on a
/// k10temp is Tctl and on a coretemp is core 0, so the labels are what actually
/// distinguish "the CPU is this hot" from "one core is this hot".
const PACKAGE_LABELS: [&str; 4] = ["tctl", "tdie", "package id 0", "package"];

/// A temperature in degrees Celsius and the sensor it came from, for the tooltip.
pub(crate) type Reading = (f64, String);

/// The CPU package temperature, found by chip name rather than by index.
///
/// `hwmon` numbering is assigned in probe order and probe order is not stable across boots
/// -- the `NVMe` controller and the GPU race the CPU sensor for `hwmon0` -- so anything
/// that hardcodes an index reports the SSD's temperature after an unlucky reboot. The scan
/// is eight small reads and costs well under a millisecond, which is cheaper than being
/// wrong.
pub(crate) fn cpu_temperature() -> Option<Reading> {
    let chips = cpu_chips();
    for wanted_chip in CPU_TEMP_CHIPS {
        let Some(root) = chips.iter().find(|(name, _)| name == wanted_chip) else {
            continue;
        };
        if let Some(reading) = from_chip(wanted_chip, &root.1) {
            return Some(reading);
        }
    }
    thermal_zone()
}

/// Every hwmon directory whose `name` is one of the chips worth asking, keyed by that
/// name. The first directory to claim a name keeps it -- `setdefault`, over a list
/// sorted by path, so the answer does not depend on `readdir` order.
fn cpu_chips() -> Vec<(String, PathBuf)> {
    let mut chips: Vec<(String, PathBuf)> = Vec::new();
    for hwmon in sorted_children(Path::new("/sys/class/hwmon")) {
        let Some(name) = read_text(&hwmon.join("name")) else {
            continue;
        };
        if CPU_TEMP_CHIPS.contains(&name.as_str())
            && !chips.iter().any(|(existing, _)| *existing == name)
        {
            chips.push((name, hwmon));
        }
    }
    chips
}

/// One chip's answer: the first of [`PACKAGE_LABELS`] it publishes, or `temp1_input` if
/// it labels nothing.
///
/// The labels are matched case-insensitively because `Tctl` and `Package id 0` are
/// spelled with capitals in sysfs, but the *reported* label keeps the file's own casing
/// -- the tooltip says `k10temp Tctl`, not `k10temp tctl`.
fn from_chip(chip: &str, root: &Path) -> Option<Reading> {
    let labelled = labels(root);
    for wanted in PACKAGE_LABELS {
        let Some((label, input)) = labelled
            .iter()
            .find(|(key, _, _)| key == wanted)
            .map(|(_, label, input)| (label, input))
        else {
            continue;
        };
        if let Some(milli) = read_int(input) {
            return Some((as_float(milli) / 1000.0, format!("{chip} {label}")));
        }
    }
    let milli = read_int(&root.join("temp1_input"))?;
    Some((as_float(milli) / 1000.0, chip.to_string()))
}

/// Every `tempN_label` under a chip, as (lowercased label, label as written, the
/// matching `tempN_input`). First writer of a lowercased label wins, over a
/// path-sorted list.
fn labels(root: &Path) -> Vec<(String, String, PathBuf)> {
    let mut found: Vec<(String, String, PathBuf)> = Vec::new();
    for path in sorted_children(root) {
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !name.starts_with("temp") || !name.ends_with("_label") {
            continue;
        }
        let Some(label) = read_text(&path).filter(|label| !label.is_empty()) else {
            continue;
        };
        let key = label.to_lowercase();
        if found.iter().any(|(existing, _, _)| *existing == key) {
            continue;
        }
        let input = path.with_file_name(name.replace("_label", "_input"));
        found.push((key, label, input));
    }
    found
}

/// Last resort for boxes with no `hwmon` CPU driver at all -- most ARM `SoCs`, some VMs.
/// Reported under its own zone type so nobody mistakes it for a package reading.
fn thermal_zone() -> Option<Reading> {
    for zone in sorted_children(Path::new("/sys/class/thermal")) {
        let name = zone
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        if !name.starts_with("thermal_zone") {
            continue;
        }
        let zone_type = read_text(&zone.join("type"))
            .unwrap_or_default()
            .to_lowercase();
        if !(zone_type.contains("x86_pkg")
            || zone_type.contains("cpu")
            || zone_type.contains("soc"))
        {
            continue;
        }
        if let Some(milli) = read_int(&zone.join("temp")) {
            return Some((as_float(milli) / 1000.0, zone_type));
        }
    }
    None
}

#[cfg(test)]
// Byte-parity tests: a fixture row of the wrong shape is a broken fixture and panicking
// on it is the report, and a double that is only approximately the Python's is a failure
// rather than a pass -- so indexing and exact float comparison are both the point here.
#[allow(
    clippy::indexing_slicing,
    clippy::float_cmp,
    clippy::cast_precision_loss
)]
mod tests {
    use super::{from_chip, labels, CPU_TEMP_CHIPS, PACKAGE_LABELS};
    use crate::scratch::Scratch;
    use std::fs;

    fn chip(scratch: &Scratch, files: &[(&str, &str)]) -> std::path::PathBuf {
        let root = scratch.join("hwmon0");
        fs::create_dir_all(&root).expect("mkdir");
        for (name, body) in files {
            fs::write(root.join(name), body).expect("write");
        }
        root
    }

    #[test]
    fn the_preference_order_puts_the_on_die_sensors_first() {
        assert_eq!(CPU_TEMP_CHIPS, ["k10temp", "coretemp", "zenpower"]);
        assert_eq!(PACKAGE_LABELS, ["tctl", "tdie", "package id 0", "package"]);
    }

    #[test]
    fn a_labelled_package_reading_wins_and_keeps_the_labels_own_casing() {
        let scratch = Scratch::new("temp-labelled");
        let root = chip(
            &scratch,
            &[
                ("temp1_label", "Tctl\n"),
                ("temp1_input", "68750\n"),
                ("temp2_label", "Tccd1\n"),
                ("temp2_input", "51000\n"),
            ],
        );
        assert_eq!(
            from_chip("k10temp", &root),
            Some((68.75, "k10temp Tctl".to_string()))
        );
    }

    #[test]
    fn the_label_preference_beats_the_file_order() {
        let scratch = Scratch::new("temp-order");
        let root = chip(
            &scratch,
            &[
                ("temp1_label", "Tccd1\n"),
                ("temp1_input", "51000\n"),
                ("temp2_label", "Tdie\n"),
                ("temp2_input", "60500\n"),
            ],
        );
        assert_eq!(
            from_chip("k10temp", &root),
            Some((60.5, "k10temp Tdie".to_string()))
        );
    }

    #[test]
    fn an_unlabelled_chip_falls_back_to_temp1_input_and_reports_only_the_chip() {
        let scratch = Scratch::new("temp-bare");
        let root = chip(&scratch, &[("temp1_input", "45000\n")]);
        assert_eq!(
            from_chip("coretemp", &root),
            Some((45.0, "coretemp".to_string()))
        );
    }

    #[test]
    fn a_chip_with_nothing_readable_is_nothing() {
        let scratch = Scratch::new("temp-empty");
        let root = chip(&scratch, &[("name", "k10temp\n")]);
        assert_eq!(from_chip("k10temp", &root), None);
    }

    #[test]
    fn a_label_with_no_input_beside_it_falls_through_to_the_next_candidate() {
        let scratch = Scratch::new("temp-orphan");
        let root = chip(
            &scratch,
            &[
                ("temp1_label", "Tctl\n"),
                ("temp2_label", "Tdie\n"),
                ("temp2_input", "60500\n"),
            ],
        );
        assert_eq!(
            from_chip("k10temp", &root),
            Some((60.5, "k10temp Tdie".to_string()))
        );
    }

    #[test]
    fn each_label_maps_to_the_input_beside_it() {
        let scratch = Scratch::new("temp-pairs");
        let root = chip(
            &scratch,
            &[("temp1_label", "Tctl\n"), ("temp10_label", "Tdie\n")],
        );
        let found = labels(&root);
        // Path-sorted, so temp10 comes before temp1 -- the same order the Python's
        // sorted(glob(...)) produces.
        assert_eq!(found[0].0, "tdie");
        assert!(found[0].2.ends_with("temp10_input"));
        assert_eq!(found[1].0, "tctl");
        assert!(found[1].2.ends_with("temp1_input"));
    }
}
