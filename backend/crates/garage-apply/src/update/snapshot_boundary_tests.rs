//! Scratch-home tests for the pre-update copy boundary.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use garage_core::paths::Paths;
use garage_core::traits::{Output, RunError, Runner};

use super::{save_at, SnapshotFacts};
use crate::update::transcript::Report;

static NEXT: AtomicU64 = AtomicU64::new(1);
const STAMP: &str = "20260814-123456";

struct BoundaryRunner {
    packages_allowed: bool,
}

impl Runner for BoundaryRunner {
    fn run(&self, command: &[&str], _timeout: Duration) -> Result<Output, RunError> {
        assert!(self.packages_allowed, "a dry run never collects packages");
        assert_eq!(command, ["pacman", "-Qqe"]);
        Ok(Output {
            status: 0,
            stdout: "base\n".to_owned(),
            stderr: String::new(),
        })
    }

    fn spawn_detached(&self, _command: &[&str]) -> Result<(), RunError> {
        unreachable!("a backup never detaches a process")
    }

    fn run_streamed(&self, _command: &[&str], _cwd: Option<&Path>) -> Result<i32, RunError> {
        unreachable!("a backup never streams a process")
    }
}

struct Scratch {
    root: PathBuf,
    paths: Paths,
}

impl Scratch {
    fn new(name: &str) -> Self {
        let serial = NEXT.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "garage-update-boundary-{name}-{}-{serial}",
            std::process::id()
        ));
        drop(std::fs::remove_dir_all(&root));
        let home = root.join("home");
        std::fs::create_dir_all(&home).expect("scratch home");
        let env: HashMap<String, String> =
            [("HOME".to_owned(), home.to_string_lossy().into_owned())]
                .into_iter()
                .collect();
        Self {
            paths: Paths::from_env_map(&env),
            root,
        }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        drop(std::fs::remove_dir_all(&self.root));
    }
}

#[test]
fn an_unwritable_backup_root_refuses_convergence() {
    let scratch = Scratch::new("unwritable");
    std::fs::write(scratch.paths.home.join(".garage-backup"), "not a directory")
        .expect("blocked backup root");
    let mut report = Report::new(false, true);

    let proceed = save_at(
        &scratch.paths,
        &BoundaryRunner {
            packages_allowed: true,
        },
        &mut report,
        facts(false),
    )
    .expect("report remains writable");
    assert!(!proceed);
    assert!(report
        .captured()
        .unwrap_or_default()
        .contains("REFUSE: could not complete the pre-update backup"));
}

#[test]
fn dry_run_prints_one_skip_line_and_leaves_no_backup_root() {
    let scratch = Scratch::new("dry-run");
    let mut report = Report::new(true, true);

    let proceed = save_at(
        &scratch.paths,
        &BoundaryRunner {
            packages_allowed: false,
        },
        &mut report,
        facts(false),
    )
    .expect("dry-run report");
    assert!(proceed);
    assert_eq!(
        report.captured(),
        Some("    Pre-update backup skipped entirely in a dry run.\n")
    );
    assert!(!scratch.paths.home.join(".garage-backup").exists());
}

#[test]
fn snapper_is_one_advisory_line_and_is_never_driven() {
    let scratch = Scratch::new("snapper");
    let mut report = Report::new(false, true);

    let proceed = save_at(
        &scratch.paths,
        &BoundaryRunner {
            packages_allowed: true,
        },
        &mut report,
        facts(true),
    )
    .expect("snapper advisory");
    assert!(proceed);
    let lines: Vec<&str> = report
        .captured()
        .unwrap_or_default()
        .lines()
        .filter(|line| line.contains("snapper"))
        .collect();
    assert_eq!(
        lines,
        ["    snapper is available; run it yourself if wanted: sudo snapper create -d \"before garage update\""]
    );
}

fn facts(snapper_available: bool) -> SnapshotFacts<'static> {
    SnapshotFacts {
        before_commit: "before",
        stamp: STAMP,
        snapper_available,
    }
}
