//! Capability traits whose implementations live elsewhere (`garage-proc`), so that
//! pure crates can name the need without gaining the power to spawn processes.

use std::path::Path;

use thiserror::Error;

/// Why Lua refused a generated fragment.
///
/// `detail` is `luac`'s own complaint with the chunk name taken out. `luac` reports
/// `luac: <chunk>:<line>: ...`, and the chunk is the temporary file's name, elided from
/// the middle once it is long enough. Neither half means anything to whoever sees this,
/// so an implementation rewrites the leading `luac: <chunk>:<line>: ` to `line <line>: `
/// before storing the rest here.
#[derive(Debug, Error)]
#[error("{detail}")]
pub struct LuaCheckError {
    /// `luac`'s message, with the chunk name rewritten away as described above.
    pub detail: String,
}

/// Ask Lua whether a candidate fragment parses, before anything installs it.
///
/// `hyprland.lua` `dofile()`s the generated fragments. Hyprland syntax-checks
/// `hyprland.lua` before it tears down the live config, but that check does not follow a
/// `dofile`, so a malformed fragment is only discovered at reload -- by which point it is
/// already on disk and every later session loads it again. `luac -p` finds it here, while
/// the previous good fragment is still in place.
///
/// A missing `luac` is not a reason to refuse the setting: the check is a safety net over
/// generated output, not a dependency of the feature. An implementation that cannot find
/// the binary returns `Ok(())`. That policy belongs to the implementation
/// (`garage-proc`), because only the half that may spawn a process can know whether
/// `luac` is on the machine; this crate names the need and nothing else.
pub trait LuaSyntaxCheck {
    /// Check `candidate`, a complete fragment already written to a temporary path.
    ///
    /// # Errors
    ///
    /// Returns [`LuaCheckError`] when `luac` parses the candidate and rejects it. A
    /// `luac` that cannot be found, or that cannot be run at all, is not an error here:
    /// see the trait docs.
    fn check(&self, candidate: &Path) -> Result<(), LuaCheckError>;
}
