//! [`DoctorCx`], and the small shared reads every probe is built out of.
//!
//! The context is not a [`SessionCx`](crate::cx::SessionCx): a health check moves nothing, so
//! it has no business holding a context whose whole purpose is that it can. What it does need
//! is the checkout it is reporting on -- resolved once by [`checkout_root`] rather than by
//! each probe, because three of them want it and the failure of finding it is the command's
//! rather than a check's -- somewhere to read from, and something to ask.
//!
//! The rest is the Python's own small helpers, gathered here because both [`super`] and
//! [`crate::repair`] use them: `tilde()`, `version_parts()`, the two local-time stamps, and
//! `shutil.which()`.

use std::fmt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use garage_core::manifest::{self, ManifestError, PackageEntry, UnitEntry};
use garage_core::paths::Paths;
use garage_core::traits::Runner;

use crate::error::ApplyError;

/// Everything a probe reads from, and the one thing it may ask.
pub(crate) struct DoctorCx<'a> {
    /// Where every user file lives.
    pub(crate) paths: &'a Paths,
    /// The process boundary. Every probe that shells out goes through it, so a test can
    /// answer `pacman`, `fc-list`, `systemctl` and `Hyprland` without a machine.
    pub(crate) proc: &'a dyn Runner,
    /// The dotfiles checkout this binary is part of.
    pub(crate) root: PathBuf,
    /// `<root>/desktop`, the stow package. Held rather than recomputed because
    /// [`super::stow`]'s `points_into()` is asked it once per managed path.
    pub(crate) tree: PathBuf,
    /// Where `packages.list`, `units.list` and `fonts.list` are read from. See [`super`]'s
    /// module doc; resolved by [`manifest_dir`].
    pub(crate) manifest: PathBuf,
    /// How `shutil.which(name) is not None` is answered. See [`Installed`].
    pub(crate) installed: Installed,
}

/// How "is this binary on the machine" is answered.
///
/// Two probes ask it -- `fc-list` and `luac` -- and the real answer is a property of this
/// process's `PATH`, which is global to the process and therefore not something a fixture
/// running beside other fixtures may set. Naming the answer is how a scenario pins those two
/// without touching the environment every other test in the binary shares.
#[derive(Debug, Clone)]
pub(crate) enum Installed {
    /// Ask `PATH`, which is what a real run does.
    Path,
    /// Exactly these names are installed, and nothing else is.
    #[cfg_attr(
        not(test),
        allow(
            dead_code,
            reason = "the fixtures are the only caller; production always asks PATH"
        )
    )]
    Named(Vec<String>),
}

impl Installed {
    /// Whether `name` can be found.
    pub(crate) fn has(&self, name: &str) -> bool {
        match self {
            Self::Path => which(name).is_some(),
            Self::Named(names) => names.iter().any(|known| known == name),
        }
    }
}

/// Hand-written for the same reason [`SessionCx`](crate::cx::SessionCx)'s is: a trait object
/// carries no `Debug`.
impl fmt::Debug for DoctorCx<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DoctorCx")
            .field("root", &self.root)
            .field("manifest", &self.manifest)
            .finish_non_exhaustive()
    }
}

impl<'a> DoctorCx<'a> {
    /// Resolve the checkout and the manifest directory once, for a whole run.
    ///
    /// # Errors
    ///
    /// Whatever [`checkout_root`] returns.
    pub(crate) fn new(paths: &'a Paths, proc: &'a dyn Runner) -> Result<Self, ApplyError> {
        Ok(Self::at(paths, proc, checkout_root()?))
    }

    /// The same context over a checkout named outright, which is what the fixtures drive:
    /// [`checkout_root`] reads this process's own executable, and a test binary is not inside
    /// the scratch checkout it just built.
    pub(crate) fn at(paths: &'a Paths, proc: &'a dyn Runner, root: PathBuf) -> Self {
        Self {
            tree: root.join("desktop"),
            manifest: manifest_dir(paths, &root),
            root,
            paths,
            proc,
            installed: Installed::Path,
        }
    }

    /// The package set, or the manifest error that stops the doctor naming one.
    pub(crate) fn packages(&self) -> Result<Vec<PackageEntry>, ManifestError> {
        manifest::load_packages(&self.manifest)
    }

    /// The per-user units, whole: `units.list` is the set `bootstrap.sh` enables and the set
    /// the doctor checks.
    pub(crate) fn units(&self) -> Result<Vec<UnitEntry>, ManifestError> {
        manifest::load_units(&self.manifest)
    }

    /// The fontconfig family names, in file order -- which is `DOCTOR_FONTS`' order, pinned
    /// by `tests/test_manifest.py`.
    pub(crate) fn fonts(&self) -> Result<Vec<String>, ManifestError> {
        manifest::load_fonts(&self.manifest)
    }

    /// A path as the user would type it. `tilde()`, with the home this run resolved.
    pub(crate) fn tilde(&self, path: &Path) -> String {
        tilde(&self.paths.home, path)
    }
}

/// A path as the user would type it, for report lines.
pub(crate) fn tilde(home: &Path, path: &Path) -> String {
    path.strip_prefix(home).map_or_else(
        |_| path.display().to_string(),
        |relative| format!("~/{}", relative.display()),
    )
}

/// The first three integers in a version string, for numeric comparison.
///
/// `[int(part) for part in re.findall(r"\d+", text)[:3]]`, padded to three with zeroes. A run
/// of digits too long for the type saturates rather than wrapping: any such version is
/// already far above every floor this compares against.
pub(crate) fn version_parts(text: &str) -> [u64; 3] {
    let mut parts = [0_u64; 3];
    let mut found = 0;
    let mut digits = String::new();
    for letter in text.chars().chain(std::iter::once(' ')) {
        if letter.is_ascii_digit() {
            digits.push(letter);
            continue;
        }
        if !digits.is_empty() {
            if let Some(slot) = parts.get_mut(found) {
                *slot = digits.parse().unwrap_or(u64::MAX);
            }
            found += 1;
            digits.clear();
            if found == 3 {
                break;
            }
        }
    }
    parts
}

/// The dotfiles checkout this binary is part of.
///
/// The Python resolves `__file__`, which is reached through the stow link at
/// `~/.local/bin/garage`, and walks up out of `desktop/.local/bin` -- "the only definition
/// that lets `update` restow the same tree it just pulled". The same three parents off this
/// process's own resolved executable names the same checkout from either place the binary can
/// sit: `<checkout>/desktop/.local/bin/garage` once it is stowed, and
/// `<checkout>/backend/target/<profile>/garage` while it is being built.
///
/// # Errors
///
/// [`ApplyError::Settings`] carrying the Python's own sentence when the resolved path is not
/// three levels inside something that looks like a Garage checkout.
pub(crate) fn checkout_root() -> Result<PathBuf, ApplyError> {
    let binary = std::env::current_exe()
        .and_then(|path| path.canonicalize())
        .map_err(|error| ApplyError::Settings(error.to_string()))?;
    let root = binary
        .ancestors()
        .nth(4)
        .filter(|root| !root.as_os_str().is_empty())
        .unwrap_or(Path::new("/"))
        .to_path_buf();
    if !root.join("desktop/.stow-local-ignore").is_file() {
        return Err(ApplyError::Settings(format!(
            "{} is not inside a Garage checkout",
            binary.display()
        )));
    }
    Ok(root)
}

/// Where the three manifests are read from: the checkout's own copy, then the one an install
/// could publish under `$XDG_DATA_HOME/garage/manifest`.
///
/// The checkout wins because that is the copy `bootstrap.sh` reads and the copy a pull moves.
/// The fallback exists for the shape an installed-without-a-checkout Garage would have; it is
/// deliberately second, so a running checkout never reads a stale published copy.
fn manifest_dir(paths: &Paths, root: &Path) -> PathBuf {
    let tracked = root.join("system/manifest");
    if tracked.is_dir() {
        return tracked;
    }
    paths
        .application_dirs
        .first()
        .and_then(|applications| applications.parent())
        .unwrap_or(&paths.home)
        .join("garage/manifest")
}

/// Seconds since the Unix epoch, for the two timestamps that are printed.
pub(crate) fn now_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| i64::try_from(since.as_secs()).unwrap_or(0))
}

/// `time.strftime("%Y-%m-%dT%H:%M:%S%z", time.localtime(seconds))`.
///
/// Local time with its UTC offset, which is ISO 8601 and is what a person reading their own
/// report expects to see. `tz-rs` reads `TZ` first and `/etc/localtime` after it, exactly as
/// libc does for the Python's `time.localtime()`, so both binaries resolve the same wall
/// clock on the same machine; a machine with neither falls back to UTC, which is libc's own
/// fallback.
pub(crate) fn local_iso8601(seconds: i64) -> String {
    let Ok(utc) = tz::UtcDateTime::from_timespec(seconds, 0) else {
        return String::new();
    };
    let local = tz::TimeZone::local()
        .ok()
        .and_then(|zone| utc.project(zone.as_ref()).ok());
    let (year, month, day, hour, minute, second, offset) = local.map_or_else(
        || {
            (
                utc.year(),
                utc.month(),
                utc.month_day(),
                utc.hour(),
                utc.minute(),
                utc.second(),
                0,
            )
        },
        |here| {
            (
                here.year(),
                here.month(),
                here.month_day(),
                here.hour(),
                here.minute(),
                here.second(),
                here.local_time_type().ut_offset(),
            )
        },
    );
    let sign = if offset < 0 { '-' } else { '+' };
    let minutes = offset.abs() / 60;
    format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}{sign}{:02}{:02}",
        minutes / 60,
        minutes % 60
    )
}

/// `time.strftime(BACKUP_STAMP)`: `%Y%m%d-%H%M%S` in local time, for [`crate::repair`]'s
/// backup names.
pub(crate) fn local_backup_stamp(seconds: i64) -> String {
    let stamp = local_iso8601(seconds);
    // `YYYY-MM-DDTHH:MM:SS+ZZZZ` -> `YYYYMMDD-HHMMSS`: the same fields with the separators the
    // Python's second format string leaves out, rather than a second clock read that could
    // land in the next second.
    let kept: String = stamp
        .chars()
        .take(19)
        .filter(char::is_ascii_digit)
        .collect();
    let (date, time) = kept.split_at(kept.len().min(8));
    format!("{date}-{time}")
}

/// `shutil.which()`: the first executable of that name on `PATH`, or `None`.
fn which(name: &str) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt as _;

    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).find_map(|directory| {
        let candidate = directory.join(name);
        let metadata = std::fs::metadata(&candidate).ok()?;
        (metadata.is_file() && metadata.permissions().mode() & 0o111 != 0).then_some(candidate)
    })
}
