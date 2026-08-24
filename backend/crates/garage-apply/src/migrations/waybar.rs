//! Migration 002: narrowly remove Garage-owned Waybar links.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use garage_core::paths::Paths;

use super::Outcome;
use crate::ApplyError;

const WAYBAR_FILES: [&str; 2] = ["config.jsonc", "waybar-base.css"];

pub(super) fn waybar_residue(paths: &Paths, dry_run: bool) -> Result<Outcome, ApplyError> {
    let module = paths.home.join(".local/bin/garage-waybar-module");
    let config = paths.home.join(".config/waybar");
    let remove_module = garage_module_link(paths, &module)?;
    let config_removal = inspect_config(&config)?;
    if !remove_module && config_removal.is_none() {
        return Ok(Outcome::NothingToDo);
    }

    let mut parts = Vec::new();
    let verb = if dry_run { "would remove" } else { "removed" };
    if remove_module {
        parts.push(format!("{verb} ~/.local/bin/garage-waybar-module"));
        if !dry_run {
            fs::remove_file(&module).map_err(|error| io_error("remove", &module, &error))?;
        }
    }
    if let Some(removal) = config_removal {
        parts.push(format!("{verb} ~/.config/waybar"));
        if !dry_run {
            removal.remove()?;
        }
    }
    Ok(Outcome::Changed(parts.join("; ")))
}

fn garage_module_link(paths: &Paths, path: &Path) -> Result<bool, ApplyError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(io_error("inspect", path, &error)),
    };
    if !metadata.file_type().is_symlink() {
        return Ok(false);
    }
    let target = fs::read_link(path).map_err(|error| io_error("read", path, &error))?;
    let expected = paths
        .home
        .join(".local/lib/garage/bin/garage-waybar-module");
    Ok(resolve_link(path, &target) == expected)
}

#[derive(Debug)]
enum ConfigRemoval {
    Link(PathBuf),
    Directory(PathBuf, Vec<PathBuf>),
}

impl ConfigRemoval {
    fn remove(self) -> Result<(), ApplyError> {
        match self {
            Self::Link(path) => {
                fs::remove_file(&path).map_err(|error| io_error("remove", &path, &error))
            }
            Self::Directory(path, children) => {
                for child in children {
                    fs::remove_file(&child).map_err(|error| io_error("remove", &child, &error))?;
                }
                fs::remove_dir(&path).map_err(|error| io_error("remove", &path, &error))
            }
        }
    }
}

fn inspect_config(path: &Path) -> Result<Option<ConfigRemoval>, ApplyError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(io_error("inspect", path, &error)),
    };
    if metadata.file_type().is_symlink() {
        let target = fs::read_link(path).map_err(|error| io_error("read", path, &error))?;
        return Ok(is_legacy_waybar_target(&resolve_link(path, &target))
            .then(|| ConfigRemoval::Link(path.to_owned())));
    }
    if !metadata.is_dir() {
        return Ok(None);
    }
    let mut children = Vec::new();
    for entry in fs::read_dir(path).map_err(|error| io_error("read", path, &error))? {
        let child = entry
            .map_err(|error| io_error("read", path, &error))?
            .path();
        let name = child
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        let metadata =
            fs::symlink_metadata(&child).map_err(|error| io_error("inspect", &child, &error))?;
        if !WAYBAR_FILES.contains(&name) || !metadata.file_type().is_symlink() {
            return Ok(None);
        }
        let target = fs::read_link(&child).map_err(|error| io_error("read", &child, &error))?;
        if !is_legacy_waybar_target(&resolve_link(&child, &target)) {
            return Ok(None);
        }
        children.push(child);
    }
    Ok((!children.is_empty()).then(|| ConfigRemoval::Directory(path.to_owned(), children)))
}

fn resolve_link(link: &Path, target: &Path) -> PathBuf {
    if target.is_absolute() {
        target.to_owned()
    } else {
        link.parent().unwrap_or_else(|| Path::new("/")).join(target)
    }
}

fn is_legacy_waybar_target(path: &Path) -> bool {
    path.to_string_lossy().contains("/desktop/.config/waybar/")
        || path.to_string_lossy().ends_with("/desktop/.config/waybar")
}

fn io_error(action: &str, path: &Path, error: &std::io::Error) -> ApplyError {
    ApplyError::Io(format!("{}: could not {action}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::symlink;

    use super::waybar_residue;
    use crate::migrations::Outcome;
    use crate::testing::{Script, World};

    #[test]
    fn garage_owned_links_are_removed() {
        let world = World::plain("migration-002-owned", Script::new());
        let bin = world.home.join(".local/bin");
        fs::create_dir_all(&bin).expect("bin");
        symlink(
            world
                .home
                .join(".local/lib/garage/bin/garage-waybar-module"),
            bin.join("garage-waybar-module"),
        )
        .expect("module link");
        let config = world.home.join(".config/waybar");
        fs::create_dir_all(&config).expect("config");
        symlink(
            world
                .home
                .join("checkout/desktop/.config/waybar/config.jsonc"),
            config.join("config.jsonc"),
        )
        .expect("config link");

        let outcome = waybar_residue(&world.paths, false).expect("migration succeeds");
        assert!(matches!(outcome, Outcome::Changed(_)));
        assert!(fs::symlink_metadata(bin.join("garage-waybar-module")).is_err());
        assert!(!config.exists());
    }

    #[test]
    fn user_owned_files_are_left_alone() {
        let world = World::plain("migration-002-user", Script::new());
        let config = world.home.join(".config/waybar");
        fs::create_dir_all(&config).expect("config");
        fs::write(config.join("config.jsonc"), "user config").expect("file");
        assert_eq!(
            waybar_residue(&world.paths, false).expect("migration succeeds"),
            Outcome::NothingToDo
        );
        assert!(config.join("config.jsonc").exists());
    }
}
