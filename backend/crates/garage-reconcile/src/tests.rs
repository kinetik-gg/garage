//! Scratch-tree tests for desired-state expansion and the read-only diff.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use garage_core::paths::Paths;

use crate::{desired_state, diff, Action};

static SERIAL: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
struct World {
    root: PathBuf,
    checkout: PathBuf,
    paths: Paths,
}

impl World {
    fn new(label: &str) -> Self {
        let serial = SERIAL.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "garage-reconcile-{label}-{}-{serial}",
            std::process::id()
        ));
        drop(fs::remove_dir_all(&root));
        let checkout = root.join("checkout");
        let home = root.join("home");
        fs::create_dir_all(checkout.join("desktop/.config")).expect("desktop tree");
        fs::create_dir_all(checkout.join("system/manifest")).expect("manifest tree");
        fs::create_dir_all(&home).expect("scratch HOME");
        let env: HashMap<String, String> = [("HOME".to_owned(), home.display().to_string())]
            .into_iter()
            .collect();
        Self {
            root,
            checkout,
            paths: Paths::from_env_map(&env),
        }
    }

    fn manifest(&self, packages: &str, managed: &str) {
        let dir = self.checkout.join("system/manifest");
        fs::write(dir.join("packages.list"), packages).expect("packages manifest");
        fs::write(dir.join("managed-paths.list"), managed).expect("paths manifest");
        fs::write(dir.join("units.list"), "waybar.service running\n").expect("units manifest");
    }

    fn tracked(&self, relative: &str) -> PathBuf {
        let path = self.checkout.join("desktop").join(relative);
        fs::create_dir_all(path.parent().expect("tracked parent")).expect("tracked parent");
        fs::write(&path, relative).expect("tracked file");
        path
    }

    fn home(&self, relative: &str) -> PathBuf {
        self.paths.home.join(relative)
    }
}

impl Drop for World {
    fn drop(&mut self) {
        drop(fs::remove_dir_all(&self.root));
    }
}

fn link(source: impl AsRef<Path>, target: impl AsRef<Path>) {
    let target = target.as_ref();
    fs::create_dir_all(target.parent().expect("link parent")).expect("link parent");
    std::os::unix::fs::symlink(source, target).expect("scratch symlink");
}

#[test]
fn desired_expands_stow_ignores_and_package_ownership() {
    let world = World::new("desired");
    world.tracked(".config/kept");
    world.tracked(".config/generated");
    fs::write(
        world.checkout.join("desktop/.stow-local-ignore"),
        "^/\\.config/generated$\n",
    )
    .expect("ignore file");
    world.manifest(
        "kitty\n",
        "stow-tree desktop/\ngenerated .config/kitty/theme.conf kitty\n\
         generated .config/btop/themes/vanta.theme btop\n",
    );

    let desired = desired_state(&world.paths, &world.checkout).expect("desired state");
    let names: Vec<&str> = desired
        .paths
        .iter()
        .map(|path| path.path.as_str())
        .collect();
    assert_eq!(names, [".config/kept", ".config/kitty/theme.conf"]);
    assert_eq!(desired.excluded.len(), 1);
    assert_eq!(
        desired.excluded.first().map(|path| path.owner.as_str()),
        Some("btop")
    );
    assert_eq!(
        desired.units.first().map(|unit| unit.name.as_str()),
        Some("waybar.service")
    );
}

#[test]
fn diff_uses_all_five_doctor_outcomes_and_collision_safe_backup_root() {
    let world = World::new("five-states");
    for name in ["linked", "other", "broken", "plain", "missing"] {
        world.tracked(&format!(".config/{name}"));
    }
    fs::write(world.checkout.join("desktop/.stow-local-ignore"), "").expect("ignore file");
    world.manifest("", "stow-tree desktop/\n");
    link(
        world.checkout.join("desktop/.config/linked"),
        world.home(".config/linked"),
    );
    let other = world.root.join("other/desktop/.config/other");
    fs::create_dir_all(other.parent().expect("other parent")).expect("other parent");
    fs::write(&other, "other").expect("other file");
    link(&other, world.home(".config/other"));
    link("/definitely/gone", world.home(".config/broken"));
    fs::write(world.home(".config/plain"), "mine").expect("plain conflict");
    let collision = world.home(".garage-backup/fixed/.config/plain");
    fs::create_dir_all(collision.parent().expect("collision parent")).expect("collision parent");
    fs::write(collision, "older").expect("older backup");

    let desired = desired_state(&world.paths, &world.checkout).expect("desired state");
    let plan = diff(&world.paths, &world.checkout, desired, "fixed");
    assert_eq!(plan.actual.linked, 1);
    assert_eq!(plan.actual.other, 1);
    assert_eq!(plan.actual.broken, 1);
    assert_eq!(plan.actual.plain, 1);
    assert_eq!(plan.actual.missing, 1);
    assert_eq!(plan.plan.len(), 4);
    assert!(plan
        .plan
        .iter()
        .any(|item| item.path == ".config/other" && item.action == Action::Relink));
    let backup = plan
        .plan
        .iter()
        .find(|item| item.action == Action::BackupAndLink)
        .and_then(|item| item.backup.as_deref());
    assert_eq!(
        backup,
        Some(
            world
                .home(".garage-backup/fixed-2/.config/plain")
                .to_string_lossy()
                .as_ref()
        )
    );
}
