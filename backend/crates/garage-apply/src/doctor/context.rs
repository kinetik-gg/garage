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
        Ok(Self::at(paths, proc, checkout_root(paths)?))
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

/// The dotfiles checkout this binary belongs to, across both published layouts.
///
/// The stowed and development layouts resolve from the executable first:
/// `<checkout>/desktop/.local/bin/garage` and `<checkout>/backend/target/<profile>/garage`
/// both name the checkout after four ancestors. This path has to win because bootstrap runs
/// the freshly built doctor's executable before any configuration link has to exist.
///
/// An installed binary instead lives at `~/.local/lib/garage/bin/garage`, whose fourth
/// ancestor is `~/.local`. Once that candidate fails the checkout marker, the shipped
/// defaults link at [`Paths::defaults_path`] identifies the checkout that is stowed onto this
/// machine: resolve it, require the exact `desktop/.config/garage/preferences.defaults.toml`
/// suffix, and walk back to the same root. The Python never needs this second route because
/// it runs the original script through `~/.local/bin/garage`; resolving `__file__` follows
/// that stow link straight back into `<checkout>/desktop/.local/bin`.
///
/// # Errors
///
/// [`ApplyError::Settings`] carrying the Python's own sentence when neither the executable
/// nor the stowed defaults link identifies something that looks like a Garage checkout.
pub(crate) fn checkout_root(paths: &Paths) -> Result<PathBuf, ApplyError> {
    let binary = std::env::current_exe()
        .and_then(|path| path.canonicalize())
        .map_err(|error| ApplyError::Settings(error.to_string()))?;
    checkout_root_from(&binary, &paths.defaults_path)
}

/// Resolve a checkout from an already-canonical executable, then from the stowed defaults.
fn checkout_root_from(binary: &Path, defaults_path: &Path) -> Result<PathBuf, ApplyError> {
    checkout_from_executable(binary)
        .or_else(|| checkout_from_defaults(defaults_path))
        .ok_or_else(|| {
            ApplyError::Settings(format!(
                "{} is not inside a Garage checkout",
                binary.display()
            ))
        })
}

/// The original route, kept first so a checkout can bootstrap before it has been stowed.
fn checkout_from_executable(binary: &Path) -> Option<PathBuf> {
    let root = binary
        .ancestors()
        .nth(4)
        .filter(|root| !root.as_os_str().is_empty())
        .unwrap_or(Path::new("/"))
        .to_path_buf();
    looks_like_checkout(&root).then_some(root)
}

/// The installed route: only the stow link to the checkout's shipped defaults is authority.
fn checkout_from_defaults(defaults_path: &Path) -> Option<PathBuf> {
    let metadata = std::fs::symlink_metadata(defaults_path).ok()?;
    if !metadata.file_type().is_symlink() {
        return None;
    }
    let defaults = defaults_path.canonicalize().ok()?;
    if !defaults.ends_with("desktop/.config/garage/preferences.defaults.toml") {
        return None;
    }
    let root = defaults.ancestors().nth(4)?.to_path_buf();
    looks_like_checkout(&root).then_some(root)
}

/// The marker both the Rust and Python discovery paths use.
fn looks_like_checkout(root: &Path) -> bool {
    root.join("desktop/.stow-local-ignore").is_file()
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

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::checkout_root_from;

    static SERIAL: AtomicU64 = AtomicU64::new(0);

    fn scratch(label: &str) -> PathBuf {
        let serial = SERIAL.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "garage-checkout-discovery-{label}-{}-{serial}",
            std::process::id()
        ));
        drop(fs::remove_dir_all(&root));
        fs::create_dir_all(&root).expect("scratch directory");
        root
    }

    fn plant_checkout(checkout: &std::path::Path) -> PathBuf {
        let shipped = checkout.join("desktop/.config/garage/preferences.defaults.toml");
        fs::create_dir_all(shipped.parent().expect("defaults parent")).expect("defaults parent");
        fs::write(&shipped, "[schema]\npreferences_version = 5\n").expect("shipped defaults");
        fs::write(checkout.join("desktop/.stow-local-ignore"), "").expect("checkout marker");
        shipped
    }

    #[test]
    fn an_installed_binary_finds_the_checkout_through_the_stowed_defaults() {
        let scratch = scratch("installed");
        let checkout = scratch.join("checkout");
        plant_checkout(&checkout);

        let defaults = scratch.join("home/.config/garage/preferences.defaults.toml");
        fs::create_dir_all(defaults.parent().expect("stow parent")).expect("stow parent");
        std::os::unix::fs::symlink(
            "../../../checkout/desktop/.config/garage/preferences.defaults.toml",
            &defaults,
        )
        .expect("relative stow link");
        let binary = scratch.join("home/.local/lib/garage/bin/garage");

        assert_eq!(
            checkout_root_from(&binary, &defaults).expect("checkout through defaults"),
            checkout.canonicalize().expect("canonical checkout")
        );
        drop(fs::remove_dir_all(scratch));
    }

    #[test]
    fn executable_ancestry_wins_over_a_stowed_link_to_another_checkout() {
        let scratch = scratch("executable-first");
        let executable_checkout = scratch.join("executable-checkout");
        plant_checkout(&executable_checkout);
        let linked_checkout = scratch.join("linked-checkout");
        let shipped = plant_checkout(&linked_checkout);
        let defaults = scratch.join("home/.config/garage/preferences.defaults.toml");
        fs::create_dir_all(defaults.parent().expect("stow parent")).expect("stow parent");
        std::os::unix::fs::symlink(shipped, &defaults).expect("defaults link");
        let binary = executable_checkout.join("backend/target/debug/garage");

        assert_eq!(
            checkout_root_from(&binary, &defaults).expect("checkout through executable"),
            executable_checkout
        );
        drop(fs::remove_dir_all(scratch));
    }

    #[test]
    fn discovery_keeps_the_existing_error_when_neither_route_finds_a_checkout() {
        let scratch = scratch("missing");
        let defaults = scratch.join("home/.config/garage/preferences.defaults.toml");
        fs::create_dir_all(defaults.parent().expect("defaults parent")).expect("defaults parent");
        fs::write(&defaults, "not a stow link\n").expect("plain defaults");
        let binary = scratch.join("home/.local/lib/garage/bin/garage");

        let error = checkout_root_from(&binary, &defaults).expect_err("discovery must fail");
        assert_eq!(
            error.to_string(),
            format!("{} is not inside a Garage checkout", binary.display())
        );
        drop(fs::remove_dir_all(scratch));
    }
}
