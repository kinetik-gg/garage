//! The advisory free-space gate before `bootstrap.sh` can enter `pacman -Syu`.
//!
//! This machine supplied the thresholds, measured on 2026-08-14: pacman's cache was
//! 3.8 GiB and is never auto-cleaned, a full `-Syu` pulls roughly 1--2 GiB, and the
//! release target directory was about 250 MiB (266 MiB at measurement time). Below 2 GiB
//! there is not credible room for that work, so a real update refuses; below 5 GiB the
//! margin is uncomfortable, so it warns. The figures are deliberately simple floors, not
//! an attempt to predict the exact transaction.
//!
//! This is advisory infrastructure, not a guarantee or a reservation. Space can disappear
//! between this check and pacman (the accepted TOCTOU window), and package expansion varies.
//! Its job is to catch a filesystem that is already nearly full before a rolling-release
//! upgrade starts, because failing inside that upgrade is the expensive failure.
//!
//! Available space is `statvfs.f_bavail * statvfs.f_frsize`: blocks an unprivileged process
//! may use, not `f_bfree`, which includes blocks reserved from it. `$HOME`, the checkout and
//! `/` are all inspected, then deduplicated by the device id from `metadata().dev()` so one
//! backing filesystem produces one verdict line.

use std::collections::HashSet;
use std::fs;
use std::os::unix::fs::MetadataExt as _;
use std::path::{Path, PathBuf};

use garage_core::paths::Paths;
use rustix::fs::statvfs;

use crate::error::ApplyError;

use super::transcript::Report;

const GIB: u64 = 1024 * 1024 * 1024;
const HARD_FLOOR: u64 = 2 * GIB;
const SOFT_FLOOR: u64 = 5 * GIB;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct StatFigure {
    available_blocks: u64,
    fragment_size: u64,
}

impl StatFigure {
    pub(super) const fn new(available_blocks: u64, fragment_size: u64) -> Self {
        Self {
            available_blocks,
            fragment_size,
        }
    }

    const fn available_bytes(self) -> u64 {
        self.available_blocks.saturating_mul(self.fragment_size)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Filesystem {
    device: u64,
    mountpoint: PathBuf,
    figure: StatFigure,
}

impl Filesystem {
    pub(super) fn new(device: u64, mountpoint: PathBuf, figure: StatFigure) -> Self {
        Self {
            device,
            mountpoint,
            figure,
        }
    }

    fn available_bytes(&self) -> u64 {
        self.figure.available_bytes()
    }
}

pub(super) trait SpaceProbe {
    fn filesystem(&self, path: &Path) -> Result<Filesystem, String>;
}

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct SystemSpace;

impl SpaceProbe for SystemSpace {
    fn filesystem(&self, path: &Path) -> Result<Filesystem, String> {
        let canonical =
            fs::canonicalize(path).map_err(|error| format!("{}: {error}", path.display()))?;
        let device = metadata_device(&canonical)?;
        let mountpoint = mountpoint(&canonical, device)?;
        let stat =
            statvfs(&canonical).map_err(|error| format!("{}: {error}", canonical.display()))?;
        Ok(Filesystem::new(
            device,
            mountpoint,
            StatFigure::new(stat.f_bavail, stat.f_frsize),
        ))
    }
}

#[cfg(test)]
#[derive(Debug, Clone, Copy)]
pub(super) struct FixedSpace {
    figure: StatFigure,
}

#[cfg(test)]
impl FixedSpace {
    pub(super) const fn gib(available: u64) -> Self {
        Self {
            figure: StatFigure::new(available, GIB),
        }
    }
}

#[cfg(test)]
impl SpaceProbe for FixedSpace {
    fn filesystem(&self, _path: &Path) -> Result<Filesystem, String> {
        Ok(Filesystem::new(1, PathBuf::from("/"), self.figure))
    }
}

/// Print one verdict per backing filesystem and say whether convergence may proceed.
pub(super) fn check(
    paths: &Paths,
    checkout: &Path,
    report: &mut Report,
    probe: &dyn SpaceProbe,
) -> Result<bool, ApplyError> {
    report.step("Checking free space for the upgrade")?;
    let filesystems = distinct_filesystems(probe, [&paths.home, checkout, Path::new("/")])?;
    let mut refused = false;
    for filesystem in filesystems {
        let available = filesystem.available_bytes();
        let verdict = Verdict::for_bytes(available);
        refused |= verdict == Verdict::Refuse;
        report.note(&verdict_line(
            verdict,
            report.dry_run,
            &filesystem.mountpoint,
            available,
        ))?;
    }
    if refused {
        report.note("Free space with `sudo pacman -Sc`, or run `cargo clean` in backend/.")?;
        if report.dry_run {
            report.note("continuing the dry run anyway, because a dry run changes nothing.")?;
        }
    }
    Ok(!refused || report.dry_run)
}

fn distinct_filesystems<const N: usize>(
    probe: &dyn SpaceProbe,
    paths: [&Path; N],
) -> Result<Vec<Filesystem>, ApplyError> {
    let mut devices = HashSet::new();
    let mut filesystems = Vec::new();
    for path in paths {
        let filesystem = probe.filesystem(path).map_err(|error| {
            ApplyError::Io(format!(
                "could not inspect free space before the upgrade: {error}"
            ))
        })?;
        if devices.insert(filesystem.device) {
            filesystems.push(filesystem);
        }
    }
    Ok(filesystems)
}

fn metadata_device(path: &Path) -> Result<u64, String> {
    fs::metadata(path)
        .map(|metadata| metadata.dev())
        .map_err(|error| format!("{}: {error}", path.display()))
}

fn mountpoint(path: &Path, device: u64) -> Result<PathBuf, String> {
    let mut current = path.to_path_buf();
    while let Some(parent) = current.parent() {
        if metadata_device(parent)? != device {
            break;
        }
        current = parent.to_path_buf();
    }
    Ok(current)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verdict {
    Ready,
    Warn,
    Refuse,
}

impl Verdict {
    const fn for_bytes(available: u64) -> Self {
        if available < HARD_FLOOR {
            Self::Refuse
        } else if available < SOFT_FLOOR {
            Self::Warn
        } else {
            Self::Ready
        }
    }
}

fn verdict_line(verdict: Verdict, dry_run: bool, mountpoint: &Path, available: u64) -> String {
    let free = format_gib(available);
    match verdict {
        Verdict::Ready => format!("OK {}: {free} GiB free.", mountpoint.display()),
        Verdict::Warn => format!(
            "WARN {}: {free} GiB free; below the 5 GiB comfort floor.",
            mountpoint.display()
        ),
        Verdict::Refuse if dry_run => format!(
            "WOULD-REFUSE {}: {free} GiB free; need at least 2 GiB before upgrading.",
            mountpoint.display()
        ),
        Verdict::Refuse => format!(
            "REFUSE {}: {free} GiB free; need at least 2 GiB before upgrading.",
            mountpoint.display()
        ),
    }
}

fn format_gib(bytes: u64) -> String {
    let tenths = (u128::from(bytes) * 10 + u128::from(GIB / 2)) / u128::from(GIB);
    format!("{}.{:01}", tenths / 10, tenths % 10)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[derive(Debug)]
    struct FakeSpace {
        filesystems: HashMap<PathBuf, Filesystem>,
    }

    impl FakeSpace {
        fn new(entries: impl IntoIterator<Item = (PathBuf, Filesystem)>) -> Self {
            Self {
                filesystems: entries.into_iter().collect(),
            }
        }
    }

    impl SpaceProbe for FakeSpace {
        fn filesystem(&self, path: &Path) -> Result<Filesystem, String> {
            self.filesystems
                .get(path)
                .cloned()
                .ok_or_else(|| format!("no fake statvfs figure for {}", path.display()))
        }
    }

    fn filesystem(device: u64, mountpoint: &str, blocks: u64, fragment_size: u64) -> Filesystem {
        Filesystem::new(
            device,
            PathBuf::from(mountpoint),
            StatFigure::new(blocks, fragment_size),
        )
    }

    #[test]
    fn verdict_uses_the_fake_statvfs_figure_at_both_floors() {
        let just_under_hard = StatFigure::new(HARD_FLOOR / 4096 - 1, 4096);
        let hard = StatFigure::new(HARD_FLOOR / 4096, 4096);
        let soft = StatFigure::new(SOFT_FLOOR / 4096, 4096);

        assert_eq!(
            Verdict::for_bytes(just_under_hard.available_bytes()),
            Verdict::Refuse
        );
        assert_eq!(Verdict::for_bytes(hard.available_bytes()), Verdict::Warn);
        assert_eq!(Verdict::for_bytes(soft.available_bytes()), Verdict::Ready);
    }

    #[test]
    fn two_checked_paths_on_one_device_produce_one_filesystem() {
        let home = PathBuf::from("/home/tester");
        let checkout = PathBuf::from("/home/tester/garage");
        let fake = FakeSpace::new([
            (home.clone(), filesystem(7, "/", 6, GIB)),
            (checkout.clone(), filesystem(7, "/", 6, GIB)),
        ]);

        let found = distinct_filesystems(&fake, [&home, &checkout]).expect("fake statvfs");

        assert_eq!(found, vec![filesystem(7, "/", 6, GIB)]);
    }

    #[test]
    fn dry_run_messages_are_pinned_and_a_refusal_does_not_stop_it() {
        let paths = Paths::from_env_map(
            &[("HOME".to_owned(), "/home/tester".to_owned())]
                .into_iter()
                .collect(),
        );
        let checkout = PathBuf::from("/checkout");
        let fake = FakeSpace::new([
            (paths.home.clone(), filesystem(1, "/home", 6, GIB)),
            (checkout.clone(), filesystem(2, "/checkout", 4, GIB)),
            (PathBuf::from("/"), filesystem(3, "/", 3, GIB / 2)),
        ]);
        let mut report = Report::new(true, true);

        let proceed = check(&paths, &checkout, &mut report, &fake).expect("space check");

        assert!(proceed);
        assert_eq!(
            report.captured(),
            Some(concat!(
                "\n==> Checking free space for the upgrade\n",
                "    OK /home: 6.0 GiB free.\n",
                "    WARN /checkout: 4.0 GiB free; below the 5 GiB comfort floor.\n",
                "    WOULD-REFUSE /: 1.5 GiB free; need at least 2 GiB before upgrading.\n",
                "    Free space with `sudo pacman -Sc`, or run `cargo clean` in backend/.\n",
                "    continuing the dry run anyway, because a dry run changes nothing.\n"
            ))
        );
    }

    #[test]
    fn real_run_refusal_message_is_pinned_and_stops_convergence() {
        let paths = Paths::from_env_map(
            &[("HOME".to_owned(), "/home/tester".to_owned())]
                .into_iter()
                .collect(),
        );
        let checkout = PathBuf::from("/checkout");
        let low = filesystem(1, "/", 1, GIB);
        let fake = FakeSpace::new([
            (paths.home.clone(), low.clone()),
            (checkout.clone(), low.clone()),
            (PathBuf::from("/"), low),
        ]);
        let mut report = Report::new(false, true);

        let proceed = check(&paths, &checkout, &mut report, &fake).expect("space check");

        assert!(!proceed);
        assert_eq!(
            report.captured(),
            Some(concat!(
                "\n==> Checking free space for the upgrade\n",
                "    REFUSE /: 1.0 GiB free; need at least 2 GiB before upgrading.\n",
                "    Free space with `sudo pacman -Sc`, or run `cargo clean` in backend/.\n"
            ))
        );
    }
}
