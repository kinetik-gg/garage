//! Snapshots
//!
//! Full-system snapshots with the counters a rate needs held in memory.
//!
//! [`Snapshotter`] deliberately does not touch the bar's history files. The bar owns
//! those and guards them with a minimum sample interval; a second writer at 1 Hz would
//! double the sample rate and the two would race over the same 120 slots. The popover
//! reads that history exactly once, as the seed, and keeps its own from there.

use crate::dirs::{Dirs, WIDGETS};
use crate::fault::Fault;
use crate::files::{as_float, rate};
use crate::json::{object, Object, Value};
use crate::pyfmt::py_round;
use crate::sources::{cpu, disk, gpu, memory, net, temp};
use crate::state::load_state;

/// One read of every counter a rate is a delta between, with the moment it was taken.
#[derive(Debug, Clone)]
struct Counters {
    /// `time.time()` -- wall clock, because the snapshot publishes it and the popover
    /// plots against it.
    timestamp: f64,
    cpu: cpu::Counters,
    cores: Vec<cpu::Counters>,
    received: Option<i64>,
    sent: Option<i64>,
    reads: Option<i64>,
    writes: Option<i64>,
}

/// The rate counters, held between snapshots rather than on disk.
#[derive(Debug)]
pub(crate) struct Snapshotter {
    device: Option<String>,
    sector_size: i64,
    interface: Option<String>,
    previous: Option<Counters>,
}

impl Snapshotter {
    /// Resolve the block device and the interface once, as the Python's `__init__` does.
    ///
    /// A failure to read `/proc/net/route` here is swallowed into "no interface", which
    /// is what the Python's own constructor does by never wrapping the call -- the
    /// `ValueError` would escape the constructor entirely, and there is nothing in this
    /// binary that could catch it there.
    pub(crate) fn new() -> Self {
        let device = disk::detect_block_device();
        let sector_size = device.as_deref().map_or(512, disk::sector_size);
        Self {
            device,
            sector_size,
            interface: net::default_interface().ok().flatten(),
            previous: None,
        }
    }

    fn counters(&self) -> Result<Counters, Fault> {
        let (received, sent) = self
            .interface
            .as_deref()
            .map_or((None, None), net::counters);
        let (reads, writes) = self.device.as_deref().map_or((None, None), disk::counters);
        Ok(Counters {
            timestamp: now(),
            cpu: cpu::counters()?,
            cores: cpu::core_counters()?,
            received,
            sent,
            reads,
            writes,
        })
    }

    /// A counter delta needs two reads. `--once` has no earlier process to inherit them
    /// from, so it takes both itself and pays this in latency.
    ///
    /// # Errors
    ///
    /// Returns whatever `/proc/stat` could not give.
    pub(crate) fn prime(&mut self) -> Result<(), Fault> {
        self.previous = Some(self.counters()?);
        Ok(())
    }

    /// One snapshot, and the counters kept for the next one.
    ///
    /// # Errors
    ///
    /// Returns whatever a source could not give. In `--stream` an `OSError` or a
    /// `ValueError` becomes an error object on the wire and the loop carries on; the
    /// other three kinds end the stream, matching the Python's narrower `except`.
    pub(crate) fn snapshot(&mut self) -> Result<Value, Fault> {
        let current = self.counters()?;
        let mut previous = self.previous.clone();
        let elapsed = previous
            .as_ref()
            .map_or(0.0, |before| current.timestamp - before.timestamp);
        let cores = per_core(&current, previous.as_ref());

        // Re-detected each snapshot: a VPN coming up or a dock being plugged in moves
        // the default route mid-stream, and a stream that pinned the interface at
        // startup would silently report zero forever.
        //
        // Dropping `previous` here is what makes the *whole* snapshot fall back to its
        // no-interval answers -- the CPU load below reads zero for one tick too, because
        // the Python computes it after this point and the per-core figures above it.
        // That asymmetry is the Python's, and it is preserved rather than tidied.
        let interface = net::default_interface()?;
        if interface != self.interface {
            self.interface.clone_from(&interface);
            previous = None;
        }

        let memory = memory::values()?;
        let (celsius, label) = split_temperature(temp::cpu_temperature());
        self.previous = Some(current.clone());
        Ok(Value::Object(object! {
            "ts" => Value::Float(current.timestamp),
            "cpu" => Self::cpu_section(&current, previous.as_ref(), cores),
            "temp" => Value::Object(object! {
                "cpu_c" => Value::from(celsius),
                "label" => Value::from(label),
            }),
            "memory" => Value::Object(object! {
                "used" => Value::Int(memory.used),
                "total" => Value::Int(memory.total),
                "available" => Value::Int(memory.available),
                "pressure_some_avg10" => Value::from(memory::pressure()),
            }),
            "network" => self.network_section(&current, previous.as_ref(), elapsed),
            "disk" => self.disk_section(&current, previous.as_ref(), elapsed),
            "gpus" => Value::List(gpu::discover().iter().map(Value::from).collect()),
        }))
    }

    /// The aggregate load, the per-core figures and the kernel's own load averages.
    ///
    /// `previous` here is the one the interface check may already have dropped, which is
    /// why an interface that moved zeroes this tick's CPU load as well; `cores` was
    /// computed before that and is not affected. See [`Snapshotter::snapshot`].
    fn cpu_section(current: &Counters, previous: Option<&Counters>, cores: Vec<Value>) -> Value {
        Value::Object(object! {
            "load" => Value::Float(py_round(
                cpu::percent(current.cpu, previous.map(|before| before.cpu)),
                2,
            )),
            "per_core" => Value::List(cores),
            "loadavg" => cpu::loadavg().map_or(Value::Null, |load| {
                Value::List(load.into_iter().map(Value::Float).collect())
            }),
        })
    }

    /// Bytes per second each way on whichever interface carries the default route now.
    fn network_section(
        &self,
        current: &Counters,
        previous: Option<&Counters>,
        elapsed: f64,
    ) -> Value {
        Value::Object(object! {
            "iface" => self.interface.clone().map_or(Value::Null, Value::str),
            "rx_bps" => Value::Float(py_round(
                rate(current.received, previous.and_then(|before| before.received), elapsed),
                1,
            )),
            "tx_bps" => Value::Float(py_round(
                rate(current.sent, previous.and_then(|before| before.sent), elapsed),
                1,
            )),
        })
    }

    /// Bytes per second each way on the disk backing `/`, sectors scaled by the device's
    /// own sector size rather than the 512 the stat line is nominally quoted in.
    fn disk_section(&self, current: &Counters, previous: Option<&Counters>, elapsed: f64) -> Value {
        Value::Object(object! {
            "device" => self.device.clone().map_or(Value::Null, Value::str),
            "read_bps" => Value::Float(py_round(
                rate(current.reads, previous.and_then(|before| before.reads), elapsed)
                    * as_float(self.sector_size),
                1,
            )),
            "write_bps" => Value::Float(py_round(
                rate(current.writes, previous.and_then(|before| before.writes), elapsed)
                    * as_float(self.sector_size),
                1,
            )),
        })
    }
}

/// One percentage per core, but only when the two reads saw the same number of cores.
/// A core coming online mid-stream makes the two lists disagree, and pairing them by
/// index would then attribute one core's jiffies to another.
fn per_core(current: &Counters, previous: Option<&Counters>) -> Vec<Value> {
    let Some(previous) = previous.filter(|before| before.cores.len() == current.cores.len()) else {
        return Vec::new();
    };
    current
        .cores
        .iter()
        .zip(previous.cores.iter())
        .map(|(now, before)| Value::Float(py_round(cpu::percent(*now, Some(*before)), 2)))
        .collect()
}

fn split_temperature(reading: Option<temp::Reading>) -> (Option<f64>, Option<String>) {
    match reading {
        Some((celsius, label)) => (Some(celsius), Some(label)),
        None => (None, None),
    }
}

/// The bar's stored history, so the popover's graphs open already drawn.
///
/// Flat lists of numbers throughout, one key per series rather than tuples per point, so
/// a consumer never has to branch on which widget it is holding.
pub(crate) fn seed_object(dirs: &Dirs) -> Value {
    let mut seed = Object::new();
    for widget in WIDGETS {
        let state = load_state(&dirs.state_file(widget));
        if let Some(history) = present(&state, "history") {
            seed.insert(widget, history);
        }
        if let Some(second) = present(&state, "history2") {
            let suffix = if widget == "network" { "up" } else { "vram" };
            seed.insert(format!("{widget}_{suffix}"), second);
        }
    }
    Value::Object(object! {
        "ts" => Value::Float(now()),
        "seed" => Value::Object(seed),
    })
}

/// A stored series, but only when it is a list with something in it -- and copied as it
/// stands, ints and all, because the seed is republished rather than recomputed.
fn present(state: &Object, key: &str) -> Option<Value> {
    let history = state.get(key)?.as_list()?;
    (!history.is_empty()).then(|| Value::List(history.to_vec()))
}

/// `time.time()` -- seconds since the epoch as a float. A clock before 1970 is not a
/// thing this has to survive, so the failure folds to zero rather than propagating.
pub(crate) fn now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |since| since.as_secs_f64())
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
    use super::{per_core, present, seed_object, split_temperature, Counters};
    use crate::data::POINTS as SEED_POINTS;
    use crate::dirs::Dirs;
    use crate::json::{dumps, object, Object, Value};
    use crate::scratch::Scratch;
    use crate::sources::cpu;
    use std::fs;

    fn counters(timestamp: f64, cores: &[(i64, i64)]) -> Counters {
        Counters {
            timestamp,
            cpu: cpu::Counters { total: 0, idle: 0 },
            cores: cores
                .iter()
                .map(|(total, idle)| cpu::Counters {
                    total: *total,
                    idle: *idle,
                })
                .collect(),
            received: None,
            sent: None,
            reads: None,
            writes: None,
        }
    }

    #[test]
    fn per_core_pairs_the_two_reads_by_index_and_rounds_to_two_places() {
        let before = counters(0.0, &[(1000, 900), (1000, 500)]);
        let now = counters(1.0, &[(1300, 1100), (1300, 500)]);
        assert_eq!(
            per_core(&now, Some(&before)),
            vec![Value::Float(33.33), Value::Float(100.0),]
        );
    }

    #[test]
    fn a_core_appearing_between_two_reads_yields_no_per_core_figures_at_all() {
        let before = counters(0.0, &[(1000, 900)]);
        let now = counters(1.0, &[(1300, 1100), (1300, 1100)]);
        assert_eq!(per_core(&now, Some(&before)), Vec::<Value>::new());
        assert_eq!(per_core(&now, None), Vec::<Value>::new());
    }

    #[test]
    fn a_missing_temperature_splits_into_two_nulls() {
        assert_eq!(split_temperature(None), (None, None));
        assert_eq!(
            split_temperature(Some((68.75, "k10temp Tctl".to_string()))),
            (Some(68.75), Some("k10temp Tctl".to_string()))
        );
    }

    #[test]
    fn a_series_is_only_republished_when_it_is_a_non_empty_list() {
        let state = object! {
            "history" => Value::List(vec![Value::Float(1.0)]),
            "history2" => Value::List(vec![]),
            "device" => Value::str("nvme0n1"),
        };
        assert_eq!(
            present(&state, "history"),
            Some(Value::List(vec![Value::Float(1.0)]))
        );
        assert_eq!(present(&state, "history2"), None);
        assert_eq!(present(&state, "device"), None);
        assert_eq!(present(&state, "missing"), None);
    }

    #[test]
    fn a_seed_names_the_second_series_after_what_it_is() {
        let scratch = Scratch::new("seed");
        let dirs = Dirs::scratch(scratch.path());
        fs::create_dir_all(&dirs.state).expect("mkdir");
        fs::write(
            dirs.state_file("network"),
            r#"{"history": [1.0], "history2": [2.0]}"#,
        )
        .expect("write");
        fs::write(
            dirs.state_file("gpu"),
            r#"{"history": [3.0], "history2": [4.0]}"#,
        )
        .expect("write");

        let rendered = dumps(&seed_object(&dirs));
        assert!(
            rendered.contains(r#""network": [1.0], "network_up": [2.0]"#),
            "{rendered}"
        );
        assert!(
            rendered.contains(r#""gpu": [3.0], "gpu_vram": [4.0]"#),
            "{rendered}"
        );
    }

    #[test]
    fn a_seed_from_an_empty_state_directory_is_an_empty_seed() {
        let scratch = Scratch::new("seed-empty");
        let dirs = Dirs::scratch(scratch.path());
        let rendered = dumps(&seed_object(&dirs));
        assert!(rendered.contains(r#""seed": {}"#), "{rendered}");
        assert!(rendered.starts_with(r#"{"ts": "#), "{rendered}");
    }

    #[test]
    fn the_seed_carries_a_full_bar_history_when_there_is_one() {
        let scratch = Scratch::new("seed-full");
        let dirs = Dirs::scratch(scratch.path());
        fs::create_dir_all(&dirs.state).expect("mkdir");
        let history = Value::List(vec![Value::Float(5.0); SEED_POINTS]);
        let state = object! { "history" => history };
        fs::write(dirs.state_file("cpu"), dumps(&Value::Object(state))).expect("write");

        let Value::Object(top) = seed_object(&dirs) else {
            panic!("a seed is an object");
        };
        let Some(Value::Object(seed)) = top.get("seed") else {
            panic!("a seed carries a seed");
        };
        let cpu = seed
            .get("cpu")
            .and_then(Value::as_list)
            .expect("cpu series");
        assert_eq!(cpu.len(), SEED_POINTS);
    }

    #[test]
    fn an_unreadable_state_file_simply_contributes_nothing() {
        let scratch = Scratch::new("seed-broken");
        let dirs = Dirs::scratch(scratch.path());
        fs::create_dir_all(&dirs.state).expect("mkdir");
        fs::write(dirs.state_file("cpu"), "not json").expect("write");
        assert!(dumps(&seed_object(&dirs)).contains(r#""seed": {}"#));
        assert_eq!(Object::new(), Object::new());
    }
}
