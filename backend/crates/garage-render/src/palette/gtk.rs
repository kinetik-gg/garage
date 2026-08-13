//! `gtk3_palette_css()` and `gtk4_palette_css()`: the palette as `@define-color` tokens.
//!
//! Two functions rather than one, because the two GTK generations do not carry the same
//! token set: GTK3 needs the pre-libadwaita `theme_*` tokens `adw-gtk3` still reads, and GTK4
//! needs the status pairs and the shade libadwaita draws with. Neither list is padded out to
//! the other -- a token a toolkit does not define today is a token its stylesheet is
//! currently getting from its own theme, and defining it here would silently restyle the
//! desktop.
//!
//! `gtk4_palette_css()` carries both schemes in one file, behind a `prefers-color-scheme`
//! media query, for a reason GTK3 does not share: GTK loads `gtk.css` once per process, so a
//! per-scheme `@import` would freeze the palette a running app launched with. libadwaita
//! re-evaluates the media query when the portal's `color-scheme` setting changes, which is
//! what `push_theme()` moves -- so a GTK4 app re-themes live and a GTK3 app does not, and
//! that difference is exactly why GTK3's file stays per-scheme while GTK4's does not.
//!
//! Both appearances' GTK3 files are written on every render, not just the resolved one, for
//! the reason `render_toolkits()`'s own doc gives: a render interrupted between the two would
//! otherwise leave a stylesheet importing a palette file that is not there, which GTK reads
//! as a silently dropped palette rather than an error anyone would see.
//!
//! Doc-only: both return `String`, composed into files `render_toolkits()` writes rather than
//! writing one of their own.
