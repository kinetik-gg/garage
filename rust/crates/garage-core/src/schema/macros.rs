//! `preferences!`: the codegen half of the preference table.
//!
//! Nothing here decides anything. Every judgement -- what a usable value is,
//! what an unusable one is replaced with, what note that costs, where `set`
//! goes afterwards -- lives in an ordinary function in `coerce`, `defaults` or
//! `routes`, and this macro only wires each entry up to them. The expansion is
//! the same handful of arms per key, so the shape is reviewed once here and the
//! *content* once in `prefs.rs`, the file the product is written in.

/// The preference table, as one declaration per key. Grammar, per entry --
/// `prefs.rs` is it in use:
///
/// ```text
/// Variant = field: Type [=> RouteVariant] [, renames ["old" => "new"]] ;
/// ```
///
/// `Variant` is the `PreferenceKey` spelling of the dotted key, `field` the key
/// as `preferences.toml` spells it, and `Type` the kind: the type *is* the
/// constraint, so `RangedInt<1000, 10000>` is `"kind": "int", "minimum": 1000,
/// "maximum": 10000`. A missing route is "not settable at all".
macro_rules! preferences {
    // Emitted by `route()` below. Split out because a macro repetition cannot
    // expand to two different expressions in place.
    (@route $route:ident) => { Some(Route::$route) };
    (@route) => { None };

    ($(
        $(#[$section_doc:meta])*
        $Section:ident = $section:ident {
            $(
                $(#[$key_doc:meta])*
                $Key:ident = $key:ident : $ty:ty
                    $(=> $route:ident)?
                    $(, renames [$($from:literal => $to:literal),+ $(,)?])? ;
            )+
        }
    )+) => {
        $(
            $(#[$section_doc])*
            #[derive(Clone, Debug, PartialEq)]
            pub struct $Section {
                $(
                    $(#[$key_doc])*
                    pub $key: $ty,
                )+
            }
        )+

        /// Every section, which is every preference this build has. Layer 1
        /// and layer 2 are both this type: the shipped defaults parsed once,
        /// and the stored file coerced onto them.
        #[derive(Clone, Debug, PartialEq)]
        pub struct Preferences {
            $(
                #[doc = concat!("The `[", stringify!($section), "]` section.")]
                pub $section: $Section,
            )+
        }

        /// The sections the validation pass walks, in the shipped file's
        /// order. `[schema]` is not one of them: the version stamp is
        /// bookkeeping, written unconditionally, and coercing it would mean
        /// coercing the very thing a migration reads to decide whether to run.
        #[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
        pub enum Section {
            $(
                #[doc = concat!("`[", stringify!($section), "]`.")]
                $Section,
            )+
        }

        impl Section {
            /// Every section, in declaration order.
            pub const ALL: &'static [Self] = &[$(Self::$Section,)+];

            /// The name this section carries in `preferences.toml`.
            #[must_use]
            pub const fn as_str(self) -> &'static str {
                match self { $(Self::$Section => stringify!($section),)+ }
            }
        }

        impl ::core::fmt::Display for Section {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        /// One variant per key in the table, and nothing else exists: the
        /// Python's "a key with no entry here does not exist", moved into the
        /// type system. `set` cannot be handed an unknown key, the validation
        /// pass cannot skip one, the departures walk cannot invent one.
        #[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
        pub enum PreferenceKey { $($($Key,)+)+ }

        impl PreferenceKey {
            /// Every key, in declaration order -- which is the order the
            /// coercion pass walks and the departures are written.
            pub const ALL: &'static [Self] = &[$($(Self::$Key,)+)+];

            /// Which section this key belongs to.
            #[must_use]
            pub const fn section(self) -> Section {
                match self { $($(Self::$Key => Section::$Section,)+)+ }
            }

            /// The key on its own, without the section.
            #[must_use]
            pub const fn name(self) -> &'static str {
                match self { $($(Self::$Key => stringify!($key),)+)+ }
            }

            /// What `set` walks after the file is written, or `None` for a key
            /// that is not settable at all.
            #[must_use]
            pub const fn route(self) -> Option<Route> {
                match self { $($(Self::$Key => preferences!(@route $($route)?),)+)+ }
            }
        }

        impl ::core::fmt::Display for PreferenceKey {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                write!(f, "{}.{}", self.section().as_str(), self.name())
            }
        }

        impl ::core::str::FromStr for PreferenceKey {
            type Err = ParseKeyError;

            // The dotted spelling, which is what `garage set` is given. A
            // linear scan rather than a generated `match`: `stringify!` cannot
            // be used in pattern position, and sixty-odd comparisons on a path
            // taken once per process is not worth a second table.
            fn from_str(text: &str) -> Result<Self, Self::Err> {
                let split = text.split_once('.');
                Self::ALL
                    .iter()
                    .copied()
                    .find(|key| split == Some((key.section().as_str(), key.name())))
                    .ok_or_else(|| ParseKeyError::new(text))
            }
        }

        impl Preferences {
            /// This configuration's value for one key, as it is stored.
            #[must_use]
            pub fn get(&self, key: PreferenceKey) -> ::toml::Value {
                match key {
                    $($(PreferenceKey::$Key =>
                        $crate::schema::coerce::Store::store(&self.$section.$key),)+)+
                }
            }

            /// Every key and its value, in declaration order -- the departures
            /// walk. `preference_deltas()` is driven by the shipped defaults so
            /// the emitted file comes out in that file's own key order, and
            /// declaration order is that order (`declared_order_matches_the_file`).
            pub fn each_key<F: FnMut(PreferenceKey, ::toml::Value)>(&self, mut visit: F) {
                for key in PreferenceKey::ALL.iter().copied() {
                    visit(key, self.get(key));
                }
            }

            /// Layer 1: every default read out of a parsed defaults table.
            ///
            /// # Errors
            ///
            /// `MissingDefault` for the first key the table does not carry at
            /// the type the schema declares. Hard rather than coerced on
            /// purpose -- see `Defaults`.
            pub fn from_defaults(table: &::toml::Table) -> Result<Self, MissingDefault> {
                Ok(Self { $($section: $Section {
                    $($key: $crate::schema::defaults::require(table, PreferenceKey::$Key)?,)+
                },)+ })
            }

            /// `validate_preferences()`: layer 2 put at values this build can
            /// render, with one note per substitution.
            ///
            /// `raw` is the stored file rather than the merged view, so a key
            /// it does not carry takes the default silently -- which is what
            /// the Python's merge-then-validate does for the same file.
            #[must_use]
            pub fn coerce_from(
                raw: &::toml::Table,
                defaults: &$crate::schema::defaults::Defaults,
                notes: &mut $crate::schema::notes::Notes,
            ) -> Self {
                let shipped = defaults.values();
                let mut coerced = shipped.clone();
                $(
                    let stored = raw
                        .get(stringify!($section))
                        .and_then(::toml::Value::as_table);
                    $(
                        coerced.$section.$key = $crate::schema::coerce::field(
                            stored.and_then(|table| table.get(stringify!($key))),
                            &[$($(($from, $to)),+)?],
                            PreferenceKey::$Key,
                            &shipped.$section.$key,
                            notes,
                        );
                    )+
                )+
                $crate::schema::coerce::cross_key_checks(&mut coerced, shipped, notes);
                coerced
            }

            /// `set`: one key onto a raw value, then the cross-key pass.
            ///
            /// The Python stores the raw value and lets `validate_preferences`
            /// coerce it on the next line; a typed field cannot hold a value
            /// this build refuses, so the two steps are fused here and the note
            /// they produce is the same one.
            ///
            /// # Errors
            ///
            /// `SetError::NotSettable` for a key with no route, and
            /// `SetError::Defaults` when the compiled defaults -- the fallback
            /// a bad value is put back onto -- cannot be read.
            pub fn set(
                &mut self,
                key: PreferenceKey,
                value: &::toml::Value,
                notes: &mut $crate::schema::notes::Notes,
            ) -> Result<(), SetError> {
                if key.route().is_none() {
                    return Err(SetError::NotSettable { key });
                }
                let defaults = $crate::schema::defaults::Defaults::compiled()?;
                let shipped = defaults.values();
                match key {
                    $($(PreferenceKey::$Key => {
                        self.$section.$key = $crate::schema::coerce::field(
                            Some(value),
                            &[$($(($from, $to)),+)?],
                            key,
                            &shipped.$section.$key,
                            notes,
                        );
                    })+)+
                }
                $crate::schema::coerce::cross_key_checks(self, shipped, notes);
                Ok(())
            }
        }
    };
}

pub(crate) use preferences;
