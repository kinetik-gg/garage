//! `write_lua` — candidate file + `luac -p` preflight, then atomic install.

use std::fs;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::fs::atomic::{atomic_write, create_temporary, discard, parent_of, AtomicWriteError};
use crate::traits::{LuaCheckError, LuaSyntaxCheck};

/// Why a generated Lua fragment did not reach the disk.
#[derive(Debug, Error)]
pub enum LuaWriteError {
    /// The directory the fragment belongs in could not be created.
    #[error("{}: could not create the directory it belongs in: {source}", path.display())]
    Parents {
        /// The fragment.
        path: PathBuf,
        /// The underlying failure.
        source: io::Error,
    },
    /// The candidate could not be staged for the check.
    #[error("{}: could not stage a candidate beside it: {source}", path.display())]
    Candidate {
        /// The fragment.
        path: PathBuf,
        /// The underlying failure.
        source: io::Error,
    },
    /// Lua refused to parse the candidate. Named by the fragment's filename alone,
    /// because that is the half of the path the reader recognises.
    #[error("{name}: generated Lua is invalid: {source}")]
    Invalid {
        /// The fragment's filename.
        name: String,
        /// What `luac` said, with its chunk name rewritten away.
        source: LuaCheckError,
    },
    /// The checked fragment could not be installed.
    #[error(transparent)]
    Install(#[from] AtomicWriteError),
}

/// Install a generated Lua fragment only once Lua agrees it parses.
///
/// `hyprland.lua` `dofile()`s these. Hyprland syntax-checks `hyprland.lua` before it tears
/// down the live config, but that check does not follow a `dofile`, so a malformed
/// fragment is only discovered at reload -- by which point it is already on disk and every
/// later session loads it again. [`LuaSyntaxCheck`] finds it here, while the previous good
/// fragment is still in place.
///
/// The text is written to a candidate beside the target, because `luac` has to be handed a
/// file and the target must not be that file: a fragment that fails the check has to leave
/// the last good one exactly where it was. The candidate is removed whichever way the
/// check goes -- a rejected one is debris, and an accepted one has already served its
/// purpose, since the install writes the same text again through
/// [`atomic_write`] rather than renaming the file `luac` just read.
///
/// A missing `luac` passes: that policy belongs to the [`LuaSyntaxCheck`] implementation,
/// and is documented on the trait.
///
/// # Errors
///
/// Returns [`LuaWriteError`] if the parent directory cannot be created, the candidate
/// cannot be staged, the check rejects the fragment, or the install fails. The target
/// keeps its previous contents in all four cases.
pub fn write_lua(path: &Path, text: &str, lua: &dyn LuaSyntaxCheck) -> Result<(), LuaWriteError> {
    fs::create_dir_all(parent_of(path)).map_err(|source| LuaWriteError::Parents {
        path: path.to_path_buf(),
        source,
    })?;
    let (candidate, mut handle) =
        create_temporary(path).map_err(|source| LuaWriteError::Candidate {
            path: path.to_path_buf(),
            source,
        })?;
    let staged = handle
        .write_all(text.as_bytes())
        .and_then(|()| handle.flush())
        .map_err(|source| LuaWriteError::Candidate {
            path: path.to_path_buf(),
            source,
        });
    drop(handle);
    let verdict = staged.and_then(|()| {
        lua.check(&candidate)
            .map_err(|source| LuaWriteError::Invalid {
                name: file_label(path),
                source,
            })
    });
    discard(&candidate);
    verdict?;
    atomic_write(path, text).map_err(LuaWriteError::Install)
}

fn file_label(path: &Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::{write_lua, LuaWriteError};
    use crate::fs::scratch::Scratch;
    use crate::traits::{LuaCheckError, LuaSyntaxCheck};
    use std::fs;
    use std::path::Path;

    /// Stands in for the `luac -p` implementation in `garage-proc`. `verdict` is what the
    /// real one would report: `None` for a fragment that parses, and otherwise the
    /// already-rewritten detail line.
    #[derive(Debug)]
    struct FakeLua {
        verdict: Option<&'static str>,
    }

    impl LuaSyntaxCheck for FakeLua {
        fn check(&self, candidate: &Path) -> Result<(), LuaCheckError> {
            assert!(
                candidate.is_file(),
                "the check is handed a file that exists"
            );
            match self.verdict {
                None => Ok(()),
                Some(detail) => Err(LuaCheckError {
                    detail: detail.to_owned(),
                }),
            }
        }
    }

    fn entries(directory: &Path) -> Vec<String> {
        let mut found: Vec<String> = fs::read_dir(directory)
            .expect("directory is readable")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        found.sort();
        found
    }

    #[test]
    fn a_rejected_fragment_leaves_nothing_behind() {
        let scratch = Scratch::new("lua-reject");
        let fragment = scratch.join("preferences.lua");
        let lua = FakeLua {
            verdict: Some("line 3: unexpected symbol near '}'"),
        };

        let failure = write_lua(&fragment, "return {\n", &lua).expect_err("must be refused");

        assert!(matches!(failure, LuaWriteError::Invalid { .. }));
        assert_eq!(
            failure.to_string(),
            "preferences.lua: generated Lua is invalid: line 3: unexpected symbol near '}'"
        );
        assert!(!fragment.exists(), "the target must not be created");
        assert_eq!(
            entries(scratch.path()),
            Vec::<String>::new(),
            "no candidate may survive"
        );
    }

    #[test]
    fn a_rejected_fragment_leaves_the_previous_one_in_place() {
        let scratch = Scratch::new("lua-previous");
        let fragment = scratch.join("displays.lua");
        fs::write(&fragment, "-- good\n").expect("previous fragment");
        let lua = FakeLua {
            verdict: Some("line 1: syntax error"),
        };

        write_lua(&fragment, "-- bad\n", &lua).expect_err("must be refused");

        assert_eq!(
            fs::read_to_string(&fragment).expect("read back"),
            "-- good\n"
        );
        assert_eq!(entries(scratch.path()), vec!["displays.lua".to_owned()]);
    }

    #[test]
    fn an_accepted_fragment_is_installed_and_the_candidate_is_gone() {
        let scratch = Scratch::new("lua-accept");
        let fragment = scratch.join("nested").join("workspaces.lua");
        let lua = FakeLua { verdict: None };

        write_lua(&fragment, "return {}\n", &lua).expect("must be installed");

        assert_eq!(
            fs::read_to_string(&fragment).expect("read back"),
            "return {}\n"
        );
        assert_eq!(
            entries(super::parent_of(&fragment)),
            vec!["workspaces.lua".to_owned()]
        );
    }
}
