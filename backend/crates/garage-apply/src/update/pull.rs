//! `pull_checkout()`, and the `git_output()` wrapper both lifecycle commands share.
//!
//! Never raises, and treats every reason it cannot pull as a note rather than a failure: an
//! update that refused to relink because a remote is missing or unreachable would be useless
//! on the machines Garage actually runs on -- every install today is local-only, so the
//! no-upstream branch is the one that actually runs.
//!
//! Separated from [`super`]'s remaining steps so it can be exercised against a scratch
//! repository with a real upstream, which is the Python's own reason for splitting it out of
//! `update()`.

use std::path::Path;
use std::time::Duration;

use garage_core::traits::Runner;

/// `git -C <root> <arguments>`, with the Python's `(returncode, stdout.strip())` shape.
///
/// The same two-line wrapper both lifecycle commands want -- `doctor`'s `checkout_commit()` is
/// the other caller -- so the timeout has one place to be wrong rather than two.
pub(crate) fn git_output(
    proc: &dyn Runner,
    root: &Path,
    arguments: &[&str],
    timeout: u64,
) -> (i32, String) {
    let root = root.to_string_lossy().into_owned();
    let mut command: Vec<&str> = vec!["git", "-C", &root];
    command.extend_from_slice(arguments);
    proc.run(&command, Duration::from_secs(timeout))
        .map_or((1, String::new()), |probe| {
            (probe.status, probe.stdout.trim().to_owned())
        })
}

/// Fast-forward the checkout onto its upstream, and say what happened.
///
/// Returns (report lines, problems). Never raises, and treats every reason it cannot pull as
/// a note rather than a failure: an update that refused to relink because a remote is missing
/// or unreachable would be useless on the machines Garage actually runs on.
pub(super) fn pull_checkout(
    proc: &dyn Runner,
    root: &Path,
    dry_run: bool,
) -> (Vec<String>, Vec<String>) {
    let mut lines: Vec<String> = Vec::new();
    let problems: Vec<String> = Vec::new();
    if !root.join(".git").exists() {
        return (
            vec!["not a git checkout, so there is nothing to pull.".to_owned()],
            problems,
        );
    }
    let (status, upstream) = git_output(
        proc,
        root,
        &[
            "rev-parse",
            "--abbrev-ref",
            "--symbolic-full-name",
            "@{upstream}",
        ],
        30,
    );
    if status != 0 || upstream.is_empty() {
        // The normal case today: Garage's repositories are local-only, so there is no origin
        // to pull from.
        return (
            vec![
                "no upstream configured, skipping pull; continuing with the local tree.".to_owned(),
            ],
            problems,
        );
    }
    lines.push(format!("upstream  {upstream}"));
    // Allowed in a dry run: fetch writes only to .git's own object store and remote refs,
    // never to the working tree or to $HOME, and without it there is no honest answer to
    // "what would this pull in".
    if git_output(proc, root, &["fetch", "--quiet"], 120).0 != 0 {
        let remote = upstream.split('/').next().unwrap_or("");
        lines.push(format!(
            "could not fetch from {remote}; continuing with the local tree."
        ));
        return (lines, problems);
    }
    let incoming = git_output(
        proc,
        root,
        &["log", "--oneline", &format!("HEAD..{upstream}")],
        30,
    )
    .1;
    if incoming.is_empty() {
        lines.push(format!("already up to date with {upstream}."));
        return (lines, problems);
    }
    let (more, problems) = merge_incoming(proc, root, dry_run, &upstream, &incoming);
    lines.extend(more);
    (lines, problems)
}

/// How long a `git merge --ff-only` is given. A local fast-forward, so the only thing it can
/// be waiting on is the disk.
const MERGE_TIMEOUT: u64 = 60;

/// The half of [`pull_checkout`] that runs once there is something to merge. Answers its own
/// lines and problems, which the caller appends to what it already has.
fn merge_incoming(
    proc: &dyn Runner,
    root: &Path,
    dry_run: bool,
    upstream: &str,
    incoming: &str,
) -> (Vec<String>, Vec<String>) {
    let mut lines: Vec<String> = Vec::new();
    let mut problems: Vec<String> = Vec::new();
    let commits: Vec<&str> = incoming.lines().collect();
    if dry_run {
        lines.push(format!(
            "[dry-run] would fast-forward {} commit(s):",
            commits.len()
        ));
        lines.extend(commits.iter().take(20).map(|line| format!("      {line}")));
        if commits.len() > 20 {
            lines.push(format!("      and {} more", commits.len() - 20));
        }
        return (lines, problems);
    }
    if !git_output(proc, root, &["status", "--porcelain"], 30)
        .1
        .is_empty()
    {
        // The working tree belongs to whoever is editing it. A merge across local changes is
        // exactly the operation that loses work, so it is refused rather than forced.
        lines.push(format!(
            "{} commit(s) waiting, but the checkout has local changes;",
            commits.len()
        ));
        lines.push("skipping the merge. Commit or stash them and re-run.".to_owned());
        problems.push("pull skipped: the checkout has local changes".to_owned());
        return (lines, problems);
    }
    if let Some(detail) = fast_forward(proc, root, upstream) {
        lines.push(format!("could not fast-forward onto {upstream}:"));
        lines.push(format!("      {detail}"));
        problems.push(format!("pull failed: cannot fast-forward onto {upstream}"));
        return (lines, problems);
    }
    let head = git_output(proc, root, &["rev-parse", "--short", "HEAD"], 30).1;
    lines.push(format!(
        "fast-forwarded {} commit(s) to {head}.",
        commits.len()
    ));
    (lines, problems)
}

/// `git merge --ff-only <upstream>`: `None` when it worked, and git's own complaint when it
/// did not -- stderr if it had any, stdout otherwise, which is the Python's `or`.
fn fast_forward(proc: &dyn Runner, root: &Path, upstream: &str) -> Option<String> {
    let merged = proc.run(
        &[
            "git",
            "-C",
            &root.to_string_lossy(),
            "merge",
            "--ff-only",
            upstream,
        ],
        Duration::from_secs(MERGE_TIMEOUT),
    );
    // A command that could not be run at all is a failed merge carrying the reason, which is
    // exactly what the Python's `run()` synthesises: `CompletedProcess(command, 1, "",
    // str(error))`, read back through the same `stderr or stdout`.
    let merged = match merged {
        Ok(output) => output,
        Err(error) => return Some(error.detail),
    };
    if merged.status == 0 {
        return None;
    }
    let stderr = merged.stderr.trim();
    Some(if stderr.is_empty() {
        merged.stdout.trim().to_owned()
    } else {
        stderr.to_owned()
    })
}
