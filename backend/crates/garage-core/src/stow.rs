//! Stow's managed tree and the five filesystem outcomes shared by doctor and reconcile.
//!
//! The tree walk mirrors `stow --no-folding`: every non-ignored leaf becomes one link in
//! `$HOME`, while directories remain real directories. Only path-anchored regular
//! expressions from `.stow-local-ignore` participate in the path test; the file-name defaults
//! are applied directly. A generated toolkit file must stay out of this set, or each theme
//! switch would look like link damage.
//!
//! Classification deliberately distinguishes a healthy link here, a healthy link into a
//! moved/other Garage checkout, a broken or unrelated link, a plain path, and a missing path.
//! Doctor reports that analysis; reconcile turns precisely the same answer into a plan.

use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use regex::Regex;

use crate::paths::Paths;

/// Where a stow link from this checkout can legitimately be.
#[must_use]
pub fn scan_roots(paths: &Paths) -> [PathBuf; 4] {
    [
        paths.config_home.clone(),
        paths.home.join(".local/bin"),
        paths.home.join(".local/share"),
        paths.home.join("Wallpaper"),
    ]
}

/// Everything stow will place, as paths relative to `$HOME`.
#[must_use]
pub fn managed_paths(root: &Path) -> Vec<String> {
    let tree = root.join("desktop");
    let patterns = stow_ignores(root);
    let mut paths = Vec::new();
    let mut queue = vec![tree.clone()];
    while let Some(here) = queue.pop() {
        walk_one(&tree, &here, &patterns, &mut queue, &mut paths);
    }
    paths.sort();
    paths
}

/// How one desired stow leaf currently resolves from `$HOME`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StowOutcome {
    /// A live symlink into this checkout.
    Linked,
    /// A live link at the same relative path in another Garage checkout.
    Other {
        /// The other checkout's root.
        checkout: PathBuf,
    },
    /// A dangling link into this checkout, or an unrelated symlink.
    Broken,
    /// A real file or directory where the link belongs.
    Plain,
    /// No directory entry exists at all.
    Missing,
}

/// Classify one relative path with the same rules doctor uses for the full tree.
#[must_use]
pub fn classify(root: &Path, home: &Path, relative: &str) -> StowOutcome {
    let target = home.join(relative);
    if target.is_symlink() {
        return classify_link(root, relative, &target);
    }
    if target.exists() {
        StowOutcome::Plain
    } else {
        StowOutcome::Missing
    }
}

/// How every managed stow leaf currently resolves from `$HOME`.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct StowState {
    /// How many resolve into this checkout.
    pub linked: usize,
    /// Live links into a different Garage checkout.
    pub other: Vec<String>,
    /// Dangling or unrelated links.
    pub broken: Vec<String>,
    /// Real paths sitting where links belong.
    pub plain: Vec<String>,
    /// Paths with no directory entry.
    pub missing: Vec<String>,
    /// Other checkout roots, deduplicated and sorted.
    pub others: BTreeSet<String>,
    /// Every managed leaf, however it resolved.
    pub total: usize,
}

/// Classify the complete managed tree. See [`StowState`].
#[must_use]
pub fn stow_state(root: &Path, home: &Path) -> StowState {
    let mut state = StowState::default();
    for relative in managed_paths(root) {
        state.total += 1;
        record_outcome(&mut state, &relative, classify(root, home, &relative));
    }
    state
}

/// Symlinks into this checkout whose target no longer exists.
#[must_use]
pub fn dangling_repo_links(paths: &Paths, root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut seen = HashSet::new();
    for scan_root in scan_roots(paths) {
        if scan_root.is_dir() {
            sweep(root, scan_root, &mut seen, &mut found);
        }
    }
    found.sort();
    found
}

/// Every symlink under the managed roots that resolves or points lexically into this checkout.
///
/// Unlike [`dangling_repo_links`], healthy links are included. Guarded prune uses this reverse
/// walk to find a link that a previous manifest shipped but the current tree no longer names.
#[must_use]
pub fn checkout_links(paths: &Paths, root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut seen = HashSet::new();
    for scan_root in scan_roots(paths) {
        if scan_root.is_dir() {
            sweep_links(root, scan_root, &mut seen, &mut found);
        }
    }
    found.sort();
    found
}

/// Where a symlink points after exactly one hop, absolute and unresolved.
#[must_use]
pub fn link_hop(path: &Path) -> Option<PathBuf> {
    let target = fs::read_link(path).ok()?;
    let joined = if target.is_absolute() {
        target
    } else {
        path.parent().unwrap_or(Path::new("")).join(target)
    };
    Some(normpath(&joined))
}

/// Whether a symlink points into `tree`, including through symlinked checkout components.
#[must_use]
pub fn points_into(path: &Path, tree: &Path) -> bool {
    if link_hop(path).is_some_and(|hop| hop.starts_with(tree)) {
        return true;
    }
    realpath(path).starts_with(tree)
}

fn walk_one(
    tree: &Path,
    here: &Path,
    patterns: &[Regex],
    queue: &mut Vec<PathBuf>,
    paths: &mut Vec<String>,
) {
    let (directories, files) = scan(here);
    let (leaves, branches): (Vec<String>, Vec<String>) = directories
        .into_iter()
        .partition(|name| here.join(name).is_symlink());
    for name in branches {
        maybe_descend(tree, here, patterns, queue, &name);
    }
    for name in files.into_iter().chain(leaves) {
        maybe_record(tree, here, patterns, paths, &name);
    }
}

fn maybe_descend(
    tree: &Path,
    here: &Path,
    patterns: &[Regex],
    queue: &mut Vec<PathBuf>,
    name: &str,
) {
    let Ok(relative) = here.join(name).strip_prefix(tree).map(Path::to_path_buf) else {
        return;
    };
    let anchored = format!("/{}", relative.display());
    let ignored = name == ".git" || patterns.iter().any(|pattern| pattern.is_match(&anchored));
    if !ignored {
        queue.push(here.join(name));
    }
}

fn maybe_record(tree: &Path, here: &Path, patterns: &[Regex], paths: &mut Vec<String>, name: &str) {
    let Ok(relative) = here.join(name).strip_prefix(tree).map(Path::to_path_buf) else {
        return;
    };
    if matches!(name, ".stow-local-ignore" | ".gitignore") || name.ends_with('~') {
        return;
    }
    let relative = relative.display().to_string();
    if !patterns
        .iter()
        .any(|pattern| pattern.is_match(&format!("/{relative}")))
    {
        paths.push(relative);
    }
}

fn record_outcome(state: &mut StowState, relative: &str, outcome: StowOutcome) {
    match outcome {
        StowOutcome::Linked => state.linked += 1,
        StowOutcome::Other { checkout } => {
            state.others.insert(checkout.display().to_string());
            state.other.push(relative.to_owned());
        }
        StowOutcome::Broken => state.broken.push(relative.to_owned()),
        StowOutcome::Plain => state.plain.push(relative.to_owned()),
        StowOutcome::Missing => state.missing.push(relative.to_owned()),
    }
}

fn classify_link(root: &Path, relative: &str, target: &Path) -> StowOutcome {
    if points_into(target, &root.join("desktop")) {
        return if target.exists() {
            StowOutcome::Linked
        } else {
            StowOutcome::Broken
        };
    }
    let suffix = format!("/desktop/{relative}");
    let hop = link_hop(target).map(|path| path.to_string_lossy().into_owned());
    match hop {
        Some(hop) if hop.ends_with(&suffix) && target.exists() => {
            let end = hop.len().saturating_sub(suffix.len());
            StowOutcome::Other {
                checkout: PathBuf::from(hop.get(..end).unwrap_or("")),
            }
        }
        Some(_) | None => StowOutcome::Broken,
    }
}

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

fn sweep(root: &Path, scan_root: PathBuf, seen: &mut HashSet<PathBuf>, found: &mut Vec<PathBuf>) {
    let mut queue = vec![scan_root];
    while let Some(here) = queue.pop() {
        let (directories, files) = scan(&here);
        for name in directories.iter().chain(files.iter()) {
            let path = here.join(name);
            let dangles = path.is_symlink()
                && seen.insert(path.clone())
                && !path.exists()
                && points_into(&path, &root.join("desktop"));
            if dangles {
                found.push(path);
            } else if !path.is_symlink() && directories.contains(name) {
                queue.push(path);
            }
        }
    }
}

fn sweep_links(
    root: &Path,
    scan_root: PathBuf,
    seen: &mut HashSet<PathBuf>,
    found: &mut Vec<PathBuf>,
) {
    let mut queue = vec![scan_root];
    while let Some(here) = queue.pop() {
        let (directories, files) = scan(&here);
        for name in directories.iter().chain(files.iter()) {
            let path = here.join(name);
            let ours = path.is_symlink() && seen.insert(path.clone()) && points_into(&path, root);
            if ours {
                found.push(path);
            } else if !path.is_symlink() && directories.contains(name) {
                queue.push(path);
            }
        }
    }
}

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
        keep_component(&mut kept, part, absolute);
    }
    let joined = format!("{initial}{}", kept.join("/"));
    PathBuf::from(if joined.is_empty() { "." } else { &joined })
}

fn keep_component<'a>(kept: &mut Vec<&'a str>, part: &'a str, absolute: bool) {
    if part.is_empty() || part == "." {
        return;
    }
    let keep_parent = part == ".."
        && (!absolute && kept.is_empty() || kept.last().is_some_and(|last| *last == ".."));
    if part != ".." || keep_parent {
        kept.push(part);
    } else if !kept.is_empty() {
        kept.pop();
    }
}

fn realpath(path: &Path) -> PathBuf {
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
        resolve_component(&mut resolved, &mut pending, &part, &mut hops, HOPS);
    }
    resolved
}

fn resolve_component(
    resolved: &mut PathBuf,
    pending: &mut Vec<String>,
    part: &str,
    hops: &mut u32,
    limit: u32,
) {
    if part.is_empty() || part == "." {
        return;
    }
    if part == ".." {
        resolved.pop();
        return;
    }
    let candidate = resolved.join(part);
    match fs::read_link(&candidate) {
        Ok(target) if *hops < limit => {
            *hops += 1;
            if target.is_absolute() {
                *resolved = PathBuf::from("/");
            }
            pending.extend(target.to_string_lossy().split('/').rev().map(str::to_owned));
        }
        Ok(_) | Err(_) => *resolved = candidate,
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{normpath, realpath};

    #[test]
    fn normpath_matches_posix_including_double_slash() {
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
    fn realpath_tolerates_a_missing_final_target() {
        let scratch = std::env::temp_dir().join(format!(
            "garage-core-realpath-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        drop(std::fs::remove_dir_all(&scratch));
        std::fs::create_dir_all(scratch.join("real")).expect("scratch");
        std::os::unix::fs::symlink(scratch.join("real"), scratch.join("link")).expect("symlink");
        assert_eq!(
            realpath(&scratch.join("link/gone")),
            realpath(&scratch).join("real/gone")
        );
        drop(std::fs::remove_dir_all(scratch));
    }
}
