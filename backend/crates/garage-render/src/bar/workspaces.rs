//! `render_bar_workspaces()`: publish the bar's left side -- menu, workspaces, then media.
//!
//! A fragment of its own rather than folded into either neighbour: `waybar-clock.jsonc` is
//! written on a region change and `waybar-widgets.jsonc` on a bar change, and sharing either
//! file would make each writer responsible for reproducing the other's content or silently
//! dropping it.
//!
//! Only `modules-left` is published. `persistent-workspaces` used to be spelled out in
//! `config.jsonc`, monitor by monitor, as a second copy of the workspace plan; waybar 0.15
//! already loads persistent workspaces from Hyprland's own workspace rules -- which
//! [`crate::workspaces::plan::render_workspaces`] generates -- so the copy is gone rather
//! than generated, and the bar follows a count change with nothing left to keep in step.
//!
//! Written unconditionally, even with no plan at all: `config.jsonc` no longer names
//! `modules-left` itself, so an absent fragment would be an empty left side rather than a
//! wrong one -- there is no static fallback to fall back to.

use garage_core::fs::atomic::atomic_write;
use garage_core::toml_emit::{toml_value, Value};

use crate::cx::RenderCx;
use crate::error::RenderError;

/// `WAYBAR_MODULES_LEFT` (garage:142). waybar cannot append to an array from an include --
/// an option is won by whichever file names it first, and `config.jsonc` is read before
/// anything it includes -- so the whole list has to be published here for either variant of
/// either module to reach the bar. `custom/menu` is carried along unchanged.
const WAYBAR_MODULES_LEFT: &[&str] = &["custom/menu"];

/// `WAYBAR_WORKSPACE_MODULE` (garage:143).
const WAYBAR_WORKSPACE_MODULE: &str = "ext/workspaces";

/// `json.dumps({"modules-left": modules}, indent=2) + "\n"`, written out by hand.
///
/// Two-space indent, `": "` between key and value, `",\n"` between array items, and an empty
/// array stays inline as `[]` -- all of which is what `CPython`'s encoder does with
/// `indent=2`. The names are quoted through [`toml_value`], which is the same JSON string
/// encoder the Python's own TOML emitter borrows, so a module name is escaped exactly once
/// and in exactly one place.
fn modules_left_json(modules: &[&str]) -> String {
    if modules.is_empty() {
        return "{\n  \"modules-left\": []\n}\n".to_owned();
    }
    let items: Vec<String> = modules
        .iter()
        .map(|name| {
            format!(
                "    {}",
                toml_value(&Value::Str((*name).to_owned())).unwrap_or_default()
            )
        })
        .collect();
    format!(
        "{{\n  \"modules-left\": [\n{}\n  ]\n}}\n",
        items.join(",\n")
    )
}

/// Write the bar's left side: the menu, the workspace indicator, and media if enabled.
///
/// # Errors
///
/// [`RenderError::Atomic`] if the fragment cannot be replaced.
pub(crate) fn render_bar_workspaces(cx: &RenderCx<'_>) -> Result<(), RenderError> {
    let mut modules: Vec<&str> = WAYBAR_MODULES_LEFT.to_vec();
    if cx.prefs().workspaces.indicator {
        modules.push(WAYBAR_WORKSPACE_MODULE);
    }
    if cx.prefs().bar.media_player {
        modules.push("custom/media");
    }
    atomic_write(
        &cx.paths().fragments.waybar_workspaces,
        &modules_left_json(&modules),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::modules_left_json;

    /// The exact bytes `json.dumps({"modules-left": [...]}, indent=2) + "\n"` produces --
    /// checked here rather than only through the fixtures, because this is the one shape in
    /// this crate that is JSON rather than Lua or TOML.
    #[test]
    fn the_encoder_matches_cpython_indent_two() {
        assert_eq!(
            modules_left_json(&["custom/menu"]),
            "{\n  \"modules-left\": [\n    \"custom/menu\"\n  ]\n}\n"
        );
        assert_eq!(
            modules_left_json(&["custom/menu", "ext/workspaces", "custom/media"]),
            "{\n  \"modules-left\": [\n    \"custom/menu\",\n    \"ext/workspaces\",\n    \
             \"custom/media\"\n  ]\n}\n"
        );
        assert_eq!(modules_left_json(&[]), "{\n  \"modules-left\": []\n}\n");
    }
}
