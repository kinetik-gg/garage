//! `garage update`: pull, check upgrade space, preserve the host preferences, sweep dead
//! links, then delegate to `bootstrap.sh`.
//!
//! # The delegate-to-bootstrap banner
//!
//! Design: pull, check upgrade space, preserve layer 2, sweep, then delegate. Not a native
//! reimplementation, and the choice is not close. What an update has to do on rolling Arch is
//! make this machine match the checkout again: install packages the list has gained, enable
//! units it has gained, write per-user files that are new, put every link back, and leave the
//! user's own files alone.
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
//! The lifecycle guards, dead-link sweep and plugin decision stay here because bootstrap
//! cannot answer them. The sweep needs to see what the checkout no longer ships; bootstrap
//! deploys plugins unconditionally, while update compares the running ABI first and passes
//! `GARAGE_SKIP_PLUGIN_DEPLOY=1` so bootstrap does not do that work twice.
//!
//! Takes `argv` and returns an exit code, prints lines rather than the JSON response
//! envelope, and streams `bootstrap.sh`'s own output to the terminal rather than capturing it
//! -- the same reason [`Runner::run_streamed`] exists rather than [`Runner::run`].
//!
//! Step 6 is the one step that both renders and pushes: the Python loads the preferences
//! (which runs any schema migration a pull brought in), renders everything, and then calls
//! `push_accent()`, `push_corner_radius()` and `push_theme()` -- "the toolkit configs it just
//! rewrote are half a theme until the portal setting, xsettingsd and the two signalled
//! clients agree with them". Not `apply_preferences()`, which would additionally seed
//! `displays.toml`, dress the wallpaper and restart `hypridle`: the compositor's own reload is
//! step 8, which knows how to skip a TTY, and an update is a convergence rather than a session
//! start. `--dry-run` reaches none of it.

use std::ffi::OsString;
use std::path::PathBuf;

use garage_core::paths::Paths;
use garage_core::traits::{Runner, DEFAULT_RUN_TIMEOUT};
use garage_prefs::load_preferences;
use garage_proc::{Hyprctl, Luac};
use garage_render::all::render_all;
use garage_render::cx::RenderCx;

use crate::corner::push_corner_radius;
use crate::cx::SessionCx;
use crate::doctor::{
    dangling_repo_links, hyprland_report, plugin_state, stow_state, tilde, DoctorCx,
};
use crate::error::ApplyError;
use crate::keybind::load_keybindings;
use crate::route::push_accent as apply_accent_push;
use crate::terminal::resolve_browser;
use crate::theme::push_theme;

mod lock;
mod pull;
mod snapshot;
mod space;
#[cfg(test)]
mod traces;
mod transcript;

use lock::UpdateLock;
pub use lock::UpdateLockError;
pub(crate) use pull::git_output;
use pull::pull_checkout;
use space::{SpaceProbe, SystemSpace};
use transcript::Report;

/// `garage update [--dry-run]`.
///
/// # Errors
///
/// [`ApplyError::UpdateLock`] if another update is running or the lock cannot be prepared;
/// [`ApplyError::Io`] if free space cannot be inspected or a real run's private transcript
/// cannot be created or flushed;
/// [`ApplyError::Settings`] for an argument this command does not take, for a binary that is
/// not inside a Garage checkout, and for a checkout with no `bootstrap.sh`; whatever the
/// render raises; and whatever step 6's push half raises.
pub fn update(paths: &Paths, proc: &dyn Runner, argv: &[String]) -> Result<i32, ApplyError> {
    let run = UpdateRun {
        root: None,
        captured: None,
        space: &SystemSpace,
    };
    update_at(paths, proc, argv, run)
}

/// The path/report/probe overrides used by the trace fixtures around one update.
struct UpdateRun<'a> {
    root: Option<PathBuf>,
    captured: Option<&'a mut String>,
    space: &'a dyn SpaceProbe,
}

fn update_at(
    paths: &Paths,
    proc: &dyn Runner,
    argv: &[String],
    run: UpdateRun<'_>,
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
    let cx = match run.root {
        Some(root) => DoctorCx::at(paths, proc, root),
        None => DoctorCx::new(paths, proc)?,
    };
    let mut report = Report::new(dry_run, run.captured.is_some());
    let outcome = run_steps(&cx, &mut report, dry_run, run.space);
    if let (Some(sink), Some(text)) = (run.captured, report.captured()) {
        sink.push_str(text);
    }
    outcome
}

/// The eight steps, and the summary that reads their problems back.
fn run_steps(
    cx: &DoctorCx<'_>,
    report: &mut Report,
    dry_run: bool,
    space_probe: &dyn SpaceProbe,
) -> Result<i32, ApplyError> {
    let _lock = UpdateLock::acquire(&cx.paths.locks.update)?;
    let paths = cx.paths;
    let transcript = transcript::open_for_run(paths, dry_run)?;
    let checkout = prepare_checkout(cx, dry_run);
    if let Some(file) = transcript {
        report.attach(file);
    }
    transcript::header(report, paths, &cx.root, &checkout.before, &checkout.after)?;
    let mut problems = checkout.problems;
    report_checkout(report, &checkout.lines)?;

    if !space::check(paths, &cx.root, report, space_probe)? {
        return Ok(1);
    }
    if !snapshot::save(paths, cx.proc, report, &checkout.before)? {
        return Ok(1);
    }
    sweep_step(cx, report, &mut problems)?;
    if bootstrap_step(cx, report)? != 0 {
        return Ok(1);
    }
    render_step(paths, cx.proc, report)?;
    plugin_step(cx, report, &mut problems, &checkout.before, &checkout.after)?;
    reload_step(cx, report, &mut problems)?;
    finish(report, dry_run, &problems)
}

fn finish(report: &mut Report, dry_run: bool, problems: &[String]) -> Result<i32, ApplyError> {
    report.say("\n")?;
    if problems.is_empty() {
        report.say(if dry_run {
            "Dry run complete. Nothing above was changed.\n"
        } else {
            "Update complete.\n"
        })?;
        return Ok(0);
    }
    report.say("Update finished with problems:\n")?;
    for problem in problems {
        report.say(&format!("  - {problem}\n"))?;
    }
    Ok(1)
}

/// Step 1 is prepared before the header is written, so its first lines can carry both commits.
struct CheckoutStep {
    before: String,
    after: String,
    lines: Vec<String>,
    problems: Vec<String>,
}

fn prepare_checkout(cx: &DoctorCx<'_>, dry_run: bool) -> CheckoutStep {
    let before = git_output(cx.proc, &cx.root, &["rev-parse", "HEAD"], 30).1;
    let (lines, problems) = pull_checkout(cx.proc, &cx.root, dry_run);
    let after = git_output(cx.proc, &cx.root, &["rev-parse", "HEAD"], 30).1;
    CheckoutStep {
        before,
        after,
        lines,
        problems,
    }
}

fn report_checkout(report: &mut Report, lines: &[String]) -> Result<(), ApplyError> {
    report.step("Updating the checkout")?;
    for line in lines {
        report.note(line)?;
    }
    Ok(())
}

/// Step 4: links to files the update deleted.
fn sweep_step(
    cx: &DoctorCx<'_>,
    report: &mut Report,
    problems: &mut Vec<String>,
) -> Result<(), ApplyError> {
    report.step("Sweeping links to files this checkout no longer ships")?;
    let stale = dangling_repo_links(cx);
    if stale.is_empty() {
        report.note("none found.")?;
    }
    for path in &stale {
        let target = std::fs::read_link(path).unwrap_or_default();
        report.info(&format!(
            "remove {} -> {}",
            cx.tilde(path),
            target.display()
        ))?;
        if !report.dry_run {
            if let Err(error) = std::fs::remove_file(path) {
                report.note(&format!("could not remove {}: {error}", cx.tilde(path)))?;
                problems.push(format!("could not remove {}", cx.tilde(path)));
            }
        }
    }
    if !stale.is_empty() && !report.dry_run {
        report.note(&format!("removed {} dangling link(s).", stale.len()))?;
    }
    Ok(())
}

/// Step 5: converge this machine on the checkout, by handing the terminal to `bootstrap.sh`.
///
/// The Python copies `os.environ`, adds its two variables and passes the copy to the child.
/// [`Runner::run_streamed`] hands the child this process's own environment, so the two
/// variables are set here and taken back off afterwards -- the child sees exactly what the
/// Python's child sees, and so does the plugin rebuild that runs later in the same process.
///
/// Bootstrap's stdout and stderr are deliberately not tee'd into the transcript.
/// `run_streamed` has to hand it the inherited terminal so sudo works; teeing that stream
/// while keeping the prompt interactive needs a pty. That pty design is rejected here. The
/// parent records the complete invocation and where the omitted child output went instead.
fn bootstrap_step(cx: &DoctorCx<'_>, report: &mut Report) -> Result<i32, ApplyError> {
    report.step("Converging this machine on the checkout")?;
    let script = cx.root.join("bootstrap.sh");
    if !script.is_file() {
        return Err(ApplyError::Settings(format!(
            "{} is missing; this checkout cannot converge itself",
            script.display()
        )));
    }
    let forced = !stow_state(cx).other.is_empty();
    if forced {
        // Garage is demonstrably installed here, just from another clone, so the freshness
        // gate would refuse for the wrong reason: it looks for a link into *this* checkout.
        // Forcing is safe because the evidence of an existing install is in hand; it is not
        // forced when nothing is linked, which is a first install and the gate's actual job.
        report.note("this home is linked to another checkout; re-pointing it here.")?;
    }
    let script = script.to_string_lossy().into_owned();
    let mut command: Vec<&str> = vec![&script];
    if report.dry_run {
        command.push("--dry-run");
    }
    // update owns the plugin decision; see the plugin step below. Guards restore a value the
    // caller already had, including on a runner or transcript error.
    let skip_plugin_deploy = Environment::set("GARAGE_SKIP_PLUGIN_DEPLOY", "1");
    let force = forced.then(|| Environment::set("GARAGE_FORCE", "1"));
    transcript::bootstrap_invocation(report, &cx.root, &command)?;
    // Run in a dry run too, and deliberately: `bootstrap.sh` has its own `--dry-run`, and the
    // whole reason to delegate is that its answer to "what would change" is the authoritative
    // one. Streamed rather than captured because the output is a progress report and because
    // pacman's sudo prompt needs the tty.
    let outcome = cx.proc.run_streamed(&command, Some(&cx.root));
    drop(force);
    drop(skip_plugin_deploy);
    let status = match outcome {
        Ok(status) => status,
        Err(error) => {
            report.note(&format!("bootstrap exit    unavailable ({})", error.detail))?;
            return Err(ApplyError::Settings(error.detail));
        }
    };
    report.note(&format!("bootstrap exit    {status}"))?;
    if status != 0 {
        report.say(&format!(
            "\ngarage update: bootstrap.sh exited {status}; stopping here rather than \
             reloading a half-converged desktop.\n"
        ))?;
    }
    Ok(status)
}

/// One temporary process-environment override, restored on every return path.
struct Environment {
    name: &'static str,
    previous: Option<OsString>,
}

impl Environment {
    fn set(name: &'static str, value: &str) -> Self {
        let previous = std::env::var_os(name);
        std::env::set_var(name, value);
        Self { name, previous }
    }
}

impl Drop for Environment {
    fn drop(&mut self) {
        match self.previous.take() {
            Some(value) => std::env::set_var(self.name, value),
            None => std::env::remove_var(self.name),
        }
    }
}

/// Step 6: migrations and the generated state.
///
/// The load *is* the migration: it is version-gated and runs inside `load_preferences()`, so a
/// pull that bumped the schema is applied here. The corrected file itself is written on the
/// next save, which is the same contract every other command works under.
fn render_step(paths: &Paths, proc: &dyn Runner, report: &mut Report) -> Result<(), ApplyError> {
    report.step("Rendering the generated state")?;
    if report.dry_run {
        report.info("would load the preferences (running any schema migration) and render")?;
        return Ok(());
    }
    let config =
        load_preferences(paths, None).map_err(|error| ApplyError::Settings(error.to_string()))?;
    let monitors = Hyprctl::new(proc);
    let lua = Luac::new(proc);
    let mut cx = SessionCx::new(RenderCx::new(&config, paths, &monitors, &lua), proc);
    {
        // Scoped, so the resolver's borrow of the context ends before the three pushes below
        // take it mutably. `render_all()` writes all three general markers here, exactly as
        // the Python's does: an update holds a runner.
        let session: &SessionCx<'_> = &cx;
        let resolve = resolve_browser(session);
        render_all(
            session.render(),
            &load_keybindings(paths, None),
            Some(&resolve),
        )?;
    }
    // `render_all()` only writes files now. These three are what it used to push from inside
    // itself, and an update still owes them: the toolkit configs it just rewrote are half a
    // theme until the portal setting, xsettingsd and the two signalled clients agree with
    // them. The compositor's own reload is step 8, which knows how to skip a TTY.
    apply_accent_push(&mut cx);
    push_corner_radius(&mut cx);
    push_theme(&mut cx)?;
    report.note(&format!(
        "rewrote the fragments under {}.",
        tilde(&paths.home, &paths.generated)
    ))?;
    Ok(())
}

/// Step 7: the plugin ABI, and whether a redeploy is owed.
fn plugin_step(
    cx: &DoctorCx<'_>,
    report: &mut Report,
    problems: &mut Vec<String>,
    before: &str,
    after: &str,
) -> Result<(), ApplyError> {
    report.step("Checking the Hyprland plugin ABI")?;
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
        report.note("Hyprland reports no ABI string; skipping.")?;
    } else if !state.ever {
        report.note("no plugins have ever been deployed here; skipping.")?;
    } else if state.stale.is_empty() && state.behind.is_empty() && !pins_changed {
        report.note(&format!(
            "{} are in step with the running build {build}; skipping the rebuild.",
            crate::doctor::PLUGIN_NAMES.join(", ")
        ))?;
    } else {
        redeploy(cx, report, problems, &state, pins_changed)?;
    }
    Ok(())
}

/// The half of [`plugin_step`] that runs once a rebuild is owed.
fn redeploy(
    cx: &DoctorCx<'_>,
    report: &mut Report,
    problems: &mut Vec<String>,
    state: &crate::doctor::PluginState,
    pins_changed: bool,
) -> Result<(), ApplyError> {
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
    report.note(&format!("redeploy needed ({reason})."))?;
    let mut rebuild: PathBuf = cx
        .paths
        .home
        .join(".config/hypr/scripts/garage-rebuild-plugins");
    if !rebuild.is_file() {
        rebuild = cx
            .root
            .join("desktop/.config/hypr/scripts/garage-rebuild-plugins");
    }
    report.info(&format!("run {}", rebuild.display()))?;
    if report.dry_run {
        return Ok(());
    }
    // Streamed and interactive for the same reasons as bootstrap: it installs into /usr/lib
    // and asks for sudo once.
    let status = cx
        .proc
        .run_streamed(&[&rebuild.to_string_lossy()], None)
        .unwrap_or(1);
    if status != 0 {
        report.note("the plugin rebuild failed; the desktop runs without them.")?;
        problems.push("plugin rebuild failed".to_owned());
    }
    Ok(())
}

/// Step 8: the reload, which knows how to skip a TTY.
fn reload_step(
    cx: &DoctorCx<'_>,
    report: &mut Report,
    problems: &mut Vec<String>,
) -> Result<(), ApplyError> {
    report.step("Reloading Hyprland")?;
    if report.dry_run {
        report.info("would run hyprctl reload")?;
        return Ok(());
    }
    let reachable = cx
        .proc
        .run(&["hyprctl", "version"], DEFAULT_RUN_TIMEOUT)
        .is_ok_and(|probe| probe.status == 0);
    if !reachable {
        report.note("no compositor answers here; skipping the reload.")?;
        report.note("the new configuration is picked up at your next login.")?;
        return Ok(());
    }
    let reloaded = cx
        .proc
        .run(&["hyprctl", "reload"], DEFAULT_RUN_TIMEOUT)
        .is_ok_and(|probe| probe.status == 0);
    if reloaded {
        report.note("reloaded.")?;
    } else {
        report.note("hyprctl reload failed; the new configuration lands at your next login.")?;
        problems.push("hyprctl reload failed".to_owned());
    }
    Ok(())
}
