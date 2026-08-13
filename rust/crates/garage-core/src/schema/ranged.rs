//! The two number kinds, and the ranges that are the whole of their
//! constraint.
//!
//! `"int"` and `"float"` are range-checked identically -- `in_number_range()`
//! is one function for both -- and the kind's name records what the renderer
//! will coerce the number to, not an integrality check the schema has never
//! made. So the two types here differ only in how they carry their bounds, and
//! both keep the number in the shape the file stored it in.

use std::marker::PhantomData;

use crate::schema::coerce::{Coerce, Store};

/// A stored number, as the file spells it.
///
/// A whole number stored for a float key stays whole and a fractional one
/// stored for an int key stays fractional: the Python leaves both in the file
/// untouched, and rewriting either would change a file this build is supposed
/// to agree with byte for byte.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Number {
    /// A TOML integer.
    Int(i64),
    /// A TOML float, always finite -- see [`Number::parse`].
    Float(f64),
}

impl Number {
    /// The value as arithmetic wants it.
    // i64 -> f64 is lossy past 2^53. No range in the schema comes close, and a
    // lossless conversion does not exist to use instead.
    #[allow(clippy::cast_precision_loss)]
    #[must_use]
    pub fn as_f64(self) -> f64 {
        match self {
            Self::Int(number) => number as f64,
            Self::Float(number) => number,
        }
    }

    /// The value as `int()` would produce it: truncated toward zero, which is
    /// what every renderer that writes a pixel or a second does with it.
    // `as` saturates at the bounds rather than wrapping, and every range this
    // is reached through is far inside them.
    #[allow(clippy::cast_possible_truncation)]
    #[must_use]
    pub fn as_i64(self) -> i64 {
        match self {
            Self::Int(number) => number,
            Self::Float(number) => number as i64,
        }
    }

    /// `in_number_range()`'s first half: what may be range-checked at all.
    fn parse(value: &toml::Value) -> Option<Self> {
        // A bool is an int in Python, and the predicate's first line refuses
        // it. Here `as_bool` is simply a different accessor, and a TOML `true`
        // is never an integer -- but the order is kept so the two read alike.
        if value.as_bool().is_some() {
            return None;
        }
        if let Some(number) = value.as_integer() {
            return Some(Self::Int(number));
        }
        // Non-finite floats fail here rather than being range-checked. NaN
        // compares False against both bounds, so the two-sided rejection this
        // replaced let nan and inf straight through, and the int() conversions
        // in the renderers then raised OverflowError -- which main() does not
        // catch, so it reached the user as a traceback instead of a message.
        value
            .as_float()
            .filter(|number| number.is_finite())
            .map(Self::Float)
    }
}

impl Store for Number {
    fn store(&self) -> toml::Value {
        match self {
            Self::Int(number) => toml::Value::Integer(*number),
            Self::Float(number) => toml::Value::Float(*number),
        }
    }
}

/// `minimum <= value <= maximum`, inclusive both ends, for a value that has
/// already passed [`Number::parse`].
#[must_use]
pub fn in_range(value: Number, minimum: f64, maximum: f64) -> bool {
    let number = value.as_f64();
    minimum <= number && number <= maximum
}

/// `"int"` with its range in the type: `RangedInt<1000, 10000>` is
/// `"minimum": 1000, "maximum": 10000`.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct RangedInt<const MIN: i64, const MAX: i64>(Number);

impl<const MIN: i64, const MAX: i64> RangedInt<MIN, MAX> {
    /// The number as the renderers take it.
    #[must_use]
    pub fn get(self) -> i64 {
        self.0.as_i64()
    }

    /// The number as it is stored, which is not always an integer.
    #[must_use]
    pub const fn stored(self) -> Number {
        self.0
    }
}

impl<const MIN: i64, const MAX: i64> Coerce for RangedInt<MIN, MAX> {
    // Both bounds are small integers -- the widest is 86400 -- so the f64 the
    // shared range check takes them as is exact.
    #[allow(clippy::cast_precision_loss)]
    fn coerce(value: &toml::Value) -> Option<Self> {
        Number::parse(value)
            .filter(|number| in_range(*number, MIN as f64, MAX as f64))
            .map(Self)
    }
}

impl<const MIN: i64, const MAX: i64> Store for RangedInt<MIN, MAX> {
    fn store(&self) -> toml::Value {
        self.0.store()
    }
}

/// The ends of one float range, and the reason they are where they are.
///
/// Associated constants rather than const generic parameters: `f64` is not
/// allowed as one, and putting the bounds in a marker type keeps them next to
/// their justification instead of scattering scaled integers through the table.
pub trait FloatRange {
    /// Inclusive lower end.
    const MIN: f64;
    /// Inclusive upper end.
    const MAX: f64;
}

/// Declares one range marker with its bounds documented.
macro_rules! float_range {
    ($(#[$doc:meta])* $name:ident, $min:expr, $max:expr) => {
        $(#[$doc])*
        #[derive(Copy, Clone, Debug, PartialEq, Eq)]
        pub struct $name;

        impl FloatRange for $name {
            const MIN: f64 = $min;
            const MAX: f64 = $max;
        }
    };
}

float_range!(
    /// `MOTION_SPEED_RANGE`. Half speed is already slow enough to feel like a
    /// fault; double is the point where the slide stops registering as
    /// movement at all and may as well be Reduce Motion.
    MotionSpeed,
    0.5,
    2.0
);

float_range!(
    /// `bar.padding_scale`. One is the shipped spacing exactly, two is as
    /// loose as the bar gets before the right side runs into the clock.
    PaddingScale,
    1.0,
    2.0
);

float_range!(
    /// `input.pointer_sensitivity`. libinput's own range.
    PointerSensitivity,
    -1.0,
    1.0
);

float_range!(
    /// `GLASS_TRANSPARENCY_MAX`. Past roughly a quarter the desktop starts
    /// reading through body text, which is not a setting worth offering.
    GlassTransparency,
    0.0,
    0.25
);

float_range!(
    /// The three unit-interval material weights. 0 and 1 are both meaningful
    /// ends, so the range is closed on both.
    UnitInterval,
    0.0,
    1.0
);

/// `"float"`, with its range carried by a marker type -- see [`FloatRange`].
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct RangedFloat<R: FloatRange>(Number, PhantomData<R>);

impl<R: FloatRange> RangedFloat<R> {
    /// The number as the renderers take it.
    #[must_use]
    pub fn get(self) -> f64 {
        self.0.as_f64()
    }

    /// The number as it is stored, which is not always a float.
    #[must_use]
    pub const fn stored(self) -> Number {
        self.0
    }
}

impl<R: FloatRange> Coerce for RangedFloat<R> {
    fn coerce(value: &toml::Value) -> Option<Self> {
        Number::parse(value)
            .filter(|number| in_range(*number, R::MIN, R::MAX))
            .map(|number| Self(number, PhantomData))
    }
}

impl<R: FloatRange> Store for RangedFloat<R> {
    fn store(&self) -> toml::Value {
        self.0.store()
    }
}

#[cfg(test)]
mod tests {
    use super::{in_range, Number, RangedFloat, RangedInt, UnitInterval};
    use crate::schema::coerce::{Coerce, Store};

    fn parse(text: &str) -> toml::Value {
        let table: toml::Table = format!("value = {text}").parse().unwrap();
        table.get("value").unwrap().clone()
    }

    #[test]
    fn numbers_keep_the_shape_the_file_stored_them_in() {
        let whole = RangedFloat::<UnitInterval>::coerce(&parse("1")).unwrap();
        assert_eq!(whole.stored(), Number::Int(1));
        assert_eq!(whole.store(), parse("1"));
        let fractional = RangedInt::<0, 10>::coerce(&parse("2.5")).unwrap();
        assert_eq!(fractional.get(), 2);
        assert_eq!(fractional.store(), parse("2.5"));
    }

    #[test]
    fn ranges_are_inclusive_and_refuse_the_non_finite() {
        assert!(RangedInt::<1000, 10000>::coerce(&parse("1000")).is_some());
        assert!(RangedInt::<1000, 10000>::coerce(&parse("10000")).is_some());
        assert!(RangedInt::<1000, 10000>::coerce(&parse("999")).is_none());
        assert!(RangedInt::<1000, 10000>::coerce(&parse("true")).is_none());
        assert!(RangedFloat::<UnitInterval>::coerce(&parse("nan")).is_none());
        assert!(RangedFloat::<UnitInterval>::coerce(&parse("inf")).is_none());
        assert!(RangedFloat::<UnitInterval>::coerce(&parse("-0.0")).is_some());
        assert!(in_range(Number::Float(0.5), 0.5, 2.0));
        assert!(!in_range(Number::Float(2.5), 0.5, 2.0));
    }
}
