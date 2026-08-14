//! Human CLI surface for one-shot machine migrations.

use std::fmt::Write as _;

use garage_apply::migrations::{run_migrations, Outcome, Report, Status, REGISTRY};
use garage_apply::ApplyError;
use garage_core::paths::Paths;

/// Run the shipped registry and print its report as lines for a person.
pub(crate) fn migrate(paths: &Paths, arguments: &[String]) -> Result<i32, ApplyError> {
    let dry_run = options(arguments)?;
    let report = run_migrations(paths, REGISTRY, dry_run);
    let (text, status) = transcript(&report);
    print!("{text}");
    Ok(status)
}

fn options(arguments: &[String]) -> Result<bool, ApplyError> {
    let mut dry_run = false;
    for argument in arguments {
        if argument == "--dry-run" {
            dry_run = true;
        } else {
            return Err(ApplyError::Settings(format!(
                "Usage: garage migrate [--dry-run]  (unexpected argument: {argument})"
            )));
        }
    }
    Ok(dry_run)
}

/// Render the runner's complete result in the same header/rows/summary voice as doctor.
fn transcript(report: &Report) -> (String, i32) {
    let mut out = if report.dry_run {
        String::from("Garage migrate (dry run: nothing will be changed)\n")
    } else {
        String::from("Garage migrate\n")
    };

    if let Some(error) = &report.stamp_error {
        let _ = writeln!(out, "\n  FAIL  migration stamp  {error}");
        let _ = writeln!(out, "\n1 problem(s) found.");
        return (out, 1);
    }
    if report.entries.is_empty() {
        let _ = writeln!(out, "\nNothing to apply.");
        return (out, 0);
    }

    let width = report
        .entries
        .iter()
        .map(|entry| entry.id.len())
        .max()
        .unwrap_or(0);
    let mut failures = 0;
    let mut eligible = 0;
    for entry in &report.entries {
        let (word, detail) = status_line(&entry.status);
        if matches!(entry.status, Status::Failed(_)) {
            failures += 1;
        }
        if !matches!(entry.status, Status::Skipped) {
            eligible += 1;
        }
        let _ = writeln!(
            out,
            "  {word:<4}  {:<width$}  {} -- {detail}",
            entry.id, entry.summary
        );
    }

    if failures > 0 {
        let _ = writeln!(out, "\n{failures} migration(s) failed.");
        return (out, 1);
    }
    if eligible == 0 {
        let _ = writeln!(out, "\nNothing to apply.");
    } else if report.dry_run {
        let _ = writeln!(out, "\nDry run complete. Nothing was changed.");
    } else {
        let _ = writeln!(out, "\n{eligible} migration(s) settled.");
    }
    (out, 0)
}

fn status_line(status: &Status) -> (&'static str, &str) {
    match status {
        Status::Skipped => ("skip", "already applied"),
        Status::DryRun(Outcome::Changed(detail)) | Status::Applied(Outcome::Changed(detail)) => {
            ("ok", detail)
        }
        Status::DryRun(Outcome::NothingToDo) | Status::Applied(Outcome::NothingToDo) => {
            ("ok", "nothing to do")
        }
        Status::Failed(error) => ("FAIL", error),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use garage_apply::migrations::{Outcome, Report, Status};
    use garage_core::paths::Paths;

    use super::{options, transcript};

    fn paths(label: &str) -> Paths {
        let home = std::env::temp_dir().join(format!(
            "garage-cli-migrate-{label}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let env: HashMap<String, String> =
            [("HOME".to_owned(), home.to_string_lossy().into_owned())]
                .into_iter()
                .collect();
        Paths::from_env_map(&env)
    }

    #[test]
    fn empty_registry_reports_nothing_and_writes_no_stamp_in_either_mode() {
        for dry_run in [false, true] {
            let paths = paths(if dry_run { "empty-dry" } else { "empty" });
            let report = garage_apply::migrations::run_migrations(&paths, &[], dry_run);
            let (text, status) = transcript(&report);
            assert_eq!(status, 0);
            assert!(text.contains("Nothing to apply."));
            assert_eq!(text.contains("dry run"), dry_run);
            assert!(!paths.migrations.exists());
            drop(std::fs::remove_dir_all(&paths.home));
        }
    }

    #[test]
    fn an_unknown_argument_mirrors_repairs_refusal() {
        let error = options(&["--dryrun".to_owned()]).expect_err("unknown flag");
        assert_eq!(
            error.to_string(),
            "Usage: garage migrate [--dry-run]  (unexpected argument: --dryrun)"
        );
    }

    #[test]
    fn failed_report_rows_set_failure_status_and_keep_later_rows_visible() {
        let report = Report {
            dry_run: false,
            stamp_error: None,
            entries: vec![
                garage_apply::migrations::Entry {
                    id: "900-fixture-failure",
                    summary: "exercise the failure row",
                    status: Status::Failed("fixture refusal".to_owned()),
                },
                garage_apply::migrations::Entry {
                    id: "901-fixture-later",
                    summary: "exercise the later row",
                    status: Status::Applied(Outcome::NothingToDo),
                },
            ],
        };
        let (text, status) = transcript(&report);
        assert_eq!(status, 1);
        assert!(text.contains("FAIL  900-fixture-failure"));
        assert!(text.contains("ok    901-fixture-later"));
        assert!(text.ends_with("1 migration(s) failed.\n"));
    }
}
