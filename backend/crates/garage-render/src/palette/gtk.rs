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
//! Both return `String`, composed into files `render_toolkits()` writes rather than writing
//! one of their own.

use garage_core::paths::Paths;
use garage_core::schema::enums::Scheme;

use crate::error::RenderError;
use crate::palette::table::{role, GTK3_TOKENS, GTK4_TOKENS};
use crate::template::shipped::{
    GTK3_PALETTE_HEAD, GTK3_PALETTE_TOKEN, GTK4_PALETTE, GTK4_PALETTE_DARK_TOKEN,
    GTK4_PALETTE_LIGHT_TOKEN,
};
use crate::template::vars::template_vars;
use crate::template::{Shipped, Template};

template_vars!(
    /// Which appearance a header names.
    Gtk3HeadVars { scheme: Scheme }
);

template_vars!(
    /// One token line, for all three of the token templates: what a token is called and
    /// what colour it is. Which tokens exist is [`GTK3_TOKENS`]/[`GTK4_TOKENS`]' business
    /// -- a table, walked in code -- and how one is spelled is the template's.
    TokenVars {
        token: &'static str,
        color: &'static str,
    }
);

template_vars!(
    /// The two blocks of already-spelled token lines the GTK4 sheet wraps.
    Gtk4Vars { light: String, dark: String }
);

/// The GTK3 palette `adw-gtk3` reads, as `@define-color` over the palette
/// (garage:3752-3757).
///
/// # Errors
///
/// [`RenderError::Template`] if either template names a variable this does not supply.
pub(crate) fn gtk3_palette_css(paths: &Paths, scheme: Scheme) -> Result<String, RenderError> {
    let mut css = Template::load(paths, GTK3_PALETTE_HEAD).expand(&Gtk3HeadVars { scheme })?;
    css.push_str(&token_lines(
        paths,
        GTK3_PALETTE_TOKEN,
        scheme,
        GTK3_TOKENS,
    )?);
    Ok(css)
}

/// The GTK4/libadwaita palette, both appearances in one file (garage:3760-3780).
///
/// GTK loads `gtk.css` once per process, so a per-scheme import would freeze the palette a
/// running app launched with. libadwaita re-evaluates the media query when the portal's
/// color-scheme changes, which is what `push_theme()` moves.
///
/// # Errors
///
/// [`RenderError::Template`] if any of the three templates names a variable this does not
/// supply.
pub(crate) fn gtk4_palette_css(paths: &Paths) -> Result<String, RenderError> {
    let css = Template::load(paths, GTK4_PALETTE).expand(&Gtk4Vars {
        light: token_lines(paths, GTK4_PALETTE_LIGHT_TOKEN, Scheme::Light, GTK4_TOKENS)?,
        dark: token_lines(paths, GTK4_PALETTE_DARK_TOKEN, Scheme::Dark, GTK4_TOKENS)?,
    })?;
    Ok(css)
}

/// One table of tokens, each through the same line template.
///
/// The loop is here and the line is a file, which is the split the whole of this module
/// turns on: the token list is a table pinned against the Python's own by
/// [`crate::palette::parity`], and adding one is a code change; how a line of CSS is
/// spelled is not.
fn token_lines(
    paths: &Paths,
    shipped: Shipped,
    scheme: Scheme,
    tokens: &[(&'static str, &'static str)],
) -> Result<String, RenderError> {
    let template = Template::load(paths, shipped);
    let mut lines = String::new();
    for &(token, name) in tokens {
        lines.push_str(&template.expand(&TokenVars {
            token,
            color: colour(scheme, name),
        })?);
    }
    Ok(lines)
}

/// A token's colour. Every name in the two token tables is a `PALETTE` role by construction
/// -- `palette::parity`'s `gtk3_tokens_match_the_python_dump` and its GTK4 twin pin both
/// tables against the Python source -- so a miss would be a broken build, not something a
/// render could hit,
/// and the empty string it falls back to would be visible in the very first generated file.
fn colour(scheme: Scheme, name: &str) -> &'static str {
    role(scheme, name).unwrap_or_default()
}
