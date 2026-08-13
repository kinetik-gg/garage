//! [`index_rows`] -- the depth-bounded directory walk that produces every row
//! [`crate::refresh::refresh_index`] writes. Names and paths only, never file contents.

use std::path::Path;

use crate::fold::casefold;

/// Directory names a walk never descends into, regardless of depth -- build output and
/// dependency trees that are typically enormous and never worth searching by filename.
const EXCLUDED_DIRECTORIES: [&str; 6] = [
    "node_modules",
    "__pycache__",
    "target",
    "build",
    "dist",
    "vendor",
];

/// `"directory"` or `"file"` -- stored in the `kind` column and returned verbatim in a
/// search hit, so the two spellings are exactly the Python's string literals rather than a
/// Rust-idiomatic pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Directory,
    File,
}

impl Kind {
    #[must_use]
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Directory => "directory",
            Self::File => "file",
        }
    }
}

/// One row of the `files` table, before it reaches `SQLite`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FileRow {
    pub path: String,
    pub name: String,
    pub name_fold: String,
    pub parent: String,
    pub path_fold: String,
    pub kind: Kind,
    pub modified_ns: i64,
}

/// Whether a directory name is skipped when deciding what to descend into -- a dotfile, or
/// one of the fixed build/dependency names in [`EXCLUDED_DIRECTORIES`].
///
/// The dotfile half of this check can never fire from [`index_rows`]'s own call site: every
/// entry whose name starts with `.` is already skipped before a directory is considered for
/// descent. It is kept here anyway, matching the Python's `excluded_directory`, which is
/// written as a general-purpose predicate rather than one inlined at its one call site.
#[must_use]
pub(crate) fn excluded_directory(name: &str) -> bool {
    name.starts_with('.') || EXCLUDED_DIRECTORIES.contains(&name)
}

/// Walk `root` up to `max_depth` levels deep, in original (top level: no dots), returning
/// every directory and file found -- names and paths only, contents never touched.
///
/// Skipped outright, at every level: dotfiles and symlinks (never yielded, never descended
/// into), and anything that is neither a directory nor a regular file once its type is
/// known. A directory whose entries cannot be listed -- gone, not a directory after all, or
/// permission denied -- contributes nothing and does not stop its siblings from being
/// walked. `max_depth` counts from 1 at `root`'s direct children; a child at exactly
/// `max_depth` is still yielded, but never descended into.
#[must_use]
pub(crate) fn index_rows(root: &Path, max_depth: i64) -> Vec<FileRow> {
    let mut rows = Vec::new();
    let mut stack = vec![(root.to_path_buf(), 0i64)];
    while let Some((directory, parent_depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
        entries.sort_by(|left, right| {
            casefold(&left.file_name().to_string_lossy())
                .cmp(&casefold(&right.file_name().to_string_lossy()))
        });
        for entry in entries {
            let Some((row, descend)) = visit_entry(&entry, parent_depth, max_depth) else {
                continue;
            };
            if descend {
                stack.push((entry.path(), parent_depth + 1));
            }
            rows.push(row);
        }
    }
    rows
}

/// One directory entry, turned into its row plus whether it should be descended into --
/// `None` for anything skipped outright: a dotfile, a symlink, neither a directory nor a
/// regular file, or one whose type or modification time could not be read.
fn visit_entry(
    entry: &std::fs::DirEntry,
    parent_depth: i64,
    max_depth: i64,
) -> Option<(FileRow, bool)> {
    let name = entry.file_name().to_string_lossy().into_owned();
    let file_type = entry.file_type().ok()?;
    if name.starts_with('.') || file_type.is_symlink() {
        return None;
    }
    let depth = parent_depth + 1;
    let is_directory = file_type.is_dir();
    let is_file = file_type.is_file();
    if !(is_directory || is_file) {
        return None;
    }
    let modified_ns = modified_ns(entry).ok()?;
    let path = entry.path();
    let parent = path.parent()?;
    let path_text = path.to_string_lossy().into_owned();
    let row = FileRow {
        path: path_text.clone(),
        name: name.clone(),
        name_fold: casefold(&name),
        parent: parent.to_string_lossy().into_owned(),
        path_fold: casefold(&path_text),
        kind: if is_directory {
            Kind::Directory
        } else {
            Kind::File
        },
        modified_ns,
    };
    let descend = is_directory && depth < max_depth && !excluded_directory(&name);
    Some((row, descend))
}

/// `stat.st_mtime_ns`: whole nanoseconds since the epoch, from the same `lstat` the entry's
/// metadata already came from.
fn modified_ns(entry: &std::fs::DirEntry) -> std::io::Result<i64> {
    use std::os::unix::fs::MetadataExt as _;
    let metadata = entry.metadata()?;
    Ok(metadata.mtime() * 1_000_000_000 + metadata.mtime_nsec())
}

#[cfg(test)]
mod tests {
    use super::{excluded_directory, index_rows, Kind};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    static SERIAL: AtomicU64 = AtomicU64::new(0);

    struct Scratch {
        path: PathBuf,
    }

    impl Scratch {
        fn new(label: &str) -> Self {
            let serial = SERIAL.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "garage-file-index-scan-{label}-{}-{serial}",
                std::process::id()
            ));
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            drop(fs::remove_dir_all(&self.path));
        }
    }

    #[test]
    fn excluded_directory_matches_dotfiles_and_the_fixed_set() {
        assert!(excluded_directory(".git"));
        assert!(excluded_directory("node_modules"));
        assert!(excluded_directory("target"));
        assert!(!excluded_directory("Documents"));
    }

    /// Mirrors the Python's
    /// `test_depth_hidden_directories_and_symlinks_are_bounded`.
    #[test]
    fn depth_dotfiles_excluded_names_and_symlinks_are_bounded() {
        let scratch = Scratch::new("bounds");
        let root = scratch.path().join("Projects");
        fs::create_dir_all(root.join("one/two")).unwrap();
        fs::write(root.join("one/visible.txt"), "").unwrap();
        fs::write(root.join("one/two/too-deep.txt"), "").unwrap();
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::write(root.join(".git/secret.txt"), "").unwrap();
        fs::create_dir_all(root.join("node_modules")).unwrap();
        fs::write(root.join("node_modules/package.js"), "").unwrap();
        std::os::unix::fs::symlink(root.join("one"), root.join("linked")).unwrap();

        let rows = index_rows(&root, 2);
        let names: Vec<&str> = rows.iter().map(|row| row.name.as_str()).collect();
        assert!(names.contains(&"visible.txt"));
        for hidden in ["too-deep.txt", "secret.txt", "package.js", "linked"] {
            assert!(!names.contains(&hidden), "{hidden} should not be indexed");
        }
    }

    #[test]
    fn a_file_at_exactly_max_depth_is_indexed_but_not_descended() {
        let scratch = Scratch::new("exact-depth");
        let root = scratch.path().join("root");
        fs::create_dir_all(root.join("a/b")).unwrap();
        fs::write(root.join("a/b/leaf.txt"), "").unwrap();
        fs::write(root.join("a/too-deep.txt"), "").unwrap();

        // "a" is depth 1, "b" is depth 2, "leaf.txt" would be depth 3.
        let rows = index_rows(&root, 2);
        let names: Vec<&str> = rows.iter().map(|row| row.name.as_str()).collect();
        assert!(names.contains(&"a"));
        assert!(names.contains(&"b"));
        assert!(!names.contains(&"leaf.txt"));
    }

    #[test]
    fn kind_and_fold_columns_are_populated() {
        let scratch = Scratch::new("kind");
        let root = scratch.path().join("root");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("Budget.ODS"), "").unwrap();
        fs::create_dir_all(root.join("Sub")).unwrap();

        let rows = index_rows(&root, 8);
        let file = rows.iter().find(|row| row.name == "Budget.ODS").unwrap();
        assert_eq!(file.kind, Kind::File);
        assert_eq!(file.name_fold, "budget.ods");
        assert!(file.path_fold.ends_with("budget.ods"));

        let dir = rows.iter().find(|row| row.name == "Sub").unwrap();
        assert_eq!(dir.kind, Kind::Directory);
    }

    #[test]
    fn a_missing_root_yields_nothing() {
        let scratch = Scratch::new("missing");
        let rows = index_rows(&scratch.path().join("nonexistent"), 8);
        assert!(rows.is_empty());
    }
}
