//! `current_wallpaper()`, `monitor_names()`, `solid_wallpaper()` and `wallpaper_target()`:
//! what one appearance's wallpaper resolves to on disk, before anything is put on screen.
//!
//! A colour is an image too. hyprpaper has no colour mode, so a chosen swatch has to reach it
//! as a file: `solid_wallpaper()` renders a flat PNG named after the colour and reuses it, so
//! re-picking a swatch costs no `magick` run and every colour keeps a distinct path -- which
//! is exactly what hyprpaper's path-keyed cache needs in order to notice a change at all. It
//! is sized to the largest monitor so the image never has to be scaled up, and written
//! outside the stow tree because `~/.local/share` is not a symlink farm.
//!
//! An appearance that has never been given a picture falls back to whatever `current` already
//! points at, which keeps the desktop it has rather than blanking it. `None` is the one case
//! with nothing to fall back to either -- a first session, before any wallpaper has been
//! chosen.
//!
//! The mime check runs `file --brief --mime-type -L`. The `-L` is load-bearing: without it
//! `file` reports `inode/symlink` and every shipped wallpaper is rejected, because the
//! shipped set lives in the dotfiles repo and reaches `~/Wallpaper` as stow symlinks -- which
//! is the normal case here rather than the exotic one.

use std::path::{Path, PathBuf};
use std::time::Duration;

use garage_core::schema::enums::{Scheme, WallpaperSource};
use garage_core::schema::Preferences;

use crate::command::{json_list, run, run_within};
use crate::cx::SessionCx;
use crate::error::ApplyError;

/// `magick` is given fifteen seconds rather than the usual four (garage:3556): a first render
/// on a 6K panel is slower than a signal, and the alternative to waiting is a desktop with no
/// wallpaper.
const MAGICK_TIMEOUT: Duration = Duration::from_secs(15);

/// `current_wallpaper()` (garage:3532-3536): what the `current` symlink resolves to, or `""`.
///
/// `Path.resolve(strict=True)` raises `OSError` for a broken or absent link, which the Python
/// answers with the empty string -- and no real path can equal it, so a caller comparing
/// against this reads "nothing is on screen" for both.
pub(crate) fn current_wallpaper(paths: &garage_core::paths::Paths) -> String {
    std::fs::canonicalize(&paths.wallpaper.current)
        .map_or_else(|_| String::new(), |path| path.display().to_string())
}

/// `monitor_names()` (garage:3539-3544): every connector `hyprctl monitors` reports.
///
/// The truthiness gate is the Python's: a record whose `name` is absent or empty is dropped,
/// because a `hyprctl hyprpaper wallpaper ,path` with an empty connector would address every
/// monitor at once rather than the one this record stands for.
pub(crate) fn monitor_names(cx: &SessionCx<'_>) -> Vec<String> {
    json_list(cx, &["hyprctl", "monitors", "-j"])
        .iter()
        .filter(|monitor| monitor.is_object())
        .filter_map(|monitor| monitor.get("name").map(py_str))
        .filter(|name| !name.is_empty())
        .collect()
}

/// `str(value)` for the scalars `hyprctl` puts in these fields.
fn py_str(value: &serde_json::Value) -> String {
    value
        .as_str()
        .map_or_else(|| value.to_string(), str::to_owned)
}

/// `solid_wallpaper()` (garage:3547-3570): a flat PNG of one colour, rendered once and reused.
///
/// # Errors
///
/// [`ApplyError::Settings`] carrying `magick`'s own complaint, or `"Unable to render
/// {color}"` when it had none -- which also covers a `magick` that exited zero and wrote
/// nothing.
pub(crate) fn solid_wallpaper(cx: &SessionCx<'_>, color: &str) -> Result<PathBuf, ApplyError> {
    let directory = &cx.render().paths().wallpaper.directory;
    let name = format!("solid-{}.png", color.trim_start_matches('#').to_lowercase());
    let path = directory.join(name);
    if path.is_file() {
        return Ok(path);
    }
    let geometry = largest_geometry(cx);
    std::fs::create_dir_all(directory).map_err(|error| ApplyError::Io(error.to_string()))?;
    let result = run_within(
        cx,
        &[
            "magick",
            "-size",
            &geometry,
            &format!("xc:{color}"),
            &path.display().to_string(),
        ],
        MAGICK_TIMEOUT,
    );
    if result.status != 0 || !path.is_file() {
        let detail = result.stderr.trim();
        return Err(ApplyError::Settings(if detail.is_empty() {
            format!("Unable to render {color}")
        } else {
            detail.to_owned()
        }));
    }
    Ok(path)
}

/// `{max(width)}x{max(height)}` over every monitor with a real size, or `1920x1080`.
///
/// The maxima are taken independently, which is the Python's own `max(width for width, _ in
/// sizes)` and `max(height for _, height in sizes)`: the image is cut to cover the widest and
/// the tallest, not to match any single display's aspect.
fn largest_geometry(cx: &SessionCx<'_>) -> String {
    let sizes: Vec<(i64, i64)> = json_list(cx, &["hyprctl", "monitors", "-j"])
        .iter()
        .filter(|monitor| monitor.is_object())
        .map(|monitor| (py_int(monitor, "width"), py_int(monitor, "height")))
        .filter(|(width, height)| *width > 0 && *height > 0)
        .collect();
    let (width, height) = sizes.iter().fold((0, 0), |(widest, tallest), (w, h)| {
        (widest.max(*w), tallest.max(*h))
    });
    if sizes.is_empty() {
        return "1920x1080".to_owned();
    }
    format!("{width}x{height}")
}

/// `int(item.get(key, 0))`, truncating toward zero the way Python's `int()` does.
#[allow(clippy::cast_possible_truncation)]
fn py_int(monitor: &serde_json::Value, key: &str) -> i64 {
    monitor
        .get(key)
        .and_then(|value| {
            // `int(3.9)` is 3, and a monitor dimension past `i64` is not a resolution.
            value
                .as_i64()
                .or_else(|| value.as_f64().map(|found| found as i64))
        })
        .unwrap_or(0)
}

/// `wallpaper_target()` (garage:3611-3637): the image file one appearance resolves to.
///
/// # Errors
///
/// [`ApplyError::Settings`] with `"Wallpaper does not exist: {path}"` or `"Wallpaper is not
/// an image: {path}"`, or whatever [`solid_wallpaper`] refuses.
pub(crate) fn wallpaper_target(
    cx: &SessionCx<'_>,
    scheme: Scheme,
) -> Result<Option<PathBuf>, ApplyError> {
    let paths = cx.render().paths();
    let prefs = cx.render().prefs();
    if source_of(prefs, scheme) == WallpaperSource::Color {
        return solid_wallpaper(cx, color_of(prefs, scheme)).map(Some);
    }
    let configured = picture_of(prefs, scheme);
    let wallpaper = if configured.is_empty() {
        current_wallpaper(paths)
    } else {
        configured.to_owned()
    };
    if wallpaper.is_empty() {
        return Ok(None);
    }
    let path = expanduser(&paths.home, &wallpaper);
    if !path.is_file() {
        return Err(ApplyError::Settings(format!(
            "Wallpaper does not exist: {}",
            path.display()
        )));
    }
    let mime = run(
        cx,
        &[
            "file",
            "--brief",
            "--mime-type",
            "-L",
            &path.display().to_string(),
        ],
    )
    .stdout;
    if !mime.trim().starts_with("image/") {
        return Err(ApplyError::Settings(format!(
            "Wallpaper is not an image: {}",
            path.display()
        )));
    }
    Ok(Some(path))
}

/// `Path(value).expanduser()` for the two forms a stored wallpaper actually takes.
///
/// **Parity note, stated plainly:** `~otheruser/...` is left alone here, where Python's
/// `expanduser()` would consult the password database. Nothing writes that form -- the pane's
/// file picker hands back an absolute path and the shipped set lives under `~/Wallpaper` --
/// and reading `/etc/passwd` to resolve a path no caller produces would be a dependency
/// bought for a case that does not occur. A bare `~` and a leading `~/` are both handled,
/// which is every form the product can reach.
fn expanduser(home: &Path, value: &str) -> PathBuf {
    if value == "~" {
        return home.to_path_buf();
    }
    value
        .strip_prefix("~/")
        .map_or_else(|| PathBuf::from(value), |rest| home.join(rest))
}

fn source_of(prefs: &Preferences, scheme: Scheme) -> WallpaperSource {
    match scheme {
        Scheme::Light => prefs.appearance.wallpaper_light_source,
        Scheme::Dark => prefs.appearance.wallpaper_dark_source,
    }
}

fn color_of(prefs: &Preferences, scheme: Scheme) -> &str {
    match scheme {
        Scheme::Light => prefs.appearance.wallpaper_light_color.as_str(),
        Scheme::Dark => prefs.appearance.wallpaper_dark_color.as_str(),
    }
}

fn picture_of(prefs: &Preferences, scheme: Scheme) -> &str {
    match scheme {
        Scheme::Light => prefs.appearance.wallpaper_light.as_str(),
        Scheme::Dark => prefs.appearance.wallpaper_dark.as_str(),
    }
}
