//! `template_vars!`: one struct, one [`TemplateVars`](super::TemplateVars) impl.
//!
//! Every renderer's variable map is the same three things written out -- the fields, a
//! `match` from name to field, and the list of names -- and writing them by hand is three
//! places for a typo to hide in. Here the field name *is* the placeholder name, by
//! construction, so a renamed field is a renamed variable and the drift tests in
//! [`super::shipped`] catch the template that still spells it the old way.
//!
//! Every field is rendered with `Display`, which is what makes an `i64` timeout, a `bool`
//! and a `&'static str` colour all spellable without the caller formatting them first.

/// Declare a variable map: a struct with `Display` fields, and the trait impl that hands
/// them to the engine by name.
macro_rules! template_vars {
    ($(#[$note:meta])* $name:ident { $($field:ident: $type:ty),+ $(,)? }) => {
        $(#[$note])*
        #[derive(Debug)]
        pub(crate) struct $name {
            $($field: $type),+
        }

        impl $crate::template::TemplateVars for $name {
            fn get(&self, name: &str) -> Option<std::borrow::Cow<'_, str>> {
                match name {
                    $(stringify!($field) => {
                        Some(std::borrow::Cow::Owned(self.$field.to_string()))
                    })+
                    _ => None,
                }
            }

            fn names() -> &'static [&'static str] {
                &[$(stringify!($field)),+]
            }
        }
    };
}

pub(crate) use template_vars;
