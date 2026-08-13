//! `plugin_state()`: the live ABI comparison `garage-rebuild-plugins --check` makes,
//! reimplemented here rather than shelled out to.
//!
//! A deliberate choice, not a duplication for its own sake. `--check` is built to run
//! unattended from a login unit: it always exits `0` whether or not the plugins are stale,
//! since a red unit in the session is worse than a quiet one, and when they are stale it
//! raises a sticky desktop notification as a side effect. Neither behaviour belongs in a
//! read-only health check or in `update`'s skip decision -- one gives no answer worth
//! interpreting, the other paints the user's screen. What the check actually computes is four
//! `readlink`s and a string compare, all read-only and none of them worth a subprocess to
//! reach.
//!
//! "Never deployed here" is kept apart from "stale": Kinetik Glass's source is not published,
//! so most machines have no plugins at all and are not behind on anything, and folding that
//! case into "stale" would make every ordinary machine's report look broken.
//!
//! "Behind" is a second, separate condition from "stale": the deployed build loads fine, it
//! is just not the commit `system/plugin-pins` names -- what a local-only checkout looks like
//! after a pin bump with no pull for `update` to notice moving in.
//!
//! Everything here reads deployed plugin symlinks and the pins file and returns a state
//! value, not `Result<(), ApplyError>` over a [`SessionCx`](crate::cx::SessionCx).

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::time::Duration;

use super::stow::link_hop;
use super::{DoctorCx, PLUGIN_NAMES};

/// `Hyprland --version` output, or `""` when the binary is not installed.
///
/// Read from the binary rather than from `hyprctl`, so it answers on a TTY with no compositor
/// running -- which is where a `garage doctor` after a failed login is run.
pub(crate) fn hyprland_report(cx: &DoctorCx<'_>) -> String {
    cx.proc
        .run(&["Hyprland", "--version"], Duration::from_secs(10))
        .map_or_else(
            |_| String::new(),
            |probe| {
                if probe.status == 0 {
                    probe.stdout
                } else {
                    String::new()
                }
            },
        )
}

/// The version out of a `Hyprland --version` report, or `""`.
///
/// `^Hyprland\s+v?(\d+\.\d+(?:\.\d+)?)` first, then `^Tag:\s*v?(...)`, both multiline. Written
/// out rather than compiled because the two shapes are fixed and the input is one short
/// report.
pub(crate) fn hyprland_version(report: &str) -> String {
    let named = report.lines().find_map(|line| {
        let rest = line.strip_prefix("Hyprland")?;
        let trimmed = rest.trim_start();
        // `\s+`, so at least one space has to have been eaten.
        (trimmed.len() < rest.len()).then(|| dotted_version(trimmed))?
    });
    let tagged = || {
        report
            .lines()
            .find_map(|line| dotted_version(line.strip_prefix("Tag:")?.trim_start()))
    };
    named.or_else(tagged).unwrap_or_default()
}

/// `v?(\d+\.\d+(?:\.\d+)?)` anchored at the start of `text`. `None` when it does not match,
/// which for this pattern means "fewer than two dotted numbers".
fn dotted_version(text: &str) -> Option<String> {
    let mut rest = text.strip_prefix('v').unwrap_or(text);
    let mut groups: Vec<&str> = Vec::new();
    while groups.len() < 3 {
        let digits = rest.len()
            - rest
                .trim_start_matches(|letter: char| letter.is_ascii_digit())
                .len();
        if digits == 0 {
            break;
        }
        groups.push(rest.get(..digits)?);
        rest = rest.get(digits..)?;
        match rest.strip_prefix('.') {
            Some(next) => rest = next,
            None => break,
        }
    }
    (groups.len() >= 2).then(|| groups.join("."))
}

/// `^Version ABI string:\s*(\S+)`, or `""`.
pub(crate) fn hyprland_abi(report: &str) -> String {
    report
        .lines()
        .find_map(|line| line.strip_prefix("Version ABI string:"))
        .and_then(|rest| rest.split_whitespace().next())
        .unwrap_or_default()
        .to_owned()
}

/// Which ABI directory a stable plugin symlink resolves into, or `""`.
///
/// One hop only, like `garage-rebuild-plugins`' own `deployed_abi()`: the link is absolute
/// and resolving further would follow the `.so` out of the tree.
fn deployed_plugin_abi(plugin_root: &Path, name: &str) -> String {
    let Some(hop) = link_hop(&plugin_root.join(format!("{name}.so"))) else {
        return String::new();
    };
    if hop.extension().is_none_or(|suffix| suffix != "so")
        || hop.parent().and_then(Path::parent) != Some(plugin_root)
    {
        return String::new();
    }
    hop.parent()
        .and_then(Path::file_name)
        .map(|abi| abi.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// The commit each plugin should be deployed at, from `system/plugin-pins`.
///
/// The checkout's copy, not `/usr/lib/kinetik/plugin-pins`: the installed copy is republished
/// from this one at deploy time, so after a pull it is exactly the file that can be behind.
pub(crate) fn pinned_plugins(root: &Path) -> BTreeMap<String, String> {
    let mut pins = BTreeMap::new();
    let Ok(text) = fs::read_to_string(root.join("system/plugin-pins")) else {
        return pins;
    };
    for line in text.lines() {
        let (key, value) = line.split_once('=').unwrap_or((line, ""));
        let name = match key.trim() {
            "glass_pin" => "kinetik-glass",
            "hyprexpo_pin" => "hyprexpo",
            _ => continue,
        };
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        pins.insert(name.to_owned(), value.trim_matches('"').to_owned());
    }
    pins
}

/// The commit baked into a deployed plugin's file name, or `""`.
///
/// `garage-rebuild-plugins` installs to `<abi>/<name>-<commit>.so`, so the pin a build was
/// made from is readable without opening the file.
fn deployed_plugin_pin(plugin_root: &Path, name: &str) -> String {
    let Some(hop) = link_hop(&plugin_root.join(format!("{name}.so"))) else {
        return String::new();
    };
    if hop.extension().is_none_or(|suffix| suffix != "so") {
        return String::new();
    }
    let file = hop
        .file_name()
        .map(|file| file.to_string_lossy().into_owned())
        .unwrap_or_default();
    let stem = file.strip_prefix(&format!("{name}-")).unwrap_or(&file);
    stem.strip_suffix(".so").unwrap_or(stem).to_owned()
}

/// The live ABI comparison, as one value. See the module doc for what each field means.
#[derive(Debug, Clone, Default)]
pub(crate) struct PluginState {
    /// The running compositor's ABI string, or `""` when it reports none.
    pub(crate) abi: String,
    /// Whether anything has ever been deployed under the plugin root.
    pub(crate) ever: bool,
    /// Plugins not built for the running ABI, or not deployed at all. Empty when there is no
    /// ABI to compare against.
    pub(crate) stale: Vec<String>,
    /// Plugins that load fine but were built from a commit the pins file no longer names.
    pub(crate) behind: Vec<String>,
}

/// `plugin_state(report)`.
pub(crate) fn plugin_state(cx: &DoctorCx<'_>, report: &str) -> PluginState {
    let plugin_root = &cx.paths.plugin_root;
    let abi = hyprland_abi(report);
    let ever = any_deployed_object(plugin_root);
    let mut stale: Vec<String> = Vec::new();
    for name in PLUGIN_NAMES {
        if deployed_plugin_abi(plugin_root, name) != abi
            || !plugin_root.join(format!("{name}.so")).exists()
        {
            stale.push(name.to_owned());
        }
    }
    // Behind is different from stale: the deployed build loads fine, it is just not the
    // commit the pins file names -- what a local-only checkout looks like after a pin bump,
    // where no pull exists for update to notice the move in.
    let pins = pinned_plugins(&cx.root);
    let mut behind: Vec<String> = Vec::new();
    for name in PLUGIN_NAMES {
        let deployed_pin = deployed_plugin_pin(plugin_root, name);
        let pinned = pins.get(name).filter(|pin| !pin.is_empty());
        if !stale.iter().any(|entry| entry == name)
            && pinned.is_some()
            && !deployed_pin.is_empty()
            && pinned != Some(&deployed_pin)
        {
            behind.push(name.to_owned());
        }
    }
    let has_abi = !abi.is_empty();
    PluginState {
        abi,
        ever,
        stale: if has_abi { stale } else { Vec::new() },
        behind: if has_abi { behind } else { Vec::new() },
    }
}

/// `any(PLUGIN_ROOT.glob("*/*.so"))`: has anything ever been deployed here.
fn any_deployed_object(plugin_root: &Path) -> bool {
    let Ok(entries) = fs::read_dir(plugin_root) else {
        return false;
    };
    entries.flatten().any(|entry| {
        fs::read_dir(entry.path()).is_ok_and(|inner| {
            inner.flatten().any(|object| {
                object
                    .path()
                    .extension()
                    .is_some_and(|suffix| suffix == "so")
            })
        })
    })
}

#[cfg(test)]
mod tests {
    use super::{hyprland_abi, hyprland_version};

    /// The shape `Hyprland --version` actually prints, trimmed to the two lines that are
    /// read.
    const REPORT: &str = "Hyprland 0.51.1 built from branch  at commit deadbeef\n\
                          Date: Mon Jan 1 00:00:00 2026\n\
                          Tag: v0.51.1, commits: 5678\n\
                          \n\
                          flags: (if any)\n\
                          Version ABI string: v0.51.1_abi\n";

    #[test]
    fn the_version_comes_off_the_first_line() {
        assert_eq!(hyprland_version(REPORT), "0.51.1");
        assert_eq!(hyprland_version("Hyprland v0.56.0\n"), "0.56.0");
        assert_eq!(hyprland_version("Hyprland 0.56\n"), "0.56");
    }

    #[test]
    fn the_tag_line_answers_when_the_first_one_does_not() {
        assert_eq!(hyprland_version("Tag: v0.49.0, commits: 1\n"), "0.49.0");
        assert_eq!(hyprland_version("nothing here\n"), "");
    }

    #[test]
    fn the_abi_string_is_the_first_field_after_the_label() {
        assert_eq!(hyprland_abi(REPORT), "v0.51.1_abi");
        assert_eq!(hyprland_abi("no abi here\n"), "");
    }
}
