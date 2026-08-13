//! `RenderError`: what a renderer can fail with.

use garage_core::fs::atomic::AtomicWriteError;
use garage_core::fs::lua::LuaWriteError;
use garage_core::fs::marker::MarkerWriteError;
use thiserror::Error;

/// Why a renderer could not complete.
///
/// Every variant is a real failure a renderer can produce: an unwritable path, a malformed
/// fragment `luac` rejected, a marker whose symlink could not be removed, a `displays.toml`
/// that does not parse, a palette role that is not an opaque hex. Each wraps the writer's own
/// error type rather than flattening it, so a caller sees exactly which path and which
/// underlying `io::Error` failed.
///
/// There is no scaffold variant here any more. Phase 3 replaced the last renderer stub, and
/// with it the `PortPending` this enum carried through the port: every `RenderStep` reaches a
/// real implementation, and a renderer that fails now fails for a reason the machine gave.
#[derive(Debug, Error)]
pub enum RenderError {
    /// A generated Lua fragment could not be staged, checked or installed --
    /// [`crate::preferences::render_preferences`]'s `hyprland.lua` fragment.
    #[error(transparent)]
    Lua(#[from] LuaWriteError),

    /// A watched marker could not be written in place -- the material, accent or reduce
    /// motion markers.
    #[error(transparent)]
    Marker(#[from] MarkerWriteError),

    /// A whole-file rewrite could not be completed -- `hypridle.conf`.
    #[error(transparent)]
    Atomic(#[from] AtomicWriteError),

    /// A layer-2 TOML file the workspace plan reads could not be read or parsed:
    /// `displays.toml` or `workspace-blocks.toml`.
    ///
    /// The Python's `load_toml()` raises `SettingsError(f"{path.name}: {error}")` for both
    /// halves -- an `OSError` opening it and a `TOMLDecodeError` parsing it -- and this
    /// keeps that shape: the file's own name, then whatever the reader complained about.
    /// Only the shape is parity, not the text; `tomllib` and the `toml` crate word a syntax
    /// error differently and neither wording is depended on.
    #[error("{name}: {detail}")]
    Toml {
        /// The file's name alone, which is the half the reader recognises.
        name: String,
        /// What the read or the parse complained about.
        detail: String,
    },

    /// A generated fragment could not be removed -- `workspaces.lua`, when the plan comes
    /// back empty. `Path.unlink(missing_ok=True)`'s remaining failure modes: an unwritable
    /// directory, a path that is not a file. An already-absent fragment is success, exactly
    /// as `missing_ok=True` makes it.
    #[error("{}: could not be removed: {source}", path.display())]
    Remove {
        /// The fragment that is meant to stop existing.
        path: std::path::PathBuf,
        /// The underlying failure.
        source: std::io::Error,
    },

    /// `int()` or `float()` refused a value a hand-edited `displays.toml` -- or a Displays
    /// pane payload -- put where a coordinate, a scale, a transform or a VRR mode belongs.
    ///
    /// Text owned by [`crate::displays::NumberError`], which spells it as `CPython` does;
    /// see that type for the one place the shape differs from the Python's.
    #[error(transparent)]
    Number(#[from] crate::displays::NumberError),

    /// A palette role that must be an opaque `#rrggbb` is composited -- [`crate::theme`]'s
    /// `opaque()` refusing to hand `rgba(...)` to `qt6ct` or to `hyprlock`, neither of whose
    /// parsers can spell one, and Qt's would fall back to Fusion's own grey without saying
    /// so. Both fields carry the Python's own `repr()` spelling, because the message is the
    /// Python's byte for byte.
    ///
    /// Unreachable from the shipped tables -- every role `QT_ROLES` and the four lock
    /// colours name is an opaque hex -- which is the point of it being an error rather than a
    /// fallback: it fires at the render that repointed a role at a composited one, where the
    /// mapping table is, rather than on the next login.
    #[error(
        "palette role {role} is {value}, which is composited; \
         this toolkit can only be handed an opaque colour"
    )]
    CompositedRole {
        /// The role name, as `repr()` spells it.
        role: String,
        /// What it resolved to, as `repr()` spells it.
        value: String,
    },
}
