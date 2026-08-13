//! Pointer-aware session-menu actions.
//!
//! `menu.toggle` shares the panel action's cursor-to-output hit test. `menu.dismiss` reads one
//! layer snapshot, stands down while a confirmation or screenshot overlay owns the click, and
//! closes the session menu only when the pointer is outside its half-open box.

use serde_json::Value;

use super::hyprland::{cursor_position, output_at_cursor, Point};
use crate::command::{json_object, run_checked};
use crate::cx::SessionCx;
use crate::error::ApplyError;

const BLOCKING_NAMESPACES: [&str; 2] = ["garage-session-confirmation", "garage-screenshot"];
const MENU_NAMESPACE: &str = "garage-session-menu";

#[derive(Clone, Copy, Debug, PartialEq)]
struct SurfaceBox {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

impl SurfaceBox {
    fn contains(self, point: Point) -> bool {
        point.x >= self.x
            && point.x < self.x + self.width
            && point.y >= self.y
            && point.y < self.y + self.height
    }
}

pub(super) fn toggle(cx: &SessionCx<'_>) -> Result<(), ApplyError> {
    let output = output_at_cursor(cx)?;
    run_checked(
        cx,
        &[
            "qs",
            "ipc",
            "-c",
            "garage",
            "call",
            "shell",
            "sessionOn",
            &output.name,
        ],
    )
    .map(drop)
}

pub(super) fn dismiss(cx: &SessionCx<'_>) -> Result<(), ApplyError> {
    let layers = Value::Object(json_object(cx, &["hyprctl", "layers", "-j"]));
    if has_namespace(&layers, &BLOCKING_NAMESPACES) {
        return Ok(());
    }
    let Some(bounds) = find_surface_box(&layers, MENU_NAMESPACE) else {
        return Ok(());
    };
    let Some(cursor) = cursor_position(cx).map(Point::floored) else {
        return Ok(());
    };
    if bounds.contains(cursor) {
        return Ok(());
    }
    run_checked(
        cx,
        &["qs", "ipc", "-c", "garage", "call", "shell", "closeSession"],
    )
    .map(drop)
}

fn has_namespace(value: &Value, names: &[&str]) -> bool {
    match value {
        Value::Object(object) => {
            object
                .get("namespace")
                .and_then(Value::as_str)
                .is_some_and(|namespace| names.contains(&namespace))
                || object.values().any(|child| has_namespace(child, names))
        }
        Value::Array(items) => items.iter().any(|child| has_namespace(child, names)),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => false,
    }
}

fn find_surface_box(value: &Value, namespace: &str) -> Option<SurfaceBox> {
    match value {
        Value::Object(object) => {
            if object.get("namespace").and_then(Value::as_str) == Some(namespace) {
                return Some(SurfaceBox {
                    x: object.get("x")?.as_f64()?,
                    y: object.get("y")?.as_f64()?,
                    width: object.get("w")?.as_f64()?,
                    height: object.get("h")?.as_f64()?,
                });
            }
            object
                .values()
                .find_map(|child| find_surface_box(child, namespace))
        }
        Value::Array(items) => items
            .iter()
            .find_map(|child| find_surface_box(child, namespace)),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::super::action;
    use crate::testing::{Script, World};

    const CURSOR: &str = include_str!("panel/testdata/cursor-edge.json");
    const MONITORS: &str = include_str!("panel/testdata/monitors.json");
    const LAYERS: &str = r#"{
        "DP-2": {"levels": {"3": [
            {"namespace": "garage-session-menu", "x": 100, "y": 100, "w": 300, "h": 400}
        ]}}
    }"#;

    #[test]
    fn toggle_uses_the_shared_scaled_monitor_hit_test() {
        let world = World::plain(
            "menu-toggle",
            Script::new()
                .answering("hyprctl cursorpos -j", 0, CURSOR, "")
                .answering("hyprctl monitors -j", 0, MONITORS, ""),
        );
        action(&world.paths, world.runner(), "menu.toggle", None).expect("the cursor is on DP-2");
        assert_eq!(
            world.trace(),
            [
                "hyprctl cursorpos -j",
                "hyprctl monitors -j",
                "qs ipc -c garage call shell sessionOn DP-2",
            ]
        );
    }

    #[test]
    fn dismiss_reads_layers_once_and_closes_only_from_outside_the_menu() {
        let inside = World::plain(
            "menu-inside",
            Script::new()
                .answering("hyprctl layers -j", 0, LAYERS, "")
                .answering("hyprctl cursorpos -j", 0, r#"{"x": 399.9, "y": 499.9}"#, ""),
        );
        action(&inside.paths, inside.runner(), "menu.dismiss", None).expect("inside stands down");
        assert_eq!(
            inside.trace(),
            ["hyprctl layers -j", "hyprctl cursorpos -j"]
        );

        let outside = World::plain(
            "menu-outside",
            Script::new()
                .answering("hyprctl layers -j", 0, LAYERS, "")
                .answering("hyprctl cursorpos -j", 0, r#"{"x": 400, "y": 499}"#, ""),
        );
        action(&outside.paths, outside.runner(), "menu.dismiss", None)
            .expect("the right edge is outside");
        assert_eq!(
            outside.trace(),
            [
                "hyprctl layers -j",
                "hyprctl cursorpos -j",
                "qs ipc -c garage call shell closeSession",
            ]
        );
    }

    #[test]
    fn confirmation_and_screenshot_layers_keep_the_click_for_their_overlay() {
        for namespace in ["garage-session-confirmation", "garage-screenshot"] {
            let layers = LAYERS.replace("garage-session-menu", namespace);
            let world = World::plain(
                "menu-overlay",
                Script::new().answering("hyprctl layers -j", 0, &layers, ""),
            );
            action(&world.paths, world.runner(), "menu.dismiss", None)
                .expect("an overlay owns the click");
            assert_eq!(world.trace(), ["hyprctl layers -j"]);
        }
    }

    #[test]
    fn missing_or_malformed_live_state_is_a_quiet_no_op() {
        for layers in ["not json", "{}"] {
            let world = World::plain(
                "menu-no-state",
                Script::new().answering("hyprctl layers -j", 0, layers, ""),
            );
            action(&world.paths, world.runner(), "menu.dismiss", None)
                .expect("there is no safe dismissal target");
            assert_eq!(world.trace(), ["hyprctl layers -j"]);
        }
    }
}
