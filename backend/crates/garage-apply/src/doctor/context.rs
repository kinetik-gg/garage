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

use garage_core::manifest::{self, ManifestError, PackageEntry, UnitEntry};
use garage_core::paths::Paths;
pub(crate) use garage_core::time::{local_backup_stamp, local_iso8601, now_seconds};
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
    garage_core::checkout::checkout_root(paths)
        .map_err(|error| ApplyError::Settings(error.to_string()))
}

/// Where the three manifests are read from: the checkout's own copy, then the one an install
/// could publish under `$XDG_DATA_HOME/garage/manifest`.
///
/// The checkout wins because that is the copy `bootstrap.sh` reads and the copy a pull moves.
/// The fallback exists for the shape an installed-without-a-checkout Garage would have; it is
/// deliberately second, so a running checkout never reads a stale published copy.
fn manifest_dir(paths: &Paths, root: &Path) -> PathBuf {
    garage_core::checkout::manifest_dir(paths, root)
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
