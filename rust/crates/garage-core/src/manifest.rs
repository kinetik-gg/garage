//! Readers for the line-delimited manifests under `system/manifest/`.
//!
//! The package set, the per-user units, the font families and the managed paths
//! are data files rather than arrays inside `bootstrap.sh` or tuples inside the
//! Python backend, so that three readers can agree on one copy. The format is
//! deliberately the least that three languages can parse identically:
//!
//! * whitespace-separated fields, one record per line;
//! * `#` starts a comment, on its own line or trailing after the fields;
//! * blank lines, and lines left blank by stripping a comment, are skipped.
//!
//! `fonts.list` is the one exception to the field split: fontconfig family names
//! contain spaces (`Plus Jakarta Sans`), so that file is one family per line
//! with no flag column, and the whole line after comment-stripping is the name.
//!
//! Parsing is separated from reading on purpose. The `parse_*` functions take a
//! `&str`, so a test can exercise a malformed line from a string literal without
//! writing one to disk; the `load_*` functions are the thin wrapper that adds
//! the file read.

use std::fs;
use std::io;
use std::path::Path;

use thiserror::Error;

/// Name of the package manifest inside the manifest directory.
pub const PACKAGES_FILE: &str = "packages.list";
/// Name of the per-user unit manifest inside the manifest directory.
pub const UNITS_FILE: &str = "units.list";
/// Name of the font family manifest inside the manifest directory.
pub const FONTS_FILE: &str = "fonts.list";
/// Name of the managed path manifest inside the manifest directory.
pub const MANAGED_PATHS_FILE: &str = "managed-paths.list";

/// What went wrong reading or parsing a manifest.
///
/// Every parse variant carries the file name and the 1-based `line` number, so a
/// message points at the line to fix rather than at the manifest as a whole.
#[derive(Debug, Error)]
pub enum ManifestError {
    /// The file could not be read at all.
    #[error("{file}: {source}")]
    Unreadable {
        file: &'static str,
        source: io::Error,
    },
    /// A field this record requires is absent. `field` says which.
    #[error("{file}:{line}: missing {field}")]
    MissingField {
        file: &'static str,
        line: usize,
        field: &'static str,
    },
    /// A flag or kind keyword the format does not define. `field` says which
    /// column `value` came from.
    #[error("{file}:{line}: unknown {field} {value:?}")]
    UnknownValue {
        file: &'static str,
        line: usize,
        field: &'static str,
        value: String,
    },
    /// The line carried a field past the last one this record defines.
    #[error("{file}:{line}: unexpected extra field {value:?}")]
    ExtraField {
        file: &'static str,
        line: usize,
        value: String,
    },
}

/// One package, and whether `garage doctor` asserts it is present by name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageEntry {
    /// Arch package name, as pacman knows it.
    pub name: String,
    /// Set by the `critical` flag: a deliberately small subset, not a judgement
    /// that everything unflagged is optional.
    pub critical: bool,
}

/// What a healthy session looks like for a unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitKind {
    /// A long-running daemon, or a timer: it should be active.
    Running,
    /// `Type=oneshot`: it runs, exits, and `inactive` is its healthy state, so
    /// only `enabled` means anything.
    Oneshot,
}

impl UnitKind {
    fn from_flag(flag: &str) -> Option<Self> {
        match flag {
            "running" => Some(Self::Running),
            "oneshot" => Some(Self::Oneshot),
            _ => None,
        }
    }
}

/// One per-user systemd unit this checkout enables.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitEntry {
    /// Unit name including its suffix, e.g. `waybar.service`.
    pub name: String,
    /// Whether a live session should show it running.
    pub kind: UnitKind,
}

/// How a managed path comes to exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathKind {
    /// A repository directory stowed into `$HOME` as symlinks. Not enumerated:
    /// it expands to the tree walk minus the patterns in `.stow-local-ignore`.
    StowTree,
    /// Written as a real file by the render pass and replaced on every theme
    /// change, which is exactly why it is not stowed.
    Generated,
    /// Compiled or downloaded by `bootstrap.sh` into the user's private prefix.
    Artifact,
    /// A real file the user is expected to edit; written once if absent and
    /// then left alone.
    Override,
}

impl PathKind {
    fn from_keyword(keyword: &str) -> Option<Self> {
        match keyword {
            "stow-tree" => Some(Self::StowTree),
            "generated" => Some(Self::Generated),
            "artifact" => Some(Self::Artifact),
            "override" => Some(Self::Override),
            _ => None,
        }
    }
}

/// One path Garage puts on disk outside the package manager.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedPath {
    /// How the path comes to exist.
    pub kind: PathKind,
    /// `$HOME`-relative location, or a repository directory for a stow tree.
    /// A trailing `/` means the directory and everything under it.
    pub path: String,
    /// The single package the path belongs to, where there is exactly one.
    pub owner: Option<String>,
}

/// Everything before the first `#`, which is the record part of a line.
fn strip_comment(line: &str) -> &str {
    line.split_once('#').map_or(line, |(record, _)| record)
}

/// Reject a field past the end of the record, so a typo is not silently eaten.
fn reject_extra<'a>(
    mut rest: impl Iterator<Item = &'a str>,
    file: &'static str,
    line: usize,
) -> Result<(), ManifestError> {
    match rest.next() {
        None => Ok(()),
        Some(value) => Err(ManifestError::ExtraField {
            file,
            line,
            value: value.to_owned(),
        }),
    }
}

/// Parse the contents of `packages.list`.
///
/// # Errors
///
/// [`ManifestError::UnknownValue`] for a second field that is not `critical`,
/// and [`ManifestError::ExtraField`] for anything past it.
pub fn parse_packages(text: &str) -> Result<Vec<PackageEntry>, ManifestError> {
    let file = PACKAGES_FILE;
    let mut entries = Vec::new();
    for (index, raw) in text.lines().enumerate() {
        let line = index + 1;
        let mut fields = strip_comment(raw).split_whitespace();
        let Some(name) = fields.next() else { continue };
        let critical = match fields.next() {
            None => false,
            Some("critical") => true,
            Some(value) => {
                return Err(ManifestError::UnknownValue {
                    file,
                    line,
                    field: "package flag",
                    value: value.to_owned(),
                })
            }
        };
        reject_extra(fields, file, line)?;
        entries.push(PackageEntry {
            name: name.to_owned(),
            critical,
        });
    }
    Ok(entries)
}

/// Parse the contents of `units.list`.
///
/// # Errors
///
/// [`ManifestError::MissingField`] for a unit with no kind, and
/// [`ManifestError::UnknownValue`] for a kind that is neither `running` nor
/// `oneshot` -- a unit nobody classified is a unit the doctor stops checking.
pub fn parse_units(text: &str) -> Result<Vec<UnitEntry>, ManifestError> {
    let file = UNITS_FILE;
    let mut entries = Vec::new();
    for (index, raw) in text.lines().enumerate() {
        let line = index + 1;
        let mut fields = strip_comment(raw).split_whitespace();
        let Some(name) = fields.next() else { continue };
        let Some(flag) = fields.next() else {
            return Err(ManifestError::MissingField {
                file,
                line,
                field: "unit kind (running or oneshot)",
            });
        };
        let Some(kind) = UnitKind::from_flag(flag) else {
            return Err(ManifestError::UnknownValue {
                file,
                line,
                field: "unit kind",
                value: flag.to_owned(),
            });
        };
        reject_extra(fields, file, line)?;
        entries.push(UnitEntry {
            name: name.to_owned(),
            kind,
        });
    }
    Ok(entries)
}

/// Parse the contents of `fonts.list`: one fontconfig family per line.
///
/// # Errors
///
/// None today -- every non-blank line is a family name, because a family name
/// may contain spaces and so this file has no field structure to violate. The
/// `Result` matches the other parsers so a caller handles all four alike.
pub fn parse_fonts(text: &str) -> Result<Vec<String>, ManifestError> {
    Ok(text
        .lines()
        .map(|raw| strip_comment(raw).trim())
        .filter(|family| !family.is_empty())
        .map(str::to_owned)
        .collect())
}

/// Parse the contents of `managed-paths.list`.
///
/// # Errors
///
/// [`ManifestError::UnknownValue`] for an unrecognised kind keyword,
/// [`ManifestError::MissingField`] for a kind with no path, and
/// [`ManifestError::ExtraField`] for a fourth field.
pub fn parse_managed_paths(text: &str) -> Result<Vec<ManagedPath>, ManifestError> {
    let file = MANAGED_PATHS_FILE;
    let mut entries = Vec::new();
    for (index, raw) in text.lines().enumerate() {
        let line = index + 1;
        let mut fields = strip_comment(raw).split_whitespace();
        let Some(keyword) = fields.next() else {
            continue;
        };
        let Some(kind) = PathKind::from_keyword(keyword) else {
            return Err(ManifestError::UnknownValue {
                file,
                line,
                field: "path kind",
                value: keyword.to_owned(),
            });
        };
        let Some(path) = fields.next() else {
            return Err(ManifestError::MissingField {
                file,
                line,
                field: "path",
            });
        };
        let owner = fields.next().map(str::to_owned);
        reject_extra(fields, file, line)?;
        entries.push(ManagedPath {
            kind,
            path: path.to_owned(),
            owner,
        });
    }
    Ok(entries)
}

fn read(dir: &Path, file: &'static str) -> Result<String, ManifestError> {
    fs::read_to_string(dir.join(file)).map_err(|source| ManifestError::Unreadable { file, source })
}

/// Read and parse `packages.list` from a manifest directory.
///
/// # Errors
///
/// [`ManifestError::Unreadable`] if the file cannot be read, or any error
/// [`parse_packages`] returns.
pub fn load_packages(dir: &Path) -> Result<Vec<PackageEntry>, ManifestError> {
    parse_packages(&read(dir, PACKAGES_FILE)?)
}

/// Read and parse `units.list` from a manifest directory.
///
/// # Errors
///
/// [`ManifestError::Unreadable`] if the file cannot be read, or any error
/// [`parse_units`] returns.
pub fn load_units(dir: &Path) -> Result<Vec<UnitEntry>, ManifestError> {
    parse_units(&read(dir, UNITS_FILE)?)
}

/// Read and parse `fonts.list` from a manifest directory.
///
/// # Errors
///
/// [`ManifestError::Unreadable`] if the file cannot be read.
pub fn load_fonts(dir: &Path) -> Result<Vec<String>, ManifestError> {
    parse_fonts(&read(dir, FONTS_FILE)?)
}

/// Read and parse `managed-paths.list` from a manifest directory.
///
/// # Errors
///
/// [`ManifestError::Unreadable`] if the file cannot be read, or any error
/// [`parse_managed_paths`] returns.
pub fn load_managed_paths(dir: &Path) -> Result<Vec<ManagedPath>, ManifestError> {
    parse_managed_paths(&read(dir, MANAGED_PATHS_FILE)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// The real `system/manifest/`, so the tests check the shipped files rather
    /// than a fixture that can agree with a broken parser.
    fn manifest_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../system/manifest")
    }

    #[test]
    fn real_packages_parse() {
        let packages = load_packages(&manifest_dir()).unwrap();
        assert!(!packages.is_empty(), "packages.list parsed to nothing");

        let names: Vec<&str> = packages.iter().map(|p| p.name.as_str()).collect();
        let critical: Vec<&str> = packages
            .iter()
            .filter(|p| p.critical)
            .map(|p| p.name.as_str())
            .collect();
        assert!(!critical.is_empty(), "no package is flagged critical");
        for name in &critical {
            assert!(names.contains(name), "{name} is critical but not a package");
        }
    }

    #[test]
    fn real_units_parse() {
        let units = load_units(&manifest_dir()).unwrap();
        assert!(!units.is_empty(), "units.list parsed to nothing");
        assert!(units.iter().any(|u| u.kind == UnitKind::Running));
        assert!(units.iter().any(|u| u.kind == UnitKind::Oneshot));
        for unit in &units {
            assert!(unit.name.contains('.'), "{} has no unit suffix", unit.name);
        }
    }

    #[test]
    fn real_fonts_parse() {
        let fonts = load_fonts(&manifest_dir()).unwrap();
        assert!(!fonts.is_empty(), "fonts.list parsed to nothing");
        // The spaces are the reason this file has no flag column.
        assert!(fonts.iter().any(|f| f.contains(' ')));
    }

    #[test]
    fn real_managed_paths_parse() {
        let paths = load_managed_paths(&manifest_dir()).unwrap();
        assert!(!paths.is_empty(), "managed-paths.list parsed to nothing");
        assert!(paths
            .iter()
            .any(|p| p.kind == PathKind::StowTree && p.path == "desktop/"));
        assert!(paths.iter().any(|p| p.owner.is_some()));
    }

    #[test]
    fn comments_and_blanks_are_skipped() {
        let packages =
            parse_packages("# header\n\n  \nkitty critical # the terminal\nfish\n").unwrap();
        assert_eq!(
            packages,
            vec![
                PackageEntry {
                    name: "kitty".to_owned(),
                    critical: true,
                },
                PackageEntry {
                    name: "fish".to_owned(),
                    critical: false,
                },
            ]
        );
    }

    #[test]
    fn unknown_package_flag_names_file_and_line() {
        let error = parse_packages("kitty\nfish essential\n").unwrap_err();
        assert!(matches!(
            error,
            ManifestError::UnknownValue { file, line, .. } if file == PACKAGES_FILE && line == 2
        ));
        assert_eq!(
            error.to_string(),
            "packages.list:2: unknown package flag \"essential\""
        );
    }

    #[test]
    fn extra_package_field_is_rejected() {
        let error = parse_packages("kitty critical extra\n").unwrap_err();
        assert!(matches!(error, ManifestError::ExtraField { line: 1, .. }));
    }

    #[test]
    fn unit_without_a_kind_is_rejected() {
        let error = parse_units("waybar.service running\ncliphist.service\n").unwrap_err();
        assert!(matches!(
            error,
            ManifestError::MissingField { file, line, .. } if file == UNITS_FILE && line == 2
        ));
    }

    #[test]
    fn unknown_unit_kind_is_rejected() {
        let error = parse_units("waybar.service sometimes\n").unwrap_err();
        assert_eq!(
            error.to_string(),
            "units.list:1: unknown unit kind \"sometimes\""
        );
    }

    #[test]
    fn font_families_keep_their_spaces() {
        let fonts = parse_fonts("# families\nPlus Jakarta Sans\nGeist Mono  # mono\n").unwrap();
        assert_eq!(fonts, vec!["Plus Jakarta Sans", "Geist Mono"]);
    }

    #[test]
    fn managed_path_owner_is_optional() {
        let paths =
            parse_managed_paths("generated .config/gtk-3.0/gtk.css\nartifact x/y kitty\n").unwrap();
        assert_eq!(paths.first().and_then(|p| p.owner.clone()), None);
        assert_eq!(
            paths.get(1).and_then(|p| p.owner.clone()),
            Some("kitty".to_owned())
        );
    }

    #[test]
    fn unknown_path_kind_is_rejected() {
        let error = parse_managed_paths("stow-tree desktop/\ncopied a/b\n").unwrap_err();
        assert_eq!(
            error.to_string(),
            "managed-paths.list:2: unknown path kind \"copied\""
        );
    }

    #[test]
    fn managed_path_without_a_path_is_rejected() {
        let error = parse_managed_paths("generated\n").unwrap_err();
        assert!(matches!(
            error,
            ManifestError::MissingField { line: 1, field, .. } if field == "path"
        ));
    }
}
