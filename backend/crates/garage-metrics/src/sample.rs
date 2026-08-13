//! Read one widget's sources and return its display fields.
//!
//! Each function returns the fields to merge into the state -- `value`/`value2` feed the
//! graph, `display`/`extra`/`tooltip` feed the strip, `counters` carry the next delta.
//! Only the requested widget's sources are touched: waybar runs one of these per tick per
//! module, and nvidia-smi costs ~28ms that the CPU widget must not pay.
//!
//! The key order of each returned object is the wire order of the state file, because
//! `state.update(current)` appends the new keys in the order they were built. Do not
//! reorder an [`object!`] here without expecting the state file's bytes to move.

use crate::fault::Fault;
use crate::files::{as_float, as_int, compact_rate, log_scale, rate, MIB};
use crate::json::{object, Object, Value};
use crate::pyfmt::grouped;
use crate::sources::{cpu, disk, gpu, memory, net, temp};

/// Two callers can overlap: waybar's tick and a manual run, or a tick that ran long.
/// Both take the lock, so the second one would otherwise sample the counters again
/// microseconds later and push a meaningless near-zero rate into the history. The guard
/// makes the second caller reuse what the first stored.
pub(crate) const MIN_SAMPLE_INTERVAL: f64 = 1.5;

/// A gibibyte, which is what every memory and VRAM figure in a tooltip is quoted in.
const GIB: f64 = 1024.0 * 1024.0 * 1024.0;

/// Read one widget's sources and return its display fields.
///
/// # Errors
///
/// Returns a [`Fault`] for anything the Python would have raised out of here, in the
/// same exception class -- see [`crate::fault`] for why the class survives the port.
pub(crate) fn sample_widget(widget: &str, state: &Object, now: f64) -> Result<Object, Fault> {
    match widget {
        "cpu" => sample_cpu(state),
        "memory" => sample_memory(),
        "temp" => sample_temp(),
        "network" => sample_network(state, now),
        "disk" => sample_disk(state, now),
        // The Python has no final `if`: anything that is not one of the five above falls
        // through to the GPU branch. `main` has already refused every name that is not a
        // widget, so in practice this arm is only ever reached as "gpu".
        _ => sample_gpu(),
    }
}

fn sample_cpu(state: &Object) -> Result<Object, Fault> {
    let counters = cpu::counters()?;
    let percent = cpu::percent(counters, stored_counters(state));
    let mut parts = vec![format!("CPU {percent:.1}%")];
    if let Some(load) = cpu::loadavg().filter(|load| !load.is_empty()) {
        let (Some(one), Some(five), Some(fifteen)) = (load.first(), load.get(1), load.get(2))
        else {
            return Err(Fault::index());
        };
        parts.push(format!("load {one:.2} {five:.2} {fifteen:.2}"));
    }
    Ok(object! {
        "value" => Value::Float(percent),
        "display" => Value::str(format!("{percent:.0}%")),
        "tooltip_parts" => Value::strings(parts),
        "active" => Value::Bool(percent >= 25.0),
        "counters" => Value::List(vec![Value::Int(counters.total), Value::Int(counters.idle)]),
    })
}

/// The previous tick's `[total, idle]`, if that is what the state actually holds.
///
/// `isinstance(previous, list) and len(previous) == 2` in the Python: anything else --
/// a first run, a state file from a different widget -- is no previous reading at all,
/// and a CPU percentage with no interval behind it is zero.
fn stored_counters(state: &Object) -> Option<cpu::Counters> {
    let stored = state.get("counters")?.as_list()?;
    let [total, idle] = stored else {
        return None;
    };
    Some(cpu::Counters {
        total: as_int(total.as_number()?),
        idle: as_int(idle.as_number()?),
    })
}

fn sample_memory() -> Result<Object, Fault> {
    let memory = memory::values()?;
    let pressure = memory::pressure();
    let mut parts = vec![
        format!(
            "Memory {:.1} / {:.1} GiB ({:.1}%)",
            as_float(memory.used) / GIB,
            as_float(memory.total) / GIB,
            memory.percent
        ),
        format!("available {:.1} GiB", as_float(memory.available) / GIB),
    ];
    if let Some(pressure) = pressure {
        parts.push(format!("pressure {pressure:.2}%"));
    }
    Ok(object! {
        "value" => Value::Float(memory.percent),
        "display" => Value::str(format!("{:.0}%", memory.percent)),
        "tooltip_parts" => Value::strings(parts),
        "active" => Value::Bool(memory.percent >= 70.0),
    })
}

fn sample_temp() -> Result<Object, Fault> {
    let (celsius, label) =
        temp::cpu_temperature().ok_or_else(|| Fault::os("no CPU temperature sensor"))?;
    let label = if label.is_empty() {
        "unknown sensor".to_string()
    } else {
        label
    };
    Ok(object! {
        "value" => Value::Float(celsius.clamp(0.0, 100.0)),
        "display" => Value::str(format!("{celsius:.0}\u{b0}")),
        "tooltip_parts" => Value::strings([format!("CPU {celsius:.1}\u{b0}C"), label]),
        "active" => Value::Bool(celsius >= 75.0),
    })
}

/// Re-detected every tick rather than cached: `/proc/net/route` is one cheap read, and a
/// VPN coming up moves the default route to a new interface while the old one is still
/// present, so a cached name would keep reporting a link that no longer carries anything.
fn sample_network(state: &Object, now: f64) -> Result<Object, Fault> {
    let interface = net::default_interface()?
        .filter(|name| !name.is_empty())
        .ok_or_else(|| Fault::os("no default route"))?;
    let (received, sent) = net::counters(&interface);
    let (Some(received), Some(sent)) = (received, sent) else {
        return Err(Fault::os(format!("no counters for {interface}")));
    };
    // Counters belong to an interface, so a route that moved invalidates them -- a fresh
    // card's small counters against the old card's large ones is a negative delta, and
    // clamping that to zero would still be a made-up number rather than a missing one.
    let previous = previous_rates(state, &interface, now);
    let (down, up) = match previous {
        Some((before_rx, before_tx, elapsed)) => (
            rate(Some(received), Some(before_rx), elapsed) / MIB,
            rate(Some(sent), Some(before_tx), elapsed) / MIB,
        ),
        None => (0.0, 0.0),
    };
    Ok(object! {
        "value" => Value::Float(log_scale(down)),
        "value2" => Value::Float(log_scale(up)),
        "display" => Value::str(compact_rate(down)),
        "extra" => Value::str(compact_rate(up)),
        "tooltip_parts" => Value::strings([
            interface.clone(),
            format!("\u{2193} {} MiB/s", grouped(down, 2)),
            format!("\u{2191} {} MiB/s", grouped(up, 2)),
        ]),
        "active" => Value::Bool(down + up >= 0.1),
        "device" => Value::str(interface),
        "counters" => Value::List(vec![
            Value::Int(received),
            Value::Int(sent),
            Value::Float(now),
        ]),
    })
}

/// Cached across ticks because detection shells out to findmnt and the root device does
/// not move while the session is up. Revalidated every tick against the stat file, so an
/// override or a re-plug still lands.
fn sample_disk(state: &Object, now: f64) -> Result<Object, Fault> {
    let device = resolve_disk(state).ok_or_else(|| Fault::os("no block device backing /"))?;
    let (read_sectors, write_sectors) = disk::counters(&device);
    let Some(read_sectors) = read_sectors else {
        return Err(Fault::os(format!("no stat for {device}")));
    };
    let (mut read_mib, mut write_mib) = (0.0, 0.0);
    if let Some((before_read, before_write, elapsed)) = previous_rates(state, &device, now) {
        let size = as_float(disk::sector_size(&device));
        read_mib = rate(Some(read_sectors), Some(before_read), elapsed) * size / MIB;
        write_mib = rate(write_sectors, Some(before_write), elapsed) * size / MIB;
    }
    let total_mib = read_mib + write_mib;
    let mut parts = vec![
        device.clone(),
        format!("read {} MiB/s", grouped(read_mib, 1)),
        format!("write {} MiB/s", grouped(write_mib, 1)),
    ];
    if let Some(celsius) = disk::temperature(&device) {
        parts.push(format!("{celsius:.1}\u{b0}C"));
    }
    parts.push("log scale".to_string());
    Ok(object! {
        "value" => Value::Float(log_scale(total_mib)),
        "display" => Value::str(compact_rate(total_mib)),
        "tooltip_parts" => Value::strings(parts),
        "active" => Value::Bool(total_mib >= 1.0),
        "device" => Value::str(device),
        "counters" => Value::List(vec![
            Value::Int(read_sectors),
            Value::from(write_sectors),
            Value::Float(now),
        ]),
    })
}

/// The override, then the cached name if it still has a stat file, then a fresh
/// detection.
///
/// A cached value that is truthy but not a string is spelled with `str()`, because that
/// is what the Python's `f"/sys/class/block/{device}/stat"` does to it. The Python then
/// carries the *original* object into the state it writes back, where this carries the
/// string; nothing in this crate can write a non-string `device`, so the difference is
/// reachable only from a hand-edited state file and is recorded rather than chased.
fn resolve_disk(state: &Object) -> Option<String> {
    let cached = disk::device_override().or_else(|| {
        state
            .get("device")
            .filter(|value| value.is_truthy())
            .map(Value::py_str)
    });
    match cached {
        Some(device) if disk::has_stat(&device) => Some(device),
        _ => disk::detect_block_device(),
    }
}

/// The stored `[first, second, timestamp]` triple, but only if it belongs to the device
/// being sampled now.
///
/// The comparison is `state.get("device") == device` in the Python, with a `str` on the
/// right, so a stored value of any other type is unequal however it would print --
/// Python's `5 == "5"` is `False`. Matching only on [`Value::Str`] is what keeps that
/// true here, where comparing the `str()` of both sides would have said yes.
fn previous_rates(state: &Object, device: &str, now: f64) -> Option<(i64, i64, f64)> {
    match state.get("device") {
        Some(Value::Str(stored)) if stored == device => (),
        _ => return None,
    }
    let stored = state.get("counters")?.as_list()?;
    let [first, second, timestamp] = stored else {
        return None;
    };
    Some((
        as_int(first.as_number()?),
        as_int(second.as_number()?),
        now - timestamp.as_number()?,
    ))
}

fn sample_gpu() -> Result<Object, Fault> {
    let gpus = gpu::discover();
    let card = gpu::primary(&gpus).ok_or_else(|| Fault::os("no GPU found"))?;
    let vram_percent = match (card.vram_used, card.vram_total) {
        (Some(used), Some(total)) if total != 0 => Some(as_float(used) / as_float(total) * 100.0),
        _ => None,
    };
    Ok(object! {
        "value" => Value::Float(card.load.unwrap_or(0.0)),
        "value2" => Value::from(vram_percent),
        "display" => Value::str(card.load.map_or_else(
            || "--".to_string(),
            |load| format!("{load:.0}%"),
        )),
        "extra" => Value::str(card.vram_used.map_or_else(
            String::new,
            |used| format!("{:.1}G", as_float(used) / GIB),
        )),
        "tooltip_parts" => Value::strings(gpu_tooltip(&gpus, card)),
        "active" => Value::Bool(card.load.is_some_and(|load| load >= 10.0)),
    })
}

fn gpu_tooltip(gpus: &[gpu::Gpu], card: &gpu::Gpu) -> Vec<String> {
    let mut parts = vec![card.name.clone()];
    if let Some(load) = card.load {
        // The Python interpolates `gpu['load_kind']` unconditionally here, so a card
        // with a load and no kind would say `None 5%`. All three readers set the two
        // together, so that spelling is unreachable -- kept anyway rather than quietly
        // dropping the line, because dropping it would be the invented behaviour.
        let kind = card.load_kind.unwrap_or("None");
        parts.push(format!("{kind} {load:.0}%"));
    }
    match (card.vram_used, card.vram_total) {
        (Some(used), Some(total)) if total != 0 => parts.push(format!(
            "VRAM {:.1} / {:.1} GiB",
            as_float(used) / GIB,
            as_float(total) / GIB
        )),
        (_, Some(total)) if total != 0 => {
            parts.push(format!("VRAM {:.1} GiB", as_float(total) / GIB));
        }
        _ => (),
    }
    if let Some(celsius) = card.temp_c {
        parts.push(format!("{celsius:.0}\u{b0}C"));
    }
    if gpus.len() > 1 {
        parts.push(format!("+{} more GPU", gpus.len() - 1));
    }
    parts
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
    use super::{gpu_tooltip, previous_rates, stored_counters};
    use crate::json::{object, Object, Value};
    use crate::sources::gpu::Gpu;

    fn card(load: Option<f64>, used: Option<i64>, total: Option<i64>) -> Gpu {
        Gpu {
            name: "NVIDIA GeForce RTX 5080".to_string(),
            vendor: "nvidia",
            load,
            load_kind: load.map(|_| "utilization"),
            vram_used: used,
            vram_total: total,
            temp_c: Some(37.0),
            discrete: true,
        }
    }

    #[test]
    fn a_cpu_counter_pair_is_only_reused_when_it_is_a_pair() {
        let two = object! { "counters" => Value::List(vec![Value::Int(9), Value::Int(4)]) };
        let counters = stored_counters(&two).expect("a pair");
        assert_eq!(counters.total, 9);
        assert_eq!(counters.idle, 4);

        assert!(stored_counters(&Object::new()).is_none());
        let three = object! {
            "counters" => Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)]),
        };
        assert!(stored_counters(&three).is_none());
        let not_a_list = object! { "counters" => Value::Int(1) };
        assert!(stored_counters(&not_a_list).is_none());
    }

    #[test]
    fn a_rate_triple_belonging_to_another_device_is_not_reused() {
        let state = object! {
            "device" => Value::str("enp11s0"),
            "counters" => Value::List(vec![
                Value::Int(100),
                Value::Int(200),
                Value::Float(1000.0),
            ]),
        };
        assert_eq!(
            previous_rates(&state, "enp11s0", 1002.0),
            Some((100, 200, 2.0))
        );
        assert_eq!(previous_rates(&state, "wlan0", 1002.0), None);
        assert_eq!(previous_rates(&Object::new(), "enp11s0", 1002.0), None);
    }

    #[test]
    fn a_rate_triple_of_the_wrong_length_is_not_reused() {
        let state = object! {
            "device" => Value::str("nvme0n1"),
            "counters" => Value::List(vec![Value::Int(1), Value::Int(2)]),
        };
        assert_eq!(previous_rates(&state, "nvme0n1", 1.0), None);
    }

    #[test]
    fn a_gpu_tooltip_names_the_card_its_load_its_vram_and_its_temperature() {
        let one = card(Some(1.0), Some(2_241_855_488), Some(17_094_934_528));
        assert_eq!(
            gpu_tooltip(std::slice::from_ref(&one), &one),
            [
                "NVIDIA GeForce RTX 5080",
                "utilization 1%",
                "VRAM 2.1 / 15.9 GiB",
                "37\u{b0}C",
            ]
        );
    }

    #[test]
    fn a_second_gpu_is_counted_rather_than_listed() {
        let one = card(Some(1.0), None, None);
        let two = vec![one.clone(), one.clone()];
        assert_eq!(
            gpu_tooltip(&two, &one).last().map(String::as_str),
            Some("+1 more GPU")
        );
    }

    #[test]
    fn a_card_with_a_total_but_no_used_figure_reports_only_the_total() {
        let one = card(None, None, Some(17_094_934_528));
        let parts = gpu_tooltip(std::slice::from_ref(&one), &one);
        assert_eq!(parts[1], "VRAM 15.9 GiB");
        // No load means no load line at all rather than a zero.
        assert!(!parts.iter().any(|part| part.contains("utilization")));
    }
}
