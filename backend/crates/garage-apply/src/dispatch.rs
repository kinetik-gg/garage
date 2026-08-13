//! Where an [`ApplyStep`] becomes a call.
//!
//! [`run_apply`]'s match is exhaustive by the workspace lint
//! (`clippy::wildcard_enum_match_arm = "deny"`, set crate-wide in the workspace manifest), so
//! a variant added to [`ApplyStep`] breaks this file by name until it is given an arm here.
//! Two variants -- [`ApplyStep::Accent`] and [`ApplyStep::RunOrRaise`] -- have no dedicated
//! module of their own; see [`crate::route`]'s doc for why both live there instead.

use garage_core::schema::routes::ApplyStep;

use crate::cx::SessionCx;
use crate::error::ApplyError;
use crate::{
    bar, border, corner, file_index, glass, locale, motion, night_shift, region, route, terminal,
    theme, wallpaper, workspaces,
};

/// Run one [`ApplyStep`], dispatching to the module that owns it.
///
/// # Errors
///
/// Whatever the dispatched applier returns. Every variant reaches a real implementation.
pub fn run_apply(step: ApplyStep, cx: &mut SessionCx<'_>) -> Result<(), ApplyError> {
    match step {
        ApplyStep::Wallpaper => wallpaper::apply_wallpaper(cx, wallpaper::Moved::Ask),
        ApplyStep::LiveWallpaper(scheme) => wallpaper::apply_live_wallpaper(cx, scheme),
        ApplyStep::Accent => route::apply_accent(cx),
        ApplyStep::CornerRadius => corner::apply_corner_radius(cx),
        ApplyStep::Border => border::apply_border(cx),
        ApplyStep::Motion => motion::apply_motion(cx),
        ApplyStep::BarStyle => bar::apply_bar_style(cx),
        ApplyStep::ThemeIfSchemeMoved => theme::apply_theme_if_scheme_moved(cx),
        ApplyStep::Glass => glass::apply_glass(cx),
        ApplyStep::NightShift => {
            // The Python drops the flag here too: `("apply_night_shift",)` is a route step
            // like any other, and only `night-shift-sync` reads the answer.
            let _took = night_shift::apply_night_shift(cx);
            Ok(())
        }
        ApplyStep::Terminal => terminal::apply_terminal(cx),
        ApplyStep::FileIndex => file_index::apply_file_index(cx),
        ApplyStep::RunOrRaise { command, message } => route::run_or_raise(cx, command, message),
        ApplyStep::Locale => locale::apply_locale(cx),
        ApplyStep::Region => region::apply_region(cx),
        ApplyStep::WorkspacePlan => workspaces::apply_workspace_plan(cx),
        ApplyStep::BarWorkspaces => bar::apply_bar_workspaces(cx),
        ApplyStep::BarWidgets => bar::apply_bar_widgets(cx),
    }
}
