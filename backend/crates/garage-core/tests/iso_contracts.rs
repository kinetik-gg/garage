use std::io;
use std::path::Path;
use std::process::Command;

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[expect(
    clippy::disallowed_methods,
    reason = "installer contracts must exercise the shell entry points before the Rust backend exists"
)]
fn run_contract(script: &str) -> TestResult {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .ok_or_else(|| io::Error::other("garage-core must live at backend/crates/garage-core"))?;
    let output = Command::new("bash")
        .arg(repository.join("tests").join(script))
        .current_dir(repository)
        .output()?;
    assert!(
        output.status.success(),
        "{script} failed ({}):\n{}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

#[test]
fn iso_build_uses_only_the_selected_commit() -> TestResult {
    run_contract("iso-build-contracts.sh")
}

#[test]
fn first_boot_retry_preserves_offline_state_and_finishes_cleanup() -> TestResult {
    run_contract("iso-first-boot-contracts.sh")
}
