//! Reading `displays.toml`, and resolving the mirrors Hyprland will honour.

use crate::displays::{DisplayEntry, DisplayLayout, LayoutValue, MirrorRefusal};
use crate::error::RenderError;
use crate::workspaces::blocks::load_toml;

/// `load_display_config()` (garage:2417-2423): the saved layout, or the refusal.
///
/// # Errors
///
/// [`RenderError::Toml`] for a file that cannot be read or parsed -- `load_toml()`'s own
/// `SettingsError(f"{path.name}: {error}")` -- and for a `display` key that is present but is
/// not an array of tables, which is the one shape check the Python makes here.
pub fn load_display_config(path: &std::path::Path) -> Result<DisplayLayout, RenderError> {
    let raw = load_toml(path)?;
    let refuse = || RenderError::Toml {
        name: "displays.toml".to_owned(),
        detail: "display must be an array of tables".to_owned(),
    };
    let displays = match raw.get("display") {
        None => Vec::new(),
        Some(value) => value
            .as_array()
            .ok_or_else(refuse)?
            .iter()
            .map(|item| match item {
                // A non-table element is not refused: the Python only checks that `display`
                // is a list, and every reader below reaches for `.get()` on the element,
                // which would raise AttributeError for a scalar. An empty record answers
                // every `.get()` with its default, which is the nearest survivable reading.
                toml::Value::Table(table) => DisplayEntry::from_fields(
                    table
                        .iter()
                        .map(|(key, held)| (key.clone(), LayoutValue::from_toml(held)))
                        .collect(),
                ),
                toml::Value::String(_)
                | toml::Value::Integer(_)
                | toml::Value::Float(_)
                | toml::Value::Boolean(_)
                | toml::Value::Datetime(_)
                | toml::Value::Array(_) => DisplayEntry::default(),
            })
            .collect(),
    };
    Ok(DisplayLayout {
        primary: raw
            .get("primary")
            .map(LayoutValue::from_toml)
            .map_or_else(String::new, |held| held.py_str()),
        displays,
    })
}

/// The source each display mirrors, for the mirrors Hyprland will honour.
///
/// Hyprland's `setMirror` refuses to mirror the display itself or another mirror, and a
/// disabled output has nothing to copy. It logs and ignores the rule in those cases, so the
/// layout on screen would quietly stop matching the one that was saved.
///
/// Lenient: a combination Hyprland would refuse simply mirrors nothing. See
/// [`strict_mirror_targets`] for the apply path's reading of the same rules, and the module
/// doc for why the two differ.
#[must_use]
pub fn mirror_targets(displays: &[DisplayEntry]) -> Vec<(String, String)> {
    resolve_mirrors(displays, false).unwrap_or_default()
}

/// [`mirror_targets`] with the refusals turned back on, for `apply_display_layout()`.
///
/// # Errors
///
/// [`MirrorRefusal`] naming the first display whose mirror Hyprland would ignore, in the
/// order the layout lists them.
pub fn strict_mirror_targets(
    displays: &[DisplayEntry],
) -> Result<Vec<(String, String)>, MirrorRefusal> {
    resolve_mirrors(displays, true)
}

/// Both readings of the mirror rules, from the one body the Python has.
fn resolve_mirrors(
    displays: &[DisplayEntry],
    strict: bool,
) -> Result<Vec<(String, String)>, MirrorRefusal> {
    // `{str(item.get("output", "")): str(item.get("mirror") or "") for item in displays if
    // item.get("enabled", True)}` -- a dict comprehension, so a repeated output keeps its
    // first position and its last mirror.
    let mut enabled: Vec<(String, String)> = Vec::new();
    for entry in displays.iter().filter(|entry| entry.enabled()) {
        let (output, source) = (entry.output(), entry.mirror());
        match enabled.iter_mut().find(|(name, _)| *name == output) {
            Some(existing) => existing.1 = source,
            None => enabled.push((output, source)),
        }
    }
    let mut targets: Vec<(String, String)> = Vec::new();
    for (output, source) in &enabled {
        if source.is_empty() {
            continue;
        }
        let theirs = enabled
            .iter()
            .find(|(name, _)| name == source)
            .map(|(_, mirror)| mirror.as_str());
        let problem = if source == output {
            "cannot mirror itself".to_owned()
        } else {
            match theirs {
                None => format!("cannot mirror {source}: that display is turned off"),
                Some(mirror) if !mirror.is_empty() => {
                    format!("cannot mirror {source}: that display is itself a mirror")
                }
                Some(_) => String::new(),
            }
        };
        if problem.is_empty() {
            targets.push((output.clone(), source.clone()));
        } else if strict {
            return Err(MirrorRefusal(format!("{output} {problem}")));
        }
    }
    Ok(targets)
}
