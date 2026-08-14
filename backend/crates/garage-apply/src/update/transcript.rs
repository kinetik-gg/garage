//! The private, rotating line log behind one real `garage update`.
//!
//! Files live under `Paths::state_root/updates` and take the same
//! `%Y%m%d-%H%M%S` local stamp as reconcile's backup roots. A second run in the same second
//! takes the backup-root convention's `-2`, `-3`, ... suffix rather than replacing the first
//! run. Every new file is mode `0600`.
//!
//! Rotation happens before the new file is opened. The newest nine existing files are kept,
//! ordered by the stamp in their names rather than by mtime; the new file makes ten. This is
//! deliberately a name contract. Copying a transcript can rewrite its mtime without making
//! the update it describes any newer.
//!
//! # The streamed-output boundary
//!
//! A transcript contains every line the parent [`Report`] prints, but it does not
//! contain `bootstrap.sh`'s own output. [`Runner::run_streamed`] gives that child the
//! process's inherited terminal so pacman's sudo prompt remains interactive. A pipe-and-tee
//! design is **rejected**: teeing while preserving an interactive controlling terminal needs
//! a pty, and this command does not introduce a pty implementation merely to copy the
//! bootstrap's progress stream. The report records the bootstrap argv, cwd, two environment
//! variables, exit status, and the fact that its output went to the terminal instead.
//!
//! [`Paths::state_root/updates`]: garage_core::paths::Paths::state_root
//! [`Runner::run_streamed`]: garage_core::traits::Runner::run_streamed

use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};

use garage_core::paths::Paths;
use garage_core::shlex::shlex_quote;
use garage_core::time::{local_backup_stamp, now_seconds};

use crate::error::ApplyError;

/// The old files retained before opening this run's tenth slot.
const RETAIN_BEFORE_OPEN: usize = 9;

/// The printer: `step()`, `info()` and `note()`, which differ only in their prefix.
///
/// Prints as it goes, which is the Python's behaviour and is load-bearing here rather than
/// incidental: `bootstrap.sh` and `garage-rebuild-plugins` are handed this process's own
/// terminal part-way through, so a report assembled and printed at the end would arrive after
/// the output it was introducing. A fixture captures instead, because the terminal transcript
/// is the thing being compared and nothing is really streaming in a test.
///
/// A real update attaches one optional line sink after the pull has supplied both header
/// commits. [`Report::say`] writes the same bytes there and flushes after every call, before a
/// later step can hand the terminal away. A process killed in pacman therefore leaves every
/// complete parent line up to that hand-off durable on disk.
pub(super) struct Report {
    /// Whether `[dry-run] ` is stamped on the lines that describe a mutation.
    pub(super) dry_run: bool,
    /// `Some` collects instead of printing. `None` in every real run.
    captured: Option<String>,
    /// The private transcript file in a real, non-dry run.
    sink: Option<Box<dyn Write>>,
}

impl Report {
    pub(super) fn new(dry_run: bool, captured: bool) -> Self {
        Self {
            dry_run,
            captured: captured.then(String::new),
            sink: None,
        }
    }

    /// The collected terminal side, for trace fixtures.
    pub(super) fn captured(&self) -> Option<&str> {
        self.captured.as_deref()
    }

    /// Start mirroring every subsequent line into `sink`.
    pub(super) fn attach(&mut self, sink: impl Write + 'static) {
        self.sink = Some(Box::new(sink));
    }

    /// One line, printed or collected, then written and flushed to the optional sink.
    pub(super) fn say(&mut self, line: &str) -> Result<(), ApplyError> {
        match self.captured.as_mut() {
            Some(sink) => sink.push_str(line),
            None => print!("{line}"),
        }
        if let Some(sink) = self.sink.as_mut() {
            sink.write_all(line.as_bytes())
                .and_then(|()| sink.flush())
                .map_err(|error| {
                    ApplyError::Io(format!("could not write the update transcript: {error}"))
                })?;
        }
        Ok(())
    }

    /// A step banner: a blank line, then `==> `.
    pub(super) fn step(&mut self, text: &str) -> Result<(), ApplyError> {
        self.say(&format!("\n==> {text}\n"))
    }

    /// A line about something that would be changed. Stamped in a dry run.
    pub(super) fn info(&mut self, text: &str) -> Result<(), ApplyError> {
        let prefix = if self.dry_run { "[dry-run] " } else { "" };
        self.say(&format!("    {prefix}{text}\n"))
    }

    /// A line that is true either way.
    pub(super) fn note(&mut self, text: &str) -> Result<(), ApplyError> {
        self.say(&format!("    {text}\n"))
    }
}

/// Open a private transcript for a real run; cross no write barrier in a dry run.
pub(super) fn open_for_run(paths: &Paths, dry_run: bool) -> Result<Option<File>, ApplyError> {
    if dry_run {
        return Ok(None);
    }
    let directory = paths.state_root.join("updates");
    let stamp = local_backup_stamp(now_seconds());
    let (_, file) = open(&paths.state_root, &stamp).map_err(|error| {
        ApplyError::Io(format!(
            "{}: could not prepare the update transcript: {error}",
            directory.display()
        ))
    })?;
    Ok(Some(file))
}

/// The invariant first lines, then the two useful path facts the old report carried.
pub(super) fn header(
    report: &mut Report,
    paths: &Paths,
    root: &Path,
    before: &str,
    after: &str,
) -> Result<(), ApplyError> {
    let qualifier = if report.dry_run {
        " (dry run: nothing will be changed)"
    } else {
        ""
    };
    report.say(&format!("Garage update{qualifier}\n"))?;
    report.note(&format!(
        "checkout commit before pull  {}",
        known_commit(before)
    ))?;
    report.note(&format!(
        "checkout commit after pull   {}",
        known_commit(after)
    ))?;
    report.note(&format!("binary                       {}", binary_build()))?;
    report.note(&format!("checkout                     {}", root.display()))?;
    report.note(&format!(
        "home                         {}",
        paths.home.display()
    ))
}

/// Record everything about the bootstrap child except the pty-requiring output stream.
pub(super) fn bootstrap_invocation(
    report: &mut Report,
    root: &Path,
    command: &[&str],
) -> Result<(), ApplyError> {
    report.note(&format!(
        "bootstrap argv    {}",
        command
            .iter()
            .map(|part| shlex_quote(part))
            .collect::<Vec<_>>()
            .join(" ")
    ))?;
    report.note(&format!("bootstrap cwd     {}", root.display()))?;
    report.note(&format!(
        "bootstrap env     GARAGE_SKIP_PLUGIN_DEPLOY={}",
        environment_value("GARAGE_SKIP_PLUGIN_DEPLOY")
    ))?;
    report.note(&format!(
        "bootstrap env     GARAGE_FORCE={}",
        environment_value("GARAGE_FORCE")
    ))?;
    report.note("bootstrap output  terminal (not included in this transcript)")?;
    if !report.dry_run {
        report.note("it will ask for sudo: a re-run upgrades the system and installs any")?;
        report.note("package the list has gained since this machine was set up.")?;
    }
    Ok(())
}

fn known_commit(commit: &str) -> &str {
    if commit.is_empty() {
        "(unknown)"
    } else {
        commit
    }
}

pub(super) fn binary_build() -> String {
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    format!(
        "garage {} ({profile} build, {}/{})",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH
    )
}

fn environment_value(name: &str) -> String {
    std::env::var_os(name).map_or_else(
        || "<unset>".to_owned(),
        |value| value.to_string_lossy().into_owned(),
    )
}

/// Rotate the directory and create this run's private transcript.
pub(super) fn open(state_root: &Path, stamp: &str) -> io::Result<(PathBuf, File)> {
    let directory = state_root.join("updates");
    fs::create_dir_all(&directory)?;
    rotate(&directory)?;
    create_unique(&directory, stamp)
}

/// Delete the oldest names until nine old transcripts remain.
fn rotate(directory: &Path) -> io::Result<()> {
    let mut files: Vec<(NameOrder, PathBuf)> = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_file() || kind.is_symlink() {
            files.push((NameOrder::of(&entry.file_name()), entry.path()));
        }
    }
    files.sort_by(|left, right| left.0.cmp(&right.0));
    let remove = files.len().saturating_sub(RETAIN_BEFORE_OPEN);
    for (_, path) in files.into_iter().take(remove) {
        fs::remove_file(path)?;
    }
    Ok(())
}

/// Open the stamp itself, then collision suffixes matching reconcile's backup roots.
fn create_unique(directory: &Path, stamp: &str) -> io::Result<(PathBuf, File)> {
    let mut suffix = 1_u64;
    loop {
        let name = if suffix == 1 {
            stamp.to_owned()
        } else {
            format!("{stamp}-{suffix}")
        };
        let path = directory.join(name);
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
        {
            Ok(file) => {
                file.set_permissions(fs::Permissions::from_mode(0o600))?;
                return Ok((path, file));
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                suffix = suffix.saturating_add(1);
            }
            Err(error) => return Err(error),
        }
    }
}

/// Sort stamp collisions numerically (`-10` after `-9`), with the full name as a tiebreaker.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct NameOrder {
    stamp: String,
    collision: u64,
    name: String,
}

impl NameOrder {
    fn of(name: &OsStr) -> Self {
        let name = name.to_string_lossy().into_owned();
        let (stamp, collision) = collision_parts(&name);
        Self {
            stamp,
            collision,
            name,
        }
    }
}

/// Split only the suffix we create; the stamp's own middle hyphen is part of the key.
fn collision_parts(name: &str) -> (String, u64) {
    let Some((stamp, suffix)) = (name.len() > 16).then(|| name.rsplit_once('-')).flatten() else {
        return (name.to_owned(), 1);
    };
    if stamp.len() != 15 {
        return (name.to_owned(), 1);
    }
    suffix.parse::<u64>().map_or_else(
        |_| (name.to_owned(), 1),
        |number| (stamp.to_owned(), number),
    )
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::fs;
    use std::io::{self, Write as _};
    use std::os::unix::fs::PermissionsExt as _;
    use std::path::PathBuf;
    use std::rc::Rc;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::{open, Report};

    static SERIAL: AtomicU64 = AtomicU64::new(0);

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(label: &str) -> Self {
            let serial = SERIAL.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "garage-update-transcript-{label}-{}-{serial}",
                std::process::id()
            ));
            fs::create_dir_all(&path).expect("scratch directory");
            Self(path)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            drop(fs::remove_dir_all(&self.0));
        }
    }

    #[test]
    fn rotation_keeps_the_newest_nine_names_plus_the_new_private_file() {
        let scratch = Scratch::new("rotation");
        let updates = scratch.0.join("updates");
        fs::create_dir_all(&updates).expect("updates directory");
        for second in 0..12 {
            let name = format!("20260814-1200{second:02}");
            fs::write(updates.join(name), format!("old {second}\n")).expect("old transcript");
        }
        // Give the oldest name the newest mtime. Rotation must still discard it.
        fs::write(updates.join("20260814-120000"), "copied last\n")
            .expect("rewrite oldest transcript");

        let (path, mut file) = open(&scratch.0, "20260814-120012").expect("new transcript");
        file.write_all(b"new\n").expect("write transcript");
        file.flush().expect("flush transcript");

        let mut names: Vec<String> = fs::read_dir(&updates)
            .expect("read updates")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        let expected: Vec<String> = (3..=12)
            .map(|second| format!("20260814-1200{second:02}"))
            .collect();
        assert_eq!(names, expected);
        assert_eq!(
            fs::metadata(path)
                .expect("transcript metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }

    #[derive(Clone)]
    struct FlushProbe(Rc<RefCell<(Vec<u8>, usize)>>);

    impl io::Write for FlushProbe {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0.borrow_mut().0.extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            self.0.borrow_mut().1 += 1;
            Ok(())
        }
    }

    #[test]
    fn report_writes_and_flushes_the_sink_once_for_every_line_said() {
        let observed = Rc::new(RefCell::new((Vec::new(), 0)));
        let mut report = Report::new(false, true);
        report.attach(FlushProbe(Rc::clone(&observed)));

        report.say("first line\n").expect("first line");
        assert_eq!(&observed.borrow().0, b"first line\n");
        assert_eq!(observed.borrow().1, 1);
        report.say("second line\n").expect("second line");
        assert_eq!(&observed.borrow().0, b"first line\nsecond line\n");
        assert_eq!(observed.borrow().1, 2);
    }
}
