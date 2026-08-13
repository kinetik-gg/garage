//! `garage doctor`: a read-only health report for a person at a terminal, not the QML client.
//!
//! # Plumbing, deliberately unlike everything above it
//!
//! Every other command in this crate answers the QML client and prints one JSON object; this
//! one -- together with [`crate::repair`] and [`crate::update`] -- answers a person at a
//! terminal and prints lines, so nothing here reaches `make_snapshot()` and nothing here goes
//! through the `{"ok", "data", "error"}` response envelope. The CLI's command dispatch routes
//! these three ahead of the JSON path for exactly that reason. `doctor --report` is the one
//! partial exception: it prints JSON, because a bug report has to be pasted somewhere, but it
//! is still the person's command and still not the response envelope -- see [`report`].
//!
//! # A check is (label, probe)
//!
//! [`checks`] is the probe list: each probe returns a status ([`Status::Ok`] healthy,
//! [`Status::Note`] true and reported but not a problem -- a machine with no plugins
//! deployed, a session that is not running because this is a TTY -- or [`Status::Fail`], a
//! real problem whose hint says what to do about it), a detail string, and a hint. Exit
//! status ignores `note`, because a health check that fails on a TTY is a health check nobody
//! runs. The probes return their verdict rather than printing it, so [`report`]'s
//! `doctor --report` serializes the same list the printed report walks rather than a second
//! copy of it that could drift.
//!
//! [`stow`] is the stow-link and dangling-link half (`stow_state()`, `dangling_repo_links()`,
//! `managed_paths()`), and [`plugins`] is the Hyprland plugin ABI comparison, reimplemented
//! here rather than shelled out to `garage-rebuild-plugins --check` -- see its own doc for
//! why running that script's logic inline is the deliberate choice.
//!
//! Everything here takes `argv`/returns a [`Verdict`] or an exit code, not
//! `Result<(), ApplyError>` over a [`SessionCx`](crate::cx::SessionCx): a health check moves
//! nothing, so it needs no applier context -- only somewhere to read from ([`Paths`]) and
//! something to ask ([`Runner`]), which is what [`DoctorCx`] carries.
//!
//! # The manifests, read at runtime
//!
//! **Deliberate departure from the Python, and the one in this module.** The Python names its
//! own `DOCTOR_PACKAGES`, `DOCTOR_UNITS` and `DOCTOR_FONTS` tuples in the source; this reads
//! them out of `system/manifest/{packages,units,fonts}.list` on every run, through
//! [`garage_core::manifest`]. `bootstrap.sh` already reads those files, so the doctor reading
//! them too makes one copy of the data serve all three readers -- and a package added to the
//! list is checked by the next `garage doctor` with no rebuild of anything. The subset rules
//! are the file headers' own: `packages.list`'s `critical` flag "marks the subset
//! `garage doctor` checks by name", and `units.list` is checked whole, because that file
//! says "bootstrap.sh enables every line here; `garage doctor` checks them".
//!
//! `tests/test_manifest.py` pins the Python constants against the same files for the length
//! of the parity window, so the *sets* agree; what does not agree is order (the file groups
//! packages by function, the tuple does not) and breadth (`units.list` is the full set the
//! Python's `DOCTOR_UNITS` is a subset of, and `doctor --report` lists every package rather
//! than the critical eight). Those are written down in `tests/differential/deviations.toml`.

mod checks;
mod context;
#[cfg(test)]
mod parity;
mod plugins;
mod report;
mod stow;

pub(crate) use checks::build_label;
#[cfg(test)]
pub(crate) use context::Installed;
pub(crate) use context::{
    local_backup_stamp, local_iso8601, now_seconds, tilde, version_parts, DoctorCx,
};
pub(crate) use plugins::{hyprland_report, plugin_state, PluginState};
pub(crate) use stow::{dangling_repo_links, stow_state};

use garage_core::paths::Paths;
use garage_core::traits::Runner;

use crate::error::ApplyError;

/// The support floor. Hyprland's config language and option names move between releases, and
/// the tracked `hyprland.lua` is written against this one; below it the desktop fails in ways
/// that read as Garage bugs rather than as an old compositor. Compared numerically -- see
/// [`version_parts`] -- because the day `0.56.10` ships, a string comparison puts it below
/// `0.56.9`.
pub(crate) const MINIMUM_HYPRLAND: &str = "0.56.0";

/// The two stable symlinks `hyprland.lua` loads, under
/// [`Paths::plugin_root`](garage_core::paths::Paths::plugin_root). Duplicated from
/// `garage-rebuild-plugins` rather than parsed out of it: they are a published layout that
/// the pacman hook also hardcodes, not a private detail.
pub(crate) const PLUGIN_NAMES: [&str; 2] = ["kinetik-glass", "hyprexpo"];

/// How a check came out.
///
/// Three words, one mapping for both surfaces -- see [`Status::word`] -- so a pasted JSON
/// report and a pasted terminal transcript say the same word about the same check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Status {
    /// Healthy.
    Ok,
    /// True, reported, and not a problem: a machine with no plugins deployed, a home that has
    /// never rendered, a session that is not running because this is a TTY. The exit status
    /// ignores these.
    Note,
    /// A real problem. The hint says what to do about it.
    Fail,
}

impl Status {
    /// `DOCTOR_STATUS`: the word this status is printed and serialized as.
    pub(crate) const fn word(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Note => "note",
            Self::Fail => "FAIL",
        }
    }
}

/// What one probe answered: a status, the detail line, and the hint a failure carries.
#[derive(Debug, Clone)]
pub(crate) struct Verdict {
    /// The verdict itself.
    pub(crate) status: Status,
    /// The sentence printed after the check's name, and the report's `detail`.
    pub(crate) detail: String,
    /// What to do about it. Empty unless the status is [`Status::Fail`], and empty for some
    /// of those too -- a failure with nothing useful to suggest says nothing.
    pub(crate) hint: String,
}

impl Verdict {
    /// `("ok", detail, "")`.
    fn ok(detail: String) -> Self {
        Self {
            status: Status::Ok,
            detail,
            hint: String::new(),
        }
    }

    /// `("note", detail, "")`.
    fn note(detail: String) -> Self {
        Self {
            status: Status::Note,
            detail,
            hint: String::new(),
        }
    }

    /// `("note", detail, hint)`: the one note that carries one.
    ///
    /// `check_stow`'s "linked into another checkout" case. The printed transcript never shows
    /// it -- only a `fail` prints its hint -- but `doctor --report` serializes every field of
    /// every check, so dropping it would make the two surfaces disagree.
    fn note_with_hint(detail: String, hint: String) -> Self {
        Self {
            status: Status::Note,
            detail,
            hint,
        }
    }

    /// `("fail", detail, hint)`.
    fn fail(detail: String, hint: String) -> Self {
        Self {
            status: Status::Fail,
            detail,
            hint,
        }
    }
}

/// One probe: everything it needs is in the context, and all it answers is a verdict.
type Probe = fn(&DoctorCx<'_>) -> Verdict;

/// `doctor_checks()`: the ten checks, in the order both surfaces walk them.
pub(crate) fn doctor_checks() -> [(&'static str, Probe); 10] {
    [
        ("hyprland", checks::check_hyprland),
        ("packages", checks::check_packages),
        ("fonts", checks::check_fonts),
        ("stow links", checks::check_stow),
        ("dead links", checks::check_dangling),
        ("units", checks::check_units),
        ("plugins", checks::check_plugins),
        ("fragments", checks::check_fragments),
        ("preferences", checks::check_preferences),
        ("compositor", checks::check_compositor),
    ]
}

/// `garage doctor [--report]`: a read-only health report. 0 when healthy, 1 when something is
/// wrong.
///
/// `--report` prints the same checks as JSON instead of as lines, and keeps the same exit
/// status: a report is still an answer to "is this install healthy", and a script that
/// switched forms should not also have to switch how it reads the result.
///
/// # Errors
///
/// [`ApplyError::Settings`] for an argument this command does not take, and for a binary that
/// is not inside a Garage checkout. Both reach the user as `garage doctor: {error}` on
/// stderr, which is what the Python's `main()` does with the `SettingsError` it catches.
pub fn doctor(paths: &Paths, proc: &dyn Runner, argv: &[String]) -> Result<i32, ApplyError> {
    let mut wants_report = false;
    for argument in argv {
        if argument == "--report" {
            wants_report = true;
        } else {
            return Err(ApplyError::Settings(format!(
                "Usage: garage doctor [--report]  (unexpected argument: {argument})"
            )));
        }
    }
    let cx = DoctorCx::new(paths, proc)?;
    let (text, status) = if wants_report {
        report::report_text(&cx)
    } else {
        transcript(&cx)
    };
    print!("{text}");
    Ok(status)
}

/// The aligned transcript: `doctor()`'s non-`--report` half, and the status it answers with.
///
/// Built as a string rather than printed line by line, so a test can hold the exact bytes.
/// Nothing else writes to stdout on the way past -- the probes return their verdict and the
/// preferences notes go to a sink -- so one `print!` at the end is the same output in the same
/// order.
pub(crate) fn transcript(cx: &DoctorCx<'_>) -> (String, i32) {
    use std::fmt::Write as _;

    let mut out = String::new();
    let _ = writeln!(
        out,
        "Garage doctor\n  checkout  {}\n  home      {}\n",
        cx.root.display(),
        cx.paths.home.display()
    );
    let checks = doctor_checks();
    let width = checks.iter().map(|(name, _)| name.len()).max().unwrap_or(0);
    let mut failures = 0;
    for (name, probe) in checks {
        let verdict = probe(cx);
        if verdict.status == Status::Fail {
            failures += 1;
        }
        let _ = writeln!(
            out,
            "  {:<4}  {:<width$}  {}",
            verdict.status.word(),
            name,
            verdict.detail
        );
        if verdict.status == Status::Fail && !verdict.hint.is_empty() {
            let _ = writeln!(out, "        {:width$}  -> {}", "", verdict.hint);
        }
    }
    if failures > 0 {
        let _ = writeln!(out, "\n{failures} problem(s) found.");
        return (out, 1);
    }
    let _ = writeln!(out, "\nNo problems found.");
    (out, 0)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{local_backup_stamp, local_iso8601, tilde, version_parts};

    #[test]
    fn a_version_is_three_numbers_however_many_the_string_has() {
        assert_eq!(version_parts("0.56.0"), [0, 56, 0]);
        assert_eq!(version_parts("Hyprland v0.51.1 built"), [0, 51, 1]);
        assert_eq!(version_parts("0.56"), [0, 56, 0]);
        assert_eq!(version_parts(""), [0, 0, 0]);
        // The whole reason it is numeric: a string compare puts 0.56.10 below 0.56.9.
        assert!(version_parts("0.56.10") > version_parts("0.56.9"));
        // Only the first three runs are read.
        assert_eq!(version_parts("1.2.3.4"), [1, 2, 3]);
    }

    #[test]
    fn a_path_under_home_is_printed_the_way_it_is_typed() {
        let home = Path::new("/home/tester");
        assert_eq!(
            tilde(
                home,
                Path::new("/home/tester/.config/garage/preferences.toml")
            ),
            "~/.config/garage/preferences.toml"
        );
        assert_eq!(
            tilde(home, Path::new("/usr/lib/kinetik")),
            "/usr/lib/kinetik"
        );
    }

    /// Both stamps off one instant, under whatever zone the test box has: the fields have to
    /// agree, because a backup name that disagreed with the report's clock would be a second
    /// clock read rather than a second format of the first.
    #[test]
    fn the_two_stamps_are_the_same_instant_in_two_shapes() {
        let iso = local_iso8601(1_700_000_000);
        let stamp = local_backup_stamp(1_700_000_000);
        assert_eq!(iso.len(), 24, "{iso}");
        assert_eq!(stamp.len(), 15, "{stamp}");
        let digits: String = iso.chars().take(19).filter(char::is_ascii_digit).collect();
        assert_eq!(stamp.replace('-', ""), digits);
    }
}
