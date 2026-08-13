//! `garage update`: pull, sweep dead links, then delegate to `bootstrap.sh`.
//!
//! # The delegate-to-bootstrap banner
//!
//! Design: pull, sweep, then delegate. Not a native reimplementation, and the choice is not
//! close. What an update has to do on rolling Arch is make this machine match the checkout
//! again: install packages the list has gained, enable units it has gained, write per-user
//! files that are new, put every link back, and leave the user's own files alone.
//! `bootstrap.sh` already does all of that, idempotently and by design -- its freshness gate
//! short-circuits on an install it can already see, packages go in with `--needed`,
//! `systemctl enable` is bookkeeping, generated files are written only when absent, and every
//! mutating call goes through one `run()` chokepoint a `--dry-run` flag turns off. It is also
//! the only path exercised by the clean-install rehearsal.
//!
//! A native implementation would have to duplicate the part that is genuinely hard: the
//! pre-stow scan that classifies every managed path, moves real files into a timestamped
//! backup, and deletes links from a moved checkout. Without it, `stow --restow` aborts on the
//! first tracked file that has become a real file and leaves a half-linked home. With it, two
//! implementations of the same careful logic drift apart, and the one this crate would carry
//! is the one no rehearsal covers.
//!
//! What delegation costs, honestly: `bootstrap.sh` prints for a TTY -- steps, warnings, a
//! summary -- which is acceptable here precisely because `garage update` is a TTY command
//! too, and a re-run includes `sudo pacman -Syu`, so `update` asks for sudo and can upgrade
//! the whole system, which is stated up front rather than designed around, since installing
//! newly listed packages without a full upgrade first is an unsupported partial upgrade on
//! Arch.
//!
//! Two things `update` keeps for itself, because `bootstrap.sh` cannot answer them: the
//! dead-link sweep ([`crate::doctor`]'s `dangling_repo_links()`), which needs to know what the
//! checkout no longer ships -- the one thing a scan of the *current* manifest cannot see --
//! and the plugin decision, since `bootstrap.sh` deploys unconditionally (right for an
//! install, wasteful for an update that rebuilds an unmoved ABI), so `update` compares the
//! running ABI against what is deployed itself and passes
//! `GARAGE_SKIP_PLUGIN_DEPLOY=1` so `bootstrap.sh` does not do it twice.
//!
//! Takes `argv` and returns an exit code, prints lines rather than the JSON response
//! envelope, and streams `bootstrap.sh`'s own output to the terminal rather than capturing it
//! -- the same reason [`Runner::run_streamed`] exists rather than [`Runner::run`].
//!
//! [`pull`] is the git half, kept apart so it can be read on its own: the fast-forward, the
//! refusals it treats as notes rather than failures, and the `git_output()` wrapper
//! `doctor`'s `checkout_commit()` shares with it.
//!
//! # What is owed
//!
//! Step 4's push half. The Python loads the preferences (which runs any schema migration a
//! pull brought in), renders, and then calls `push_accent()`, `push_corner_radius()` and
//! `push_theme()` -- "the toolkit configs it just rewrote are half a theme until the portal
//! setting, xsettingsd and the two signalled clients agree with them". The render is ported
//! and runs; those three pushes have no entry point in this crate yet, so a real (non
//! `--dry-run`) update stops there with [`ApplyError::PortPending`] naming them, the same
//! shape an exception out of `push_theme()` would give the Python. `--dry-run` never reaches
//! it.

use std::path::PathBuf;

use garage_core::paths::Paths;
use garage_core::shlex::shlex_quote;
use garage_core::traits::{Runner, DEFAULT_RUN_TIMEOUT};
use garage_prefs::load_preferences;
use garage_proc::{Hyprctl, Luac, System};
use garage_render::all::render_all;
use garage_render::cx::RenderCx;

use crate::doctor::{dangling_repo_links, hyprland_report, plugin_state, stow_state, DoctorCx};
use crate::error::ApplyError;
use crate::keybind::load_keybindings;

mod pull;
#[cfg(test)]
mod traces;

pub(crate) use pull::git_output;
use pull::pull_checkout;

/// The printer: `step()`, `info()` and `note()`, which differ only in their prefix.
///
/// Prints as it goes, which is the Python's behaviour and is load-bearing here rather than
/// incidental: `bootstrap.sh` and `garage-rebuild-plugins` are handed this process's own
/// terminal part-way through, so a report assembled and printed at the end would arrive after
/// the output it was introducing. A fixture passes [`Report::captured`] instead, because the
/// transcript is the thing being compared and nothing is really streaming in a test.
struct Report {
    /// Whether `[dry-run] ` is stamped on the lines that describe a mutation.
    dry_run: bool,
    /// `Some` collects instead of printing. `None` in every real run.
    captured: Option<String>,
}

impl Report {
    /// One line, printed or collected.
    fn say(&mut self, line: &str) {
        match self.captured.as_mut() {
            Some(sink) => sink.push_str(line),
            None => print!("{line}"),
        }
    }

    /// A step banner: a blank line, then `==> `.
    fn step(&mut self, text: &str) {
        self.say(&format!("\n==> {text}\n"));
    }

    /// A line about something that would be changed. Stamped in a dry run.
    fn info(&mut self, text: &str) {
        let prefix = if self.dry_run { "[dry-run] " } else { "" };
        self.say(&format!("    {prefix}{text}\n"));
    }

    /// A line that is true either way.
    fn note(&mut self, text: &str) {
        self.say(&format!("    {text}\n"));
    }
}

/// `garage update [--dry-run]`.
///
/// # Errors
///
/// [`ApplyError::Settings`] for an argument this command does not take, for a binary that is
/// not inside a Garage checkout, and for a checkout with no `bootstrap.sh`; whatever the
/// render raises; and [`ApplyError::PortPending`] for step 4's push half -- see the module
/// doc.
pub fn update(paths: &Paths, proc: &dyn Runner, argv: &[String]) -> Result<i32, ApplyError> {
    update_at(paths, proc, argv, None, None)
}

/// [`update`] with the checkout named and the transcript optionally collected, which is what
/// the trace fixtures drive: [`checkout_root`](crate::doctor::checkout_root) reads this
/// process's own executable, and a test binary is not inside the scratch checkout it built.
///
/// # Errors
///
/// The same as [`update`].
pub(crate) fn update_at(
    paths: &Paths,
    proc: &dyn Runner,
    argv: &[String],
    root: Option<PathBuf>,
    captured: Option<&mut String>,
) -> Result<i32, ApplyError> {
    let mut dry_run = false;
    for argument in argv {
        if argument == "--dry-run" {
            dry_run = true;
        } else {
            return Err(ApplyError::Settings(format!(
                "Usage: garage update [--dry-run]  (unexpected: {argument})"
            )));
        }
    }
    let cx = match root {
        Some(root) => DoctorCx::at(paths, proc, root),
        None => DoctorCx::new(paths, proc)?,
    };
    let collect = captured.is_some();
    let mut report = Report {
        dry_run,
        captured: collect.then(String::new),
    };
    let outcome = run_steps(&cx, &mut report, dry_run);
    if let (Some(sink), Some(text)) = (captured, report.captured) {
        sink.push_str(&text);
    }
    outcome
}

/// The six steps, and the summary that reads their problems back.
fn run_steps(cx: &DoctorCx<'_>, report: &mut Report, dry_run: bool) -> Result<i32, ApplyError> {
    let paths = cx.paths;
    let mut problems: Vec<String> = Vec::new();
    report.say(&format!(
        "Garage update{}\n",
        if dry_run {
            " (dry run: nothing will be changed)"
        } else {
            ""
        }
    ));
    report.note(&format!("checkout  {}", cx.root.display()));
    report.note(&format!("home      {}", paths.home.display()));

    let (before, after) = checkout_step(cx, report, &mut problems);
    sweep_step(cx, report, &mut problems);
    if bootstrap_step(cx, report)? != 0 {
        return Ok(1);
    }
    render_step(paths, report)?;
    plugin_step(cx, report, &mut problems, &before, &after);
    reload_step(cx, report, &mut problems);

    report.say("\n");
    if problems.is_empty() {
        report.say(if dry_run {
            "Dry run complete. Nothing above was changed.\n"
        } else {
            "Update complete.\n"
        });
        return Ok(0);
    }
    report.say("Update finished with problems:\n");
    for problem in &problems {
        report.say(&format!("  - {problem}\n"));
    }
    Ok(1)
}

/// Step 1: the checkout itself. Answers `(before, after)` for the plugin step's pin diff.
fn checkout_step(
    cx: &DoctorCx<'_>,
    report: &mut Report,
    problems: &mut Vec<String>,
) -> (String, String) {
    report.step("Updating the checkout");
    let before = git_output(cx.proc, &cx.root, &["rev-parse", "HEAD"], 30).1;
    let (lines, pull_problems) = pull_checkout(cx.proc, &cx.root, report.dry_run);
    for line in &lines {
        report.note(line);
    }
    problems.extend(pull_problems);
    let after = git_output(cx.proc, &cx.root, &["rev-parse", "HEAD"], 30).1;
    (before, after)
}

/// Step 2: links to files the update deleted.
fn sweep_step(cx: &DoctorCx<'_>, report: &mut Report, problems: &mut Vec<String>) {
    report.step("Sweeping links to files this checkout no longer ships");
    let stale = dangling_repo_links(cx);
    if stale.is_empty() {
        report.note("none found.");
    }
    for path in &stale {
        let target = std::fs::read_link(path).unwrap_or_default();
        report.info(&format!(
            "remove {} -> {}",
            cx.tilde(path),
            target.display()
        ));
        if !report.dry_run {
            if let Err(error) = std::fs::remove_file(path) {
                report.note(&format!("could not remove {}: {error}", cx.tilde(path)));
                problems.push(format!("could not remove {}", cx.tilde(path)));
            }
        }
    }
    if !stale.is_empty() && !report.dry_run {
        report.note(&format!("removed {} dangling link(s).", stale.len()));
    }
}

/// Step 3: converge this machine on the checkout, by handing the terminal to `bootstrap.sh`.
///
/// The Python copies `os.environ`, adds its two variables and passes the copy to the child.
/// [`Runner::run_streamed`] hands the child this process's own environment, so the two
/// variables are set here and taken back off afterwards -- the child sees exactly what the
/// Python's child sees, and so does the plugin rebuild that runs later in the same process.
fn bootstrap_step(cx: &DoctorCx<'_>, report: &mut Report) -> Result<i32, ApplyError> {
    report.step("Converging this machine on the checkout");
    let script = cx.root.join("bootstrap.sh");
    if !script.is_file() {
        return Err(ApplyError::Settings(format!(
            "{} is missing; this checkout cannot converge itself",
            script.display()
        )));
    }
    // update owns the plugin decision; see the plugin step below.
    std::env::set_var("GARAGE_SKIP_PLUGIN_DEPLOY", "1");
    let forced = !stow_state(cx).other.is_empty();
    if forced {
        // Garage is demonstrably installed here, just from another clone, so the freshness
        // gate would refuse for the wrong reason: it looks for a link into *this* checkout.
        // Forcing is safe because the evidence of an existing install is in hand; it is not
        // forced when nothing is linked, which is a first install and the gate's actual job.
        std::env::set_var("GARAGE_FORCE", "1");
        report.note("this home is linked to another checkout; re-pointing it here.");
    }
    let script = script.to_string_lossy().into_owned();
    let mut command: Vec<&str> = vec![&script];
    if report.dry_run {
        command.push("--dry-run");
    }
    report.note(&format!(
        "running {}",
        command
            .iter()
            .map(|part| shlex_quote(part))
            .collect::<Vec<_>>()
            .join(" ")
    ));
    if !report.dry_run {
        report.note("it will ask for sudo: a re-run upgrades the system and installs any");
        report.note("package the list has gained since this machine was set up.");
    }
    // Run in a dry run too, and deliberately: `bootstrap.sh` has its own `--dry-run`, and the
    // whole reason to delegate is that its answer to "what would change" is the authoritative
    // one. Streamed rather than captured because the output is a progress report and because
    // pacman's sudo prompt needs the tty.
    let status = cx
        .proc
        .run_streamed(&command, Some(&cx.root))
        .map_err(|error| ApplyError::Settings(error.detail))?;
    std::env::remove_var("GARAGE_SKIP_PLUGIN_DEPLOY");
    if forced {
        std::env::remove_var("GARAGE_FORCE");
    }
    if status != 0 {
        report.say(&format!(
            "\ngarage update: bootstrap.sh exited {status}; stopping here rather than \
             reloading a half-converged desktop.\n"
        ));
    }
    Ok(status)
}

/// Step 4: migrations and the generated state.
///
/// The load *is* the migration: it is version-gated and runs inside `load_preferences()`, so a
/// pull that bumped the schema is applied here. The corrected file itself is written on the
/// next save, which is the same contract every other command works under.
fn render_step(paths: &Paths, report: &mut Report) -> Result<(), ApplyError> {
    report.step("Rendering the generated state");
    if report.dry_run {
        report.info("would load the preferences (running any schema migration) and render");
        return Ok(());
    }
    let config =
        load_preferences(paths, None).map_err(|error| ApplyError::Settings(error.to_string()))?;
    let system = System;
    let monitors = Hyprctl::new(&system);
    let lua = Luac::new(&system);
    let cx = RenderCx::new(&config, paths, &monitors, &lua);
    render_all(&cx, &load_keybindings(paths, None))?;
    // `render_all()` only writes files now. push_accent(), push_corner_radius() and
    // push_theme() are what it used to push from inside itself, and an update still owes them:
    // the toolkit configs it just rewrote are half a theme until the portal setting,
    // xsettingsd and the two signalled clients agree with them. The compositor's own reload is
    // step 6, which knows how to skip a TTY.
    Err(ApplyError::PortPending(
        "update's push_accent/push_corner_radius/push_theme",
    ))
}

/// Step 5: the plugin ABI, and whether a redeploy is owed.
fn plugin_step(
    cx: &DoctorCx<'_>,
    report: &mut Report,
    problems: &mut Vec<String>,
    before: &str,
    after: &str,
) {
    report.step("Checking the Hyprland plugin ABI");
    let state = plugin_state(cx, &hyprland_report(cx));
    let pins_changed = !before.is_empty()
        && !after.is_empty()
        && before != after
        && !git_output(
            cx.proc,
            &cx.root,
            &[
                "diff",
                "--name-only",
                &format!("{before}..{after}"),
                "--",
                "system/plugin-pins",
            ],
            30,
        )
        .1
        .is_empty();
    let build = crate::doctor::build_label(&state.abi);
    if state.abi.is_empty() {
        report.note("Hyprland reports no ABI string; skipping.");
    } else if !state.ever {
        report.note("no plugins have ever been deployed here; skipping.");
    } else if state.stale.is_empty() && state.behind.is_empty() && !pins_changed {
        report.note(&format!(
            "{} are in step with the running build {build}; skipping the rebuild.",
            crate::doctor::PLUGIN_NAMES.join(", ")
        ));
    } else {
        redeploy(cx, report, problems, &state, pins_changed);
    }
}

/// The half of [`plugin_step`] that runs once a rebuild is owed.
fn redeploy(
    cx: &DoctorCx<'_>,
    report: &mut Report,
    problems: &mut Vec<String>,
    state: &crate::doctor::PluginState,
    pins_changed: bool,
) {
    let reason = if pins_changed {
        "a pin moved in this update".to_owned()
    } else if state.stale.is_empty() {
        format!(
            "deployed at a different commit than the pins file: {}",
            state.behind.join(", ")
        )
    } else {
        format!(
            "not built for the running build: {}",
            state.stale.join(", ")
        )
    };
    report.note(&format!("redeploy needed ({reason})."));
    let mut rebuild: PathBuf = cx
        .paths
        .home
        .join(".config/hypr/scripts/garage-rebuild-plugins");
    if !rebuild.is_file() {
        rebuild = cx
            .root
            .join("desktop/.config/hypr/scripts/garage-rebuild-plugins");
    }
    report.info(&format!("run {}", rebuild.display()));
    if report.dry_run {
        return;
    }
    // Streamed and interactive for the same reasons as bootstrap: it installs into /usr/lib
    // and asks for sudo once.
    let status = cx
        .proc
        .run_streamed(&[&rebuild.to_string_lossy()], None)
        .unwrap_or(1);
    if status != 0 {
        report.note("the plugin rebuild failed; the desktop runs without them.");
        problems.push("plugin rebuild failed".to_owned());
    }
}

/// Step 6: the reload, which knows how to skip a TTY.
fn reload_step(cx: &DoctorCx<'_>, report: &mut Report, problems: &mut Vec<String>) {
    report.step("Reloading Hyprland");
    if report.dry_run {
        report.info("would run hyprctl reload");
        return;
    }
    let reachable = cx
        .proc
        .run(&["hyprctl", "version"], DEFAULT_RUN_TIMEOUT)
        .is_ok_and(|probe| probe.status == 0);
    if !reachable {
        report.note("no compositor answers here; skipping the reload.");
        report.note("the new configuration is picked up at your next login.");
        return;
    }
    let reloaded = cx
        .proc
        .run(&["hyprctl", "reload"], DEFAULT_RUN_TIMEOUT)
        .is_ok_and(|probe| probe.status == 0);
    if reloaded {
        report.note("reloaded.");
    } else {
        report.note("hyprctl reload failed; the new configuration lands at your next login.");
        problems.push("hyprctl reload failed".to_owned());
    }
}
