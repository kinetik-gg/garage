//! `role_applications()`, `terminal_candidates()`, `resolve_terminal()`,
//! `terminal_command()`, `browser_command()` and `set_default_app()`: the per-role picker the
//! Default Applications pane draws from.
//!
//! `role_applications()` requires a candidate to register for *every* mimetype a role owns,
//! because selecting it writes every one of them at once -- the union of each mimetype's own
//! registrants would offer a text editor as a web browser, since editors commonly claim
//! `text/html` too, and then hand it a `https:` link it cannot open. Candidates are
//! deduplicated by display name rather than by desktop id, because one application can ship
//! two desktop files for itself (Chrome installs both `com.google.Chrome.desktop` and
//! `google-chrome.desktop`), and a combo box offering "Google Chrome" twice would give no way
//! to tell the two apart; whichever id is already in force keeps its name in the list, so the
//! current choice is always present in the offered set. `NoDisplay` entries are dropped
//! everywhere in this module for the same reason: an application saying it is not something
//! to pick (kitty's URL handler registers for `inode/directory` but is no file manager)
//! belongs in no menu and no combo.
//!
//! `terminal_candidates()` has no mimetype to search by -- there is no "a terminal" mimetype
//! -- so it searches the freedesktop `TerminalEmulator` category across every desktop file.
//!
//! `browser_command()` resolves `$BROWSER`'s replacement for a bare keybinding: `$BROWSER` is
//! `xdg-open`, which is right for anything handing it a URL and useless as a keybind target,
//! since it exits having been given nothing to open. The bind is given the browser's own
//! `Exec=` line instead, resolved from whatever is currently registered rather than stored a
//! second time as its own preference.
//!
//! `set_default_app()` writes the association through
//! [`crate::desktopfiles::mime`]'s `write_mime_defaults()`, and additionally republishes the
//! browser marker and reloads the compositor when the role is `browser` specifically --
//! `binds.lua` reads that marker for the browser keybind the same way it reads the terminal
//! one.
//!
//! Reads installed applications and writes a mime association plus (for `browser`) a marker,
//! not `Result<(), ApplyError>` over a [`SessionCx`]: reached from `action defaults.*` and from
//! [`crate::terminal`]/[`crate::snapshot::apps`], not from `Route::steps()` directly.

use std::collections::HashMap;

use garage_core::fs::marker::write_marker;
use garage_core::paths::Paths;

use crate::cx::SessionCx;
use crate::desktopfiles::entry;
use crate::desktopfiles::mime::{self, DesktopFileError};

/// The roles the General pane offers a default application for, and every mimetype each one
/// owns (garage:772-782). The first type is the reported current default; setting a role
/// writes all of them, so choosing an image viewer covers JPEG as well as PNG. The
/// browser's three are what `xdg-settings` would have written for `default-web-browser`.
pub(crate) const DEFAULT_APP_ROLES: &[(&str, &[&str])] = &[
    (
        "browser",
        &[
            "x-scheme-handler/http",
            "x-scheme-handler/https",
            "text/html",
        ],
    ),
    ("mail", &["x-scheme-handler/mailto"]),
    ("files", &["inode/directory"]),
    ("editor", &["text/plain"]),
    ("image", &["image/png", "image/jpeg"]),
    ("video", &["video/mp4", "video/x-matroska"]),
    ("pdf", &["application/pdf"]),
];

/// The mimetypes a role owns, or `None` for a role name nothing here defines.
fn role_mimetypes(role: &str) -> Option<&'static [&'static str]> {
    DEFAULT_APP_ROLES
        .iter()
        .find(|(name, _)| *name == role)
        .map(|(_, types)| *types)
}

/// A role's current handler and the applications that can take it over (garage:4327-4360).
pub(crate) fn role_applications(cx: &SessionCx<'_>, types: &[&str]) -> (String, Vec<String>) {
    let paths = cx.render().paths();
    let mut current = String::new();
    let mut by_name: HashMap<String, String> = HashMap::new();
    for (index, mimetype) in types.iter().enumerate() {
        let (default, registered) = mime::mime_handlers(cx, mimetype);
        if index == 0 {
            current = default;
        }
        let offered = offered_by_name(paths, &registered, &current);
        by_name = if index == 0 {
            offered
        } else {
            by_name
                .into_iter()
                .filter(|(name, _)| offered.contains_key(name))
                .collect()
        };
    }
    let mut candidates = sorted_candidates(&by_name);
    resolve_current_fallback(paths, &mut current, &by_name, &mut candidates);
    (current, candidates)
}

/// The visible (non-`NoDisplay`) applications for one mimetype, keyed by display name; the
/// entry already in force wins a name collision, so it is never displaced by another id.
fn offered_by_name(paths: &Paths, registered: &[String], current: &str) -> HashMap<String, String> {
    let mut offered = HashMap::new();
    for desktop_id in registered {
        let fields = entry::desktop_fields(paths, desktop_id);
        let Some(name) = fields.get("Name") else {
            continue;
        };
        if name.is_empty() {
            continue;
        }
        if fields
            .get("NoDisplay")
            .is_some_and(|value| value.eq_ignore_ascii_case("true"))
        {
            continue;
        }
        if !offered.contains_key(name) || desktop_id == current {
            offered.insert(name.clone(), desktop_id.clone());
        }
    }
    offered
}

/// The offered desktop ids, sorted by their display name -- `sorted(by_name.items())`.
fn sorted_candidates(by_name: &HashMap<String, String>) -> Vec<String> {
    let mut pairs: Vec<(&String, &String)> = by_name.iter().collect();
    pairs.sort();
    pairs.into_iter().map(|(_, id)| id.clone()).collect()
}

/// When the current handler is not among the candidates -- a `NoDisplay` alias, like
/// Chrome's `com.google.Chrome.desktop` standing in for `google-chrome.desktop` -- report
/// the visible entry instead, falling back to the id itself only when nothing visible
/// stands for it: an unshowable default is one the pane would silently replace.
fn resolve_current_fallback(
    paths: &Paths,
    current: &mut String,
    by_name: &HashMap<String, String>,
    candidates: &mut Vec<String>,
) {
    if current.is_empty() || candidates.contains(current) {
        return;
    }
    let current_name = entry::desktop_fields(paths, current)
        .get("Name")
        .cloned()
        .unwrap_or_default();
    if let Some(replacement) = by_name.get(&current_name) {
        current.clone_from(replacement);
    } else if entry::desktop_file(paths, current).is_some() {
        candidates.insert(0, current.clone());
    }
}

/// Every installed terminal emulator, by desktop id (garage:4363-4376): no mimetype names
/// "a terminal", so the `TerminalEmulator` category is searched instead. `NoDisplay` is
/// skipped -- kitty ships a second entry only for URLs.
pub(crate) fn terminal_candidates(paths: &Paths) -> Vec<String> {
    let mut ids: Vec<String> = entry::desktop_file_names(paths);
    ids.sort();
    ids.dedup();
    let mut by_name: HashMap<String, String> = HashMap::new();
    for desktop_id in &ids {
        let fields = entry::desktop_fields(paths, desktop_id);
        let is_terminal = fields
            .get("Categories")
            .is_some_and(|categories| categories.contains("TerminalEmulator"));
        let no_display = fields
            .get("NoDisplay")
            .is_some_and(|value| value.eq_ignore_ascii_case("true"));
        if is_terminal && !no_display {
            let display_name = fields
                .get("Name")
                .cloned()
                .unwrap_or_else(|| desktop_id.clone());
            by_name
                .entry(display_name)
                .or_insert_with(|| desktop_id.clone());
        }
    }
    sorted_candidates(&by_name)
}

/// The terminal in force: the chosen one for as long as it is installed (garage:4379-4383).
pub(crate) fn resolve_terminal(paths: &Paths, configured: &str) -> String {
    let candidates = terminal_candidates(paths);
    if candidates.iter().any(|candidate| candidate == configured) {
        configured.to_owned()
    } else {
        candidates.into_iter().next().unwrap_or_default()
    }
}

/// The resolved terminal's `Exec=` line, with its field codes stripped (garage:4386-4387).
///
/// **Not** trimmed -- unlike [`browser_command`], the Python hands the substitution's result
/// straight back with no `.strip()`. Kept exactly: the asymmetry is the Python's, not this
/// port's to smooth over.
pub(crate) fn terminal_command(paths: &Paths, configured: &str) -> String {
    let desktop_id = resolve_terminal(paths, configured);
    let exec = entry::desktop_fields(paths, &desktop_id)
        .get("Exec")
        .cloned()
        .unwrap_or_default();
    strip_exec_field_codes(&exec)
}

/// The default browser as a command that opens with no arguments (garage:4390-4400).
pub(crate) fn browser_command(cx: &SessionCx<'_>) -> String {
    let browser_types = role_mimetypes("browser").unwrap_or(&[]);
    let (desktop_id, _) = role_applications(cx, browser_types);
    if desktop_id.is_empty() {
        return String::new();
    }
    let exec = entry::desktop_fields(cx.render().paths(), &desktop_id)
        .get("Exec")
        .cloned()
        .unwrap_or_default();
    strip_exec_field_codes(&exec).trim().to_owned()
}

/// The field codes a desktop `Exec` carries for the files or URLs it is handed
/// (garage:785-787). A terminal is launched with none, so they are dropped rather than
/// reaching the command line as literal arguments.
const EXEC_FIELD_CODE_LETTERS: &[char] = &[
    'f', 'F', 'u', 'U', 'd', 'D', 'n', 'N', 'i', 'c', 'k', 'v', 'm',
];

/// `EXEC_FIELD_CODES.sub("", exec)`: `re.sub(r"\s*%[fFuUdDnNickvm]", "", exec)`, hand-written
/// since there is no regex crate here. Scans left to right, dropping each run of whitespace
/// immediately followed by `%` and one of [`EXEC_FIELD_CODE_LETTERS`] -- `re.sub`'s
/// non-overlapping leftmost match, for this one pattern.
fn strip_exec_field_codes(exec: &str) -> String {
    let chars: Vec<char> = exec.chars().collect();
    let mut out = String::with_capacity(exec.len());
    let mut index = 0;
    while let Some(&current) = chars.get(index) {
        if let Some(end) = field_code_span_end(&chars, index) {
            index = end;
            continue;
        }
        out.push(current);
        index += 1;
    }
    out
}

/// Where a field-code match starting at `start` ends, or `None` if there is none there.
fn field_code_span_end(chars: &[char], start: usize) -> Option<usize> {
    let mut cursor = start;
    while chars.get(cursor).is_some_and(|ch| ch.is_whitespace()) {
        cursor += 1;
    }
    if chars.get(cursor) != Some(&'%') {
        return None;
    }
    let letter = *chars.get(cursor + 1)?;
    EXEC_FIELD_CODE_LETTERS
        .contains(&letter)
        .then_some(cursor + 2)
}

/// Set a role's default application (garage:4403-4411).
///
/// The closing `hyprctl reload` is fire-and-forget, matching the Python's plain `run()` call
/// (`check` defaults to `False`): a refused reload does not fail this call, the same way
/// [`crate::workspaces::reload_bar`] never reports its signal's outcome either.
///
/// # Errors
///
/// [`DesktopFileError::UnknownRole`] or [`DesktopFileError::NotInstalled`] for a bad role or
/// desktop id; [`DesktopFileError::Mimeapps`] or [`DesktopFileError::Marker`] if the write
/// itself fails.
pub(crate) fn set_default_app(
    cx: &SessionCx<'_>,
    role: &str,
    desktop_id: &str,
) -> Result<(), DesktopFileError> {
    let Some(types) = role_mimetypes(role) else {
        return Err(DesktopFileError::UnknownRole(role.to_owned()));
    };
    let paths = cx.render().paths();
    if entry::desktop_file(paths, desktop_id).is_none() {
        return Err(DesktopFileError::NotInstalled(desktop_id.to_owned()));
    }
    let assignments: HashMap<String, String> = types
        .iter()
        .map(|mimetype| ((*mimetype).to_owned(), desktop_id.to_owned()))
        .collect();
    mime::write_mime_defaults(paths, &assignments)?;
    if role == "browser" {
        let command = browser_command(cx);
        write_marker(&paths.markers.browser, &format!("{command}\n"))?;
        drop(mime::run(cx, &["hyprctl", "reload"]));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fmt::Write as _;
    use std::fs;
    use std::sync::{Arc, Mutex, PoisonError};

    use garage_core::paths::Paths;
    use garage_core::schema::defaults::Defaults;
    use garage_core::traits::Output;

    use super::{
        resolve_terminal, role_applications, set_default_app, strip_exec_field_codes,
        terminal_candidates, terminal_command, DesktopFileError,
    };
    use crate::desktopfiles::entry::support::{session, RunOnly, Scratch};

    /// Every `hyprctl` invocation a [`stub_gio`] recorded.
    type ReloadCalls = Arc<Mutex<Vec<Vec<String>>>>;

    /// A `gio mime` stub answering per mimetype, and recording every `hyprctl` invocation.
    fn stub_gio(
        transcripts: impl IntoIterator<Item = (&'static str, String)>,
    ) -> (RunOnly<impl Fn(&[&str]) -> Output>, ReloadCalls) {
        let transcripts: HashMap<&'static str, String> = transcripts.into_iter().collect();
        let reload_calls: ReloadCalls = Arc::default();
        let recorder = Arc::clone(&reload_calls);
        let proc = RunOnly(move |command: &[&str]| {
            if command.first() == Some(&"hyprctl") {
                recorder
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push(command.iter().map(|part| (*part).to_owned()).collect());
                return Output {
                    status: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                };
            }
            let mimetype = command.last().copied().unwrap_or_default();
            Output {
                status: 0,
                stdout: transcripts.get(mimetype).cloned().unwrap_or_default(),
                stderr: String::new(),
            }
        });
        (proc, reload_calls)
    }

    /// A `gio mime` transcript: one default plus a `Registered applications:` list.
    fn transcript(default: &str, registered: &[&str]) -> String {
        let mut text = format!(
            "Default application for \u{201c}x\u{201d}: {default}\n\nRegistered applications:\n"
        );
        for desktop_id in registered {
            let _ = writeln!(text, "\t{desktop_id}");
        }
        text
    }

    #[test]
    fn terminal_command_strips_field_codes_and_candidates_skip_no_display_entries() {
        assert_eq!(strip_exec_field_codes("kitty %U"), "kitty");
        assert_eq!(strip_exec_field_codes("firefox %u"), "firefox");
        assert_eq!(strip_exec_field_codes("foo"), "foo");
        assert_eq!(strip_exec_field_codes("app%f"), "app");
        assert_eq!(strip_exec_field_codes("app %f %u extra"), "app extra");

        let scratch = Scratch::new("terminal-candidates");
        let paths = scratch.paths();
        Scratch::plant(
            &paths,
            "kitty.desktop",
            "[Desktop Entry]\nName=Kitty\nCategories=System;TerminalEmulator;\nExec=kitty %U\n",
        );
        Scratch::plant(
            &paths,
            "kitty-open.desktop",
            "[Desktop Entry]\nName=Kitty URL handler\nCategories=TerminalEmulator;\nNoDisplay=true\n",
        );
        Scratch::plant(
            &paths,
            "alacritty.desktop",
            "[Desktop Entry]\nName=Alacritty\nCategories=TerminalEmulator;\nExec=alacritty\n",
        );

        assert_eq!(
            terminal_candidates(&paths),
            vec!["alacritty.desktop", "kitty.desktop"]
        );
        assert_eq!(resolve_terminal(&paths, "kitty.desktop"), "kitty.desktop");
        // Not among the candidates, so the first one -- sorted by display name, Alacritty
        // before Kitty -- wins.
        assert_eq!(
            resolve_terminal(&paths, "missing.desktop"),
            "alacritty.desktop"
        );
        assert_eq!(terminal_command(&paths, "kitty.desktop"), "kitty");
    }

    /// Firefox, gedit (which would look like a browser from a single type), and Chrome's
    /// `NoDisplay` alias beside the visible entry it stands for.
    fn plant_default_apps(paths: &Paths) {
        Scratch::plant(
            paths,
            "firefox.desktop",
            "[Desktop Entry]\nName=Firefox\nExec=firefox %u\n",
        );
        Scratch::plant(
            paths,
            "gedit.desktop",
            "[Desktop Entry]\nName=Text Editor\n",
        );
        Scratch::plant(
            paths,
            "com.google.Chrome.desktop",
            "[Desktop Entry]\nName=Google Chrome\nNoDisplay=true\n",
        );
        Scratch::plant(
            paths,
            "google-chrome.desktop",
            "[Desktop Entry]\nName=Google Chrome\n",
        );
    }

    /// The `gio mime` answers [`plant_default_apps`]'s fixture needs.
    fn default_apps_transcripts() -> [(&'static str, String); 4] {
        let browser_only = transcript("firefox.desktop", &["firefox.desktop"]);
        [
            (
                "x-scheme-handler/http",
                transcript("firefox.desktop", &["firefox.desktop", "gedit.desktop"]),
            ),
            ("x-scheme-handler/https", browser_only),
            (
                "text/html",
                transcript("firefox.desktop", &["firefox.desktop", "gedit.desktop"]),
            ),
            (
                "text/plain",
                transcript(
                    "com.google.Chrome.desktop",
                    &["com.google.Chrome.desktop", "google-chrome.desktop"],
                ),
            ),
        ]
    }

    #[test]
    fn role_applications_intersects_by_type_and_reports_a_no_display_currents_alias() {
        let scratch = Scratch::new("role-applications");
        let paths = scratch.paths();
        plant_default_apps(&paths);
        let defaults = Defaults::compiled().expect("shipped defaults parse");
        let (proc, _reloads) = stub_gio(default_apps_transcripts());
        let cx = session(&paths, &defaults, &proc);

        // The real browser role table, not a copy invented for the test.
        let browser_types = super::role_mimetypes("browser").unwrap_or_default();
        let (current, candidates) = role_applications(&cx, browser_types);
        assert_eq!(current, "firefox.desktop");
        // gedit registers for http and text/html but not https, so it is excluded even
        // though it would look like a browser from either type alone.
        assert_eq!(candidates, vec!["firefox.desktop"]);

        // Chrome's own current handler is a `NoDisplay` alias of the entry the menus show;
        // the visible one stands in for it, and is the only candidate offered.
        let (current, candidates) = role_applications(&cx, &["text/plain"]);
        assert_eq!(current, "google-chrome.desktop");
        assert_eq!(candidates, vec!["google-chrome.desktop"]);
    }

    #[test]
    fn set_default_app_refuses_bad_input_and_writes_the_browser_marker_on_success() {
        let scratch = Scratch::new("set-default-app");
        let paths = scratch.paths();
        plant_default_apps(&paths);
        let defaults = Defaults::compiled().expect("shipped defaults parse");
        let (proc, reload_calls) = stub_gio(default_apps_transcripts());
        let cx = session(&paths, &defaults, &proc);

        let unknown_role = set_default_app(&cx, "printer", "cups.desktop").unwrap_err();
        assert!(matches!(unknown_role, DesktopFileError::UnknownRole(role) if role == "printer"));
        let not_installed = set_default_app(&cx, "editor", "nowhere.desktop").unwrap_err();
        assert!(
            matches!(not_installed, DesktopFileError::NotInstalled(id) if id == "nowhere.desktop")
        );

        set_default_app(&cx, "browser", "firefox.desktop").expect("browser role succeeds");
        assert_eq!(
            fs::read_to_string(&paths.markers.browser).expect("marker written"),
            "firefox\n"
        );
        assert_eq!(
            reload_calls.lock().expect("lock").first(),
            Some(&vec!["hyprctl".to_owned(), "reload".to_owned()])
        );
        let written = fs::read_to_string(&paths.mimeapps_override).expect("override written");
        assert!(written.contains("x-scheme-handler/http=firefox.desktop"));
    }
}
