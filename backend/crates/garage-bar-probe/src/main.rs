//! `garage-bar-probe` -- the Quickshell bar's context probes, as one persistent process.
//!
//! Two subsystems are watched, both because their state lives outside the shell and no
//! event source exists for them:
//!
//! * **Containers** -- which running containers `podman ps` / `docker ps` report. Unlike
//!   the waybar module this replaces, each engine is probed independently: a missing
//!   `podman` no longer masks a working `docker` (the ported Python quirk died here on
//!   purpose; see the crate changelog note in `containers.rs`).
//! * **SMB shares** -- which of the shares quoted in `~/.local/libexec/ensure-smb-mounted`
//!   are currently mounted, per `gio mount -l`.
//! * **Microphone** -- whether any pulse source is recording, parsed from
//!   `pactl list sources` exactly as the waybar module parsed it (Quickshell's Pipewire
//!   service exposes no node-level running state to read instead).
//!
//! Output is one JSON object per line on stdout, refreshed every [`CONTAINER_INTERVAL`]
//! seconds with the SMB half re-probed every third tick. The bar parses the last line and
//! renders chips from it; nothing here knows what a chip looks like.
//!
//! A probe that fails (spawn error or timeout) reports that section as `null` for the
//! ticks until it succeeds again -- an absent engine or an unreadable helper hides its
//! widget rather than drawing a stale one.
#![forbid(unsafe_code)]

mod containers;
mod exec;
mod microphone;
mod smb;
mod stream;

use std::process::ExitCode;

fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        Some("stream") => stream::run(),
        Some("once") => stream::once(),
        _ => usage_error(),
    }
}

fn usage_error() -> ExitCode {
    eprintln!("usage: garage-bar-probe stream");
    eprintln!("       garage-bar-probe once");
    ExitCode::from(2)
}
