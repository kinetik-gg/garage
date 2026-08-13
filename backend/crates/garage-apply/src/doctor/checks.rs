//! The ten probes `doctor_checks()` walks: Hyprland's version, packages, fonts, stow links,
//! dead links, systemd units, plugins, generated fragments, preferences, and the compositor.
//!
//! Each is a small, independent read: [`check_hyprland`] compares the running version against
//! [`MINIMUM_HYPRLAND`](super::MINIMUM_HYPRLAND), the support floor below which the tracked
//! `hyprland.lua` fails in ways that read as Garage bugs rather than as an old compositor,
//! compared numerically rather than as a string so `0.56.10` correctly outranks `0.56.9`.
//! [`check_packages`] runs one `pacman -Q` for the whole named set rather than one per
//! package. [`check_fonts`] asks `fc-list` by family name, not by package name, because a
//! font can be installed and still not be found by fontconfig. [`check_units`] treats "not
//! enabled" as a failure outright and "enabled but not active" as a failure only when a
//! graphical session is actually up, since nothing is expected to be running from a bare TTY.
//! [`check_compositor`] and [`check_fragments`]' `luac` step are both informational, not
//! failing, when the thing they would check is simply absent (no compositor answering, no
//! `luac` installed) -- a health check that fails outside the situation it is meant to catch
//! is a health check nobody trusts.
//!
//! [`check_preferences`] is the one probe whose hint names a Garage command rather than a
//! `pacman` or `systemctl` one: `garage repair`, because an unparseable `preferences.toml` is
//! the one problem this product can fix itself.
//!
//! Every probe returns a [`Verdict`], not `Result<(), ApplyError>` over a
//! [`SessionCx`](crate::cx::SessionCx).

use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use std::time::Duration;

use garage_core::manifest::UnitKind;
use garage_core::traits::DEFAULT_RUN_TIMEOUT;
use garage_prefs::load_preferences;

use super::plugins::{hyprland_report, hyprland_version, plugin_state};
use super::stow::{dangling_repo_links, stow_state};
use super::{version_parts, DoctorCx, Verdict, MINIMUM_HYPRLAND, PLUGIN_NAMES};

/// One command, run the way the Python's `run()` runs it: a failure to spawn is an exit
/// status of 1 with nothing on stdout, never a raise.
fn stdout_of(cx: &DoctorCx<'_>, command: &[&str], timeout: Duration) -> (i32, String) {
    cx.proc
        .run(command, timeout)
        .map_or_else(|_| (1, String::new()), |probe| (probe.status, probe.stdout))
}

/// The first ten problems, then a count of the rest: the Python's `shown` idiom, which three
/// probes share.
fn shown(problems: &[String]) -> String {
    let mut text = problems
        .iter()
        .take(10)
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(", ");
    if problems.len() > 10 {
        let _ = write!(text, ", and {} more", problems.len() - 10);
    }
    text
}

/// A manifest that cannot be read stops the check that needed it, and says which file.
///
/// The Python cannot reach this state -- its lists are literals in the source -- so there is
/// no wording to match. `fail` rather than `note` because a doctor that quietly stops checking
/// packages is worse than one that says it could not.
fn manifest_failure(error: &garage_core::manifest::ManifestError) -> Verdict {
    Verdict::fail(
        format!("the manifest could not be read: {error}"),
        "check system/manifest/ in this checkout".to_owned(),
    )
}

pub(super) fn check_hyprland(cx: &DoctorCx<'_>) -> Verdict {
    let version = hyprland_version(&hyprland_report(cx));
    if version.is_empty() {
        return Verdict::fail(
            "Hyprland is not installed, or does not report a version".to_owned(),
            "sudo pacman -S --needed hyprland".to_owned(),
        );
    }
    if version_parts(&version) < version_parts(MINIMUM_HYPRLAND) {
        return Verdict::fail(
            format!("{version} is below the {MINIMUM_HYPRLAND} support floor"),
            "sudo pacman -Syu hyprland".to_owned(),
        );
    }
    Verdict::ok(format!("{version} (floor {MINIMUM_HYPRLAND})"))
}

/// What pacman has installed for each name, or `None` where it has nothing.
///
/// One `pacman -Q` for the whole set, and every name is present in the result -- a missing
/// package is `None` rather than an absent key, so a caller cannot read "not installed" as
/// "not asked about". pacman writes its own line to stderr for each name it does not know and
/// still lists the ones it does, so only stdout is parsed and the exit status is ignored.
pub(super) fn package_versions(
    cx: &DoctorCx<'_>,
    names: &[String],
) -> Vec<(String, Option<String>)> {
    let mut command: Vec<&str> = vec!["pacman", "-Q"];
    command.extend(names.iter().map(String::as_str));
    let (_, stdout) = stdout_of(cx, &command, Duration::from_secs(15));
    let mut found: Vec<(&str, &str)> = Vec::new();
    for line in stdout.lines() {
        let mut fields = line.split_whitespace();
        if let (Some(name), Some(version)) = (fields.next(), fields.next()) {
            found.push((name, version));
        }
    }
    names
        .iter()
        .map(|name| {
            let version = found
                .iter()
                .find(|(candidate, _)| *candidate == name)
                .map(|(_, version)| (*version).to_owned());
            (name.clone(), version)
        })
        .collect()
}

/// The names `packages.list` flags `critical`: the subset this check asserts by name.
pub(super) fn critical_packages(
    cx: &DoctorCx<'_>,
) -> Result<Vec<String>, garage_core::manifest::ManifestError> {
    Ok(cx
        .packages()?
        .into_iter()
        .filter(|entry| entry.critical)
        .map(|entry| entry.name)
        .collect())
}

pub(super) fn check_packages(cx: &DoctorCx<'_>) -> Verdict {
    let names = match critical_packages(cx) {
        Ok(names) => names,
        Err(error) => return manifest_failure(&error),
    };
    let missing: Vec<String> = package_versions(cx, &names)
        .into_iter()
        .filter(|(_, version)| version.is_none())
        .map(|(name, _)| name)
        .collect();
    if missing.is_empty() {
        return Verdict::ok(format!("all {} key packages installed", names.len()));
    }
    Verdict::fail(
        format!("not installed: {}", missing.join(", ")),
        format!("sudo pacman -S --needed {}", missing.join(" ")),
    )
}

pub(super) fn check_fonts(cx: &DoctorCx<'_>) -> Verdict {
    let wanted = match cx.fonts() {
        Ok(wanted) => wanted,
        Err(error) => return manifest_failure(&error),
    };
    if !cx.installed.has("fc-list") {
        return Verdict::note("fontconfig is not installed, so nothing can be checked".to_owned());
    }
    let (_, stdout) = stdout_of(cx, &["fc-list", ":", "family"], Duration::from_secs(15));
    let families: Vec<&str> = stdout
        .lines()
        .flat_map(|line| line.split(','))
        .map(str::trim)
        .collect();
    let missing: Vec<String> = wanted
        .iter()
        .filter(|family| !families.contains(&family.as_str()))
        .cloned()
        .collect();
    if missing.is_empty() {
        return Verdict::ok(format!("{} present", wanted.join(", ")));
    }
    Verdict::fail(
        format!("fontconfig cannot find: {}", missing.join(", ")),
        "stow -R desktop && fc-cache -f".to_owned(),
    )
}

pub(super) fn check_stow(cx: &DoctorCx<'_>) -> Verdict {
    let state = stow_state(cx);
    let mut problems: Vec<String> = Vec::new();
    problems.extend(state.broken.iter().map(|name| format!("broken ~/{name}")));
    problems.extend(
        state
            .plain
            .iter()
            .map(|name| format!("not a link ~/{name}")),
    );
    problems.extend(state.missing.iter().map(|name| format!("missing ~/{name}")));
    if !problems.is_empty() {
        return Verdict::fail(
            format!(
                "{}/{} paths link here; {} problem(s): {}",
                state.linked,
                state.total,
                problems.len(),
                shown(&problems)
            ),
            "garage update  (or ./bootstrap.sh, which backs up anything in the way)".to_owned(),
        );
    }
    if !state.other.is_empty() {
        // Not a failure: the desktop is fully linked and working, just from another clone of
        // the same tree. Worth saying out loud, because editing this checkout would then
        // change nothing on screen.
        return Verdict::note_with_hint(
            format!(
                "{}/{} paths link into another checkout ({}) rather than this one",
                state.other.len(),
                state.total,
                state.others.iter().cloned().collect::<Vec<_>>().join(", ")
            ),
            "garage update  (restows from this checkout)".to_owned(),
        );
    }
    Verdict::ok(format!(
        "{} managed paths resolve into this checkout",
        state.total
    ))
}

pub(super) fn check_dangling(cx: &DoctorCx<'_>) -> Verdict {
    let stale = dangling_repo_links(cx);
    if stale.is_empty() {
        return Verdict::ok("no links point at files this checkout no longer has".to_owned());
    }
    let named: Vec<String> = stale.iter().map(|path| cx.tilde(path)).collect();
    Verdict::fail(
        format!(
            "{} link(s) point at deleted files: {}",
            stale.len(),
            shown(&named)
        ),
        "garage update  (sweeps them; a restow cannot)".to_owned(),
    )
}

/// Whether a graphical session is running for this user.
///
/// `graphical-session.target` rather than a `hyprctl` probe: `hyprctl` needs
/// `HYPRLAND_INSTANCE_SIGNATURE`, which is exported into the session's own processes and is
/// absent from an SSH login or a TTY shell on the same machine. Asking systemd is the
/// question that does not depend on where the command was typed.
pub(super) fn session_is_up(cx: &DoctorCx<'_>) -> bool {
    let (_, stdout) = stdout_of(
        cx,
        &[
            "systemctl",
            "--user",
            "is-active",
            "graphical-session.target",
        ],
        DEFAULT_RUN_TIMEOUT,
    );
    stdout.trim() == "active"
}

pub(super) fn check_units(cx: &DoctorCx<'_>) -> Verdict {
    let units = match cx.units() {
        Ok(units) => units,
        Err(error) => return manifest_failure(&error),
    };
    let session = session_is_up(cx);
    let (mut disabled, mut stopped): (Vec<String>, Vec<String>) = (Vec::new(), Vec::new());
    for unit in &units {
        let (_, enabled) = stdout_of(
            cx,
            &["systemctl", "--user", "is-enabled", &unit.name],
            DEFAULT_RUN_TIMEOUT,
        );
        if !matches!(enabled.trim(), "enabled" | "enabled-runtime") {
            disabled.push(unit.name.clone());
            continue;
        }
        if session && unit.kind == UnitKind::Running {
            let (_, active) = stdout_of(
                cx,
                &["systemctl", "--user", "is-active", &unit.name],
                DEFAULT_RUN_TIMEOUT,
            );
            if active.trim() != "active" {
                stopped.push(unit.name.clone());
            }
        }
    }
    if !disabled.is_empty() {
        return Verdict::fail(
            format!("not enabled: {}", disabled.join(", ")),
            format!("systemctl --user enable {}", disabled.join(" ")),
        );
    }
    if let Some(first) = stopped.first() {
        return Verdict::fail(
            format!("enabled but not running: {}", stopped.join(", ")),
            format!("systemctl --user status {first}"),
        );
    }
    if !session {
        return Verdict::note(format!(
            "all {} units enabled; no graphical session here, so nothing is expected to be running",
            units.len()
        ));
    }
    Verdict::ok(format!("all {} units enabled and running", units.len()))
}

pub(super) fn check_plugins(cx: &DoctorCx<'_>) -> Verdict {
    let state = plugin_state(cx, &hyprland_report(cx));
    if state.abi.is_empty() {
        return Verdict::note(
            "Hyprland reports no ABI string, so nothing can be compared".to_owned(),
        );
    }
    let build = build_label(&state.abi);
    if !state.ever {
        return Verdict::note(format!(
            "no plugins have ever been deployed here (running build {build}); \
             the desktop runs without them"
        ));
    }
    if !state.stale.is_empty() {
        return Verdict::fail(
            format!(
                "{} not built for the running build {build}",
                state.stale.join(", ")
            ),
            "~/.config/hypr/scripts/garage-rebuild-plugins".to_owned(),
        );
    }
    if !state.behind.is_empty() {
        return Verdict::fail(
            format!(
                "{} deployed at a different commit than system/plugin-pins names",
                state.behind.join(", ")
            ),
            "~/.config/hypr/scripts/garage-rebuild-plugins".to_owned(),
        );
    }
    Verdict::ok(format!(
        "{} built for the running build {build}",
        PLUGIN_NAMES.join(", ")
    ))
}

/// `state["abi"].split("_")[0][:12]`: the readable half of the ABI string.
pub(crate) fn build_label(abi: &str) -> String {
    abi.split('_')
        .next()
        .unwrap_or("")
        .chars()
        .take(12)
        .collect()
}

pub(super) fn check_fragments(cx: &DoctorCx<'_>) -> Verdict {
    let generated = &cx.paths.generated;
    if !generated.is_dir() {
        return Verdict::note(format!(
            "{} does not exist yet; nothing has rendered on this machine",
            cx.tilde(generated)
        ));
    }
    let mut fragments: Vec<std::path::PathBuf> = fs::read_dir(generated)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.extension().is_some_and(|suffix| suffix == "lua"))
                .collect()
        })
        .unwrap_or_default();
    fragments.sort();
    if fragments.is_empty() {
        return Verdict::note("no Lua fragments have been rendered yet".to_owned());
    }
    if !cx.installed.has("luac") {
        return Verdict::note(format!(
            "{} fragment(s) present; luac is not installed to check them",
            fragments.len()
        ));
    }
    let broken: Vec<String> = fragments
        .iter()
        .filter(|path| {
            stdout_of(
                cx,
                &["luac", "-p", &path.to_string_lossy()],
                DEFAULT_RUN_TIMEOUT,
            )
            .0 != 0
        })
        .filter_map(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .collect();
    if !broken.is_empty() {
        return Verdict::fail(
            format!("will not parse: {}", broken.join(", ")),
            "garage render  (rewrites every fragment from your preferences)".to_owned(),
        );
    }
    Verdict::ok(format!("{} Lua fragment(s) parse", fragments.len()))
}

pub(super) fn check_preferences(cx: &DoctorCx<'_>) -> Verdict {
    let mut notes: Vec<String> = Vec::new();
    if let Err(error) = load_preferences(cx.paths, Some(&mut notes)) {
        // The one hint that names a Garage command rather than a pacman or systemctl one,
        // because this is the one problem the product can fix itself: see `crate::repair`,
        // which keeps the unreadable file rather than asking the user to move it aside and
        // hope.
        return Verdict::fail(
            format!("cannot be loaded: {error}"),
            format!(
                "garage repair  (reports first; --reset keeps {} as a backup and writes a fresh one)",
                cx.tilde(&cx.paths.host.preferences)
            ),
        );
    }
    if notes.is_empty() {
        return Verdict::ok("loads with every value in range".to_owned());
    }
    let mut text = notes
        .iter()
        .take(3)
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join("; ");
    if notes.len() > 3 {
        let _ = write!(text, "; and {} more", notes.len() - 3);
    }
    // Not a failure by design: the coercion pass puts the shipped default over anything it
    // cannot render and says so, and the corrected value reaches the file on the next save. A
    // note may also be a key this build has no preference for, dropped from the file -- hence
    // "note(s)" rather than "value(s) coerced". Nothing is broken now either way.
    Verdict::note(format!("loads, with {} note(s): {text}", notes.len()))
}

pub(super) fn check_compositor(cx: &DoctorCx<'_>) -> Verdict {
    if stdout_of(cx, &["hyprctl", "version"], DEFAULT_RUN_TIMEOUT).0 == 0 {
        return Verdict::ok("hyprctl reaches the running compositor".to_owned());
    }
    // Informational on purpose. `doctor` is most useful from a TTY after a login that did not
    // come up, and there is no compositor to reach from there -- reporting that as a fault
    // would make the report useless in exactly the situation it exists for.
    Verdict::note(
        "no compositor answers here (a TTY or SSH shell has no HYPRLAND_INSTANCE_SIGNATURE)"
            .to_owned(),
    )
}

/// `checkout_commit()`: the commit the checkout is on, or `""` when git cannot answer.
///
/// Tolerant on purpose, and every reason to be: the tree may be a tarball with no `.git`, git
/// may not be installed, or the repository may have no commit yet. None of that is a fault
/// worth failing a bug report over -- an empty string reads as "unknown" and the rest of the
/// report is still worth having.
pub(super) fn checkout_commit(cx: &DoctorCx<'_>, root: &Path) -> String {
    let (status, head) = crate::update::git_output(cx.proc, root, &["rev-parse", "HEAD"], 30);
    if status == 0 {
        head
    } else {
        String::new()
    }
}
