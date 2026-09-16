use std::io;
use std::path::Path;
use std::process::Command;

#[test]
#[expect(
    clippy::disallowed_methods,
    reason = "the integration contract executes the real Lua configuration"
)]
fn plugin_settings_wait_for_a_completed_load() -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .ok_or_else(|| io::Error::other("garage-core must live at backend/crates/garage-core"))?;
    let output = Command::new("lua")
        .arg(root.join("tests/plugin-loading.lua"))
        .arg(root.join("desktop/.config/hypr/hyprland.lua"))
        .output()?;
    assert!(
        output.status.success(),
        "Lua plugin contracts failed: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}
