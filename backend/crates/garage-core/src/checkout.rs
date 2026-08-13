//! Checkout and manifest discovery shared by commands that inspect the installed tree.
//!
//! Development and stowed binaries can walk up from their executable. The installed binary
//! cannot: it lives under `~/.local/lib/garage`, so the stowed shipped-defaults link is its
//! authority. Keeping both routes here prevents doctor and reconcile from quietly choosing
//! different checkouts after a move.

use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::paths::Paths;

/// Why the checkout containing the running Garage could not be found.
#[derive(Debug, Error)]
pub enum CheckoutError {
    /// The running executable could not be resolved.
    #[error("{0}")]
    Executable(String),
    /// Neither supported installed layout identified a checkout.
    #[error("{} is not inside a Garage checkout", binary.display())]
    NotFound {
        /// The executable whose ancestry was considered first.
        binary: PathBuf,
    },
}

/// Find the checkout that owns the running Garage binary.
///
/// # Errors
///
/// [`CheckoutError`] when the executable cannot be resolved or neither installed layout
/// names a directory carrying Garage's checkout marker.
pub fn checkout_root(paths: &Paths) -> Result<PathBuf, CheckoutError> {
    let binary = std::env::current_exe()
        .and_then(|path| path.canonicalize())
        .map_err(|error| CheckoutError::Executable(error.to_string()))?;
    checkout_root_from(&binary, &paths.defaults_path)
}

/// Resolve a checkout from an already-canonical executable, then from shipped defaults.
///
/// This is public so an isolated caller can prove discovery against a scratch layout without
/// replacing the process-global current executable.
///
/// # Errors
///
/// [`CheckoutError::NotFound`] when neither route names a checkout.
pub fn checkout_root_from(binary: &Path, defaults_path: &Path) -> Result<PathBuf, CheckoutError> {
    checkout_from_executable(binary)
        .or_else(|| checkout_from_defaults(defaults_path))
        .ok_or_else(|| CheckoutError::NotFound {
            binary: binary.to_path_buf(),
        })
}

/// Prefer the tracked manifest, falling back to an installed data copy.
#[must_use]
pub fn manifest_dir(paths: &Paths, root: &Path) -> PathBuf {
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

fn checkout_from_executable(binary: &Path) -> Option<PathBuf> {
    let root = binary
        .ancestors()
        .nth(4)
        .filter(|root| !root.as_os_str().is_empty())
        .unwrap_or(Path::new("/"))
        .to_path_buf();
    looks_like_checkout(&root).then_some(root)
}

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

fn looks_like_checkout(root: &Path) -> bool {
    root.join("desktop/.stow-local-ignore").is_file()
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
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

    fn plant_checkout(checkout: &Path) -> PathBuf {
        let shipped = checkout.join("desktop/.config/garage/preferences.defaults.toml");
        fs::create_dir_all(shipped.parent().expect("defaults parent")).expect("defaults parent");
        fs::write(&shipped, "[schema]\npreferences_version = 5\n").expect("shipped defaults");
        fs::write(checkout.join("desktop/.stow-local-ignore"), "").expect("checkout marker");
        shipped
    }

    #[test]
    fn installed_binary_finds_checkout_through_stowed_defaults() {
        let root = scratch("installed");
        let checkout = root.join("checkout");
        plant_checkout(&checkout);
        let defaults = root.join("home/.config/garage/preferences.defaults.toml");
        fs::create_dir_all(defaults.parent().expect("stow parent")).expect("stow parent");
        std::os::unix::fs::symlink(
            "../../../checkout/desktop/.config/garage/preferences.defaults.toml",
            &defaults,
        )
        .expect("relative stow link");

        let binary = root.join("home/.local/lib/garage/bin/garage");
        assert_eq!(checkout_root_from(&binary, &defaults).unwrap(), checkout);
        drop(fs::remove_dir_all(root));
    }

    #[test]
    fn executable_checkout_wins_over_stowed_default() {
        let root = scratch("development");
        let checkout = root.join("development");
        let other = root.join("other");
        plant_checkout(&checkout);
        let shipped = plant_checkout(&other);
        let defaults = root.join("home/.config/garage/preferences.defaults.toml");
        fs::create_dir_all(defaults.parent().expect("stow parent")).expect("stow parent");
        std::os::unix::fs::symlink(shipped, &defaults).expect("defaults link");

        let binary = checkout.join("backend/target/debug/garage");
        assert_eq!(checkout_root_from(&binary, &defaults).unwrap(), checkout);
        drop(fs::remove_dir_all(root));
    }
}
