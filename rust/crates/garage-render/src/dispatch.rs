//! Where a [`RenderStep`] becomes a call.
//!
//! [`run_render`]'s match is exhaustive by the workspace lint
//! (`clippy::wildcard_enum_match_arm = "deny"`, set crate-wide in the workspace manifest), so
//! a variant added to [`RenderStep`] breaks this file by name until it is given an arm here.
//! That is deliberate: [`RenderStep`] and [`crate::error::RenderError`] are the two places a
//! new renderer has to be named, and this file is what makes forgetting one of them a build
//! failure rather than a route that silently does nothing.

use garage_core::schema::routes::RenderStep;

use crate::cx::RenderCx;
use crate::error::RenderError;
use crate::{general, idle, motion, preferences, search};

/// Run one [`RenderStep`], dispatching to the module that owns it.
///
/// # Errors
///
/// Whatever the dispatched renderer returns. [`RenderStep::Preferences`],
/// [`RenderStep::Motion`] and [`RenderStep::Idle`] reach a real implementation as of task
/// 3.3; [`RenderStep::SearchEngine`] and [`RenderStep::General`] are still stubs -- see
/// [`RenderError::PortPending`] -- and Phase 3 replaces those two bodies next, not this
/// match.
pub fn run_render(step: RenderStep, cx: &RenderCx<'_>) -> Result<(), RenderError> {
    match step {
        RenderStep::SearchEngine => search::render_search_engine(cx),
        RenderStep::Preferences => preferences::render_preferences(cx),
        RenderStep::Motion => motion::render_motion(cx),
        RenderStep::General => general::render_general(cx),
        RenderStep::Idle => idle::render_idle(cx),
    }
}
