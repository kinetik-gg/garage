//! GPUs
//!
//! Three vendors, three completely different stories, and only two of them can answer
//! "how busy are you".
//!
//!   NVIDIA  `nvidia-smi`. Nothing useful in `sysfs` under the proprietary driver --
//!           `/sys/class/drm/cardN/device` carries no busy or VRAM attributes at all --
//!           so the subprocess is not laziness, it is the only interface.
//!   AMD     `amdgpu` sysfs. `gpu_busy_percent` and `mem_info_vram_*` are free reads.
//!   Intel   No busy percentage exists outside of perf counters that need root or
//!           `CAP_PERFMON`. So this reports the frequency ratio and says so.
//!           Presenting
//!           that ratio as "utilization" would be a fabrication: a GPU can sit at
//!           max clock doing nothing.

use crate::files::{as_float, read_int, read_text, sorted_children, MIB_BYTES};
use crate::json::{object, Value};
use garage_core::process;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Where a GPU stops being an aperture carved out of system RAM and starts being a card
/// with its own memory. There is no sysfs flag for this -- an APU and a dGPU are both PCI
/// class 0x030000 behind a bridge -- and a device-id table would be stale within a
/// generation. VRAM size is the one signal that has stayed true: integrated parts carve
/// 512 MiB to 2 GiB, discrete cards start at 4. It only decides which GPU the bar's
/// single strip follows, and the bar falls back to whatever is present if the guess finds
/// nothing.
const DISCRETE_VRAM_FLOOR: i64 = 4 * 1024 * 1024 * 1024;

/// One GPU as both surfaces see it.
///
/// The field order is the wire order: `discover_gpus()` output goes straight into a
/// `--stream` snapshot's `gpus` array, and all three vendor readers build their dicts
/// with these keys in this sequence, so one struct covers all three.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Gpu {
    /// The marketing name, or the vendor's generic one where sysfs publishes none.
    pub(crate) name: String,
    /// `nvidia`, `amd` or `intel`.
    pub(crate) vendor: &'static str,
    /// How busy, 0..100, or nothing where the vendor does not say.
    pub(crate) load: Option<f64>,
    /// What [`Gpu::load`] actually measures. Never "utilization" for Intel.
    pub(crate) load_kind: Option<&'static str>,
    /// VRAM in use, in bytes.
    pub(crate) vram_used: Option<i64>,
    /// VRAM fitted, in bytes.
    pub(crate) vram_total: Option<i64>,
    /// Die temperature in degrees Celsius.
    pub(crate) temp_c: Option<f64>,
    /// Whether this is a card with its own memory. See [`DISCRETE_VRAM_FLOOR`].
    pub(crate) discrete: bool,
}

impl From<&Gpu> for Value {
    fn from(gpu: &Gpu) -> Self {
        Self::Object(object! {
            "name" => Value::str(gpu.name.clone()),
            "vendor" => Value::str(gpu.vendor),
            "load" => Value::from(gpu.load),
            "load_kind" => gpu.load_kind.map_or(Value::Null, Value::str),
            "vram_used" => Value::from(gpu.vram_used),
            "vram_total" => Value::from(gpu.vram_total),
            "temp_c" => Value::from(gpu.temp_c),
            "discrete" => Value::Bool(gpu.discrete),
        })
    }
}

/// Every GPU on the box, discrete ones first.
///
/// Order matters because the bar has room for exactly one GPU strip and the discrete
/// card is the one whose load anybody is watching. The sort is stable and keyed on "not
/// discrete", so within each group the vendors keep their discovery order --
/// NVIDIA, AMD, Intel.
pub(crate) fn discover() -> Vec<Gpu> {
    let mut gpus = nvidia();
    gpus.extend(amd());
    gpus.extend(intel());
    gpus.sort_by_key(|gpu| !gpu.discrete);
    gpus
}

/// The GPU the bar's single strip follows.
pub(crate) fn primary(gpus: &[Gpu]) -> Option<&Gpu> {
    gpus.first()
}

/// Everything nvidia-smi will say in one call, one card per line.
///
/// A line that does not split into exactly five fields, or whose numbers do not parse,
/// is skipped rather than failing the whole call: a machine with one working card and
/// one in a bad state should still draw the working one.
fn nvidia() -> Vec<Gpu> {
    let Ok(output) = process::run(
        &[
            "nvidia-smi",
            "--query-gpu=name,utilization.gpu,memory.used,memory.total,temperature.gpu",
            "--format=csv,noheader,nounits",
        ],
        Duration::from_secs(2),
    ) else {
        return Vec::new();
    };
    if output.status != 0 {
        return Vec::new();
    }
    let output = output.stdout;
    if output.is_empty() {
        return Vec::new();
    }
    output.lines().filter_map(nvidia_line).collect()
}

fn nvidia_line(line: &str) -> Option<Gpu> {
    let parts: Vec<&str> = line.split(',').map(str::trim).collect();
    let [name, load, used, total, temperature] = parts.as_slice() else {
        return None;
    };
    // nvidia-smi quotes memory in MiB under `nounits`; everything downstream is bytes.
    // Multiplied as integers, as the Python's `int(parts[2]) * MIB` is, so a large
    // card's VRAM figure is exact rather than the nearest double.
    Some(Gpu {
        name: (*name).to_string(),
        vendor: "nvidia",
        load: Some(load.parse().ok()?),
        load_kind: Some("utilization"),
        vram_used: Some(used.parse::<i64>().ok()? * MIB_BYTES),
        vram_total: Some(total.parse::<i64>().ok()? * MIB_BYTES),
        temp_c: Some(temperature.parse().ok()?),
        discrete: true,
    })
}

/// Every DRM card belonging to one PCI vendor.
///
/// `card1-HDMI-A-1` and friends are connectors, not cards, and the dash in the name is
/// what separates the two.
fn drm_devices(vendor_id: &str) -> Vec<PathBuf> {
    let mut devices = Vec::new();
    for card in sorted_children(Path::new("/sys/class/drm")) {
        let name = card
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        // `card[0-9]*` in the Python, so the digit is required and bare `card` is not
        // one; and a dash anywhere in the name means a connector rather than a card.
        let Some(rest) = name.strip_prefix("card") else {
            continue;
        };
        if rest.contains('-') || !rest.starts_with(|ch: char| ch.is_ascii_digit()) {
            continue;
        }
        let device = card.join("device");
        if read_text(&device.join("vendor")).as_deref() == Some(vendor_id) {
            devices.push(device);
        }
    }
    devices
}

/// The die temperature hanging off a DRM device's hwmon node.
fn drm_hwmon_temp(device: &Path) -> Option<f64> {
    for entry in sorted_children(&device.join("hwmon")) {
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

/// amdgpu, which publishes everything as a plain sysfs read.
///
/// A device with neither a VRAM total nor a busy percentage is not an amdgpu this can
/// say anything about, so it is left out entirely rather than listed as unknown.
fn amd() -> Vec<Gpu> {
    let mut gpus = Vec::new();
    for device in drm_devices("0x1002") {
        let total = read_int(&device.join("mem_info_vram_total"));
        let used = read_int(&device.join("mem_info_vram_used"));
        let busy = read_int(&device.join("gpu_busy_percent"));
        if total.is_none() && busy.is_none() {
            continue;
        }
        gpus.push(Gpu {
            name: read_text(&device.join("product_name"))
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| "AMD Radeon Graphics".to_string()),
            vendor: "amd",
            load: busy.map(as_float),
            load_kind: busy.map(|_| "utilization"),
            vram_used: used,
            vram_total: total,
            temp_c: drm_hwmon_temp(&device),
            discrete: is_discrete(total),
        });
    }
    gpus
}

/// `i915` and `xe`, which publish clocks and nothing else.
fn intel() -> Vec<Gpu> {
    let mut gpus = Vec::new();
    for device in drm_devices("0x8086") {
        let load = intel_load(&device);
        let total = read_int(&device.join("lmem_total_bytes"));
        gpus.push(Gpu {
            name: read_text(&device.join("product_name"))
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| "Intel Graphics".to_string()),
            vendor: "intel",
            load,
            // Never called utilization. See the module docs.
            load_kind: load.map(|_| "activity (freq proxy)"),
            vram_used: None,
            vram_total: total,
            temp_c: drm_hwmon_temp(&device),
            discrete: is_discrete(total),
        });
    }
    gpus
}

/// The frequency ratio, which is the closest thing to a load an Intel GPU will give
/// without `CAP_PERFMON`. `i915` hangs the frequency knobs off the card, `xe` off a
/// per-tile path.
fn intel_load(device: &Path) -> Option<f64> {
    let card = device.parent()?;
    let mut actual = read_int(&card.join("gt_act_freq_mhz"));
    let mut maximum = read_int(&card.join("gt_max_freq_mhz"));
    if actual.is_none() {
        actual = read_int(&device.join("tile0/gt0/freq0/act_freq"));
        maximum = read_int(&device.join("tile0/gt0/freq0/max_freq"));
    }
    // `if actual is not None and maximum` -- a maximum of zero is as useless as a
    // missing one, and dividing by it would be worse.
    match (actual, maximum) {
        (Some(actual), Some(maximum)) if maximum != 0 => {
            Some((as_float(actual) / as_float(maximum) * 100.0).min(100.0))
        }
        _ => None,
    }
}

/// `bool(total and total >= DISCRETE_VRAM_FLOOR)` -- a missing or zero total is not
/// discrete, rather than an error.
fn is_discrete(total: Option<i64>) -> bool {
    total.is_some_and(|total| total != 0 && total >= DISCRETE_VRAM_FLOOR)
}

#[cfg(test)]
mod tests {
    use super::{is_discrete, nvidia_line, Gpu, DISCRETE_VRAM_FLOOR};
    use crate::json::dumps;
    use crate::json::Value;

    /// [`super::discover`]'s sort, over a list a test can supply.
    fn sorted(mut gpus: Vec<Gpu>) -> Vec<Gpu> {
        gpus.sort_by_key(|gpu| !gpu.discrete);
        gpus
    }

    fn gpu(name: &str, discrete: bool) -> Gpu {
        Gpu {
            name: name.to_string(),
            vendor: "amd",
            load: None,
            load_kind: None,
            vram_used: None,
            vram_total: None,
            temp_c: None,
            discrete,
        }
    }

    #[test]
    fn a_nvidia_smi_line_becomes_bytes_and_degrees() {
        let parsed = nvidia_line("NVIDIA GeForce RTX 5080, 1, 2138, 16303, 37").expect("parses");
        assert_eq!(parsed.name, "NVIDIA GeForce RTX 5080");
        assert_eq!(parsed.vendor, "nvidia");
        assert_eq!(parsed.load, Some(1.0));
        assert_eq!(parsed.load_kind, Some("utilization"));
        assert_eq!(parsed.vram_used, Some(2138 * 1_048_576));
        assert_eq!(parsed.vram_total, Some(16303 * 1_048_576));
        assert_eq!(parsed.temp_c, Some(37.0));
        assert!(parsed.discrete);
    }

    #[test]
    fn a_line_with_the_wrong_shape_or_bad_numbers_is_skipped() {
        assert_eq!(nvidia_line("only, four, fields, here"), None);
        assert_eq!(nvidia_line("a, b, c, d, e, f"), None);
        assert_eq!(nvidia_line("Card, x, 1, 2, 3"), None);
        assert_eq!(nvidia_line(""), None);
    }

    #[test]
    fn the_discrete_floor_is_four_gibibytes() {
        assert_eq!(DISCRETE_VRAM_FLOOR, 4_294_967_296);
        assert!(!is_discrete(None));
        assert!(!is_discrete(Some(0)));
        assert!(!is_discrete(Some(2 * 1024 * 1024 * 1024)));
        assert!(is_discrete(Some(DISCRETE_VRAM_FLOOR)));
        assert!(is_discrete(Some(16 * 1024 * 1024 * 1024)));
    }

    #[test]
    fn discrete_cards_come_first_and_the_rest_keep_their_discovery_order() {
        let ordered = sorted(vec![
            gpu("igpu-a", false),
            gpu("dgpu", true),
            gpu("igpu-b", false),
        ]);
        let names: Vec<&str> = ordered.iter().map(|gpu| gpu.name.as_str()).collect();
        assert_eq!(names, ["dgpu", "igpu-a", "igpu-b"]);
    }

    #[test]
    fn a_gpu_serialises_with_the_keys_in_the_order_the_python_built_them() {
        let card = Gpu {
            name: "NVIDIA GeForce RTX 5080".to_string(),
            vendor: "nvidia",
            load: Some(1.0),
            load_kind: Some("utilization"),
            vram_used: Some(2_241_855_488),
            vram_total: Some(17_094_934_528),
            temp_c: Some(37.0),
            discrete: true,
        };
        assert_eq!(
            dumps(&Value::from(&card)),
            concat!(
                r#"{"name": "NVIDIA GeForce RTX 5080", "vendor": "nvidia", "load": 1.0, "#,
                r#""load_kind": "utilization", "vram_used": 2241855488, "#,
                r#""vram_total": 17094934528, "temp_c": 37.0, "discrete": true}"#
            )
        );
    }

    #[test]
    fn a_gpu_with_nothing_to_report_serialises_its_gaps_as_nulls() {
        assert_eq!(
            dumps(&Value::from(&gpu("Intel Graphics", false))),
            concat!(
                r#"{"name": "Intel Graphics", "vendor": "amd", "load": null, "#,
                r#""load_kind": null, "vram_used": null, "vram_total": null, "#,
                r#""temp_c": null, "discrete": false}"#
            )
        );
    }
}
