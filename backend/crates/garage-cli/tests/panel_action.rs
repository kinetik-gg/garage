use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static SERIAL: AtomicU64 = AtomicU64::new(0);

#[allow(clippy::disallowed_methods)]
// This is an integration test of the binary's stdout envelope and exit status, which cannot
// be observed through the Runner boundary used for the action's own subprocesses.
fn refusal(payload: &str, message: &str) -> Result<(), Box<dyn std::error::Error>> {
    let serial = SERIAL.fetch_add(1, Ordering::Relaxed);
    let home = std::env::temp_dir().join(format!(
        "garage-cli-panel-refusal-{}-{serial}",
        std::process::id()
    ));
    std::fs::create_dir_all(&home)?;
    let output = Command::new(env!("CARGO_BIN_EXE_garage"))
        .args(["action", "panel.toggle", payload])
        .env("HOME", &home)
        .output()?;
    let expected = format!("{{\"ok\":false,\"data\":null,\"error\":\"{message}\"}}\n");
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(String::from_utf8_lossy(&output.stdout), expected);
    assert!(output.stderr.is_empty());
    std::fs::remove_dir_all(home)?;
    Ok(())
}

#[test]
fn invalid_panels_and_widgets_use_the_standard_action_refusal_envelope(
) -> Result<(), Box<dyn std::error::Error>> {
    refusal(r#"{"panel":"weather"}"#, "Unknown panel: weather")?;
    refusal(
        r#"{"panel":"monitor","widget":7}"#,
        "panel.toggle requires widget to be a string",
    )?;
    Ok(())
}
