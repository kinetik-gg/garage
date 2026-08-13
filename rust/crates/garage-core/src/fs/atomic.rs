//! `atomic_write` — temp + fsync + rename. Default for anything a reader opens fresh.

use std::fs::{self, File, OpenOptions};
use std::io::{self, ErrorKind, Write as _};
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use thiserror::Error;

/// How far a temporary name is allowed to collide before the write gives up. A collision
/// needs two processes to pick the same pid, nanosecond and counter, so one retry is
/// already generous; the loop exists so that an impossible case is an error rather than
/// a hang.
const NAME_ATTEMPTS: u32 = 8;

/// Why a file could not be replaced. Each variant carries the path it was working on,
/// which is the target for everything except the write itself -- there it is the
/// temporary, because that is the file the operating system refused.
#[derive(Debug, Error)]
pub enum AtomicWriteError {
    /// The directory the file belongs in could not be created.
    #[error("{}: could not create the directory it belongs in: {source}", path.display())]
    Parents {
        /// The target.
        path: PathBuf,
        /// The underlying failure.
        source: io::Error,
    },
    /// No temporary could be opened beside the target.
    #[error("{}: could not open a temporary beside it: {source}", path.display())]
    Temporary {
        /// The target.
        path: PathBuf,
        /// The underlying failure.
        source: io::Error,
    },
    /// The bytes did not reach the disk.
    #[error("{}: could not be written: {source}", path.display())]
    Write {
        /// The temporary that was being filled.
        path: PathBuf,
        /// The underlying failure.
        source: io::Error,
    },
    /// The rename that publishes the new contents failed.
    #[error("{}: could not be replaced: {source}", path.display())]
    Install {
        /// The target.
        path: PathBuf,
        /// The underlying failure.
        source: io::Error,
    },
}

/// Replace `path` with `text`, so that no reader ever sees a half-written file.
///
/// Temporary, fsync, rename -- and all three are load-bearing. The temporary is opened in
/// the target's own directory, because a rename is only atomic within one filesystem and
/// `/tmp` is routinely a different one. The fsync is what puts the bytes on the disk
/// before the name points at them, so a machine that loses power between the two is left
/// holding the previous version rather than an empty new one. The rename is what makes
/// the swap indivisible: a reader opening the path finds the whole of one version or the
/// whole of the other, and never the middle of this write.
///
/// The temporary is removed whatever happens -- after a failure because it is debris, and
/// after a success because the rename already consumed the name, so its removal is
/// expected to find nothing there.
///
/// This is the wrong writer for a file somebody watches with inotify: the rename gives the
/// path a new inode, and a watch registered on the old one never reports the change. Those
/// go through [`crate::fs::marker::write_marker`] instead.
///
/// # Errors
///
/// Returns [`AtomicWriteError`] if the parent directory cannot be created, no temporary
/// can be opened beside the target, the bytes cannot be written or flushed to the disk, or
/// the final rename fails. The target is untouched in every one of those cases.
pub fn atomic_write(path: &Path, text: &str) -> Result<(), AtomicWriteError> {
    fs::create_dir_all(parent_of(path)).map_err(|source| AtomicWriteError::Parents {
        path: path.to_path_buf(),
        source,
    })?;
    let (temporary, handle) =
        create_temporary(path).map_err(|source| AtomicWriteError::Temporary {
            path: path.to_path_buf(),
            source,
        })?;
    let outcome = fill_and_install(&temporary, handle, text, path);
    discard(&temporary);
    outcome
}

fn fill_and_install(
    temporary: &Path,
    mut handle: File,
    text: &str,
    path: &Path,
) -> Result<(), AtomicWriteError> {
    handle
        .write_all(text.as_bytes())
        .and_then(|()| handle.flush())
        .and_then(|()| handle.sync_all())
        .map_err(|source| AtomicWriteError::Write {
            path: temporary.to_path_buf(),
            source,
        })?;
    drop(handle);
    fs::rename(temporary, path).map_err(|source| AtomicWriteError::Install {
        path: path.to_path_buf(),
        source,
    })
}

/// The directory a path lives in. A bare filename has an empty parent rather than none,
/// and an empty path is not a directory anything can be created in, so both become `.`.
pub(crate) fn parent_of(path: &Path) -> &Path {
    match path.parent() {
        Some(directory) if !directory.as_os_str().is_empty() => directory,
        _ => Path::new("."),
    }
}

/// Open a private file beside `target`, named `.{target}.{token}` so that a directory
/// listing shows what it belongs to and a sweep can recognise it as debris.
///
/// Created with `create_new`, so the name is claimed by the open itself and two writers
/// racing for one name cannot both believe they own it. The mode matches what `mkstemp`
/// gives: nothing that passes through here is readable by anyone else, however briefly.
pub(crate) fn create_temporary(target: &Path) -> io::Result<(PathBuf, File)> {
    let directory = parent_of(target);
    let name = target.file_name().unwrap_or_default().to_string_lossy();
    for attempt in 0..NAME_ATTEMPTS {
        let candidate = directory.join(format!(".{name}.{}", token(attempt)));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&candidate)
        {
            Ok(handle) => return Ok((candidate, handle)),
            Err(error) if error.kind() != ErrorKind::AlreadyExists => return Err(error),
            Err(_) => (),
        }
    }
    Err(io::Error::new(
        ErrorKind::AlreadyExists,
        "no free temporary name beside the target",
    ))
}

/// Remove a temporary, ignoring the failure to find one: the successful path renames it
/// away first, so its absence here is the ordinary case rather than a problem.
pub(crate) fn discard(temporary: &Path) {
    drop(fs::remove_file(temporary));
}

fn token(attempt: u32) -> String {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.subsec_nanos());
    let serial = SERIAL.fetch_add(1, Ordering::Relaxed);
    format!("{:x}{nanos:x}{serial:x}{attempt:x}", std::process::id())
}

#[cfg(test)]
fn stray_temporaries(directory: &Path) -> usize {
    fs::read_dir(directory)
        .expect("directory is readable")
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .as_os_str()
                .to_string_lossy()
                .starts_with('.')
        })
        .count()
}

#[cfg(test)]
mod tests {
    use super::{atomic_write, stray_temporaries};
    use crate::fs::scratch::Scratch;
    use std::ffi::OsStr;
    use std::fs;
    use std::os::unix::fs::MetadataExt;

    fn inode(path: &std::path::Path) -> u64 {
        fs::metadata(path).expect("target exists").ino()
    }

    #[test]
    fn replacing_a_file_gives_it_a_new_inode() {
        let scratch = Scratch::new("atomic-inode");
        let target = scratch.join("preferences.toml");
        fs::write(&target, "before").expect("first write");
        let before = inode(&target);

        atomic_write(&target, "after").expect("atomic write");

        assert_eq!(fs::read_to_string(&target).expect("read back"), "after");
        assert_ne!(inode(&target), before, "a rename must not reuse the inode");
    }

    #[test]
    fn the_parent_directory_is_created_and_no_temporary_is_left() {
        let scratch = Scratch::new("atomic-parents");
        let target = scratch.join("deep").join("nested").join("hypridle.conf");

        atomic_write(&target, "listener {}\n").expect("atomic write");

        assert_eq!(
            fs::read_to_string(&target).expect("read back"),
            "listener {}\n"
        );
        assert_eq!(stray_temporaries(super::parent_of(&target)), 0);
    }

    #[test]
    fn a_temporary_is_named_after_its_target() {
        let scratch = Scratch::new("atomic-naming");
        let target = scratch.join("displays.lua");
        let (temporary, handle) = super::create_temporary(&target).expect("temporary opens");
        drop(handle);

        let name = temporary
            .file_name()
            .unwrap_or(OsStr::new(""))
            .to_string_lossy()
            .into_owned();
        assert!(
            name.starts_with(".displays.lua."),
            "unexpected temporary name {name}"
        );
        assert_eq!(super::parent_of(&temporary), scratch.path());

        super::discard(&temporary);
        assert!(!temporary.exists());
        super::discard(&temporary);
    }
}
