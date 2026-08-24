//! Persisted rolling histories for the metrics stream.

use std::path::Path;

use garage_core::fs::atomic::atomic_write;

use crate::json::{dumps, loads, object, Object, Value};

const POINTS: usize = 120;
const MIB: f64 = 1_048_576.0;
const LOG_CEILING_MIB: f64 = 2048.0;

/// The complete durable history document.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct History {
    series: Object,
}

impl History {
    pub(crate) fn load(path: &Path) -> Self {
        let series = std::fs::read_to_string(path)
            .ok()
            .and_then(|text| loads(&text).ok())
            .and_then(Value::into_object)
            .unwrap_or_default();
        Self { series }
    }

    pub(crate) fn seed(&self, timestamp: f64) -> Value {
        Value::Object(object! {
            "ts" => Value::Float(timestamp),
            "seed" => Value::Object(self.series.clone()),
        })
    }

    pub(crate) fn push_snapshot(&mut self, snapshot: &Value) {
        let Some(top) = snapshot.as_object() else {
            return;
        };
        push_nested(&mut self.series, "cpu", top, "cpu", "load", percent);
        push_nested(&mut self.series, "temp", top, "temp", "cpu_c", percent);
        if let Some(memory) = section(top, "memory") {
            if let (Some(used), Some(total)) = (
                number(memory, "used"),
                number(memory, "total").filter(|value| *value > 0.0),
            ) {
                push(&mut self.series, "memory", used / total * 100.0);
            }
        }
        if let Some(network) = section(top, "network") {
            if let Some(value) = number(network, "rx_bps") {
                push(&mut self.series, "network", log_scale(value / MIB));
            }
            if let Some(value) = number(network, "tx_bps") {
                push(&mut self.series, "network_up", log_scale(value / MIB));
            }
        }
        if let Some(disk) = section(top, "disk") {
            let read = number(disk, "read_bps").unwrap_or(0.0);
            let write = number(disk, "write_bps").unwrap_or(0.0);
            push(&mut self.series, "disk", log_scale((read + write) / MIB));
        }
        push_gpu(&mut self.series, top);
    }

    pub(crate) fn write(&self, path: &Path) -> Result<(), std::io::Error> {
        atomic_write(path, &dumps(&Value::Object(self.series.clone())))
            .map_err(|error| std::io::Error::other(error.to_string()))
    }
}

fn section<'a>(top: &'a Object, key: &str) -> Option<&'a Object> {
    top.get(key).and_then(Value::as_object)
}

fn number(fields: &Object, key: &str) -> Option<f64> {
    fields.get(key).and_then(Value::as_number)
}

fn push_nested(
    series: &mut Object,
    series_key: &str,
    top: &Object,
    section_key: &str,
    value_key: &str,
    transform: fn(f64) -> f64,
) {
    if let Some(value) = section(top, section_key).and_then(|fields| number(fields, value_key)) {
        push(series, series_key, transform(value));
    }
}

fn push_gpu(series: &mut Object, top: &Object) {
    let Some(gpu) = top
        .get("gpus")
        .and_then(Value::as_list)
        .and_then(|gpus| gpus.first())
        .and_then(Value::as_object)
    else {
        return;
    };
    if let Some(load) = number(gpu, "load") {
        push(series, "gpu", percent(load));
    }
    if let (Some(used), Some(total)) = (
        number(gpu, "vram_used"),
        number(gpu, "vram_total").filter(|value| *value > 0.0),
    ) {
        push(series, "gpu_vram", percent(used / total * 100.0));
    }
}

fn percent(value: f64) -> f64 {
    value.clamp(0.0, 100.0)
}

fn log_scale(mib_per_second: f64) -> f64 {
    if mib_per_second <= 0.0 || !mib_per_second.is_finite() {
        return 0.0;
    }
    ((1.0 + mib_per_second).ln() / (1.0 + LOG_CEILING_MIB).ln() * 100.0).min(100.0)
}

fn push(series: &mut Object, key: &str, value: f64) {
    let existing = series
        .get(key)
        .and_then(Value::as_list)
        .filter(|points| !points.is_empty());
    let points = match existing {
        None => vec![Value::Float(value); POINTS],
        Some(existing) => {
            let mut points = existing.to_vec();
            points.push(Value::Float(value));
            let start = points.len().saturating_sub(POINTS);
            points.split_off(start)
        }
    };
    series.insert(key, Value::List(points));
}

#[cfg(test)]
mod tests {
    use crate::json::{object, Value};
    use crate::scratch::Scratch;

    use super::{History, POINTS};

    fn snapshot(cpu: f64) -> Value {
        Value::Object(object! {
            "cpu" => Value::Object(object! { "load" => Value::Float(cpu) }),
            "temp" => Value::Object(object! { "cpu_c" => Value::Float(55.0) }),
            "memory" => Value::Object(object! {
                "used" => Value::Int(25), "total" => Value::Int(100),
            }),
            "network" => Value::Object(object! {
                "rx_bps" => Value::Float(0.0), "tx_bps" => Value::Float(0.0),
            }),
            "disk" => Value::Object(object! {
                "read_bps" => Value::Float(0.0), "write_bps" => Value::Float(0.0),
            }),
            "gpus" => Value::List(vec![]),
        })
    }

    #[test]
    fn first_sample_primes_available_series_flat() {
        let mut history = History::default();
        history.push_snapshot(&snapshot(42.0));
        let Value::Object(seed) = history.seed(1.0) else {
            panic!("seed is an object");
        };
        let cpu = seed
            .get("seed")
            .and_then(Value::as_object)
            .and_then(|series| series.get("cpu"))
            .and_then(Value::as_list)
            .expect("cpu series");
        assert_eq!(cpu.len(), POINTS);
        assert!(cpu.iter().all(|point| *point == Value::Float(42.0)));
    }

    #[test]
    fn document_round_trips_and_keeps_only_the_tail() {
        let scratch = Scratch::new("stream-history");
        let path = scratch.join("metrics/history.json");
        let mut history = History::default();
        for value in 0..=POINTS {
            history.push_snapshot(&snapshot(value as f64));
        }
        history.write(&path).expect("history writes");
        let loaded = History::load(&path);
        assert_eq!(loaded, history);
        let Value::Object(seed) = loaded.seed(1.0) else {
            panic!("seed is an object");
        };
        let cpu = seed
            .get("seed")
            .and_then(Value::as_object)
            .and_then(|series| series.get("cpu"))
            .and_then(Value::as_list)
            .expect("cpu series");
        assert_eq!(cpu.len(), POINTS);
        assert_eq!(cpu.last(), Some(&Value::Float(100.0)));
    }
}
