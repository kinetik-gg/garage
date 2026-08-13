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

use crate::cx::SessionCx;
use crate::error::ApplyError;

/// Push the window material (glass mode, blur, tint, dispersion) into the running compositor.
///
/// # Errors
///
/// Always [`ApplyError::PortPending`] until Phase 3 replaces this stub.
pub(crate) fn apply_glass(_cx: &mut SessionCx<'_>) -> Result<(), ApplyError> {
    Err(ApplyError::PortPending("apply_glass"))
}
