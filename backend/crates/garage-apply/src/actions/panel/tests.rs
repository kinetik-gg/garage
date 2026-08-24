use serde_json::{json, Value};

use super::super::action;
use crate::testing::{Script, World};

const CURSOR: &str = include_str!("testdata/cursor-edge.json");
const OUTSIDE: &str = include_str!("testdata/cursor-outside.json");
const MONITORS: &str = include_str!("testdata/monitors.json");

fn session_script(cursor: &str) -> Script {
    Script::new()
        .answering("hyprctl cursorpos -j", 0, cursor, "")
        .answering("hyprctl monitors -j", 0, MONITORS, "")
}

fn qs_calls(world: &World) -> Vec<String> {
    world
        .trace()
        .into_iter()
        .filter(|line| line.starts_with("qs "))
        .collect()
}

#[test]
fn cursor_edges_are_half_open_and_monitor_scale_is_applied() {
    let world = World::plain("panel-cursor-edge", session_script(CURSOR));
    action(
        &world.paths,
        world.runner(),
        "panel.toggle",
        Some(&json!({"panel": "notifications"})),
    )
    .expect("the shared edge belongs to DP-2");
    assert_eq!(
        qs_calls(&world),
        ["qs -c garage ipc call shell notificationsOn DP-2"]
    );

    let outside = World::plain("panel-cursor-outside", session_script(OUTSIDE));
    let error = action(
        &outside.paths,
        outside.runner(),
        "panel.toggle",
        Some(&json!({"panel": "notifications"})),
    )
    .expect_err("the right edge is outside both half-open rectangles");
    assert_eq!(error.to_string(), "No monitor under cursor");
    assert!(qs_calls(&outside).is_empty());
}

#[test]
fn anchored_panels_carry_the_no_anchor_value_and_the_rest_do_not() {
    let world = World::plain("panel-argv", session_script(CURSOR));
    for (panel, expected) in [
        (
            "notifications",
            "qs -c garage ipc call shell notificationsOn DP-2",
        ),
        (
            "control-center",
            "qs -c garage ipc call shell controlCenterOn DP-2",
        ),
        ("monitor", "qs -c garage ipc call shell monitorOn DP-2 -1"),
        ("media", "qs -c garage ipc call shell mediaOn DP-2 -1"),
        ("ai-usage", "qs -c garage ipc call shell aiUsageOn DP-2"),
    ] {
        let value = if panel == "monitor" {
            json!({"panel": panel, "widget": "cpu"})
        } else {
            json!({"panel": panel})
        };
        action(&world.paths, world.runner(), "panel.toggle", Some(&value))
            .unwrap_or_else(|error| panic!("{panel} must reach the shell: {error}"));
        let calls = qs_calls(&world);
        assert_eq!(
            calls.last().map(String::as_str),
            Some(expected),
            "{panel}"
        );
    }
}

#[test]
fn a_qs_refusal_is_visible_but_does_not_fail_the_action() {
    let command = "qs -c garage ipc call shell notificationsOn DP-2";
    let world = World::plain("panel-qs-fails", session_script(CURSOR).failing(command));
    action(
        &world.paths,
        world.runner(),
        "panel.toggle",
        Some(&json!({"panel": "notifications"})),
    )
    .expect("the old script ended the call with || true");
    assert_eq!(qs_calls(&world), [command]);
}

#[test]
fn invalid_panel_and_widget_requests_are_refused_before_session_queries() {
    for (value, message) in [
        (json!({"panel": "weather"}), "Unknown panel: weather"),
        (
            json!({"panel": "monitor", "widget": 7}),
            "panel.toggle requires widget to be a string",
        ),
        (
            json!({"panel": "media", "widget": "cpu"}),
            "Panel media does not accept a widget",
        ),
    ] {
        let world = World::plain("panel-refusal", Script::new());
        let error = action(&world.paths, world.runner(), "panel.toggle", Some(&value))
            .expect_err("the payload is invalid");
        assert_eq!(error.to_string(), message);
        assert!(world.trace().is_empty());
    }
}

#[test]
fn monitor_accepts_extension_owned_widget_ids() {
    let world = World::plain("panel-extension-widget", session_script(CURSOR));
    action(
        &world.paths,
        world.runner(),
        "panel.toggle",
        Some(&json!({"panel": "monitor", "widget": "third-party-sensor"})),
    )
    .expect("the extension registry, not the backend, owns widget ids");
    assert_eq!(
        qs_calls(&world),
        ["qs -c garage ipc call shell monitorOn DP-2 -1"]
    );
}
