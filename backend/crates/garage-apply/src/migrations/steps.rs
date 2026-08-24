//! The shipped machine-migration registry.

use std::ffi::OsStr;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use garage_core::paths::Paths;

use super::{Migration, Outcome};
use crate::ApplyError;

/// Every one-way transformation, in the only order in which it may run.
pub const REGISTRY: &[Migration] = &[
    Migration {
        id: "001-python-backend-residue",
        summary: "remove the deleted Python backend's bytecode residue",
        run: python_backend_residue,
    },
    Migration {
        id: "002-waybar-residue",
        summary: "remove links left by the retired Waybar surface",
        run: super::waybar::waybar_residue,
    },
];

#[derive(Debug)]
struct Target {
    path: PathBuf,
    display: &'static str,
}

#[derive(Debug)]
struct Removal {
    target: Target,
    entries: usize,
    symlink: bool,
}

fn python_backend_residue(paths: &Paths, dry_run: bool) -> Result<Outcome, ApplyError> {
    let targets = [
        Target {
            path: paths.home.join(".local/bin/__pycache__"),
            display: "~/.local/bin/__pycache__",
        },
        Target {
            path: paths.home.join(".config/waybar/__pycache__"),
            display: "~/.config/waybar/__pycache__",
        },
    ];
    let mut removals = Vec::new();
    for target in targets {
        if let Some(removal) = inspect(target)? {
            removals.push(removal);
        }
    }
    if removals.is_empty() {
        return Ok(Outcome::NothingToDo);
    }

    let detail = describe(&removals, dry_run);
    if !dry_run {
        for removal in removals {
            remove(&removal)?;
        }
    }
    Ok(Outcome::Changed(detail))
}

/// Validate every deletion boundary and count the tree without following symlinks.
///
/// The caller inspects both targets before removing either one, so a wrong-type refusal at
/// the second path cannot leave the first path half-migrated and the migration unstamped.
fn inspect(target: Target) -> Result<Option<Removal>, ApplyError> {
    if target.path.file_name() != Some(OsStr::new("__pycache__")) {
        return Err(ApplyError::Settings(format!(
            "refusing to remove {}: final component is not __pycache__",
            target.path.display()
        )));
    }
    let metadata = match fs::symlink_metadata(&target.path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(io_error("inspect", &target.path, &error)),
    };
    let file_type = metadata.file_type();
    if !file_type.is_dir() && !file_type.is_symlink() {
        return Err(ApplyError::Settings(format!(
            "refusing to remove {}: expected a directory or symlink",
            target.display
        )));
    }
    let symlink = file_type.is_symlink();
    let entries = if symlink {
        1
    } else {
        count_entries(&target.path)?
    };
    Ok(Some(Removal {
        target,
        entries,
        symlink,
    }))
}

fn count_entries(path: &Path) -> Result<usize, ApplyError> {
    let children = fs::read_dir(path).map_err(|error| io_error("read", path, &error))?;
    let mut count = 1;
    for child in children {
        let child = child.map_err(|error| io_error("read", path, &error))?;
        let child_path = child.path();
        let metadata = fs::symlink_metadata(&child_path)
            .map_err(|error| io_error("inspect", &child_path, &error))?;
        count += if metadata.file_type().is_dir() {
            count_entries(&child_path)?
        } else {
            1
        };
    }
    Ok(count)
}

fn describe(removals: &[Removal], dry_run: bool) -> String {
    let verb = if dry_run { "would remove" } else { "removed" };
    removals
        .iter()
        .map(|removal| {
            let noun = if removal.entries == 1 {
                "entry"
            } else {
                "entries"
            };
            format!(
                "{verb} {} ({} {noun})",
                removal.target.display, removal.entries
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn remove(removal: &Removal) -> Result<(), ApplyError> {
    let result = if removal.symlink {
        fs::remove_file(&removal.target.path)
    } else {
        fs::remove_dir_all(&removal.target.path)
    };
    result.map_err(|error| io_error("remove", &removal.target.path, &error))
}

fn io_error(action: &str, path: &Path, error: &std::io::Error) -> ApplyError {
    ApplyError::Io(format!("{}: could not {action}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::symlink;
    use std::path::{Path, PathBuf};

    use super::{inspect, Target};
    use crate::migrations::{run_migrations, Outcome, Status, REGISTRY};
    use crate::testing::{Script, World};

    #[test]
    fn bytecode_at_both_paths_is_removed_and_stamped_changed() {
        let world = World::plain("migration-001-both", Script::new());
        seed(&local_cache(&world), "command.cpython-314.pyc");
        seed(&waybar_cache(&world), "media-status.cpython-314.pyc");

        let report = run_migrations(&world.paths, REGISTRY, false);

        assert!(matches!(
            report.entries.first().map(|entry| &entry.status),
            Some(Status::Applied(Outcome::Changed(detail)))
                if detail == "removed ~/.local/bin/__pycache__ (2 entries); removed ~/.config/waybar/__pycache__ (2 entries)"
        ));
        assert_absent(&local_cache(&world));
        assert_absent(&waybar_cache(&world));
        let stamp = fs::read_to_string(&world.paths.migrations).expect("migration stamp");
        assert!(stamp.contains("\"id\": \"001-python-backend-residue\""));
        assert!(stamp.contains("\"kind\": \"changed\""));
    }

    #[test]
    fn empty_home_is_stamped_nothing_to_do() {
        let world = World::plain("migration-001-empty", Script::new());

        let report = run_migrations(&world.paths, REGISTRY, false);

        assert!(matches!(
            report.entries.first().map(|entry| &entry.status),
            Some(Status::Applied(Outcome::NothingToDo))
        ));
        let stamp = fs::read_to_string(&world.paths.migrations).expect("migration stamp");
        assert!(stamp.contains("\"id\": \"001-python-backend-residue\""));
        assert!(stamp.contains("\"kind\": \"nothing-to-do\""));
    }

    #[test]
    fn symlinked_cache_is_unlinked_without_touching_its_target() {
        let world = World::plain("migration-001-symlink", Script::new());
        let decoy = world.home.join("decoy");
        fs::create_dir_all(&decoy).expect("decoy directory");
        fs::write(decoy.join("keep.pyc"), "keep").expect("decoy bytecode");
        let cache = waybar_cache(&world);
        fs::create_dir_all(cache.parent().expect("cache parent")).expect("Waybar directory");
        symlink(&decoy, &cache).expect("cache symlink");

        let report = run_migrations(&world.paths, REGISTRY, false);

        assert!(matches!(
            report.entries.first().map(|entry| &entry.status),
            Some(Status::Applied(Outcome::Changed(detail)))
                if detail == "removed ~/.config/waybar/__pycache__ (1 entry)"
        ));
        assert_absent(&cache);
        assert_eq!(
            fs::read_to_string(decoy.join("keep.pyc")).expect("decoy survives"),
            "keep"
        );
    }

    #[test]
    fn file_at_cache_path_is_refused_unstamped_and_retried() {
        let world = World::plain("migration-001-file", Script::new());
        let valid = local_cache(&world);
        seed(&valid, "keep-until-preflight-completes.pyc");
        let refused = waybar_cache(&world);
        fs::create_dir_all(refused.parent().expect("cache parent")).expect("Waybar directory");
        fs::write(&refused, "not a directory").expect("wrong-type cache");

        for _ in 0..2 {
            let report = run_migrations(&world.paths, REGISTRY, false);
            assert!(matches!(
                report.entries.first().map(|entry| &entry.status),
                Some(Status::Failed(detail))
                    if detail == "refusing to remove ~/.config/waybar/__pycache__: expected a directory or symlink"
            ));
            let stamp = fs::read_to_string(&world.paths.migrations)
                .expect("the independent 002 migration is stamped");
            assert!(
                !stamp.contains("\"id\": \"001-python-backend-residue\""),
                "the refused 001 migration stays eligible for the next loop"
            );
        }
        assert!(
            valid.is_dir(),
            "preflight refused before removing the other cache"
        );
        assert_eq!(
            fs::read_to_string(refused).expect("refused file survives"),
            "not a directory"
        );
    }

    #[test]
    fn dry_run_describes_both_trees_and_leaves_home_identical() {
        let world = World::plain("migration-001-dry", Script::new());
        seed(&local_cache(&world), "command.cpython-314.pyc");
        seed(&waybar_cache(&world), "media-status.cpython-314.pyc");
        let before = snapshot(&world.home);

        let report = run_migrations(&world.paths, REGISTRY, true);

        assert!(matches!(
            report.entries.first().map(|entry| &entry.status),
            Some(Status::DryRun(Outcome::Changed(detail)))
                if detail == "would remove ~/.local/bin/__pycache__ (2 entries); would remove ~/.config/waybar/__pycache__ (2 entries)"
        ));
        assert_eq!(snapshot(&world.home), before);
        assert!(!world.paths.migrations.exists());
    }

    #[test]
    fn target_guard_requires_a_literal_pycache_final_component() {
        let world = World::plain("migration-001-boundary", Script::new());
        let path = world.home.join("not-pycache");
        fs::create_dir_all(&path).expect("wrongly named directory");

        let error = inspect(Target {
            path: path.clone(),
            display: "~/not-pycache",
        })
        .expect_err("wrong final component is refused");

        assert!(error
            .to_string()
            .contains("final component is not __pycache__"));
        assert!(path.is_dir());
    }

    fn local_cache(world: &World) -> PathBuf {
        world.home.join(".local/bin/__pycache__")
    }

    fn waybar_cache(world: &World) -> PathBuf {
        world.home.join(".config/waybar/__pycache__")
    }

    fn seed(cache: &Path, name: &str) {
        fs::create_dir_all(cache).expect("cache directory");
        fs::write(cache.join(name), "bytecode").expect("fake bytecode");
    }

    fn assert_absent(path: &Path) {
        assert_eq!(
            fs::symlink_metadata(path)
                .expect_err("path is absent")
                .kind(),
            std::io::ErrorKind::NotFound
        );
    }

    fn snapshot(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
        let mut found = Vec::new();
        snapshot_at(root, root, &mut found);
        found.sort();
        found
    }

    fn snapshot_at(root: &Path, here: &Path, found: &mut Vec<(PathBuf, Vec<u8>)>) {
        let Ok(children) = fs::read_dir(here) else {
            return;
        };
        for child in children {
            let child = child.expect("snapshot entry");
            let path = child.path();
            let relative = path.strip_prefix(root).expect("path under snapshot root");
            let file_type = child.file_type().expect("snapshot file type");
            if file_type.is_dir() {
                found.push((relative.to_owned(), b"directory".to_vec()));
                snapshot_at(root, &path, found);
            } else {
                found.push((relative.to_owned(), fs::read(&path).expect("snapshot file")));
            }
        }
    }
}
