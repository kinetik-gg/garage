//! Bar state
//!
//! One JSON file per widget, holding the rolling history and whatever raw counters that
//! widget's next delta needs. Written under flock because two waybar ticks can overlap
//! and a lost update is a visible notch in the graph.

use crate::data::POINTS;
use crate::fault::Fault;
use crate::files::{compact_span, count_as_float};
use crate::json::{loads, Object, Value};
use crate::sample::{sample_widget, MIN_SAMPLE_INTERVAL};
use std::path::Path;

/// Read one widget's state file, or start from nothing.
///
/// Every failure is the same failure here, exactly as it is in the Python's
/// `except (OSError, ValueError)`: a file that is not there, a file that is not
/// readable, a file that is not JSON, and a file whose JSON is not an object all mean
/// "no history", and the tick that follows primes a fresh one. Losing the history costs
/// four minutes of graph and nothing else, which is why it is allowed to be this quiet.
pub(crate) fn load_state(path: &Path) -> Object {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| loads(&text).ok())
        .and_then(Value::into_object)
        .unwrap_or_default()
}

/// Append one point, priming a fresh graph flat rather than ramping.
///
/// A new history filled with zeros draws a line climbing from the floor to the current
/// value over the first four minutes, which reads as a load spike that never happened.
/// Filling it with the first real sample draws a flat line, which is the honest claim:
/// nothing is known about before now.
pub(crate) fn push_history(state: &mut Object, key: &str, value: f64) {
    let existing = state
        .get(key)
        .and_then(Value::as_list)
        .filter(|history| !history.is_empty());
    let history = match existing {
        None => vec![Value::Float(value); POINTS],
        Some(history) => {
            let mut points: Vec<Value> = history.to_vec();
            points.push(Value::Float(value));
            // `[-POINTS:]` keeps the tail, so the oldest points fall off the front.
            let start = points.len().saturating_sub(POINTS);
            points.split_off(start)
        }
    };
    state.insert(key, Value::List(history));
}

/// Fold one fresh sample into the widget's state, or reuse the last one.
///
/// # Errors
///
/// Returns whatever [`sample_widget`] could not read, for the caller to degrade.
pub(crate) fn update_state(widget: &str, state: &mut Object, now: f64) -> Result<(), Fault> {
    if is_fresh(state, now) {
        return Ok(());
    }
    smooth_period(state, now);
    let current = sample_widget(widget, state, now)?;
    let value = current
        .get("value")
        .and_then(Value::as_number)
        .unwrap_or(0.0);
    push_history(state, "history", value);
    if let Some(second) = current.get("value2").and_then(Value::as_number) {
        push_history(state, "history2", second);
    }
    state.update(current);
    state.insert("last_sample", Value::Float(now));
    Ok(())
}

/// Whether the last sample is recent enough to reuse.
///
/// Two callers can overlap: waybar's tick and a manual run, or a tick that ran long.
/// Both take the lock, so the second one would otherwise sample the counters again
/// microseconds later and push a meaningless near-zero rate into the history. The guard
/// makes the second caller reuse what the first stored.
///
/// A missing `last_sample` reads as zero, which is 1970 and so never fresh. An *empty*
/// history disables the guard entirely -- the `and state.get("history")` half -- so a
/// state file that somehow has a timestamp but no points still samples.
fn is_fresh(state: &Object, now: f64) -> bool {
    let last = state
        .get("last_sample")
        .and_then(Value::as_number)
        .unwrap_or(0.0);
    let has_history = state.get("history").is_some_and(Value::is_truthy);
    now - last < MIN_SAMPLE_INTERVAL && has_history
}

/// The exponentially smoothed interval between ticks.
///
/// Smoothed so the tooltip can say how much time the strip covers without storing a
/// timestamp per point. A tick that ran long or a machine that slept would otherwise
/// make one interval stand for all 120.
fn smooth_period(state: &mut Object, now: f64) {
    let Some(previous) = state.get("last_sample").filter(|value| value.is_truthy()) else {
        return;
    };
    let Some(previous) = previous.as_number() else {
        return;
    };
    let period = now - previous;
    if period <= 0.0 || period >= 60.0 {
        return;
    }
    let stored = state
        .get("period")
        .filter(|value| value.is_truthy())
        .and_then(Value::as_number);
    let smoothed = stored.map_or(period, |stored| stored * 0.8 + period * 0.2);
    state.insert("period", Value::Float(smoothed));
}

/// Degrade a widget to "n/a" in place, keeping whatever history exists.
///
/// An unplugged interface or a GPU that went away mid-session must not blank the strip
/// or fail the tick -- waybar treats a nonzero exit as a broken module and stops calling
/// it at all.
///
/// The primed history is Python's `int` zero rather than a float, and the state file
/// shows it: a widget that has never had a reading writes `[0, 0, ...]` where a sampled
/// one writes `[0.0, 0.0, ...]`. Kept because a state file is compared byte for byte
/// against the Python's, and because nothing downstream can tell the difference anyway
/// -- `_clamp_sample` turns both into the same float.
pub(crate) fn mark_unavailable(widget: &str, state: &mut Object, error: &Fault) {
    state.insert("display", Value::str("n/a"));
    state.insert("extra", Value::str(""));
    state.insert(
        "tooltip_parts",
        Value::strings([
            format!("{} unavailable", widget.to_uppercase()),
            error.to_string(),
        ]),
    );
    state.insert("active", Value::Bool(false));
    state.set_default("history", Value::List(vec![Value::Int(0); POINTS]));
}

/// One physical line, because waybar reads exactly one line as the tooltip.
///
/// The parts are the lines the old per-module tooltips had; joining them with a
/// separator is how activity-graph.py kept a multi-part tooltip inside the image
/// module's two-line protocol.
///
/// A state with no `tooltip_parts` -- or an empty list, which is the same thing to
/// Python's `or` -- falls back to the widget's own name in capitals, so a strip always
/// says at least what it is.
pub(crate) fn tooltip_for(widget: &str, state: &Object) -> String {
    let stored = state.get("tooltip_parts").filter(|value| value.is_truthy());
    let mut parts: Vec<String> = match stored.and_then(Value::as_list) {
        Some(items) => items.iter().map(Value::py_str).collect(),
        None => vec![widget.to_uppercase()],
    };
    if let Some(period) = state.get("period").filter(|value| value.is_truthy()) {
        // A non-numeric `period` is the one place this departs from the Python, which
        // would multiply a string by 120 and then raise a TypeError formatting it --
        // a traceback outside the try/except, from a state file nothing here writes.
        if let Some(period) = period.as_number() {
            parts.push(format!(
                "last {}",
                compact_span(count_as_float(POINTS) * period)
            ));
        }
    }
    parts.join(" | ")
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
    use super::{load_state, mark_unavailable, push_history, tooltip_for, update_state};
    use crate::data::POINTS;
    use crate::fault::Fault;
    use crate::json::{dumps, object, Object, Value};
    use crate::scratch::Scratch;
    use std::fs;

    fn history_of(state: &Object) -> Vec<f64> {
        state
            .get("history")
            .and_then(Value::as_list)
            .expect("a history")
            .iter()
            .filter_map(Value::as_number)
            .collect()
    }

    #[test]
    fn a_fresh_history_is_primed_flat_rather_than_from_zero() {
        let mut state = Object::new();
        push_history(&mut state, "history", 42.0);
        let history = history_of(&state);
        assert_eq!(history.len(), POINTS);
        assert!(history.iter().all(|value| *value == 42.0));
    }

    #[test]
    fn an_empty_stored_history_is_also_primed_flat() {
        let mut state = object! { "history" => Value::List(vec![]) };
        push_history(&mut state, "history", 7.5);
        assert_eq!(history_of(&state), vec![7.5; POINTS]);
    }

    #[test]
    fn a_full_history_drops_its_oldest_point_to_make_room() {
        let mut state = object! {
            "history" => Value::List((0..POINTS).map(|i| Value::Float(i as f64)).collect()),
        };
        push_history(&mut state, "history", 999.0);
        let history = history_of(&state);
        assert_eq!(history.len(), POINTS);
        assert_eq!(history[0], 1.0);
        assert_eq!(history[POINTS - 1], 999.0);
    }

    #[test]
    fn a_short_history_grows_rather_than_being_reprimed() {
        let mut state = object! { "history" => Value::List(vec![Value::Float(1.0)]) };
        push_history(&mut state, "history", 2.0);
        assert_eq!(history_of(&state), vec![1.0, 2.0]);
    }

    #[test]
    fn a_load_state_that_cannot_be_read_is_an_empty_dict() {
        let scratch = Scratch::new("state-load");
        assert_eq!(load_state(&scratch.join("missing.json")), Object::new());

        let broken = scratch.join("broken.json");
        fs::write(&broken, "not json").expect("write");
        assert_eq!(load_state(&broken), Object::new());

        let not_a_dict = scratch.join("list.json");
        fs::write(&not_a_dict, "[1, 2, 3]").expect("write");
        assert_eq!(load_state(&not_a_dict), Object::new());
    }

    #[test]
    fn a_state_file_survives_a_round_trip_with_its_key_order() {
        let scratch = Scratch::new("state-round-trip");
        let path = scratch.join("cpu.json");
        let text = r#"{"history": [1.0, 2.0], "display": "9%", "last_sample": 1.5}"#;
        fs::write(&path, text).expect("write");
        assert_eq!(dumps(&Value::Object(load_state(&path))), text);
    }

    #[test]
    fn a_recent_sample_is_reused_without_touching_a_sensor() {
        // last_sample in the future makes `now - last_sample` negative, so the guard
        // always holds -- which is exactly how the parity harness forces a pure render.
        let mut state = object! {
            "history" => Value::List(vec![Value::Float(5.0); POINTS]),
            "display" => Value::str("5%"),
            "last_sample" => Value::Float(1e12),
        };
        let before = state.clone();
        update_state("gpu", &mut state, 1000.0).expect("no sample is taken");
        assert_eq!(state, before);
    }

    #[test]
    fn the_freshness_guard_needs_both_a_recent_stamp_and_a_history() {
        let full = |stamp: f64, history: Value| {
            let state = object! { "last_sample" => Value::Float(stamp), "history" => history };
            super::is_fresh(&state, 1000.0)
        };
        let points = Value::List(vec![Value::Float(1.0); POINTS]);

        assert!(full(999.5, points.clone()), "half a second ago is fresh");
        assert!(full(1e12, points.clone()), "a stamp in the future is fresh");
        assert!(!full(998.0, points), "two seconds ago is stale");
        // A timestamp with nothing to draw must not be reused, or the strip stays blank.
        assert!(!full(999.5, Value::List(vec![])));
        assert!(!super::is_fresh(&Object::new(), 1000.0));
    }

    #[test]
    fn marking_a_widget_unavailable_keeps_the_history_it_already_had() {
        let mut state = object! {
            "history" => Value::List(vec![Value::Float(50.0); POINTS]),
            "display" => Value::str("50%"),
        };
        mark_unavailable("network", &mut state, &Fault::os("no default route"));
        assert_eq!(state.get("display"), Some(&Value::str("n/a")));
        assert_eq!(state.get("extra"), Some(&Value::str("")));
        assert_eq!(state.get("active"), Some(&Value::Bool(false)));
        assert_eq!(
            state.get("tooltip_parts"),
            Some(&Value::strings(["NETWORK unavailable", "no default route"]))
        );
        assert_eq!(history_of(&state), vec![50.0; POINTS]);
    }

    #[test]
    fn a_widget_that_has_never_worked_gets_an_integer_zero_history() {
        let mut state = Object::new();
        mark_unavailable("gpu", &mut state, &Fault::os("no GPU found"));
        let history = state
            .get("history")
            .and_then(Value::as_list)
            .expect("history");
        assert_eq!(history.len(), POINTS);
        assert!(history.iter().all(|value| *value == Value::Int(0)));
        assert!(dumps(&Value::Object(state)).contains(r#""history": [0, 0,"#));
    }

    #[test]
    fn the_unavailable_envelope_keeps_the_keys_in_the_pythons_order() {
        let mut state = Object::new();
        mark_unavailable("disk", &mut state, &Fault::os("no stat for nvme0n1"));
        let keys: Vec<&str> = state.pairs().map(|(key, _)| key).collect();
        assert_eq!(
            keys,
            ["display", "extra", "tooltip_parts", "active", "history"]
        );
    }

    #[test]
    fn a_tooltip_joins_its_parts_with_the_separator_waybar_gets_one_line_of() {
        let state = object! {
            "tooltip_parts" => Value::strings(["CPU 42.0%", "load 1.00 2.00 3.00"]),
        };
        assert_eq!(
            tooltip_for("cpu", &state),
            "CPU 42.0% | load 1.00 2.00 3.00"
        );
    }

    #[test]
    fn a_tooltip_with_a_period_says_how_much_time_the_graph_covers() {
        let state = object! {
            "tooltip_parts" => Value::strings(["CPU 42.0%"]),
            "period" => Value::Float(2.0),
        };
        assert_eq!(tooltip_for("cpu", &state), "CPU 42.0% | last 4.0 min");
    }

    #[test]
    fn a_state_with_no_parts_falls_back_to_the_widgets_own_name() {
        assert_eq!(tooltip_for("memory", &Object::new()), "MEMORY");
        let empty = object! { "tooltip_parts" => Value::List(vec![]) };
        assert_eq!(tooltip_for("memory", &empty), "MEMORY");
    }

    #[test]
    fn a_period_of_zero_leaves_the_span_off_entirely() {
        let state = object! {
            "tooltip_parts" => Value::strings(["MEM"]),
            "period" => Value::Float(0.0),
        };
        assert_eq!(tooltip_for("memory", &state), "MEM");
    }

    #[test]
    fn non_string_tooltip_parts_are_spelled_with_pythons_str() {
        let state = object! {
            "tooltip_parts" => Value::List(vec![
                Value::str("dev"),
                Value::Float(2.5),
                Value::Int(7),
                Value::Null,
                Value::Bool(true),
            ]),
        };
        assert_eq!(tooltip_for("disk", &state), "dev | 2.5 | 7 | None | True");
    }
}
