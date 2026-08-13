//! `swayosd_palette_css()`: the volume/brightness OSD palette, as GTK named colours.
//!
//! Six roles -- the OSD's edge, body, foreground, muted foreground, track and fill -- each a
//! `@define-color`, imported by a small per-scheme `style.css` alongside `swayosd-base.css`.
//! The OSD is the one surface that reads its own composited body colour directly rather than
//! through a toolkit's own theming layer, which is why it gets a palette writer of its own
//! rather than sharing GTK's.
//!
//! Returns a `String`, written by `render_toolkits()` rather than by this module.

use garage_core::paths::Paths;
use garage_core::schema::enums::Scheme;

use crate::error::RenderError;
use crate::palette::table::role;
use crate::template::shipped::SWAYOSD_PALETTE;
use crate::template::vars::template_vars;
use crate::template::Template;

template_vars!(
    /// The OSD's six roles, and the appearance its header names. The `@define-color` names
    /// they land under are swayosd's own vocabulary, which is why they are in the template
    /// and not here: the mapping from a palette role to an OSD name is what this renderer
    /// decides, and only that.
    SwayosdVars {
        scheme: Scheme,
        edge: &'static str,
        bg: &'static str,
        fg: &'static str,
        fg_muted: &'static str,
        track: &'static str,
        fill: &'static str,
    }
);

/// The volume/brightness OSD palette, as GTK named colours (garage:3803-3815).
///
/// # Errors
///
/// [`RenderError::Template`] if `swayosd-palette.tmpl` names a variable this does not
/// supply.
pub(crate) fn swayosd_palette_css(paths: &Paths, scheme: Scheme) -> Result<String, RenderError> {
    let colour = |name: &str| role(scheme, name).unwrap_or_default();
    let css = Template::load(paths, SWAYOSD_PALETTE).expand(&SwayosdVars {
        scheme,
        edge: colour("edge"),
        bg: colour("osd_bg"),
        fg: colour("osd_fg"),
        fg_muted: colour("osd_fg_muted"),
        track: colour("osd_track"),
        fill: colour("osd_fill"),
    })?;
    Ok(css)
}
