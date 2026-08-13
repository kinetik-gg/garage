//! `qt_palette_conf()`: the Qt palette `qt6ct` loads, as three positional rows of colours.
//!
//! `QPalette::ColorRole` is positional and cannot be reordered, so the row for each of
//! `active`, `inactive` and `disabled` is built by walking the same fixed role list in the
//! same order every time, and a wrapped comment naming every role is written into the file's
//! own header -- the role order is the file's only documentation of what each column is, and
//! it is what makes a hand-read of this file possible at all.
//!
//! Every role goes through [`crate::theme`]'s `opaque()`: `qt6ct` can only spell `#rrggbb`,
//! and a composited role handed to it would write the literal text `rgba(...)` into a file
//! whose parser takes neither -- Qt would then silently fall back to Fusion's own grey rather
//! than failing loudly. Better to fail here, where the role table is, than on the next login.
//!
//! Doc-only: returns a `String`, written by `render_toolkits()` rather than by this module.
