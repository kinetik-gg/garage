//! `rofi_palette_rasi()`: the launcher palette. `rasi` spells alpha as two hex digits
//! appended to the colour.
//!
//! A handful of roles handed straight through -- background, selection, border and two text
//! weights -- each with the alpha rofi's `rasi` syntax wants baked into the hex string itself
//! rather than a separate channel, and a `background-color` deliberately left transparent so
//! the base theme's own layer shows through. `@import "apple-base.rasi"` carries everything
//! that is not a colour, so this file's whole job is naming a few variables.
//!
//! Returns a `String`, written by `render_toolkits()` rather than by this module.

use garage_core::paths::Paths;
use garage_core::schema::enums::Scheme;

use crate::error::RenderError;
use crate::palette::table::role;
use crate::template::shipped::ROFI_PALETTE;
use crate::template::vars::template_vars;
use crate::template::Template;

template_vars!(
    /// The five roles rofi is handed, and the appearance the header names. The two hex
    /// digits of alpha appended to each are `rasi` syntax rather than a colour, so they
    /// stay in the template where the rest of the syntax is.
    RofiVars {
        scheme: Scheme,
        bg_raised: &'static str,
        accent: &'static str,
        on_bg: &'static str,
        fg: &'static str,
        fg_bright: &'static str,
        fg_muted: &'static str,
    }
);

/// The launcher palette (garage:3783-3800). `rasi` spells alpha as two hex digits on the
/// colour.
///
/// # Errors
///
/// [`RenderError::Template`] if `rofi-palette.tmpl` names a variable this does not supply.
pub(crate) fn rofi_palette_rasi(paths: &Paths, scheme: Scheme) -> Result<String, RenderError> {
    let colour = |name: &str| role(scheme, name).unwrap_or_default();
    let rasi = Template::load(paths, ROFI_PALETTE).expand(&RofiVars {
        scheme,
        bg_raised: colour("bg_raised"),
        accent: colour("accent"),
        on_bg: colour("on_bg"),
        fg: colour("fg"),
        fg_bright: colour("fg_bright"),
        fg_muted: colour("fg_muted"),
    })?;
    Ok(rasi)
}
