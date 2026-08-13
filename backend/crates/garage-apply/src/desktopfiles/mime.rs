//! `mime_handlers()` and `write_mime_defaults()`: reading and writing default application
//! associations.
//!
//! `mime_handlers()` shells out to `gio mime` rather than reading the mimeinfo caches
//! directly: `gio` resolves the whole XDG lookup -- desktop-prefixed lists, config
//! directories, then data directories -- exactly as the application that will actually open
//! the file does, which a hand-rolled cache reader could only approximate. Run under
//! `LC_ALL=C` so the section headers this parses by (`"Default application for"`,
//! `"Registered applications"`) cannot be translated out from under it on a non-English
//! system.
//!
//! # The `MIMEAPPS_OVERRIDE` stow rationale
//!
//! `write_mime_defaults()` writes only `[Default Applications]`, and only into a
//! desktop-prefixed override file, never into `~/.config/mimeapps.list` itself. That file is
//! a stow symlink into the dotfiles repo -- tracked, hand-curated, holding the associations
//! and removals the checkout ships -- and this generated override is what the XDG spec's
//! lookup order lets sit in front of it without editing it. Writing straight into the tracked
//! file would mean every default-application change through the pane shows up as a dirty
//! working tree in the dotfiles checkout, which is the same category of mistake `render_theme()`
//! avoids by writing only generated paths.
//!
//! Existing entries are read back and merged with the new assignments before the file is
//! rewritten whole, rather than only appending, so a role changed twice does not leave two
//! conflicting lines for the same mimetype.
//!
//! Reads and writes a small `.list`-format file and shells out to `gio`, not
//! `Result<(), ApplyError>` over a [`SessionCx`]: [`crate::desktopfiles::roles`]'s
//! `set_default_app()` is what calls both of these and turns their outcome into one.

use std::collections::HashMap;

use garage_core::fs::atomic::{atomic_write, AtomicWriteError};
use garage_core::fs::marker::MarkerWriteError;
use garage_core::paths::Paths;
use garage_core::traits::{Output, DEFAULT_RUN_TIMEOUT};
use thiserror::Error;

use crate::cx::SessionCx;

/// Why a default-application change was refused.
///
/// The Python raises `SettingsError` for both -- see [`crate::keybind::KeybindError`] for the
/// established shape this follows: the *text* is the contract, since it is what reaches the
/// user through the envelope. Lives here rather than in
/// [`crate::desktopfiles::roles`] -- where `set_default_app()` itself is -- because its last
/// two variants wrap this module's own [`AtomicWriteError`] and [`MarkerWriteError`], and a
/// type is easiest to keep honest beside the calls that produce it.
#[derive(Debug, Error)]
pub(crate) enum DesktopFileError {
    /// `set_default_app()`: a role name `DEFAULT_APP_ROLES` does not carry.
    #[error("Unknown default application: {0}")]
    UnknownRole(String),
    /// `set_default_app()`: a desktop id `desktop_file()` cannot resolve.
    #[error("{0} is not installed")]
    NotInstalled(String),
    /// The merged mimeapps override could not be written.
    #[error(transparent)]
    Mimeapps(#[from] AtomicWriteError),
    /// The browser marker could not be written.
    #[error(transparent)]
    Marker(#[from] MarkerWriteError),
}

/// `run()` (garage:1462) with `check=False`: a command that could not be run at all comes
/// back as the `CompletedProcess(command, 1, "", str(error))` the Python synthesises, the
/// same conflation [`crate::workspaces::run`] makes for the same reason. `pub(super)` rather
/// than private: [`crate::desktopfiles::roles::set_default_app`]'s own `hyprctl reload` goes
/// through this copy too, so there is one `Runner` boundary in this module rather than two.
pub(super) fn run(cx: &SessionCx<'_>, command: &[&str]) -> Output {
    cx.proc()
        .run(command, DEFAULT_RUN_TIMEOUT)
        .unwrap_or_else(|error| Output {
            status: 1,
            stdout: String::new(),
            stderr: error.detail,
        })
}

/// The current default and every registered application for a mimetype (garage:4290-4308).
///
/// `.splitlines()` vs. `.lines()`: the same narrow, unreachable-from-`gio`'s-own-output
/// departure documented on [`crate::desktopfiles::entry::desktop_fields`].
pub(crate) fn mime_handlers(cx: &SessionCx<'_>, mimetype: &str) -> (String, Vec<String>) {
    let result = run(cx, &["env", "LC_ALL=C", "gio", "mime", mimetype]);
    let mut default = String::new();
    let mut registered = Vec::new();
    let mut listing = false;
    for line in result.stdout.lines() {
        if line.starts_with('\t') {
            if listing {
                registered.push(line.trim().to_owned());
            }
        } else if line.starts_with("Default application for") {
            // "No default applications for ..." misses this on purpose: it is the answer
            // for a mimetype nothing has claimed, and `default` stays "".
            line.rsplit_once(": ")
                .map_or(line, |(_, after)| after)
                .trim()
                .clone_into(&mut default);
        } else {
            listing = line.starts_with("Registered applications");
        }
    }
    (default, registered)
}

/// Point mimetypes at a desktop id in the desktop-prefixed mimeapps list
/// (garage:4368-4381).
///
/// The existing file is read with a strict UTF-8 decode rather than the `errors="replace"`
/// leniency [`crate::desktopfiles::entry::desktop_fields`] gives a hand-editable file: unlike
/// a desktop file, `MIMEAPPS_OVERRIDE` is written only by this function, so an unreadable one
/// is either absent (the ordinary first-run case, caught by the Python's `except OSError`) or
/// corrupt in a way nothing here produced. Both are treated as "start from nothing" -- a
/// stricter reading than the Python's, which does not catch a decode failure at all and would
/// raise. Widening the catch is the safer direction for a generated file: it costs the merge
/// of one already-corrupt file, never a value some other writer is trusted to keep.
///
/// # Errors
///
/// [`AtomicWriteError`] if the merged file could not be written.
pub(crate) fn write_mime_defaults(
    paths: &Paths,
    assignments: &HashMap<String, String>,
) -> Result<(), AtomicWriteError> {
    let mut entries = existing_default_applications(paths);
    for (mimetype, desktop_id) in assignments {
        entries.insert(mimetype.clone(), desktop_id.clone());
    }
    atomic_write(
        &paths.mimeapps_override,
        &render_mimeapps_override(&entries),
    )
}

/// The `[Default Applications]` section of the override file as it stands, or empty when the
/// file is absent or unreadable.
fn existing_default_applications(paths: &Paths) -> HashMap<String, String> {
    let Ok(text) = std::fs::read_to_string(&paths.mimeapps_override) else {
        return HashMap::new();
    };
    let mut entries = HashMap::new();
    let mut section = String::new();
    for line in text.lines() {
        let stripped = line.trim();
        if stripped.starts_with('[') {
            stripped.clone_into(&mut section);
            continue;
        }
        if section != "[Default Applications]" || stripped.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = stripped.split_once('=') {
            entries.insert(key.trim().to_owned(), value.trim().to_owned());
        }
    }
    entries
}

/// The whole override file, sorted by mimetype with anything falsy dropped -- an assignment
/// whose desktop id was cleared to `""` removes the mimetype from the file rather than
/// pinning it to nothing.
fn render_mimeapps_override(entries: &HashMap<String, String>) -> String {
    let mut sorted: Vec<(&String, &String)> = entries
        .iter()
        .filter(|(_, desktop_id)| !desktop_id.is_empty())
        .collect();
    sorted.sort();
    let mut text =
        String::from("# Generated by garage. Overrides mimeapps.list.\n[Default Applications]\n");
    for (mimetype, desktop_id) in sorted {
        text.push_str(mimetype);
        text.push('=');
        text.push_str(desktop_id);
        text.push('\n');
    }
    text
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fs;

    use garage_core::schema::defaults::Defaults;
    use garage_core::traits::Output;

    use super::{mime_handlers, write_mime_defaults};
    use crate::desktopfiles::entry::support::{session, RunOnly, Scratch};

    /// A [`RunOnly`] that answers every call with one canned `gio mime` transcript,
    /// ignoring the command -- these tests only exercise `mime_handlers()`'s parsing.
    fn canned_gio(stdout: &'static str) -> RunOnly<impl Fn(&[&str]) -> Output> {
        RunOnly(move |_command: &[&str]| Output {
            status: 0,
            stdout: stdout.to_owned(),
            stderr: String::new(),
        })
    }

    #[test]
    fn mime_handlers_splits_default_and_registered_from_a_gio_transcript() {
        let scratch = Scratch::new("mime-handlers");
        let paths = scratch.paths();
        let defaults = Defaults::compiled().expect("shipped defaults parse");
        let proc = canned_gio(
            "Default application for \u{201c}text/html\u{201d}: firefox.desktop\n\n\
             Registered applications:\n\
             \tfirefox.desktop\n\
             \torg.gnome.epiphany.desktop\n\n\
             Recommended applications:\n\
             \tfirefox.desktop\n",
        );
        let cx = session(&paths, &defaults, &proc);

        let (default, registered) = mime_handlers(&cx, "text/html");
        assert_eq!(default, "firefox.desktop");
        assert_eq!(
            registered,
            vec!["firefox.desktop", "org.gnome.epiphany.desktop"]
        );
    }

    #[test]
    fn mime_handlers_reports_no_default_as_empty() {
        let scratch = Scratch::new("mime-handlers-no-default");
        let paths = scratch.paths();
        let defaults = Defaults::compiled().expect("shipped defaults parse");
        let proc = canned_gio(
            "No default applications for \u{201c}application/x-nothing\u{201d}\n\n\
             Registered applications:\n\
             \tsomething.desktop\n",
        );
        let cx = session(&paths, &defaults, &proc);

        let (default, registered) = mime_handlers(&cx, "application/x-nothing");
        assert_eq!(default, "");
        assert_eq!(registered, vec!["something.desktop"]);
    }

    #[test]
    fn write_mime_defaults_merges_over_the_existing_file_and_drops_empty_values() {
        let scratch = Scratch::new("write-mime-defaults");
        let paths = scratch.paths();
        fs::create_dir_all(&paths.config_home).expect("config home dir");
        fs::write(
            &paths.mimeapps_override,
            "# Generated by garage. Overrides mimeapps.list.\n\
             [Default Applications]\n\
             image/png=feh.desktop\n\
             video/mp4=mpv.desktop\n",
        )
        .expect("seed existing override");

        let mut assignments = HashMap::new();
        assignments.insert("image/png".to_owned(), "gwenview.desktop".to_owned());
        assignments.insert("text/plain".to_owned(), String::new());
        write_mime_defaults(&paths, &assignments).expect("write succeeds");

        assert_eq!(
            fs::read_to_string(&paths.mimeapps_override).expect("file exists"),
            "# Generated by garage. Overrides mimeapps.list.\n\
             [Default Applications]\n\
             image/png=gwenview.desktop\n\
             video/mp4=mpv.desktop\n"
        );
    }

    #[test]
    fn write_mime_defaults_starts_fresh_when_nothing_exists_yet() {
        let scratch = Scratch::new("write-mime-defaults-fresh");
        let paths = scratch.paths();
        let mut assignments = HashMap::new();
        assignments.insert(
            "x-scheme-handler/mailto".to_owned(),
            "thunderbird.desktop".to_owned(),
        );
        write_mime_defaults(&paths, &assignments).expect("write succeeds");

        assert_eq!(
            fs::read_to_string(&paths.mimeapps_override).expect("file exists"),
            "# Generated by garage. Overrides mimeapps.list.\n\
             [Default Applications]\n\
             x-scheme-handler/mailto=thunderbird.desktop\n"
        );
    }
}
