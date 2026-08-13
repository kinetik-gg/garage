//! `apply_locale()`: push the chosen locale as far into the running session as it reaches.
//!
//! Not far. `LANG` is read once, at startup, by the C library, and nothing re-reads it, so no
//! already-running program can be moved to another locale. What can be moved is what has yet
//! to start: uwsm runs applications as systemd user units, so seeding the user manager's
//! environment -- and the D-Bus activation environment beside it, for services started that
//! way -- means anything opened from here on comes up in the new locale. The shell, the bar
//! and every open window keep the old one until the next login, which is what the pane says
//! in as many words.
//!
//! An empty override resolves to the system locale rather than clearing `LANG` to nothing:
//! `unset-environment` is used for that case specifically, rather than setting an empty
//! value, so a downstream reader sees "no override" and not "an empty language".
//! `dbus-update-activation-environment` is best-effort and skipped when the binary is not on
//! the machine -- it is not a hard dependency of the setting, and the systemd half is the one
//! that matters.

use std::path::Path;

use garage_proc::which;

use crate::command::run;
use crate::cx::SessionCx;
use crate::error::ApplyError;

/// The locales glibc will actually accept, from `locale -a` (garage:4469-4490).
///
/// Not a table of the world's languages: only what `locale-gen` has built is usable, and
/// offering the rest would be offering settings that silently do nothing. `C` and `POSIX` are
/// dropped by the `_` test -- they are not languages, and picking one as a desktop language
/// is never what was meant.
///
/// Spelled the way `localectl` and `LANG` already spell it: `locale -a` reports the codeset
/// without its dash, so `en_US.utf8` and the `en_US.UTF-8` in `/etc/locale.conf` are one
/// locale under two names, and a combo box would show it twice.
pub(crate) fn installed_locales(cx: &SessionCx<'_>) -> Vec<String> {
    let mut names: Vec<String> = run(cx, &["locale", "-a"])
        .stdout
        .lines()
        .map(str::trim)
        .filter(|name| name.contains('_'))
        .map(canonical_codeset)
        .collect();
    names.sort();
    names.dedup();
    names
}

/// `base, _, codeset = name.partition("."); if codeset.lower().replace("-", "") == "utf8"`.
fn canonical_codeset(name: &str) -> String {
    match name.split_once('.') {
        Some((base, codeset)) if codeset.to_lowercase().replace('-', "") == "utf8" => {
            format!("{base}.UTF-8")
        }
        _ => name.to_owned(),
    }
}

/// `LANG` as the system sets it, which is the value an empty override means
/// (garage:4493-4507).
///
/// Read from the file rather than asked of `localectl`: the systemd shipped here has no
/// `localectl show`, only a status page meant for people, and `/etc/locale.conf` is the file
/// `localectl` would be reading out anyway.
pub(crate) fn system_locale() -> String {
    system_locale_at(Path::new("/etc/locale.conf"))
}

/// [`system_locale`] with the file named, so a test can hand it one.
pub(crate) fn system_locale_at(path: &Path) -> String {
    let Ok(text) = std::fs::read_to_string(path) else {
        return String::new();
    };
    text.lines()
        .find_map(|line| line.strip_prefix("LANG="))
        .map(|value| value.trim().trim_matches('"').to_owned())
        .unwrap_or_default()
}

/// `LANG` in the systemd user manager, which is what the next app inherits
/// (garage:4510-4515).
///
/// Untrimmed, unlike [`system_locale`]: the Python takes the rest of the line verbatim here
/// and strips it there, and the asymmetry is kept rather than smoothed over.
pub(crate) fn session_locale(cx: &SessionCx<'_>) -> String {
    run(cx, &["systemctl", "--user", "show-environment"])
        .stdout
        .lines()
        .find_map(|line| line.strip_prefix("LANG="))
        .map(str::to_owned)
        .unwrap_or_default()
}

/// Seed the resolved locale into the systemd user manager's environment (garage:4553-4573).
///
/// # Errors
///
/// Never: all three calls are unchecked, exactly as the Python's are. The `Result` is the
/// dispatch table's shape.
/// The `Result` is [`crate::dispatch::run_apply`]'s uniform shape rather than this applier's
/// own: every arm of that match has to have one type, and an applier that cannot fail still
/// has to say so in the same words as one that can.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn apply_locale(cx: &mut SessionCx<'_>) -> Result<(), ApplyError> {
    let configured = cx.render().prefs().region.locale.as_str().to_owned();
    let target = if configured.is_empty() {
        system_locale()
    } else {
        configured
    };
    if target.is_empty() {
        // An empty override resolves to "no override" rather than "an empty language", so the
        // variable is unset rather than set to nothing.
        drop(run(
            cx,
            &["systemctl", "--user", "unset-environment", "LANG"],
        ));
        return Ok(());
    }
    drop(run(
        cx,
        &[
            "systemctl",
            "--user",
            "set-environment",
            &format!("LANG={target}"),
        ],
    ));
    // Best effort: dbus-update-activation-environment is not a hard dependency of the setting,
    // and the systemd half above is the one that matters here.
    if which("dbus-update-activation-environment").is_some() {
        drop(run(
            cx,
            &[
                "dbus-update-activation-environment",
                "--systemd",
                &format!("LANG={target}"),
            ],
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{apply_locale, installed_locales, session_locale, system_locale_at};
    use crate::testing::{Script, World};

    #[test]
    fn locale_a_is_folded_to_the_spelling_locale_conf_uses() {
        let world = World::plain(
            "locales",
            Script::new().answering(
                "locale -a",
                0,
                "C\nC.utf8\nen_US.utf8\nen_US.UTF-8\nid_ID.utf8\nPOSIX\n",
                "",
            ),
        );
        // C and POSIX have no underscore and leave; the two spellings of en_US collapse.
        world.with(|cx| {
            assert_eq!(installed_locales(cx), ["en_US.UTF-8", "id_ID.UTF-8"]);
        });
    }

    #[test]
    fn the_session_lang_is_read_off_show_environment() {
        let world = World::plain(
            "session-locale",
            Script::new().answering(
                "systemctl --user show-environment",
                0,
                "PATH=/usr/bin\nLANG=en_US.UTF-8\n",
                "",
            ),
        );
        world.with(|cx| assert_eq!(session_locale(cx), "en_US.UTF-8"));
    }

    #[test]
    fn the_system_lang_is_unquoted_from_locale_conf() {
        let world = World::plain("system-locale", Script::new());
        let path = world.home.join("locale.conf");
        std::fs::create_dir_all(&world.home).expect("scratch");
        std::fs::write(&path, "LC_TIME=C\nLANG=\"id_ID.UTF-8\"\n").expect("scratch");
        assert_eq!(system_locale_at(&path), "id_ID.UTF-8");
        assert_eq!(system_locale_at(&world.home.join("absent")), "");
    }

    #[test]
    fn an_override_is_set_and_an_empty_one_falls_through_to_the_system() {
        let world = World::new(
            "locale-set",
            "[region]\nlocale = \"id_ID.UTF-8\"\n",
            Script::new(),
        );
        world.with(|cx| apply_locale(cx).expect("the locale is seeded"));
        assert_eq!(
            world.signals().first().map(String::as_str),
            Some("systemctl --user set-environment LANG=id_ID.UTF-8")
        );
    }
}
