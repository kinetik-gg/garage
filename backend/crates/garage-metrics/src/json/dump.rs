//! `json.dumps(value)` with its default arguments, byte for byte.
//!
//! The defaults are the specification here, because the Python never passes anything
//! else: `separators=(", ", ": ")` -- a space after every comma and every colon, which
//! is *not* what most encoders emit -- `ensure_ascii=True`, `allow_nan=True`, and no
//! `sort_keys`, so the dict's insertion order is the wire order.
//!
//! Two files are compared against this. The state file is read back by the next tick
//! and by the popover's seed, so it only has to parse; but a state file the Rust build
//! wrote and one the Python build wrote have to be the same bytes for the two to be
//! interchangeable on a machine mid-migration, and for the parity harness to be able to
//! say so. The stream's lines go to Quickshell, which parses them, and to the eye of
//! anybody running `--once` in a terminal.

use super::{Object, Value};
use garage_core::pyjson::write_string;
use garage_core::pyrepr::py_float_repr;
use std::fmt::Write as _;

/// `json.dumps(value)`.
pub(crate) fn dumps(value: &Value) -> String {
    let mut out = String::new();
    write_value(&mut out, value);
    out
}

fn write_value(out: &mut String, value: &Value) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Int(number) => {
            let _ = write!(out, "{number}");
        }
        Value::Float(number) => out.push_str(&float(*number)),
        Value::Str(text) => write_string(text, out),
        Value::List(items) => write_list(out, items),
        Value::Object(fields) => write_object(out, fields),
    }
}

/// How `json.dumps` spells a float: `float.__repr__` for the finite ones, and the three
/// JavaScript spellings for the rest.
///
/// `allow_nan` defaults to true, so a non-finite float is emitted rather than refused --
/// as `NaN`, `Infinity`, `-Infinity`, which are not JSON but are what `json.loads`
/// reads back, and what [`super::loads`] therefore accepts.
fn float(number: f64) -> String {
    if number.is_nan() {
        return "NaN".to_string();
    }
    if number.is_infinite() {
        return if number < 0.0 {
            "-Infinity"
        } else {
            "Infinity"
        }
        .to_string();
    }
    py_float_repr(number)
}

fn write_list(out: &mut String, items: &[Value]) {
    out.push('[');
    for (index, item) in items.iter().enumerate() {
        if index > 0 {
            out.push_str(", ");
        }
        write_value(out, item);
    }
    out.push(']');
}

fn write_object(out: &mut String, fields: &Object) {
    out.push('{');
    for (index, (key, value)) in fields.pairs().enumerate() {
        if index > 0 {
            out.push_str(", ");
        }
        write_string(key, out);
        out.push_str(": ");
        write_value(out, value);
    }
    out.push('}');
}

/// A JSON string under `ensure_ascii=True`: the two mandatory escapes, the five control
/// characters with short forms, `\u00hh` for every other byte below a space and for
/// everything above `~` (the ASCII DEL included), and a surrogate pair for anything
/// outside the Basic Multilingual Plane. Lowercase hex throughout, as `CPython`'s own
/// table has it.
///
/// This is not a theoretical concern for these two files. Every tooltip the network and
/// disk widgets build carries `↓`, `↑` or `°`, and each has to come out as a six-byte
/// `\u` escape rather than its raw UTF-8 -- two files with the same meaning and
/// different bytes is exactly what the parity harness exists to catch.
#[cfg(test)]
mod tests {
    use super::dumps;
    use crate::json::{object, Object, Value};

    #[test]
    fn separators_carry_the_space_json_dumps_defaults_to() {
        let value = Value::Object(object! {
            "a" => Value::Int(1),
            "b" => Value::List(vec![Value::Int(1), Value::Int(2)]),
        });
        assert_eq!(dumps(&value), r#"{"a": 1, "b": [1, 2]}"#);
    }

    #[test]
    fn keys_come_out_in_insertion_order_not_sorted() {
        let value = Value::Object(object! {
            "zeta" => Value::Int(1),
            "alpha" => Value::Int(2),
        });
        assert_eq!(dumps(&value), r#"{"zeta": 1, "alpha": 2}"#);
    }

    #[test]
    fn ints_and_floats_keep_their_own_spelling() {
        assert_eq!(dumps(&Value::Int(0)), "0");
        assert_eq!(dumps(&Value::Float(0.0)), "0.0");
        assert_eq!(dumps(&Value::Float(-0.0)), "-0.0");
        assert_eq!(dumps(&Value::Float(1e16)), "1e+16");
        assert_eq!(dumps(&Value::Float(1e15)), "1000000000000000.0");
    }

    #[test]
    fn non_finite_floats_use_pythons_allow_nan_spellings() {
        assert_eq!(dumps(&Value::Float(f64::NAN)), "NaN");
        assert_eq!(dumps(&Value::Float(f64::INFINITY)), "Infinity");
        assert_eq!(dumps(&Value::Float(f64::NEG_INFINITY)), "-Infinity");
    }

    #[test]
    fn the_arrows_and_degree_signs_in_a_tooltip_are_escaped() {
        // Exactly what `network`'s tooltip_parts holds.
        let value = Value::List(vec![
            Value::str("\u{2193} 0.00 MiB/s"),
            Value::str("\u{2191} 0.00 MiB/s"),
            Value::str("37\u{b0}C"),
        ]);
        assert_eq!(
            dumps(&value),
            "[\"\\u2193 0.00 MiB/s\", \"\\u2191 0.00 MiB/s\", \"37\\u00b0C\"]"
        );
    }

    #[test]
    fn control_characters_and_astral_planes_match_cpythons_table() {
        assert_eq!(dumps(&Value::str("\n\t\u{8}\u{c}\r")), r#""\n\t\b\f\r""#);
        assert_eq!(dumps(&Value::str("\u{1}\u{7f}")), "\"\\u0001\\u007f\"");
        assert_eq!(dumps(&Value::str("\u{1F3B5}")), "\"\\ud83c\\udfb5\"");
        assert_eq!(dumps(&Value::str("a\"b\\c")), r#""a\"b\\c""#);
    }

    #[test]
    fn an_empty_object_and_an_empty_list_are_not_confused() {
        assert_eq!(dumps(&Value::Object(Object::new())), "{}");
        assert_eq!(dumps(&Value::List(vec![])), "[]");
        assert_eq!(dumps(&Value::Null), "null");
        assert_eq!(dumps(&Value::Bool(true)), "true");
    }
}
