use std::fs;
use std::path::Path;

use serde_json::{json, Value};

use super::{metric_width, MediaCss, MonitorCss};
use crate::actions::action;
use crate::testing::{Script, World};

const CURSOR: &str = include_str!("testdata/cursor-edge.json");
const OUTSIDE: &str = include_str!("testdata/cursor-outside.json");
const MONITORS: &str = include_str!("testdata/monitors.json");
const WORKSPACES: &str = include_str!("testdata/workspaces.json");
const STYLE: &str = include_str!("testdata/style.css");
const WIDGETS: &str = include_str!("testdata/waybar-widgets.jsonc");
const LEFT: &str = include_str!("testdata/waybar-workspaces.jsonc");
const QS_ARGV: &str = include_str!("testdata/qs-argv.txt");

fn session_script(cursor: &str) -> Script {
    Script::new()
        .answering("hyprctl cursorpos -j", 0, cursor, "")
        .answering("hyprctl monitors -j", 0, MONITORS, "")
        .answering("hyprctl workspaces -j", 0, WORKSPACES, "")
}

fn write(path: &Path, body: &str) {
    fs::create_dir_all(path.parent().expect("a fixture path has a parent"))
        .expect("the fixture directory is writable");
    fs::write(path, body).expect("the fixture file is writable");
}

fn install_files(world: &World) {
    write(&world.paths.config_home.join("waybar/style.css"), STYLE);
    write(&world.paths.fragments.waybar_widgets, WIDGETS);
    write(&world.paths.fragments.waybar_workspaces, LEFT);
}

fn payload(panel: &str) -> Value {
    if panel == "monitor" {
        json!({"panel": panel, "widget": "cpu"})
    } else {
        json!({"panel": panel})
    }
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
fn css_scrapes_each_value_and_each_miss_uses_its_shipped_default() {
    let world = World::plain("panel-css", Script::new());
    let path = world.paths.config_home.join("waybar/style.css");
    write(&path, STYLE);
    assert_eq!(
        MonitorCss::read(&path),
        MonitorCss {
            image_padding: 9,
            tail_width: 333
        }
    );
    assert_eq!(
        MediaCss::read(&path),
        MediaCss {
            menu_margin_left: 24,
            menu_min_width: 23,
            menu_padding_right: 7,
            menu_margin_right: 17,
            workspace_padding: 8,
            media_margin_left: 19,
        }
    );

    let damaged = STYLE.replace(" {", "-scrape-miss {");
    write(&path, &damaged);
    assert_eq!(MonitorCss::read(&path), MonitorCss::DEFAULT);
    assert_eq!(MediaCss::read(&path), MediaCss::DEFAULT);
}

#[test]
fn realistic_widget_json_sums_only_metric_images_and_falls_back_as_a_unit() {
    assert_eq!(metric_width(WIDGETS, 18), Some(693));
    let damaged = WIDGETS.replace("\"size\": 91", "\"size\": \"wide\"");
    assert_eq!(metric_width(&damaged, 18), None);
    assert_eq!(metric_width("not JSON", 18), None);
}

#[test]
fn every_panel_matches_the_shell_oracles_exact_qs_argv() {
    let world = World::plain("panel-parity", session_script(CURSOR));
    install_files(&world);
    for panel in [
        "notifications",
        "control-center",
        "monitor",
        "media",
        "ai-usage",
    ] {
        let value = payload(panel);
        action(&world.paths, world.runner(), "panel.toggle", Some(&value))
            .expect("the fixture describes a valid click");
    }
    assert_eq!(qs_calls(&world), QS_ARGV.lines().collect::<Vec<_>>());
}

#[test]
fn broken_generated_files_keep_both_anchor_paths_clickable() {
    let world = World::plain("panel-fallbacks", session_script(CURSOR));
    let damaged = STYLE.replace(" {", "-scrape-miss {");
    write(&world.paths.config_home.join("waybar/style.css"), &damaged);
    write(&world.paths.fragments.waybar_widgets, "not JSON");
    write(&world.paths.fragments.waybar_workspaces, "not JSON");
    for panel in ["monitor", "media"] {
        let value = payload(panel);
        action(&world.paths, world.runner(), "panel.toggle", Some(&value))
            .expect("fallback geometry still reaches qs");
    }
    assert_eq!(
        qs_calls(&world),
        [
            "qs -c garage ipc call shell monitorOn DP-2 1600",
            "qs -c garage ipc call shell mediaOn DP-2 329",
        ]
    );
}

#[test]
fn generated_membership_can_remove_workspaces_without_querying_them() {
    let world = World::plain("panel-no-workspaces", session_script(CURSOR));
    install_files(&world);
    write(
        &world.paths.fragments.waybar_workspaces,
        "{\"modules-left\":[\"custom/menu\",\"custom/media\"]}",
    );
    action(
        &world.paths,
        world.runner(),
        "panel.toggle",
        Some(&json!({"panel": "media"})),
    )
    .expect("media opens without the workspace module");
    assert!(!world
        .trace()
        .iter()
        .any(|line| line == "hyprctl workspaces -j"));
    assert_eq!(
        qs_calls(&world),
        ["qs -c garage ipc call shell mediaOn DP-2 270"]
    );
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
            json!({"panel": "monitor", "widget": "battery"}),
            "Unknown monitor widget: battery",
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
