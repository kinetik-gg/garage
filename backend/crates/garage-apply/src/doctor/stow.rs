//! `stow_state()`, `dangling_repo_links()` and `managed_paths()`: what stow has and has not
//! placed, and the links a version bump left pointing at nothing.
//!
//! `managed_paths()` walks the checkout's `desktop/` tree the way `stow --no-folding` would,
//! against the path-anchored patterns in `.stow-local-ignore` -- only the anchored ones,
//! since the rest of that file repeats stow's own name-based defaults, which are applied by
//! name rather than by path. An anchored pattern excludes the toolkit configs Garage rewrites
//! at runtime, and a manifest that included them would report every theme switch as a broken
//! link.
//!
//! `stow_state()` reports five outcomes per managed path: `linked` (a symlink into this
//! checkout that resolves -- healthy), `other` (a stow link at the right relative path inside
//! a *different* checkout -- the desktop works, it is just served from elsewhere, which is
//! what a moved or duplicated clone looks like and what a restow corrects), `broken` (a link
//! into this checkout whose target is gone, or a link to something else entirely), `plain` (a
//! real file sitting where a link belongs, which bootstrap backs up), and `missing` (nothing
//! there at all, what a file added since the last stow looks like).
//!
//! `dangling_repo_links()` finds the gap no restow can close: a file deleted from the
//! repository between two versions is not in the *current* managed set, so a plain rescan
//! never considers the stale link it left behind, and `stow --restow` only unlinks what the
//! package still contains today. This walks the other direction instead -- scan `$HOME`'s
//! known Garage-managed roots, keep the links that point into this checkout and no longer
//! resolve -- which needs no record of what a previous version shipped. Scoped tightly to
//! four roots on purpose: `$HOME` is full of symlinks that dangle legitimately (Chrome's
//! `SingletonLock`, Discord's IPC sockets, editor session files), and a sweep considering
//! those would eventually delete one.
//!
//! Everything here inspects the filesystem and returns a report value, not
//! `Result<(), ApplyError>` over a [`SessionCx`](crate::cx::SessionCx).

use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use garage_core::paths::Paths;
use regex::Regex;

use super::DoctorCx;

/// Where a stow link from this checkout can legitimately be.
///
/// Everything under `desktop/` lands in one of these four, so a dangling repo-pointing link
/// cannot be anywhere else -- and scoping the sweep this tightly is what keeps it from ever
/// considering the many links elsewhere in `$HOME` that dangle on purpose.
pub(crate) fn scan_roots(paths: &Paths) -> [PathBuf; 4] {
    [
        paths.config_home.clone(),
        paths.home.join(".local/bin"),
        paths.home.join(".local/share"),
        paths.home.join("Wallpaper"),
    ]
}

/// `os.path.normpath()`: collapse `//`, `.` and `..` lexically, touching no disk.
///
/// Transcribed from `posixpath.normpath` rather than approximated, including the rule nobody
/// remembers: exactly two leading slashes are preserved and any other number collapses to
/// one.
fn normpath(path: &Path) -> PathBuf {
    let raw = path.to_string_lossy().into_owned();
    if raw.is_empty() {
        return PathBuf::from(".");
    }
    let leading = raw.chars().take_while(|letter| *letter == '/').count();
    let initial = match leading {
        0 => "",
        2 => "//",
        _ => "/",
    };
    let absolute = leading > 0;
    let mut kept: Vec<&str> = Vec::new();
    for part in raw.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part != ".."
            || (!absolute && kept.is_empty())
            || kept.last().is_some_and(|last| *last == "..")
        {
            kept.push(part);
        } else if !kept.is_empty() {
            kept.pop();
        }
    }
    let joined = format!("{initial}{}", kept.join("/"));
    PathBuf::from(if joined.is_empty() {
        ".".to_owned()
    } else {
        joined
    })
}

/// `os.path.realpath(strict=False)`: resolve every symlink, tolerating a target that is not
/// there.
///
/// `std::fs::canonicalize` is the wrong tool: it fails outright on a path whose last
/// component is missing, and "the target is not there" is precisely the state
/// [`dangling_repo_links`] is looking for.
fn realpath(path: &Path) -> PathBuf {
    // Ten more than the Linux kernel's own `SYMLOOP_MAX`, so a loop stops being resolved
    // instead of hanging, which is what `realpath` does with one too.
    const HOPS: u32 = 50;

    let mut pending: Vec<String> = path
        .to_string_lossy()
        .split('/')
        .rev()
        .map(str::to_owned)
        .collect();
    let mut resolved = PathBuf::from(if path.is_absolute() { "/" } else { "" });
    let mut hops = 0;
    while let Some(part) = pending.pop() {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            resolved.pop();
            continue;
        }
        let candidate = resolved.join(&part);
        match fs::read_link(&candidate) {
            Ok(target) if hops < HOPS => {
                hops += 1;
                if target.is_absolute() {
                    resolved = PathBuf::from("/");
                }
                pending.extend(target.to_string_lossy().split('/').rev().map(str::to_owned));
            }
            Ok(_) | Err(_) => resolved = candidate,
        }
    }
    resolved
}

/// Where a symlink points after exactly one hop, absolute, unresolved.
///
/// The same rule `bootstrap.sh`'s `link_hop()` follows, and for the same reason: full
/// resolution is wrong for judging a stow link, because a tracked file may itself be a
/// symlink and resolving it lands outside the checkout. `None` when the path is not a
/// symlink.
pub(crate) fn link_hop(path: &Path) -> Option<PathBuf> {
    let target = fs::read_link(path).ok()?;
    let joined = if target.is_absolute() {
        target
    } else {
        path.parent().unwrap_or(Path::new("")).join(target)
    };
    Some(normpath(&joined))
}

/// True when a symlink is one of ours in this checkout.
///
/// Lexically first, then through full resolution, so it holds whether or not the checkout's
/// own path contains symlinked components.
pub(crate) fn points_into(path: &Path, tree: &Path) -> bool {
    if link_hop(path).is_some_and(|hop| hop.starts_with(tree)) {
        return true;
    }
    realpath(path).starts_with(tree)
}

/// The path-anchored patterns from `desktop/.stow-local-ignore`.
///
/// Only those: the rest of that file repeats stow's name-based defaults, which
/// [`managed_paths`] applies by name. A pattern the engine refuses is skipped rather than
/// fatal, which is `re.error: continue`.
fn stow_ignores(root: &Path) -> Vec<Regex> {
    let Ok(text) = fs::read_to_string(root.join("desktop/.stow-local-ignore")) else {
        return Vec::new();
    };
    text.lines()
        .map(str::trim)
        .filter(|line| line.starts_with("^/"))
        .filter_map(|line| Regex::new(line).ok())
        .collect()
}

/// One directory, split the way `os.walk` splits it: names that are directories (following
/// symlinks, as `os.scandir().is_dir()` does) and everything else.
fn scan(here: &Path) -> (Vec<String>, Vec<String>) {
    let mut directories = Vec::new();
    let mut files = Vec::new();
    let Ok(entries) = fs::read_dir(here) else {
        return (directories, files);
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if entry.path().is_dir() {
            directories.push(name);
        } else {
            files.push(name);
        }
    }
    (directories, files)
}

/// Everything stow will place, as paths relative to `$HOME`.
///
/// The Python side of `bootstrap.sh`'s `managed_paths()` plus [`stow_ignores`]: with
/// `--no-folding` every leaf becomes its own symlink, so the managed set is exactly this file
/// list.
pub(crate) fn managed_paths(root: &Path) -> Vec<String> {
    let tree = root.join("desktop");
    let patterns = stow_ignores(root);
    let mut paths: Vec<String> = Vec::new();
    let mut queue = vec![tree.clone()];
    while let Some(here) = queue.pop() {
        let (directories, files) = scan(&here);
        // A symlinked directory is a leaf stow would place as one link, not a directory to
        // descend into. `.git` is never inside `desktop/`, and is skipped for the same reason
        // bootstrap skips it.
        let (leaves, branches): (Vec<String>, Vec<String>) = directories
            .into_iter()
            .partition(|name| here.join(name).is_symlink());
        for name in branches {
            let Ok(relative) = here.join(&name).strip_prefix(&tree).map(Path::to_path_buf) else {
                continue;
            };
            let anchored = format!("/{}", relative.display());
            // An ignored directory prunes its whole subtree, matching stow: the pattern names
            // the directory, so nothing under it is ever placed.
            if name != ".git" && !patterns.iter().any(|pattern| pattern.is_match(&anchored)) {
                queue.push(here.join(&name));
            }
        }
        for name in files.into_iter().chain(leaves) {
            let Ok(relative) = here.join(&name).strip_prefix(&tree).map(Path::to_path_buf) else {
                continue;
            };
            if name == ".stow-local-ignore" || name == ".gitignore" || name.ends_with('~') {
                continue;
            }
            let relative = relative.display().to_string();
            if patterns
                .iter()
                .any(|pattern| pattern.is_match(&format!("/{relative}")))
            {
                continue;
            }
            paths.push(relative);
        }
    }
    paths.sort();
    paths
}

/// How each managed path currently resolves from `$HOME`. See the module doc for the five
/// outcomes and why they are five rather than two.
#[derive(Debug, Default, Clone)]
pub(crate) struct StowState {
    /// How many resolve into this checkout. The healthy case, counted rather than listed.
    pub(crate) linked: usize,
    /// Linked, but into a different checkout of the same tree.
    pub(crate) other: Vec<String>,
    /// A link into this checkout whose target is gone, or a link to something else entirely.
    pub(crate) broken: Vec<String>,
    /// A real file sitting where a link belongs.
    pub(crate) plain: Vec<String>,
    /// Nothing there at all.
    pub(crate) missing: Vec<String>,
    /// The other checkouts `other` points into, deduplicated and sorted -- a `set` in the
    /// Python, printed through `sorted()`.
    pub(crate) others: BTreeSet<String>,
    /// Every managed path, however it resolved.
    pub(crate) total: usize,
}

/// Classify every managed path. See [`StowState`].
pub(crate) fn stow_state(cx: &DoctorCx<'_>) -> StowState {
    let mut state = StowState::default();
    for relative in managed_paths(&cx.root) {
        state.total += 1;
        let target = cx.paths.home.join(&relative);
        if target.is_symlink() {
            classify_link(cx, &mut state, relative, &target);
        } else if target.exists() {
            state.plain.push(relative);
        } else {
            state.missing.push(relative);
        }
    }
    state
}

/// The symlink half of [`stow_state`]: linked, other, or broken.
fn classify_link(cx: &DoctorCx<'_>, state: &mut StowState, relative: String, target: &Path) {
    if points_into(target, &cx.tree) {
        if target.exists() {
            state.linked += 1;
        } else {
            state.broken.push(relative);
        }
        return;
    }
    // The same relative path inside a *different* checkout, which is what a moved or
    // duplicated clone looks like: the desktop works, it is just served from elsewhere.
    let suffix = format!("/desktop/{relative}");
    let hop = link_hop(target).map(|hop| hop.to_string_lossy().into_owned());
    match hop {
        Some(hop) if hop.ends_with(&suffix) && target.exists() => {
            state
                .others
                .insert(hop.get(..hop.len() - suffix.len()).unwrap_or("").to_owned());
            state.other.push(relative);
        }
        Some(_) | None => state.broken.push(relative),
    }
}

/// Symlinks into this checkout whose target no longer exists.
///
/// The gap no restow can close -- see the module doc. Two properties are load-bearing: the
/// search is scoped to [`scan_roots`], and the target must resolve into *this* checkout, so a
/// dangling link into another Garage clone is left to that clone. A symlinked directory is
/// reported rather than descended into, which is what `os.walk`'s default gives the Python.
pub(crate) fn dangling_repo_links(cx: &DoctorCx<'_>) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = Vec::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();
    for scan_root in scan_roots(cx.paths) {
        if scan_root.is_dir() {
            sweep(cx, scan_root, &mut seen, &mut found);
        }
    }
    found.sort();
    found
}

/// One scan root, walked. Split out of [`dangling_repo_links`] because the walk is the part
/// with the two rules in it, and the loop above is only "for each of the four roots".
fn sweep(
    cx: &DoctorCx<'_>,
    scan_root: PathBuf,
    seen: &mut HashSet<PathBuf>,
    found: &mut Vec<PathBuf>,
) {
    let mut queue = vec![scan_root];
    while let Some(here) = queue.pop() {
        let (directories, files) = scan(&here);
        for name in directories.iter().chain(files.iter()) {
            let path = here.join(name);
            // A symlinked directory is judged here rather than descended into, which is what
            // `os.walk`'s "do not follow links" gives the Python. `exists()` follows the link,
            // so false on a symlink is precisely "the target is not there".
            let dangles = path.is_symlink()
                && seen.insert(path.clone())
                && !path.exists()
                && points_into(&path, &cx.tree);
            if dangles {
                found.push(path);
            } else if !path.is_symlink() && directories.contains(name) {
                queue.push(path);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{normpath, realpath};

    /// Every expectation is `python3 -c 'import os.path; print(os.path.normpath(...))'`.
    #[test]
    fn normpath_is_posixpaths_normpath_including_the_double_slash_rule() {
        let cases = [
            ("/a/b/../c", "/a/c"),
            ("a//b", "a/b"),
            ("//a/b", "//a/b"),
            ("///a/b", "/a/b"),
            ("/a/./b/", "/a/b"),
            ("/../a", "/a"),
            ("../../a", "../../a"),
            ("", "."),
            ("/", "/"),
        ];
        for (raw, expected) in cases {
            assert_eq!(normpath(Path::new(raw)), PathBuf::from(expected), "{raw}");
        }
    }

    #[test]
    fn realpath_answers_for_a_target_that_is_not_there() {
        let scratch = std::env::temp_dir().join(format!(
            "garage-doctor-realpath-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        drop(std::fs::remove_dir_all(&scratch));
        std::fs::create_dir_all(scratch.join("real")).expect("scratch");
        std::os::unix::fs::symlink(scratch.join("real"), scratch.join("link")).expect("symlink");
        // Through the link, onto a name that does not exist: canonicalize() refuses this and
        // the Python's realpath does not.
        assert_eq!(
            realpath(&scratch.join("link/gone")),
            realpath(&scratch).join("real/gone")
        );
        drop(std::fs::remove_dir_all(&scratch));
    }
}
