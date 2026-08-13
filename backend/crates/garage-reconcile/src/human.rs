//! Doctor-style aligned rendering for the human mode.

use std::fmt::Write as _;
use std::path::Path;

use crate::types::{Action, PlanItem, Report};

/// Render one complete human transcript.
#[must_use]
pub fn render_human(report: &Report) -> String {
    let mut out = String::new();
    header(&mut out, report);
    let _ = writeln!(out, "\nPlan");
    if report.plan.is_empty() && report.refused.is_empty() {
        let _ = writeln!(out, "  plan      no changes");
    } else {
        for item in &report.plan {
            action_line(&mut out, report, item);
        }
        for refusal in &report.refused {
            let _ = writeln!(
                out,
                "  refuse    ~/{}  {} ({})",
                refusal.path, refusal.reason, refusal.guard
            );
        }
    }
    result_line(&mut out, report);
    out
}

fn header(out: &mut String, report: &Report) {
    let suffix = if report.dry_run {
        " (dry run: nothing will be changed)"
    } else {
        ""
    };
    let actual = &report.actual;
    let _ = writeln!(out, "Garage reconcile{suffix}");
    let _ = writeln!(out, "  checkout  {}", report.checkout);
    let _ = writeln!(out, "  home      {}", report.home);
    let _ = writeln!(out, "  desired   {} managed path(s)", report.desired.len());
    let _ = writeln!(
        out,
        "  units     {} declared (reported only; bootstrap owns enable/restart)",
        report.units.len()
    );
    let _ = writeln!(
        out,
        "  actual    linked {}, other {}, broken {}, plain {}, missing {}",
        actual.linked, actual.other, actual.broken, actual.plain, actual.missing
    );
}

fn action_line(out: &mut String, report: &Report, item: &PlanItem) {
    let label = match item.action {
        Action::Link => "link",
        Action::Relink => "relink",
        Action::BackupAndLink => "backup",
        Action::Prune => "prune",
    };
    if let Some(backup) = &item.backup {
        let shown = tilde(&report.home, backup);
        let _ = writeln!(
            out,
            "  {label:<9} ~/{} -> {shown}, then link  ({})",
            item.path, item.reason
        );
    } else {
        let _ = writeln!(out, "  {label:<9} ~/{}  ({})", item.path, item.reason);
    }
}

fn result_line(out: &mut String, report: &Report) {
    if report.dry_run {
        let _ = writeln!(
            out,
            "\n  result    {} change(s) planned; nothing changed",
            report.plan.len()
        );
    } else {
        let _ = writeln!(out, "\n  result    {} change(s) applied", report.applied);
    }
}

fn tilde(home: &str, path: &str) -> String {
    Path::new(path).strip_prefix(home).map_or_else(
        |_| path.to_owned(),
        |relative| format!("~/{}", relative.display()),
    )
}
