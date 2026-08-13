//! Python's `==` over the values `tomllib` produces, which is not Rust's.
//!
//! Two of the port's decisions turn on comparing a stored value with another one, and both
//! were written against Python's semantics rather than TOML's: `same_default()` decides
//! whether a key is a departure or a copy of the shipped default, and
//! `compact_preferences_file()` decides whether the file needs rewriting at all. Rust's
//! `PartialEq` on [`toml::Value`] disagrees with Python on two points, and each one is worth
//! a real behavioural difference:
//!
//!   * `1 == 1.0` in Python. A UI that sends JSON `0` for a key the schema ships as `0.0`
//!     would otherwise pin a copy of the default into layer 2 forever, over nothing but a
//!     decimal point. See [`py_equal`].
//!   * a container compares its members with `PyObject_RichCompareBool`, which answers
//!     "equal" for the same object before it ever reaches `__eq__` -- so a NaN inside a dict
//!     equals itself, though a bare `nan == nan` does not. See [`py_element_equal`].
//!
//! Nothing here is a general-purpose Python emulation: it is exactly the comparisons the two
//! call sites make, on exactly the values a `preferences.toml` can hold.

/// Python's `==` over two values `tomllib` could have produced.
///
/// The one thing it does that Rust's `PartialEq` does not is compare an int to a float by
/// value, which is what lets a stored `0` be recognised as the shipped `0.0`. Bools compare
/// as the ints they are in Python -- `same_default()` is where that is ruled out at the top
/// level, and inside a list Python really does say `[True] == [1]`.
///
/// A NaN is not equal to itself here, exactly as `nan == nan` is `False` in Python. The
/// container rule that says otherwise is [`py_element_equal`], and it is deliberately not
/// this function.
#[must_use]
#[expect(
    clippy::float_cmp,
    reason = "exact IEEE equality is the behaviour being ported: Python's float `==` is \
              exact, and an epsilon would call two different stored values one departure"
)]
pub fn py_equal(left: &toml::Value, right: &toml::Value) -> bool {
    match (as_python_number(left), as_python_number(right)) {
        (Some(Number::Int(left)), Some(Number::Int(right))) => left == right,
        (Some(Number::Int(int)), Some(Number::Float(float)))
        | (Some(Number::Float(float)), Some(Number::Int(int))) => int_equals_float(int, float),
        (Some(Number::Float(left)), Some(Number::Float(right))) => left == right,
        (Some(_), None) | (None, Some(_)) => false,
        (None, None) => py_equal_other(left, right),
    }
}

/// The numeric tower Python compares across: `bool` is an `int`, and an `int` compares to a
/// `float` by value.
#[derive(Copy, Clone)]
enum Number {
    Int(i64),
    Float(f64),
}

fn as_python_number(value: &toml::Value) -> Option<Number> {
    match value {
        toml::Value::Boolean(flag) => Some(Number::Int(i64::from(*flag))),
        toml::Value::Integer(number) => Some(Number::Int(*number)),
        toml::Value::Float(number) => Some(Number::Float(*number)),
        toml::Value::String(_)
        | toml::Value::Datetime(_)
        | toml::Value::Array(_)
        | toml::Value::Table(_) => None,
    }
}

/// `int == float`, exactly, the way Python compares them.
///
/// Not `int as f64 == float`, which rounds an int past 2^53 and would call two different
/// numbers equal. A float that is not finite or not integral cannot equal any int, and one
/// outside `i64`'s range cannot equal a value that came out of a TOML integer; what is left
/// converts back losslessly.
fn int_equals_float(int: i64, float: f64) -> bool {
    const LIMIT: f64 = 9_223_372_036_854_775_808.0; // 2^63, one past i64::MAX.
    if !float.is_finite() || float.fract() != 0.0 || !(-LIMIT..LIMIT).contains(&float) {
        return false;
    }
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the range check above is exactly what makes this conversion exact"
    )]
    let truncated = float as i64;
    truncated == int
}

/// Everything Python's `==` does that is not arithmetic: strings, datetimes, and the two
/// containers, compared member by member so both rules reach inside them.
fn py_equal_other(left: &toml::Value, right: &toml::Value) -> bool {
    match (left, right) {
        (toml::Value::String(left), toml::Value::String(right)) => left == right,
        (toml::Value::Datetime(left), toml::Value::Datetime(right)) => left == right,
        (toml::Value::Array(left), toml::Value::Array(right)) => {
            left.len() == right.len() && left.iter().zip(right).all(|(l, r)| py_element_equal(l, r))
        }
        (toml::Value::Table(left), toml::Value::Table(right)) => py_equal_table(left, right),
        _ => false,
    }
}

/// Two members of a container as `CPython` compares them, which is not quite `==`.
///
/// `PyObject_RichCompareBool` -- the function every `list` and `dict` comparison calls on its
/// members -- answers "equal" for two references to the *same object* before it ever reaches
/// `__eq__`. One value class notices: a `float` NaN, which is not equal to itself, but *is*
/// itself.
///
/// That is not a curiosity here, it is the difference between a file being left alone and a
/// load failing outright. `compact_preferences_file()` compares the document it built
/// against the table it built it from, and every value in that document is the very object
/// that was in the table. So a hand-edited `animation_speed = nan` compares equal to itself,
/// the file is left as it is, and the emitter -- which refuses a non-finite float -- is never
/// reached. Without this rule the comparison would say "changed", the rewrite would run, and
/// the load would fail on a file the Python reads happily. It was a real test failure before
/// it was a comment.
///
/// Written as "two NaNs are the same NaN" rather than as an identity check because Rust has
/// no object identity to consult: every value reaching this was cloned from the table it is
/// being compared against, so same-object and same-NaN pick out exactly the same pairs.
#[must_use]
pub fn py_element_equal(left: &toml::Value, right: &toml::Value) -> bool {
    if let (toml::Value::Float(left), toml::Value::Float(right)) = (left, right) {
        if left.is_nan() && right.is_nan() {
            return true;
        }
    }
    py_equal(left, right)
}

/// Two tables as Python compares two dicts: same keys, equal values, order irrelevant.
#[must_use]
pub fn py_equal_table(left: &toml::Table, right: &toml::Table) -> bool {
    left.len() == right.len()
        && left.iter().all(|(key, value)| {
            right
                .get(key)
                .is_some_and(|other| py_element_equal(value, other))
        })
}

#[cfg(test)]
mod tests {
    use super::{py_element_equal, py_equal, py_equal_table};

    fn table(text: &str) -> toml::Table {
        text.parse().unwrap()
    }

    fn value(text: &str) -> toml::Value {
        table(&format!("value = {text}")).remove("value").unwrap()
    }

    #[test]
    fn an_int_equals_the_float_it_names() {
        assert!(py_equal(&value("0"), &value("0.0")));
        assert!(py_equal(&value("1.0"), &value("1")));
        assert!(!py_equal(&value("2"), &value("1.0")));
        assert!(!py_equal(&value("1"), &value("inf")));
        assert!(!py_equal(&value("1"), &value("nan")));
        assert!(!py_equal(&value("1"), &value("1.5")));
    }

    /// A bool is an int in Python, and outside the one place that has to notice
    /// (`same_default`) it compares as one.
    #[test]
    fn a_bool_compares_as_the_int_it_is() {
        assert!(py_equal(&value("true"), &value("1")));
        assert!(py_equal(&value("false"), &value("0.0")));
        assert!(py_equal(&value("[true]"), &value("[1]")));
    }

    /// An int past 2^53 rounds when it is cast to a float, so a cast-based comparison would
    /// call these two equal. Python does not.
    #[test]
    fn a_huge_int_is_not_the_float_it_rounds_to() {
        assert!(!py_equal(
            &value("9007199254740993"),
            &value("9007199254740992.0")
        ));
        assert!(py_equal(
            &value("9007199254740992"),
            &value("9007199254740992.0")
        ));
    }

    #[test]
    fn strings_and_datetimes_compare_as_themselves_and_not_across_kinds() {
        assert!(py_equal(&value("\"a\""), &value("\"a\"")));
        assert!(!py_equal(&value("\"a\""), &value("\"b\"")));
        assert!(py_equal(&value("07:00:00"), &value("07:00:00")));
        assert!(!py_equal(&value("\"07:00\""), &value("07:00:00")));
        assert!(!py_equal(&value("1"), &value("\"1\"")));
    }

    /// The identity rule, and the fact that it is *only* the container rule.
    #[test]
    fn a_nan_is_itself_inside_a_container_and_not_outside_one() {
        assert!(!py_equal(&value("nan"), &value("nan")));
        assert!(py_element_equal(&value("nan"), &value("nan")));
        assert!(py_equal_table(&table("a = nan\n"), &table("a = nan\n")));
        assert!(py_equal(&value("[nan]"), &value("[nan]")));
        assert!(!py_element_equal(&value("nan"), &value("1.0")));
    }

    #[test]
    fn a_table_compares_as_a_dict_does_regardless_of_order() {
        assert!(py_equal_table(
            &table("a = 1\nb = 2\n"),
            &table("b = 2\na = 1\n")
        ));
        assert!(!py_equal_table(&table("a = 1\n"), &table("a = 1\nb = 2\n")));
        assert!(!py_equal_table(&table("a = 1\n"), &table("b = 1\n")));
        assert!(py_equal_table(&table("a = 1\n"), &table("a = 1.0\n")));
    }

    #[test]
    fn an_array_compares_by_length_then_member() {
        assert!(py_equal(&value("[1, 2]"), &value("[1.0, 2]")));
        assert!(!py_equal(&value("[1]"), &value("[1, 2]")));
        assert!(!py_equal(&value("[1]"), &value("1")));
    }
}
