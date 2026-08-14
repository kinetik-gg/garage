//! One-shot transformations of machine state that the current checkout cannot derive.
//!
//! Three kinds of state have three homes. Preference-file versions belong to
//! `garage-prefs::migrate`, stamped by `[schema].preferences_version`, with each step gated
//! on the version that introduced it. Convergent machine state belongs to bootstrap and
//! `garage reconcile`, recomputed from the checkout every run. One-way machine
//! transformations belong here: state no fresh install has and no current checkout can
//! derive. The admission test is literal: if a fresh install could derive the state from its
//! checkout, it is not a migration. The corollary is enforced below: every registered
//! migration must be a no-op on a fresh install.
//!
//! A migration is run at most once after its outcome reaches the stamp. Both a change and
//! [`Outcome::NothingToDo`] settle it: recording the no-op is what stops an obsolete
//! precondition from being reconsidered forever, the same property preference migrations
//! get from the introducing-version gates explained in `garage-prefs/src/migrate.rs`.
//! Failed steps are not stamped and do not block later registry entries.
//!
//! Dry runs cross no write barrier here, but still call unstamped migrations so their
//! preconditions and would-change descriptions are real. Migration bodies must honor their
//! `dry_run` argument, and they must be idempotent even outside dry-run: their work completes
//! before the stamp is written, so a stamp write can fail after the transformation succeeded
//! and the next invocation must be safe to try again.

mod state;
mod steps;

use garage_core::paths::Paths;
use serde::{Deserialize, Serialize};

use crate::ApplyError;
use state::State;

/// One immutable registry entry.
#[derive(Debug, Clone, Copy)]
pub struct Migration {
    /// Stable `NNN-kebab` identity. The numeric prefix is unique and never reused.
    pub id: &'static str,
    /// One-line explanation suitable for a human report.
    pub summary: &'static str,
    /// Idempotent transformation, or its read-only precondition pass in dry-run mode.
    pub run: fn(&Paths, bool) -> Result<Outcome, ApplyError>,
}

/// What an eligible migration discovered.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "kind", content = "detail", rename_all = "kebab-case")]
pub enum Outcome {
    /// The transformation changed machine state; the string says what changed.
    Changed(String),
    /// The machine carried none of the legacy state this transformation removes.
    NothingToDo,
}

/// How one registry entry ended this invocation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", content = "outcome", rename_all = "kebab-case")]
pub enum Status {
    /// The durable stamp already settles this id.
    Skipped,
    /// The migration ran its preconditions, but neither its work nor its stamp was written.
    DryRun(Outcome),
    /// The work and the immediately following stamp write both completed.
    Applied(Outcome),
    /// The migration or its stamp write failed. The id remains eligible for another run.
    Failed(String),
}

/// One migration's ordered report row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Entry {
    /// Stable migration id.
    pub id: &'static str,
    /// Human explanation copied from the registry.
    pub summary: &'static str,
    /// What happened in this invocation.
    pub status: Status,
}

/// Complete result of walking a registry once.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Report {
    /// Whether every unstamped entry was restricted to its read-only path.
    pub dry_run: bool,
    /// Refusal to trust the stamp. Entries are empty when this is present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stamp_error: Option<String>,
    /// One row per registry entry, in registry order.
    pub entries: Vec<Entry>,
}

/// Walk `registry` in order, skipping every id already present in the durable stamp.
///
/// A migration failure becomes [`Status::Failed`] and iteration continues. A stamp-load
/// failure instead populates [`Report::stamp_error`] and runs nothing: without trustworthy
/// history, replaying a one-way transformation would be less safe than refusing the run.
///
/// `registry` is an argument rather than a static lookup because [`Paths`] follows the same
/// no-statics rule: scratch-tree tests drive fixture migrations through this exact mechanism,
/// and callers can make the registry as explicit as the filesystem world it operates on.
#[must_use]
pub fn run_migrations(paths: &Paths, registry: &[Migration], dry_run: bool) -> Report {
    let mut state = match State::load(paths) {
        Ok(state) => state,
        Err(error) => return stamp_failure(dry_run, error.to_string()),
    };
    let entries = registry
        .iter()
        .map(|migration| run_one(paths, migration, dry_run, &mut state))
        .collect();
    Report {
        dry_run,
        stamp_error: None,
        entries,
    }
}

fn run_one(paths: &Paths, migration: &Migration, dry_run: bool, state: &mut State) -> Entry {
    let status = if state.contains(migration.id) {
        Status::Skipped
    } else {
        run_unstamped(paths, migration, dry_run, state)
    };
    Entry {
        id: migration.id,
        summary: migration.summary,
        status,
    }
}

fn run_unstamped(paths: &Paths, migration: &Migration, dry_run: bool, state: &mut State) -> Status {
    let outcome = match (migration.run)(paths, dry_run) {
        Ok(outcome) => outcome,
        Err(error) => return Status::Failed(error.to_string()),
    };
    if dry_run {
        return Status::DryRun(outcome);
    }
    let next = state.recorded(migration.id, outcome.clone());
    match next.write(paths) {
        Ok(()) => {
            *state = next;
            Status::Applied(outcome)
        }
        Err(error) => Status::Failed(error.to_string()),
    }
}

fn stamp_failure(dry_run: bool, error: String) -> Report {
    Report {
        dry_run,
        stamp_error: Some(error),
        entries: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{hash_map::DefaultHasher, BTreeSet};
    use std::fs;
    use std::hash::{Hash, Hasher};
    use std::path::Path;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use garage_core::paths::Paths;

    use super::steps::REGISTRY;
    use super::{run_migrations, Migration, Outcome, Status};
    use crate::testing::{Script, World};
    use crate::ApplyError;

    static FAILURE_RUNS: AtomicUsize = AtomicUsize::new(0);
    static LATER_RUNS: AtomicUsize = AtomicUsize::new(0);
    static DRY_RUNS: AtomicUsize = AtomicUsize::new(0);
    static NOTHING_RUNS: AtomicUsize = AtomicUsize::new(0);
    static UNKNOWN_RUNS: AtomicUsize = AtomicUsize::new(0);

    const ONCE: Migration = Migration {
        id: "900-fixture-once",
        summary: "write a fixture marker",
        run: fixture_once,
    };
    const FAILS: Migration = Migration {
        id: "901-fixture-fails",
        summary: "refuse for the fixture",
        run: fixture_failure,
    };
    const LATER: Migration = Migration {
        id: "902-fixture-later",
        summary: "settle after a failure",
        run: fixture_later,
    };
    const DRY: Migration = Migration {
        id: "903-fixture-dry",
        summary: "describe a dry-run change",
        run: fixture_dry,
    };
    const NOTHING: Migration = Migration {
        id: "904-fixture-nothing",
        summary: "find no fixture legacy state",
        run: fixture_nothing,
    };
    const UNKNOWN: Migration = Migration {
        id: "905-fixture-unknown-version",
        summary: "must not run without trustworthy history",
        run: fixture_unknown,
    };

    #[test]
    fn changed_fixture_runs_once_is_stamped_and_then_skipped() {
        let world = World::plain("migration-once", Script::new());

        let first = run_migrations(&world.paths, &[ONCE], false);
        assert!(matches!(
            first.entries.first().map(|entry| &entry.status),
            Some(Status::Applied(Outcome::Changed(detail))) if detail == "wrote fixture marker"
        ));
        assert_changed_stamp(&world.paths);

        let second = run_migrations(&world.paths, &[ONCE], false);
        assert!(matches!(
            second.entries.first().map(|entry| &entry.status),
            Some(Status::Skipped)
        ));
        assert_eq!(
            fs::read_to_string(once_marker(&world.paths)).expect("marker"),
            "once\n"
        );
    }

    #[test]
    fn failure_is_unstamped_reruns_and_does_not_block_later_entries() {
        FAILURE_RUNS.store(0, Ordering::Relaxed);
        LATER_RUNS.store(0, Ordering::Relaxed);
        let world = World::plain("migration-failure", Script::new());
        let registry = [FAILS, LATER];

        let first = run_migrations(&world.paths, &registry, false);
        assert!(matches!(
            first.entries.first().map(|entry| &entry.status),
            Some(Status::Failed(_))
        ));
        assert!(matches!(
            first.entries.get(1).map(|entry| &entry.status),
            Some(Status::Applied(Outcome::NothingToDo))
        ));

        let second = run_migrations(&world.paths, &registry, false);
        assert!(matches!(
            second.entries.first().map(|entry| &entry.status),
            Some(Status::Failed(_))
        ));
        assert!(matches!(
            second.entries.get(1).map(|entry| &entry.status),
            Some(Status::Skipped)
        ));
        assert_eq!(FAILURE_RUNS.load(Ordering::Relaxed), 2);
        assert_eq!(LATER_RUNS.load(Ordering::Relaxed), 1);
        assert!(!stamp_text(&world.paths).contains(FAILS.id));
    }

    #[test]
    fn unknown_stamp_version_is_refused_without_reset_or_execution() {
        UNKNOWN_RUNS.store(0, Ordering::Relaxed);
        let world = World::plain("migration-version", Script::new());
        let original = "{\"version\":77,\"applied\":[]}\n";
        seed_stamp(&world.paths, original);

        let report = run_migrations(&world.paths, &[UNKNOWN], false);

        assert!(report
            .stamp_error
            .as_deref()
            .is_some_and(|error| error.contains("unsupported migration stamp version 77")));
        assert!(report.entries.is_empty());
        assert_eq!(UNKNOWN_RUNS.load(Ordering::Relaxed), 0);
        assert_eq!(stamp_text(&world.paths), original);
    }

    #[test]
    fn dry_run_executes_preconditions_but_leaves_a_byte_identical_tree() {
        DRY_RUNS.store(0, Ordering::Relaxed);
        let world = World::plain("migration-dry", Script::new());
        let before = tree_digest(&world.home);

        let report = run_migrations(&world.paths, &[DRY], true);

        assert!(matches!(
            report.entries.first().map(|entry| &entry.status),
            Some(Status::DryRun(Outcome::Changed(_)))
        ));
        assert_eq!(DRY_RUNS.load(Ordering::Relaxed), 1);
        assert_eq!(tree_digest(&world.home), before);
        assert!(!world.paths.migrations.exists());
    }

    #[test]
    fn nothing_to_do_is_stamped_and_not_reconsidered() {
        NOTHING_RUNS.store(0, Ordering::Relaxed);
        let world = World::plain("migration-nothing", Script::new());

        let first = run_migrations(&world.paths, &[NOTHING], false);
        let second = run_migrations(&world.paths, &[NOTHING], false);

        assert!(matches!(
            first.entries.first().map(|entry| &entry.status),
            Some(Status::Applied(Outcome::NothingToDo))
        ));
        assert!(matches!(
            second.entries.first().map(|entry| &entry.status),
            Some(Status::Skipped)
        ));
        assert_eq!(NOTHING_RUNS.load(Ordering::Relaxed), 1);
        assert!(stamp_text(&world.paths).contains("\"kind\": \"nothing-to-do\""));
    }

    #[test]
    fn registry_ids_are_unique_well_formed_and_strictly_ordered() {
        let mut numbers = BTreeSet::new();
        let mut previous = None;
        for migration in REGISTRY {
            let (number, slug) = migration.id.split_once('-').expect("NNN-kebab id");
            assert_eq!(number.len(), 3);
            assert!(number.chars().all(|character| character.is_ascii_digit()));
            assert!(slug.split('-').all(valid_slug_word));
            let number: u16 = number.parse().expect("numeric migration prefix");
            assert!(number > 0 && numbers.insert(number));
            assert!(previous.is_none_or(|earlier| earlier < number));
            assert!(!migration.summary.is_empty());
            previous = Some(number);
        }
    }

    #[test]
    fn every_registered_migration_is_a_no_op_on_a_fresh_install() {
        let world = World::plain("migration-fresh", Script::new());
        let before = tree_digest(&world.home);
        for migration in REGISTRY {
            assert!(matches!(
                (migration.run)(&world.paths, false),
                Ok(Outcome::NothingToDo)
            ));
        }
        assert_eq!(tree_digest(&world.home), before);
    }

    fn fixture_once(paths: &Paths, dry_run: bool) -> Result<Outcome, ApplyError> {
        let marker = once_marker(paths);
        if marker.exists() {
            return Err(ApplyError::Settings("fixture ran twice".to_owned()));
        }
        if !dry_run {
            fs::create_dir_all(&paths.state_root)
                .and_then(|()| fs::write(marker, "once\n"))
                .map_err(|error| ApplyError::Io(error.to_string()))?;
        }
        Ok(Outcome::Changed("wrote fixture marker".to_owned()))
    }

    fn fixture_failure(_paths: &Paths, _dry_run: bool) -> Result<Outcome, ApplyError> {
        FAILURE_RUNS.fetch_add(1, Ordering::Relaxed);
        Err(ApplyError::Settings("fixture refusal".to_owned()))
    }

    fn fixture_later(_paths: &Paths, _dry_run: bool) -> Result<Outcome, ApplyError> {
        if LATER_RUNS.fetch_add(1, Ordering::Relaxed) > 0 {
            return Err(ApplyError::Settings("later fixture reran".to_owned()));
        }
        Ok(Outcome::NothingToDo)
    }

    fn fixture_dry(paths: &Paths, dry_run: bool) -> Result<Outcome, ApplyError> {
        DRY_RUNS.fetch_add(1, Ordering::Relaxed);
        if !dry_run {
            fs::write(paths.home.join("dry-run-wrote"), "bad")
                .map_err(|error| ApplyError::Io(error.to_string()))?;
        }
        Ok(Outcome::Changed("would write fixture marker".to_owned()))
    }

    fn fixture_nothing(_paths: &Paths, _dry_run: bool) -> Result<Outcome, ApplyError> {
        if NOTHING_RUNS.fetch_add(1, Ordering::Relaxed) > 0 {
            return Err(ApplyError::Settings("no-op fixture reran".to_owned()));
        }
        Ok(Outcome::NothingToDo)
    }

    fn fixture_unknown(_paths: &Paths, _dry_run: bool) -> Result<Outcome, ApplyError> {
        UNKNOWN_RUNS.fetch_add(1, Ordering::Relaxed);
        Err(ApplyError::Settings(
            "unknown-version fixture executed".to_owned(),
        ))
    }

    fn once_marker(paths: &Paths) -> std::path::PathBuf {
        paths.state_root.join("fixture-once")
    }

    fn assert_changed_stamp(paths: &Paths) {
        let text = stamp_text(paths);
        let stamp: serde_json::Value = serde_json::from_str(&text).expect("stamp JSON");
        assert_eq!(
            stamp.get("version").and_then(serde_json::Value::as_u64),
            Some(1)
        );
        let entry = stamp
            .get("applied")
            .and_then(serde_json::Value::as_array)
            .and_then(|entries| entries.first())
            .expect("one applied migration");
        assert_eq!(
            entry.get("id").and_then(serde_json::Value::as_str),
            Some(ONCE.id)
        );
        let outcome = entry.get("outcome").expect("stamped outcome");
        assert_eq!(
            outcome.get("kind").and_then(serde_json::Value::as_str),
            Some("changed")
        );
        assert_eq!(
            outcome.get("detail").and_then(serde_json::Value::as_str),
            Some("wrote fixture marker")
        );
        assert!(entry
            .get("timestamp")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|held| !held.is_empty()));
        assert!(text.ends_with('\n'));
    }

    fn seed_stamp(paths: &Paths, text: &str) {
        fs::create_dir_all(&paths.state_root).expect("stamp parent");
        fs::write(&paths.migrations, text).expect("seed migration stamp");
    }

    fn stamp_text(paths: &Paths) -> String {
        fs::read_to_string(&paths.migrations).expect("migration stamp")
    }

    fn valid_slug_word(word: &str) -> bool {
        !word.is_empty()
            && word
                .chars()
                .all(|character| character.is_ascii_lowercase() || character.is_ascii_digit())
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
            if path.is_dir() {
                entries.push(format!("D {relative}"));
                collect_tree(root, &path, entries);
            } else {
                let bytes = fs::read(&path).unwrap_or_default();
                entries.push(format!("F {relative} {bytes:?}"));
            }
        }
    }
}
