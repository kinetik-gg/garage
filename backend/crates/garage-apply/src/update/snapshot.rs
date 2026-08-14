//! The fail-closed copy of layer 2 before an update converges the machine.
//!
//! [`Paths::host`] names the only four user-owned Garage files. A real update copies every
//! one that exists, records every one that does not, and adds the checkout commit from before
//! the pull plus `pacman -Qqe`. That is enough to reconstruct the host's choices without
//! pretending to be a filesystem snapshot. [`Paths::generated`] is deliberately excluded:
//! it is layer 3 cache, and one render reconstructs it from the shipped defaults and these
//! host files. Copying it here would falsely give generated output the standing of user data.
//!
//! The destination is `~/.garage-backup/<stamp>/pre-update`, the root bootstrap and reconcile
//! already use. Allocation mirrors reconcile's timestamp, then `-2`, `-3`, ... suffix loop,
//! while the atomic directory creation gives the same no-reuse guarantee as repair's
//! `create_new` loop. An existing `pre-update` is a collision, never something to merge into.
//!
//! This copy is mandatory because it runs without privilege and protects a few small files:
//! if it cannot complete, convergence is the wrong next operation. A snapper snapshot is the
//! opposite. When snapper is on `PATH`, Garage prints one command a person may run but never
//! drives it. It needs root, owns a retention policy Garage cannot choose, and a failure must
//! not abort an update. That is the same root-context boundary documented by design A in
//! `system/bin/kinetik-plugin-hook`: root bookkeeping may advise user-context work, but it
//! must not execute that work inside the privileged lifecycle.

use std::fs::{self, DirBuilder, File, OpenOptions};
use std::io::{self, Write as _};
use std::os::unix::fs::{DirBuilderExt as _, OpenOptionsExt as _};
use std::path::{Path, PathBuf};

use garage_core::paths::Paths;
use garage_core::time::{local_backup_stamp, now_seconds};
use garage_core::traits::{Runner, DEFAULT_RUN_TIMEOUT};

use crate::doctor::tilde;
use crate::error::ApplyError;

use super::transcript::Report;

const ABSENT: &str = "absent";
const COMMIT: &str = "commit";
const PACKAGES: &str = "packages";

/// Print the backup step and say whether the update may continue.
pub(super) fn save(
    paths: &Paths,
    proc: &dyn Runner,
    report: &mut Report,
    before_commit: &str,
) -> Result<bool, ApplyError> {
    if report.dry_run {
        return save_at(
            paths,
            proc,
            report,
            SnapshotFacts {
                before_commit,
                stamp: "",
                snapper_available: false,
            },
        );
    }
    save_at(
        paths,
        proc,
        report,
        SnapshotFacts {
            before_commit,
            stamp: &local_backup_stamp(now_seconds()),
            snapper_available: garage_proc::which("snapper").is_some(),
        },
    )
}

#[derive(Clone, Copy)]
struct SnapshotFacts<'a> {
    before_commit: &'a str,
    stamp: &'a str,
    snapper_available: bool,
}

/// [`save`] with the two host facts fixed for scratch-home tests.
fn save_at(
    paths: &Paths,
    proc: &dyn Runner,
    report: &mut Report,
    facts: SnapshotFacts<'_>,
) -> Result<bool, ApplyError> {
    if report.dry_run {
        report.note("Pre-update backup skipped entirely in a dry run.")?;
        return Ok(true);
    }

    report.step("Saving the host preferences before convergence")?;
    if facts.snapper_available {
        report.note(
            "snapper is available; run it yourself if wanted: sudo snapper create -d \"before garage update\"",
        )?;
    }
    match create(paths, proc, facts.before_commit, facts.stamp) {
        Ok(saved) => {
            report.note(&format!(
                "copied {} host file(s); recorded {} absent.",
                saved.copied, saved.absent
            ))?;
            report.note(&format!(
                "backup                       {}",
                tilde(&paths.home, &saved.directory)
            ))?;
            Ok(true)
        }
        Err(error) => {
            report.note(&format!(
                "REFUSE: could not complete the pre-update backup: {error}"
            ))?;
            report.note("nothing else will be changed by this update.")?;
            Ok(false)
        }
    }
}

struct Saved {
    directory: PathBuf,
    copied: usize,
    absent: usize,
}

/// Collect provenance, claim a never-reused destination, and fill it completely.
fn create(
    paths: &Paths,
    proc: &dyn Runner,
    before_commit: &str,
    stamp: &str,
) -> Result<Saved, String> {
    let packages = package_inventory(proc)?;
    let directory = claim_directory(&paths.home, stamp).map_err(|error| error.to_string())?;
    let mut absent = Vec::new();
    let mut copied = 0;
    for (name, source) in host_files(paths) {
        match File::open(source) {
            Ok(input) => {
                copy_new(input, &directory.join(name))
                    .map_err(|error| format!("could not copy {}: {error}", source.display()))?;
                copied += 1;
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => absent.push(name),
            Err(error) => {
                return Err(format!("could not read {}: {error}", source.display()));
            }
        }
    }

    write_new(&directory.join(ABSENT), &lines(&absent))
        .map_err(|error| format!("could not record absent host files: {error}"))?;
    write_new(
        &directory.join(COMMIT),
        &one_line(if before_commit.is_empty() {
            "(unknown)"
        } else {
            before_commit
        }),
    )
    .map_err(|error| format!("could not record the checkout commit: {error}"))?;
    write_new(&directory.join(PACKAGES), packages.as_bytes())
        .map_err(|error| format!("could not record explicit packages: {error}"))?;
    sync_directory(&directory)
        .map_err(|error| format!("could not make the backup durable: {error}"))?;

    Ok(Saved {
        directory,
        copied,
        absent: absent.len(),
    })
}

fn host_files(paths: &Paths) -> [(&'static str, &Path); 4] {
    [
        ("preferences.toml", &paths.host.preferences),
        ("displays.toml", &paths.host.displays),
        ("keybindings.toml", &paths.host.keybindings),
        ("workspace-blocks.toml", &paths.host.workspace_blocks),
    ]
}

fn package_inventory(proc: &dyn Runner) -> Result<String, String> {
    let output = proc
        .run(&["pacman", "-Qqe"], DEFAULT_RUN_TIMEOUT)
        .map_err(|error| format!("could not run pacman -Qqe: {}", error.detail))?;
    if output.status == 0 {
        return Ok(output.stdout);
    }
    let detail = output.stderr.trim();
    let suffix = if detail.is_empty() {
        String::new()
    } else {
        format!(": {detail}")
    };
    Err(format!("pacman -Qqe exited {}{suffix}", output.status))
}

/// Claim `<stamp>/pre-update`, suffixing the timestamp root on every collision.
fn claim_directory(home: &Path, stamp: &str) -> io::Result<PathBuf> {
    let base = home.join(".garage-backup");
    private_directories(true).create(&base)?;
    if !fs::symlink_metadata(&base)?.file_type().is_dir() {
        return Err(io::Error::other(format!(
            "{} is not a directory",
            base.display()
        )));
    }

    let mut suffix = 1_u64;
    loop {
        let name = if suffix == 1 {
            stamp.to_owned()
        } else {
            format!("{stamp}-{suffix}")
        };
        let root = base.join(name);
        match fs::symlink_metadata(&root) {
            Ok(metadata) if !metadata.file_type().is_dir() => {
                suffix = suffix.saturating_add(1);
                continue;
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                match private_directories(false).create(&root) {
                    Ok(()) => {}
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                    Err(error) => return Err(error),
                }
            }
            Err(error) => return Err(error),
        }
        let destination = root.join("pre-update");
        match private_directories(false).create(&destination) {
            Ok(()) => return Ok(destination),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                suffix = suffix.saturating_add(1);
            }
            Err(error) => return Err(error),
        }
    }
}

fn private_directories(recursive: bool) -> DirBuilder {
    let mut builder = DirBuilder::new();
    builder.recursive(recursive).mode(0o700);
    builder
}

fn copy_new(mut source: File, destination: &Path) -> io::Result<()> {
    let mut sink = new_file(destination)?;
    io::copy(&mut source, &mut sink)?;
    finish_file(&mut sink)
}

fn write_new(destination: &Path, data: &[u8]) -> io::Result<()> {
    let mut sink = new_file(destination)?;
    sink.write_all(data)?;
    finish_file(&mut sink)
}

fn new_file(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
}

fn finish_file(file: &mut File) -> io::Result<()> {
    file.flush()?;
    file.sync_all()
}

fn sync_directory(path: &Path) -> io::Result<()> {
    File::open(path)?.sync_all()
}

fn one_line(text: &str) -> Vec<u8> {
    let mut line = text.trim_end_matches('\n').as_bytes().to_vec();
    line.push(b'\n');
    line
}

fn lines(names: &[&str]) -> Vec<u8> {
    if names.is_empty() {
        Vec::new()
    } else {
        one_line(&names.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Duration;

    use garage_core::traits::{Output, RunError};

    use super::*;

    static NEXT: AtomicU64 = AtomicU64::new(1);
    const STAMP: &str = "20260814-123456";

    struct Packages {
        calls: Cell<usize>,
    }

    impl Packages {
        fn new() -> Self {
            Self {
                calls: Cell::new(0),
            }
        }
    }

    impl Runner for Packages {
        fn run(&self, command: &[&str], _timeout: Duration) -> Result<Output, RunError> {
            assert_eq!(command, ["pacman", "-Qqe"]);
            self.calls.set(self.calls.get() + 1);
            Ok(Output {
                status: 0,
                stdout: "base\ngarage\n".to_owned(),
                stderr: String::new(),
            })
        }

        fn spawn_detached(&self, _command: &[&str]) -> Result<(), RunError> {
            unreachable!("a backup never detaches a process")
        }

        fn run_streamed(&self, _command: &[&str], _cwd: Option<&Path>) -> Result<i32, RunError> {
            unreachable!("a backup never streams a process")
        }
    }

    struct Scratch {
        root: PathBuf,
        paths: Paths,
    }

    impl Scratch {
        fn new(name: &str) -> Self {
            let serial = NEXT.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir().join(format!(
                "garage-update-backup-{name}-{}-{serial}",
                std::process::id()
            ));
            drop(fs::remove_dir_all(&root));
            let home = root.join("home");
            fs::create_dir_all(&home).expect("scratch home");
            let env: HashMap<String, String> =
                [("HOME".to_owned(), home.to_string_lossy().into_owned())]
                    .into_iter()
                    .collect();
            Self {
                paths: Paths::from_env_map(&env),
                root,
            }
        }

        fn write_host(&self, name: &str, body: &str) {
            let path = self.paths.root.join(name);
            fs::create_dir_all(path.parent().expect("host parent")).expect("host root");
            fs::write(path, body).expect("host file");
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            drop(fs::remove_dir_all(&self.root));
        }
    }

    #[test]
    fn all_four_host_files_and_provenance_are_copied_but_generated_is_not() {
        let scratch = Scratch::new("all");
        for name in [
            "preferences.toml",
            "displays.toml",
            "keybindings.toml",
            "workspace-blocks.toml",
        ] {
            scratch.write_host(name, &format!("{name}\n"));
        }
        fs::create_dir_all(&scratch.paths.generated).expect("generated root");
        fs::write(scratch.paths.generated.join("preferences.lua"), "cache")
            .expect("generated cache");

        let runner = Packages::new();
        let saved = create(&scratch.paths, &runner, "0123456789abcdef", STAMP).expect("backup");
        assert_eq!(saved.copied, 4);
        assert_eq!(saved.absent, 0);
        assert_eq!(runner.calls.get(), 1);
        assert_backup_listing(&saved.directory);
        assert_eq!(
            fs::read_to_string(saved.directory.join(ABSENT)).expect("absent"),
            ""
        );
        assert_eq!(
            fs::read_to_string(saved.directory.join(COMMIT)).expect("commit"),
            "0123456789abcdef\n"
        );
        assert_eq!(
            fs::read_to_string(saved.directory.join(PACKAGES)).expect("packages"),
            "base\ngarage\n"
        );
        assert!(!saved.directory.join("generated").exists());
        for name in [
            "preferences.toml",
            "displays.toml",
            "keybindings.toml",
            "workspace-blocks.toml",
        ] {
            assert_eq!(
                fs::read_to_string(saved.directory.join(name)).expect("copied host file"),
                format!("{name}\n")
            );
        }
    }

    #[test]
    fn absent_host_files_are_named_and_only_existing_files_are_copied() {
        let scratch = Scratch::new("absent");
        scratch.write_host("preferences.toml", "preferences\n");
        scratch.write_host("workspace-blocks.toml", "blocks\n");

        let saved = create(&scratch.paths, &Packages::new(), "before", STAMP).expect("backup");
        assert_eq!(saved.copied, 2);
        assert_eq!(saved.absent, 2);
        assert_eq!(
            fs::read_to_string(saved.directory.join(ABSENT)).expect("absent record"),
            "displays.toml\nkeybindings.toml\n"
        );
        assert!(!saved.directory.join("displays.toml").exists());
        assert!(!saved.directory.join("keybindings.toml").exists());
    }

    #[test]
    fn a_planted_stamp_collision_takes_the_numeric_suffix_without_clobbering() {
        let scratch = Scratch::new("collision");
        scratch.write_host("preferences.toml", "new\n");
        let planted = scratch
            .paths
            .home
            .join(".garage-backup")
            .join(STAMP)
            .join("pre-update");
        fs::create_dir_all(&planted).expect("collision");
        fs::write(planted.join("preferences.toml"), "old\n").expect("old backup");

        let saved = create(&scratch.paths, &Packages::new(), "before", STAMP).expect("backup");
        assert_eq!(
            saved.directory,
            scratch
                .paths
                .home
                .join(".garage-backup")
                .join(format!("{STAMP}-2"))
                .join("pre-update")
        );
        assert_eq!(
            fs::read_to_string(planted.join("preferences.toml")).expect("old"),
            "old\n"
        );
        assert_eq!(
            fs::read_to_string(saved.directory.join("preferences.toml")).expect("new"),
            "new\n"
        );
    }

    fn assert_backup_listing(path: &Path) {
        let mut names: Vec<String> = fs::read_dir(path)
            .expect("backup directory")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(
            names,
            [
                "absent",
                "commit",
                "displays.toml",
                "keybindings.toml",
                "packages",
                "preferences.toml",
                "workspace-blocks.toml",
            ]
        );
    }
}

#[cfg(test)]
#[path = "snapshot_boundary_tests.rs"]
mod boundary_tests;
