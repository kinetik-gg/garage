//! The bar's three generated fragments, plus the pure CSS pieces they and the palette share.
//!
//! [`workspaces`] and [`widgets`] each write one fragment and carry a stub; [`style`] and
//! [`spacing`] are the pure string builders behind the bar's stylesheet and stay doc-only,
//! reached from [`crate::palette::waybar`] rather than from a fragment of their own.

pub(crate) mod layout;
pub(crate) mod spacing;
pub(crate) mod style;
pub(crate) mod widgets;
pub(crate) mod workspaces;
