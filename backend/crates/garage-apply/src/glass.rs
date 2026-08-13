//! `apply_glass()`: push the whole window material straight into the running compositor.
//!
//! The shell's own surfaces are client-side alpha, which decoration opacity multiplies but
//! cannot lift. Off draws no glass behind them either, so without this the session menu and
//! the panels would have no background at all -- Quickshell watches the material marker and
//! paints itself solid instead. That marker is written from here as well as from
//! `render_preferences()`, so it exists before the shell first reads it, the same way the
//! corner radius marker is handled.
//!
//! Pushed with [`crate::eval`]'s `eval_config()`, over both the core decoration options and
//! the Kinetik Glass plugin's own. If the plugin failed to load, that call fails as a whole --
//! most likely because the plugin is not loaded, in which case the fragment's own
//! `GLASS_AVAILABLE` guard is the thing that has to decide -- and the fallback is a full
//! `hyprctl reload` rather than refusing the setting outright.

use garage_core::fs::marker::write_marker;
use garage_render::lua::emit::{glass_options, lua_pairs, material_decoration};

use crate::command::run;
use crate::cx::SessionCx;
use crate::error::ApplyError;
use crate::eval::eval_config;

/// Push the window material (glass mode, blur, tint, dispersion) into the running compositor
/// (garage:4728-4750).
///
/// # Errors
///
/// [`ApplyError::Marker`] if the material marker could not be written, and
/// [`ApplyError::Settings`] when *both* the eval and the fallback reload are refused -- the
/// eval's own complaint if it had one, stdout ahead of stderr as the Python reads them, and
/// `"Unable to apply glass settings"` when it had neither.
pub(crate) fn apply_glass(cx: &mut SessionCx<'_>) -> Result<(), ApplyError> {
    let paths = cx.render().paths();
    std::fs::create_dir_all(&paths.generated).map_err(|error| ApplyError::Io(error.to_string()))?;
    let prefs = cx.render().prefs();
    write_marker(
        &paths.markers.material,
        &format!("{}\n", prefs.appearance.glass_mode.as_str()),
    )?;
    let body = format!(
        "decoration = {{{}}}, plugin = {{kinetik_glass = {{{}}}}}",
        lua_pairs(&material_decoration(prefs), ", "),
        lua_pairs(&glass_options(prefs), ", ")
    );
    let result = eval_config(cx, &body);
    if result.status == 0 {
        return Ok(());
    }
    // Most likely the plugin failed to load, in which case the fragment's own
    // GLASS_AVAILABLE guard is the thing that has to decide. Fall back to the reload this
    // used to do rather than refusing the setting outright.
    if run(cx, &["hyprctl", "reload"]).status == 0 {
        return Ok(());
    }
    let stdout = result.stdout.trim();
    let stderr = result.stderr.trim();
    Err(ApplyError::Settings(if stdout.is_empty() {
        if stderr.is_empty() {
            "Unable to apply glass settings".to_owned()
        } else {
            stderr.to_owned()
        }
    } else {
        stdout.to_owned()
    }))
}
