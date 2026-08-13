//! `str(value)`, `float(value)` and `bool(value)` over a `json.loads()` result.
//!
//! An action's payload arrives as one JSON value and is read three different ways, all of
//! them Python's rather than JSON's: `str(value)` for the desktop id and the timezone name,
//! `float(value)` for the two volume steps, and plain truthiness for the mute flags and the
//! NTP switch. None of the three is what `serde_json` would do on its own -- `str(None)` is
//! `"None"`, `float(1)` prints as `"1.0"`, and `bool([])` is false where JSON has no notion
//! of an empty array being falsy -- so each is spelled out here once, next to the reasoning,
//! rather than approximated at four call sites.

use garage_core::pyrepr::{py_float_repr, py_repr, PyValue};
use serde_json::Value;

use crate::error::ApplyError;

/// `str(value)` for a `json.loads()` result.
///
/// A string is itself; every other kind goes through `repr()`, which is what `str()` falls
/// back to for `None`, `True`, numbers and containers alike. That is why `None` reaches
/// `pactl set-default-sink` as the four characters `None` rather than as an empty argument:
/// the Python hands it straight to `str()` with no guard, and a caller that sent no payload
/// gets exactly that.
pub(crate) fn py_str(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(text)) => text.clone(),
        other => py_repr_of(other),
    }
}

/// `repr(value)` for the same.
fn py_repr_of(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => "None".to_owned(),
        Some(Value::Bool(flag)) => py_repr(PyValue::Bool(*flag)),
        Some(Value::Number(number)) => number.as_i64().map_or_else(
            || py_float_repr(number.as_f64().unwrap_or_default()),
            |integer| integer.to_string(),
        ),
        Some(Value::String(text)) => py_repr(PyValue::Str(text)),
        Some(Value::Array(items)) => {
            let parts: Vec<String> = items.iter().map(|item| py_repr_of(Some(item))).collect();
            format!("[{}]", parts.join(", "))
        }
        Some(Value::Object(map)) => {
            let parts: Vec<String> = map
                .iter()
                .map(|(key, item)| {
                    format!("{}: {}", py_repr(PyValue::Str(key)), py_repr_of(Some(item)))
                })
                .collect();
            format!("{{{}}}", parts.join(", "))
        }
    }
}

/// `bool(value)`: Python truthiness, where an empty string, an empty container and zero are
/// all false and JSON's own notion of truth does not come into it.
pub(crate) fn py_truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(flag)) => *flag,
        Some(Value::Number(number)) => number.as_f64().is_some_and(|found| found != 0.0),
        Some(Value::String(text)) => !text.is_empty(),
        Some(Value::Array(items)) => !items.is_empty(),
        Some(Value::Object(map)) => !map.is_empty(),
    }
}

/// `str(float(value))`: the volume argument `wpctl set-volume` is handed.
///
/// `float()` accepts a real number, a bool (`True` is `1.0`) and a string it can parse --
/// leading and trailing whitespace included, and `inf`/`nan` among them. Everything else
/// raises, and the two exceptions are spelled differently: a string it cannot parse is a
/// `ValueError`, which `main()` catches and reports through the envelope, while `None` or a
/// container is a `TypeError`, which `main()` does *not* catch.
///
/// **Parity gap, stated plainly:** the `TypeError` arm reaches the envelope here where the
/// Python leaves a traceback on stderr and prints nothing at all on stdout. Exit 1 either
/// way. Restoring the traceback would mean reproducing `CPython`'s, which is neither possible
/// nor useful; the message below is `TypeError`'s own text, so the *sentence* a person sees
/// is the same one.
///
/// # Errors
///
/// [`ApplyError::Settings`] carrying Python's own wording for whichever of the two it is.
pub(crate) fn py_float_argument(value: Option<&Value>) -> Result<String, ApplyError> {
    let number = match value {
        Some(Value::Number(number)) => number.as_f64().unwrap_or_default(),
        Some(Value::Bool(flag)) => f64::from(u8::from(*flag)),
        Some(Value::String(text)) => parse_float(text)?,
        other => {
            return Err(ApplyError::Settings(format!(
                "float() argument must be a string or a real number, not '{}'",
                python_type_name(other)
            )))
        }
    };
    Ok(py_float_repr(number))
}

/// `float(text)` for the forms Python accepts, and its `ValueError` for the rest.
fn parse_float(text: &str) -> Result<f64, ApplyError> {
    let trimmed = text.trim();
    // Python's float() takes `inf`, `infinity`, `nan` in any case, with an optional sign;
    // Rust's `f64::from_str` takes `inf`, `infinity` and `NaN` in any case too, so the two
    // agree without a table. What Rust additionally accepts and Python does not is nothing
    // in this grammar; what Python accepts and Rust does not is `1_0` (underscores), which a
    // JSON string carrying a volume will never be.
    trimmed.parse::<f64>().map_err(|_| {
        ApplyError::Settings(format!(
            "could not convert string to float: {}",
            py_repr(PyValue::Str(text))
        ))
    })
}

/// The name `TypeError` prints for the value's type.
fn python_type_name(value: Option<&Value>) -> &'static str {
    match value {
        None | Some(Value::Null) => "NoneType",
        Some(Value::Array(_)) => "list",
        Some(Value::Object(_)) => "dict",
        Some(Value::Bool(_)) => "bool",
        Some(Value::Number(_)) => "float",
        Some(Value::String(_)) => "str",
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{py_float_argument, py_str, py_truthy};

    #[test]
    fn a_volume_step_is_spelled_the_way_python_spells_a_float() {
        // The pane sends 0.05-per-notch steps; 1 and 0 come back with the `.0` Python's
        // `str(float(...))` appends and Rust's own `Display` would not.
        let cases = [
            (json!(0.65), "0.65"),
            (json!(1), "1.0"),
            (json!(0), "0.0"),
            (json!(0.1), "0.1"),
            (json!(true), "1.0"),
            (json!("0.35"), "0.35"),
            (json!(" 0.35 "), "0.35"),
        ];
        for (value, expected) in cases {
            assert_eq!(
                py_float_argument(Some(&value))
                    .expect("every case here is a float Python would take"),
                expected,
                "{value}"
            );
        }
    }

    #[test]
    fn an_unparseable_string_is_a_value_error_in_pythons_words() {
        let error = py_float_argument(Some(&json!("loud"))).expect_err("not a float");
        assert_eq!(
            error.to_string(),
            "could not convert string to float: 'loud'"
        );
    }

    #[test]
    fn no_payload_at_all_is_the_type_error_arm() {
        let error = py_float_argument(None).expect_err("float(None) raises");
        assert_eq!(
            error.to_string(),
            "float() argument must be a string or a real number, not 'NoneType'"
        );
    }

    #[test]
    fn str_of_a_payload_is_pythons_str_and_not_jsons() {
        assert_eq!(py_str(Some(&json!("firefox.desktop"))), "firefox.desktop");
        assert_eq!(py_str(None), "None");
        assert_eq!(py_str(Some(&json!(true))), "True");
        assert_eq!(py_str(Some(&json!(5))), "5");
        assert_eq!(py_str(Some(&json!(["a", 1]))), "['a', 1]");
    }

    #[test]
    fn truthiness_is_pythons() {
        assert!(!py_truthy(None));
        assert!(!py_truthy(Some(&json!(""))));
        assert!(!py_truthy(Some(&json!([]))));
        assert!(!py_truthy(Some(&json!(0))));
        assert!(py_truthy(Some(&json!("no"))));
        assert!(py_truthy(Some(&json!(0.5))));
    }
}
