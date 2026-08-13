//! Scratch-tree tests for desired-state expansion and the read-only diff.

use std::collections::{hash_map::DefaultHasher, HashMap};
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use garage_core::paths::Paths;

use crate::{desired_state, diff, reconcile_at, Action, Options, RunTime};

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

    fn run(&self, options: Options) -> crate::Report {
        reconcile_at(
            &self.paths,
            &self.checkout,
            options,
            &RunTime::fixed("2026-08-14T12:34:56+0700", "fixed"),
        )
        .expect("scratch reconcile")
    }

    fn ledger(&self, path: &str, owner: Option<&str>) {
        let ledger = self.paths.state_root.join("manifest.json");
        fs::create_dir_all(ledger.parent().expect("ledger parent")).expect("ledger parent");
        let owner = owner.map_or_else(|| "null".to_owned(), |name| format!("\"{name}\""));
        fs::write(
            ledger,
            format!(
                "{{\"version\":1,\"paths\":[{{\"path\":\"{path}\",\"kind\":\"generated\",\
                 \"owner\":{owner},\"timestamp\":\"older\"}}]}}\n"
            ),
        )
        .expect("seed ledger");
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
    let world = five_state_world("five-state-diff");

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

#[test]
fn every_doctor_stow_outcome_converges_to_this_checkout() {
    let world = five_state_world("five-state-converge");
    let report = world.run(Options::default());
    assert_eq!(report.applied, 4);
    for name in ["linked", "other", "broken", "plain", "missing"] {
        let target = world.home(&format!(".config/{name}"));
        assert!(target.is_symlink(), "{name} did not converge to a link");
        assert!(garage_core::stow::points_into(
            &target,
            &world.checkout.join("desktop")
        ));
    }
    assert_eq!(
        fs::read_to_string(world.home(".garage-backup/fixed-2/.config/plain"))
            .expect("plain backup"),
        "mine"
    );
    let ledger =
        fs::read_to_string(world.paths.state_root.join("manifest.json")).expect("install ledger");
    assert_eq!(ledger.matches("\"path\"").count(), 4);
}

fn five_state_world(label: &str) -> World {
    let world = World::new(label);
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
    world
}

#[test]
fn prune_refuses_an_unledgered_non_checkout_path() {
    let world = World::new("prune-refusal");
    world.manifest("", "generated .config/btop/themes/vanta.theme btop\n");
    let target = world.home(".config/btop/themes/vanta.theme");
    fs::create_dir_all(target.parent().expect("target parent")).expect("target parent");
    fs::write(&target, "user file").expect("unledgered path");

    let report = world.run(Options {
        prune: true,
        dry_run: false,
    });

    assert!(target.exists());
    assert!(report.plan.is_empty());
    assert_eq!(report.refused.len(), 1);
    assert!(!world.paths.state_root.join("reconcile.log").exists());
}

#[test]
fn prune_removes_a_ledgered_path_after_its_owner_leaves_and_logs_it() {
    let world = World::new("prune-ledger");
    let relative = ".config/btop/themes/vanta.theme";
    world.manifest("", &format!("generated {relative} btop\n"));
    let target = world.home(relative);
    fs::create_dir_all(target.parent().expect("target parent")).expect("target parent");
    fs::write(&target, "generated").expect("managed path");
    world.ledger(relative, Some("btop"));

    let report = world.run(Options {
        prune: true,
        dry_run: false,
    });

    assert!(!target.exists());
    assert_eq!(report.applied, 1);
    assert_eq!(
        report.plan.first().map(|item| item.action),
        Some(Action::Prune)
    );
    let log = fs::read_to_string(world.paths.state_root.join("reconcile.log")).expect("prune log");
    assert!(log.contains(relative));
    assert!(log.contains("owner btop removed from packages.list"));
    assert!(log.contains("2026-08-14T12:34:56+0700"));

    let ledger_path = world.paths.state_root.join("manifest.json");
    let ledger = fs::read(&ledger_path).expect("ledger after prune");
    let log_path = world.paths.state_root.join("reconcile.log");
    let log = fs::read(&log_path).expect("log after prune");
    let second = world.run(Options {
        prune: true,
        dry_run: false,
    });

    assert!(second.plan.is_empty());
    assert_eq!(second.applied, 0);
    assert_eq!(fs::read(ledger_path).expect("unchanged ledger"), ledger);
    assert_eq!(fs::read(log_path).expect("unchanged log"), log);
}

#[test]
fn prune_removes_an_unledgered_link_into_this_checkout() {
    let world = World::new("prune-checkout-link");
    fs::write(world.checkout.join("desktop/.stow-local-ignore"), "").expect("ignore file");
    world.manifest("", "stow-tree desktop/\n");
    let relative = ".config/obsolete";
    let target = world.home(relative);
    link(world.checkout.join("desktop/.config/obsolete"), &target);

    let report = world.run(Options {
        prune: true,
        dry_run: false,
    });

    assert!(!target.is_symlink());
    assert_eq!(report.applied, 1);
    assert_eq!(
        report.plan.first().map(|item| item.action),
        Some(Action::Prune)
    );
    let log = fs::read_to_string(world.paths.state_root.join("reconcile.log"))
        .expect("checkout-link prune log");
    assert!(log.contains("path removed from managed-paths.list"));
}

#[test]
fn dry_run_leaves_a_zero_filesystem_delta_even_with_converge_and_prune_work() {
    let world = World::new("dry-digest");
    world.tracked(".config/missing");
    fs::write(world.checkout.join("desktop/.stow-local-ignore"), "").expect("ignore file");
    world.manifest(
        "",
        "stow-tree desktop/\ngenerated .config/btop/themes/vanta.theme btop\n",
    );
    let obsolete = world.home(".config/btop/themes/vanta.theme");
    fs::create_dir_all(obsolete.parent().expect("obsolete parent")).expect("obsolete parent");
    fs::write(&obsolete, "old").expect("obsolete path");
    world.ledger(".config/btop/themes/vanta.theme", Some("btop"));
    let before = tree_digest(&world.root);

    let report = world.run(Options {
        dry_run: true,
        prune: true,
    });

    assert_eq!(report.plan.len(), 2);
    assert_eq!(report.applied, 0);
    assert_eq!(tree_digest(&world.root), before);
}

#[test]
fn a_second_run_has_an_empty_plan_and_empty_log_delta() {
    let world = World::new("idempotent");
    world.tracked(".config/once");
    fs::write(world.checkout.join("desktop/.stow-local-ignore"), "").expect("ignore file");
    world.manifest("", "stow-tree desktop/\n");

    let first = world.run(Options::default());
    assert_eq!(first.applied, 1);
    let ledger_path = world.paths.state_root.join("manifest.json");
    let ledger_before = fs::read(&ledger_path).expect("ledger after first run");
    let log_path = world.paths.state_root.join("reconcile.log");
    let log_before = fs::read(&log_path).unwrap_or_default();

    let second = world.run(Options::default());

    assert!(second.plan.is_empty());
    assert_eq!(second.applied, 0);
    assert_eq!(
        fs::read(ledger_path).expect("unchanged ledger"),
        ledger_before
    );
    assert_eq!(fs::read(log_path).unwrap_or_default(), log_before);
}

#[test]
fn deleting_the_hypr_tree_is_repaired_and_the_shared_doctor_model_is_green() {
    let world = World::new("deleted-hypr-tree");
    for relative in [
        ".config/hypr/hyprland.lua",
        ".config/hypr/config/autostart.lua",
    ] {
        world.tracked(relative);
    }
    fs::write(world.checkout.join("desktop/.stow-local-ignore"), "").expect("ignore file");
    world.manifest("", "stow-tree desktop/\n");

    assert_eq!(world.run(Options::default()).applied, 2);
    fs::remove_dir_all(world.home(".config/hypr")).expect("delete scratch Hypr tree");

    let repaired = world.run(Options::default());
    assert_eq!(repaired.applied, 2);
    let state = garage_core::stow::stow_state(&world.checkout, &world.paths.home);
    assert_eq!(state.total, 2);
    assert_eq!(state.linked, 2);
    assert!(state.other.is_empty());
    assert!(state.broken.is_empty());
    assert!(state.plain.is_empty());
    assert!(state.missing.is_empty());
    assert!(world.run(Options::default()).plan.is_empty());
}

fn tree_digest(root: &Path) -> u64 {
    let mut entries = Vec::new();
    collect_tree(root, root, &mut entries);
    entries.sort();
    let mut digest = DefaultHasher::new();
    entries.hash(&mut digest);
    digest.finish()
}

fn collect_tree(root: &Path, here: &Path, entries: &mut Vec<String>) {
    let Ok(children) = fs::read_dir(here) else {
        return;
    };
    for child in children.flatten() {
        let path = child.path();
        let relative = path.strip_prefix(root).unwrap_or(&path).display();
        if path.is_symlink() {
            let target = fs::read_link(&path).unwrap_or_default();
            entries.push(format!("L {relative} {}", target.display()));
        } else if path.is_dir() {
            entries.push(format!("D {relative}"));
            collect_tree(root, &path, entries);
        } else {
            let bytes = fs::read(&path).unwrap_or_default();
            entries.push(format!("F {relative} {bytes:?}"));
        }
    }
}
