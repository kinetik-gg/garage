//! `render_general()`: publish what the keybindings read, as markers rather than preferences.
//!
//! Both are read on a keypress: the launcher wrapper runs on every `SUPER+Space`, and
//! Hyprland parses `binds.lua` with no way to shell out to a helper for an answer. A marker
//! costs an open and a read; resolving the launcher choice or the terminal's `Exec=` line at
//! bind time would be felt on every press instead of once per render.
//!
//! # Two markers, not three -- a necessary deviation
//!
//! The Python's `render_general()` (garage:4446-4459) writes three markers: the launcher, the
//! resolved terminal command, and the resolved browser command. This port writes the first
//! two and, deliberately, not the third. The reason is structural, not an oversight:
//!
//! * The launcher marker is pure -- `general.builtin_launcher` read straight off the already
//!   validated [`Preferences`] this renderer is handed.
//! * The terminal marker needs a `.desktop` file lookup (`resolve_terminal()` /
//!   `terminal_command()`, garage:4383-4391), but that lookup is nothing more than directory
//!   listing and file reads -- no subprocess -- so it is a render-shaped operation, and this
//!   module carries a small copy of it (below). Duplicated from
//!   `garage-apply`'s `desktopfiles` module rather than called through it, because
//!   `garage-render` structurally cannot depend on `garage-apply` --
//!   [`crate::cx::RenderCx`]'s own docs are the reason ("a renderer has no dependency edge and
//!   no widening operation that would let it reach a `Runner` back"), and that edge runs only
//!   one way. Kept intentionally small (lookup and parse only, no mime handling, no role
//!   picker) so the duplication stays honest rather than becoming a second desktop-file
//!   resolver.
//! * The browser marker's resolution, `browser_command()` (garage:4394-4402), goes through
//!   `role_applications()` and from there through `mime_handlers()`, which runs
//!   `env LC_ALL=C gio mime <type>` as a subprocess (garage:4294-4308). A renderer has no
//!   [`Runner`](garage_core::traits::Runner) and structurally cannot grow one -- see
//!   [`crate::cx::RenderCx`]'s own "No process runner" section -- so this is the one marker
//!   `render_general()` cannot reach, full stop, wherever its logic lives. Publishing the
//!   browser marker belongs to the apply side, which does hold a `Runner`: a future
//!   `garage-apply` applier (mirroring the Python's `apply_terminal()`, which already calls
//!   `render_general()` and then reaches further) is where full three-marker parity is
//!   restored. Until that lands, the browser marker is simply not written by this crate --
//!   flagged here rather than shipped silently, per this task's own rule.

use std::collections::HashMap;
use std::path::PathBuf;

use garage_core::fs::marker::write_marker;
use garage_core::paths::Paths;

use crate::cx::RenderCx;
use crate::error::RenderError;

/// `desktop_file()` (garage:4259-4265): the first `application_dirs()` entry that carries
/// `desktop_id`, XDG lookup order. [`Paths::application_dirs`] already resolves the directory
/// list itself, from `$XDG_DATA_HOME`/`$XDG_DATA_DIRS`.
fn desktop_file(paths: &Paths, desktop_id: &str) -> Option<PathBuf> {
    paths
        .application_dirs
        .iter()
        .map(|directory| directory.join(desktop_id))
        .find(|candidate| candidate.is_file())
}

/// `desktop_fields()` (garage:4268-4287): the `[Desktop Entry]` keys of a desktop file, first
/// value wins. Read by hand for the same reason the Python gives: an `Exec` value carries `%`
/// and `;` that a general `.ini` reader's interpolation fights, and a desktop file legitimately
/// repeats a key in localised forms (`Name[fr]`, ...) some readers reject outright.
///
/// Bytes rather than `read_to_string`: an invalid desktop file should still yield whatever
/// `[Desktop Entry]` it has rather than nothing, matching the Python's
/// `errors="replace"`. `String::from_utf8_lossy` is Rust's own replacement-character reader.
fn desktop_fields(paths: &Paths, desktop_id: &str) -> HashMap<String, String> {
    let Some(path) = desktop_file(paths, desktop_id) else {
        return HashMap::new();
    };
    let Ok(bytes) = std::fs::read(&path) else {
        return HashMap::new();
    };
    let text = String::from_utf8_lossy(&bytes);
    let mut fields = HashMap::new();
    let mut section = String::new();
    for line in text.lines() {
        let stripped = line.trim();
        if stripped.starts_with('[') {
            stripped.clone_into(&mut section);
            continue;
        }
        if section != "[Desktop Entry]" || stripped.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = stripped.split_once('=') {
            fields
                .entry(key.trim().to_owned())
                .or_insert_with(|| value.trim().to_owned());
        }
    }
    fields
}

/// `terminal_candidates()` (garage:4365-4382): every installed terminal emulator, by desktop
/// id, deduplicated by display name (first id in sorted order wins) and sorted by that name.
/// There is no mimetype for "a terminal", so the freedesktop `Categories` field is the only
/// registry there is; `NoDisplay` entries are skipped, the same rule every picker in this
/// area follows.
fn terminal_candidates(paths: &Paths) -> Vec<String> {
    let mut ids: Vec<String> = paths
        .application_dirs
        .iter()
        .filter(|directory| directory.is_dir())
        .filter_map(|directory| std::fs::read_dir(directory).ok())
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            name.ends_with(".desktop").then_some(name)
        })
        .collect();
    ids.sort();
    ids.dedup();

    let mut by_name: Vec<(String, String)> = Vec::new();
    for desktop_id in ids {
        let fields = desktop_fields(paths, &desktop_id);
        let is_terminal = fields
            .get("Categories")
            .is_some_and(|categories| categories.contains("TerminalEmulator"));
        let hidden = fields
            .get("NoDisplay")
            .is_some_and(|flag| flag.eq_ignore_ascii_case("true"));
        if !is_terminal || hidden {
            continue;
        }
        let name = fields
            .get("Name")
            .cloned()
            .unwrap_or_else(|| desktop_id.clone());
        if !by_name.iter().any(|(existing, _)| *existing == name) {
            by_name.push((name, desktop_id));
        }
    }
    by_name.sort_by(|left, right| left.0.cmp(&right.0));
    by_name.into_iter().map(|(_, id)| id).collect()
}

/// `resolve_terminal()` (garage:4383-4387): the terminal in force -- the chosen one for as
/// long as it is installed, otherwise the first candidate, otherwise nothing installed at all.
fn resolve_terminal(paths: &Paths, configured: &str) -> String {
    let candidates = terminal_candidates(paths);
    if candidates.iter().any(|candidate| candidate == configured) {
        configured.to_owned()
    } else {
        candidates.into_iter().next().unwrap_or_default()
    }
}

/// `EXEC_FIELD_CODES.sub("", exec)` (garage:785-787): `re.sub(r"\s*%[fFuUdDnNickvm]", "",
/// exec)` by hand -- there is no regex dependency in this crate. Scans left to right; at each
/// position, a run of whitespace immediately followed by `%` and one of the field-code
/// letters is dropped whole (the whitespace included), matching `re.sub`'s own leftmost,
/// non-overlapping match rule for this pattern.
fn strip_exec_field_codes(exec: &str) -> String {
    const CODES: &str = "fFuUdDnNickvm";
    let chars: Vec<char> = exec.chars().collect();
    let mut out = String::with_capacity(exec.len());
    let mut at = 0;
    while at < chars.len() {
        let mut scan = at;
        while chars.get(scan).is_some_and(|ch| ch.is_whitespace()) {
            scan += 1;
        }
        let is_code = chars.get(scan) == Some(&'%')
            && chars.get(scan + 1).is_some_and(|ch| CODES.contains(*ch));
        if is_code {
            at = scan + 2;
            continue;
        }
        if let Some(ch) = chars.get(at) {
            out.push(*ch);
        }
        at += 1;
    }
    out
}

/// `terminal_command()` (garage:4390-4391): the resolved terminal's `Exec=` line, field codes
/// stripped -- a terminal is launched with no file or URL, so they never reach the command
/// line as literal arguments.
fn terminal_command(paths: &Paths, configured: &str) -> String {
    let resolved = resolve_terminal(paths, configured);
    let exec = desktop_fields(paths, &resolved)
        .get("Exec")
        .cloned()
        .unwrap_or_default();
    strip_exec_field_codes(&exec)
}

/// Write the launcher and terminal markers `binds.lua` and the launcher wrapper read.
///
/// The browser marker is not written here -- see this module's docs for why that is a
/// necessary deviation from the Python's `render_general()` rather than a gap in the port.
///
/// # Errors
///
/// [`RenderError::Marker`] if either marker could not be written.
pub(crate) fn render_general(cx: &RenderCx<'_>) -> Result<(), RenderError> {
    let launcher = if cx.prefs().general.builtin_launcher {
        "builtin"
    } else {
        "external"
    };
    write_marker(&cx.paths().markers.launcher, &format!("{launcher}\n"))?;
    let terminal = terminal_command(cx.paths(), &cx.prefs().general.terminal);
    write_marker(&cx.paths().markers.terminal, &format!("{terminal}\n"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fs;

    use garage_core::paths::Paths;
    use garage_core::schema::defaults::Defaults;
    use garage_core::schema::notes::Notes;
    use garage_core::schema::Preferences;

    use super::{
        render_general, resolve_terminal, strip_exec_field_codes, terminal_candidates,
        terminal_command,
    };
    use crate::cx::RenderCx;

    fn prefs_from(departures: &str) -> Preferences {
        let table: toml::Table = departures.parse().expect("fixture toml parses");
        let defaults = Defaults::compiled().expect("shipped defaults parse");
        let mut notes = Notes::new();
        Preferences::coerce_from(&table, &defaults, &mut notes)
    }

    /// A scratch `Paths` with real, planted `.desktop` files under
    /// `application_dirs[0]` (`$XDG_DATA_HOME/applications`).
    struct Fixture {
        paths: Paths,
    }

    impl Fixture {
        fn new(label: &str) -> Self {
            let home = std::env::temp_dir().join(format!(
                "garage-render-general-{label}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            // XDG_DATA_DIRS is pinned to a scratch-only, nonexistent path -- otherwise
            // application_dirs() would also walk the real machine's /usr/share/applications
            // and a genuinely installed terminal could leak into a "nothing installed" test.
            let env: HashMap<String, String> = [
                ("HOME".to_owned(), home.to_string_lossy().into_owned()),
                (
                    "XDG_DATA_DIRS".to_owned(),
                    home.join("no-system-applications")
                        .to_string_lossy()
                        .into_owned(),
                ),
            ]
            .into_iter()
            .collect();
            let paths = Paths::from_env_map(&env);
            let applications = applications_dir(&paths);
            fs::create_dir_all(applications).expect("applications dir is creatable");
            Self { paths }
        }

        fn plant(&self, name: &str, contents: &str) {
            fs::write(applications_dir(&self.paths).join(name), contents)
                .expect("desktop file is writable");
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            drop(fs::remove_dir_all(&self.paths.home));
        }
    }

    /// `$XDG_DATA_HOME/applications`, the one `application_dirs` entry [`Fixture`] plants
    /// into -- `.first()` rather than indexing, which the workspace lint denies outright.
    fn applications_dir(paths: &Paths) -> &std::path::Path {
        paths
            .application_dirs
            .first()
            .expect("Paths::application_dirs always names the data-home entry first")
    }

    #[test]
    fn strip_exec_field_codes_drops_the_code_and_its_leading_whitespace() {
        assert_eq!(strip_exec_field_codes("kitty %U"), "kitty");
        assert_eq!(strip_exec_field_codes("firefox %u"), "firefox");
        assert_eq!(strip_exec_field_codes("foo"), "foo");
        assert_eq!(strip_exec_field_codes("app %f %F"), "app");
        assert_eq!(strip_exec_field_codes("app%i --flag"), "app --flag");
        assert_eq!(strip_exec_field_codes(""), "");
    }

    #[test]
    fn terminal_candidates_skips_non_terminals_and_nodisplay_entries() {
        let fixture = Fixture::new("candidates");
        fixture.plant(
            "kitty.desktop",
            "[Desktop Entry]\nName=Kitty\nExec=kitty\nCategories=TerminalEmulator;\n",
        );
        fixture.plant(
            "kitty-open.desktop",
            "[Desktop Entry]\nName=Kitty URL handler\nExec=kitty\n\
             Categories=TerminalEmulator;\nNoDisplay=true\n",
        );
        fixture.plant(
            "firefox.desktop",
            "[Desktop Entry]\nName=Firefox\nExec=firefox %u\nCategories=Network;\n",
        );
        fixture.plant(
            "alacritty.desktop",
            "[Desktop Entry]\nName=Alacritty\nExec=alacritty\nCategories=TerminalEmulator;\n",
        );
        let found = terminal_candidates(&fixture.paths);
        assert_eq!(found, ["alacritty.desktop", "kitty.desktop"]);
    }

    #[test]
    fn terminal_candidates_deduplicates_by_display_name() {
        let fixture = Fixture::new("dedup");
        fixture.plant(
            "a.desktop",
            "[Desktop Entry]\nName=Term\nExec=a\nCategories=TerminalEmulator;\n",
        );
        fixture.plant(
            "b.desktop",
            "[Desktop Entry]\nName=Term\nExec=b\nCategories=TerminalEmulator;\n",
        );
        // Sorted ids: "a.desktop" before "b.desktop", so the first (a) keeps the name.
        assert_eq!(terminal_candidates(&fixture.paths), ["a.desktop"]);
    }

    #[test]
    fn resolve_terminal_keeps_the_configured_choice_only_while_it_is_installed() {
        let fixture = Fixture::new("resolve");
        fixture.plant(
            "kitty.desktop",
            "[Desktop Entry]\nName=Kitty\nExec=kitty\nCategories=TerminalEmulator;\n",
        );
        assert_eq!(
            resolve_terminal(&fixture.paths, "kitty.desktop"),
            "kitty.desktop"
        );
        assert_eq!(
            resolve_terminal(&fixture.paths, "nonesuch.desktop"),
            "kitty.desktop"
        );
    }

    #[test]
    fn resolve_terminal_falls_back_to_empty_with_nothing_installed() {
        let fixture = Fixture::new("empty");
        assert_eq!(resolve_terminal(&fixture.paths, "anything.desktop"), "");
    }

    #[test]
    fn terminal_command_resolves_and_strips_field_codes() {
        let fixture = Fixture::new("command");
        fixture.plant(
            "kitty.desktop",
            "[Desktop Entry]\nName=Kitty\nExec=kitty %U\nCategories=TerminalEmulator;\n",
        );
        assert_eq!(terminal_command(&fixture.paths, "kitty.desktop"), "kitty");
    }

    #[test]
    fn desktop_fields_keeps_only_the_first_value_and_the_desktop_entry_section() {
        let fixture = Fixture::new("fields");
        fixture.plant(
            "app.desktop",
            "[Desktop Entry]\n# a comment\nName=App\nName[fr]=Appli\n\
             Exec=app --flag\n[Desktop Action foo]\nName=Should not be read\n",
        );
        let fields = super::desktop_fields(&fixture.paths, "app.desktop");
        assert_eq!(fields.get("Name").map(String::as_str), Some("App"));
        assert_eq!(fields.get("Exec").map(String::as_str), Some("app --flag"));
    }

    struct NoMonitors;
    impl garage_core::traits::MonitorSource for NoMonitors {
        fn monitors(
            &self,
        ) -> Result<Vec<garage_core::traits::Monitor>, garage_core::traits::MonitorError> {
            Ok(vec![])
        }
    }

    struct LuaAccepts;
    impl garage_core::traits::LuaSyntaxCheck for LuaAccepts {
        fn check(
            &self,
            _candidate: &std::path::Path,
        ) -> Result<(), garage_core::traits::LuaCheckError> {
            Ok(())
        }
    }

    #[test]
    fn render_general_writes_the_launcher_and_terminal_markers_but_not_the_browser_one() {
        let fixture = Fixture::new("render");
        fixture.plant(
            "kitty.desktop",
            "[Desktop Entry]\nName=Kitty\nExec=kitty %U\nCategories=TerminalEmulator;\n",
        );
        let prefs =
            prefs_from("[general]\nterminal = \"kitty.desktop\"\nbuiltin_launcher = false\n");
        let monitors = NoMonitors;
        let lua = LuaAccepts;
        let cx = RenderCx::new(&prefs, &fixture.paths, &monitors, &lua);
        render_general(&cx).expect("render_general succeeds on a clean scratch");

        let launcher =
            fs::read_to_string(&fixture.paths.markers.launcher).expect("launcher marker");
        assert_eq!(launcher, "external\n");
        let terminal =
            fs::read_to_string(&fixture.paths.markers.terminal).expect("terminal marker");
        assert_eq!(terminal, "kitty\n");
        assert!(!fixture.paths.markers.browser.exists());
    }
}
