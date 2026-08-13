//! `apply_wallpaper()` and `apply_live_wallpaper()`: put the resolved wallpaper on screen.
//!
//! `apply_wallpaper()` re-points the `current` symlink first, atomically, then decides how
//! hyprpaper needs to hear about it. hyprpaper 0.8.4 reads `fit_mode` from its config only at
//! startup and exposes no reload over IPC, so a config change (a fit change, or the first
//! wallpaper on a machine that has never rendered one) has to restart the service; anything
//! else -- the picture changed but the fit did not -- goes over `hyprctl hyprpaper wallpaper`
//! instead, because a restart would blank every monitor for as long as hyprpaper takes to
//! come back. The resolved target matters for that IPC call: hyprpaper keys its cache on the
//! string it was handed, so re-issuing the `current` symlink's own path is a no-op even once
//! the link points somewhere new -- the resolved target is what actually changes the cache
//! key.
//!
//! `apply_live_wallpaper()` is the one-appearance route's applier: it dresses the desktop
//! only if the appearance the changed key belongs to is the one on screen right now, because
//! dressing the light appearance's wallpaper from a dark session must not change what is
//! behind the pane. `wallpaper_fit` belongs to neither half and always lands through
//! `apply_wallpaper()` directly, which is why it has no route of its own here.

use crate::cx::SessionCx;
use crate::error::ApplyError;

/// Put the resolved wallpaper for the currently active appearance on screen.
///
/// # Errors
///
/// Always [`ApplyError::PortPending`] until Phase 3 replaces this stub.
pub(crate) fn apply_wallpaper(_cx: &mut SessionCx<'_>) -> Result<(), ApplyError> {
    Err(ApplyError::PortPending("apply_wallpaper"))
}

/// Dress the desktop only if `scheme` is the appearance currently on screen.
///
/// # Errors
///
/// Always [`ApplyError::PortPending`] until Phase 3 replaces this stub.
pub(crate) fn apply_live_wallpaper(_cx: &mut SessionCx<'_>) -> Result<(), ApplyError> {
    Err(ApplyError::PortPending("apply_live_wallpaper"))
}
