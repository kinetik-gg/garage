//! Doctor-shaped adapters over [`garage_core::stow`]'s shared read-only analysis.
//!
//! The tree walk and five-way classifier live in core because reconcile consumes the same
//! facts. These wrappers retain doctor's context-shaped call sites; no second interpretation
//! of `.stow-local-ignore` or link ownership remains here.

use std::path::PathBuf;

pub(crate) use garage_core::stow::{link_hop, StowState};

use super::DoctorCx;

/// Classify every leaf in the checkout's stow tree.
pub(crate) fn stow_state(cx: &DoctorCx<'_>) -> StowState {
    garage_core::stow::stow_state(&cx.root, &cx.paths.home)
}

/// Find dangling links into this checkout under the four managed roots.
pub(crate) fn dangling_repo_links(cx: &DoctorCx<'_>) -> Vec<PathBuf> {
    garage_core::stow::dangling_repo_links(cx.paths, &cx.root)
}
