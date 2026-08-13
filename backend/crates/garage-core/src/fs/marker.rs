//! `write_marker` — in-place truncate, inode-preserving. Never an atomic rename.

use std::fs::{self, File};
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::fs::atomic::parent_of;

/// Why a marker could not be written. Every variant carries the marker's own path: there
/// is no temporary in this writer, so there is only ever one file to name.
#[derive(Debug, Error)]
pub enum MarkerWriteError {
    /// The directory the marker belongs in could not be created.
    #[error("{}: could not create the directory it belongs in: {source}", path.display())]
    Parents {
        /// The marker.
        path: PathBuf,
        /// The underlying failure.
        source: io::Error,
    },
    /// A symlink was sitting at the path and could not be removed.
    #[error("{}: could not remove the symlink in its place: {source}", path.display())]
    Unlink {
        /// The marker.
        path: PathBuf,
        /// The underlying failure.
        source: io::Error,
    },
    /// The marker could not be opened, written or flushed to the disk.
    #[error("{}: could not be written: {source}", path.display())]
    Write {
        /// The marker.
        path: PathBuf,
        /// The underlying failure.
        source: io::Error,
    },
}

/// Write a small file in place, keeping its inode.
///
/// [`crate::fs::atomic::atomic_write`] renames a temporary over the target, which is
/// correct for config files but invisible to an inotify watch registered on the original
/// inode: the watcher follows the replaced file and never reports the change. Quickshell
/// watches these markers, so they have to be truncated in place.
///
/// A symlink at the target is removed rather than followed, and that is not hypothetical:
/// every path under `~/.config` that this checkout tracks is a stow link back into the
/// repository, so a file promoted from *tracked* to *generated* -- as the whole palette
/// was -- leaves a link behind on any machine that has not restowed since. Opening it for
/// writing would follow it and truncate the tracked file, editing the repository from a
/// theme switch. `garage update` sweeps those links (`dangling_repo_links`), but a render
/// can reach the path first, and once is enough.
///
/// # Errors
///
/// Returns [`MarkerWriteError`] if the parent directory cannot be created, a symlink in
/// the way cannot be removed, or the marker cannot be opened, written or flushed to the
/// disk. Because there is no rename to hold the new contents back, a failure part-way
/// through the write leaves the marker truncated -- which is the cost of the inode being
/// the thing that has to survive.
pub fn write_marker(path: &Path, text: &str) -> Result<(), MarkerWriteError> {
    fs::create_dir_all(parent_of(path)).map_err(|source| MarkerWriteError::Parents {
        path: path.to_path_buf(),
        source,
    })?;
    if is_symlink(path) {
        fs::remove_file(path).map_err(|source| MarkerWriteError::Unlink {
            path: path.to_path_buf(),
            source,
        })?;
    }
    truncate_in_place(path, text).map_err(|source| MarkerWriteError::Write {
        path: path.to_path_buf(),
        source,
    })
}

/// Whether a symlink is sitting at the path, without following it. A path that is not
/// there at all is not a symlink, so an absent marker takes the ordinary route.
fn is_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|found| found.file_type().is_symlink())
}

fn truncate_in_place(path: &Path, text: &str) -> io::Result<()> {
    let mut handle = File::create(path)?;
    handle.write_all(text.as_bytes())?;
    handle.flush()?;
    handle.sync_all()
}

#[cfg(test)]
mod tests {
    use super::write_marker;
    use crate::fs::scratch::Scratch;
    use std::fs;
    use std::os::unix::fs::MetadataExt as _;

    fn inode(path: &std::path::Path) -> u64 {
        fs::metadata(path).expect("marker exists").ino()
    }

    #[test]
    fn rewriting_a_marker_keeps_its_inode() {
        let scratch = Scratch::new("marker-inode");
        let marker = scratch.join("accent");

        write_marker(&marker, "#89b4fa\n").expect("first write");
        let first = inode(&marker);
        write_marker(&marker, "#f38ba8\n").expect("second write");

        assert_eq!(
            inode(&marker),
            first,
            "an inotify watch is registered on this inode"
        );
        assert_eq!(fs::read_to_string(&marker).expect("read back"), "#f38ba8\n");
    }

    #[test]
    fn a_symlink_in_the_way_is_unlinked_and_its_target_survives() {
        let scratch = Scratch::new("marker-symlink");
        let tracked = scratch.join("tracked-palette");
        fs::write(&tracked, "checked in\n").expect("tracked file");
        let marker = scratch.join("material");
        std::os::unix::fs::symlink(&tracked, &marker).expect("stow-style link");

        write_marker(&marker, "generated\n").expect("write over the link");

        assert!(
            !marker.is_symlink(),
            "the link must be replaced, not followed"
        );
        assert_eq!(
            fs::read_to_string(&marker).expect("read the marker"),
            "generated\n"
        );
        assert_eq!(
            fs::read_to_string(&tracked).expect("read the repository file"),
            "checked in\n",
            "a theme switch must not edit the checkout"
        );
    }

    #[test]
    fn a_missing_parent_directory_is_created() {
        let scratch = Scratch::new("marker-parents");
        let marker = scratch.join("generated").join("color-scheme");

        write_marker(&marker, "dark\n").expect("write");

        assert_eq!(fs::read_to_string(&marker).expect("read back"), "dark\n");
    }
}
