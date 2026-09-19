//! Hyprland's event socket, read as [`MonitorEvents`].
//!
//! `$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE/.socket2.sock` streams the
//! compositor's own events as `name>>payload` lines. Only the four monitor lines are read
//! here -- `monitoradded`, `monitoraddedv2`, `monitorremoved`, `monitorremovedv2` -- and
//! every other line is discarded.
//!
//! # Why a raw `UnixStream`
//!
//! There is no Hyprland client here and no new dependency: the socket is a plain
//! `SOCK_STREAM` and `std::os::unix::net::UnixStream` connects to it, which keeps the
//! whole thing inside the `unsafe_code` forbid. A read timeout is the debounce clock: the
//! watcher asks for the next event with a quiet window, and a `TimedOut` is the answer
//! that the window closed with nothing on the wire.
//!
//! # Partial lines
//!
//! A read can land in the middle of a line. Bytes are accumulated in `pending` and only a
//! complete line -- one with its newline -- is parsed and consumed, so an event split
//! across two reads is still delivered whole.

use std::env;
use std::io::{self, Read};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use garage_core::traits::{EventError, MonitorEvent, MonitorEvents};

/// The compositor's event stream, as a source of display events.
pub struct HyprEvents {
    stream: UnixStream,
    pending: String,
}

/// Hand-written because the stream carries no useful `Debug`.
impl std::fmt::Debug for HyprEvents {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("HyprEvents").finish_non_exhaustive()
    }
}

impl HyprEvents {
    /// Connect to the running session's event socket.
    ///
    /// # Errors
    ///
    /// [`EventError`] when the environment names no socket, or the socket cannot be
    /// reached -- no compositor, or one that has not created it yet.
    pub fn connect() -> Result<Self, EventError> {
        let path = socket_path()?;
        let stream = UnixStream::connect(&path).map_err(|error| EventError {
            detail: format!("{}: {error}", path.display()),
        })?;
        Ok(Self {
            stream,
            pending: String::new(),
        })
    }
}

impl MonitorEvents for HyprEvents {
    fn next_event(&mut self, timeout: Duration) -> Result<Option<MonitorEvent>, EventError> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(event) = self.poll_line() {
                return Ok(Some(event));
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Ok(None);
            }
            self.stream
                .set_read_timeout(Some(remaining))
                .map_err(|error| EventError {
                    detail: error.to_string(),
                })?;
            let mut buffer = [0u8; 4096];
            match self.stream.read(&mut buffer) {
                Ok(0) => {
                    return Err(EventError {
                        detail: "the compositor's event socket closed".to_owned(),
                    });
                }
                Ok(read) => {
                    let chunk = buffer.get(..read).unwrap_or_default();
                    self.pending.push_str(&String::from_utf8_lossy(chunk));
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                    ) =>
                {
                    return Ok(None);
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) => {
                    return Err(EventError {
                        detail: error.to_string(),
                    });
                }
            }
        }
    }
}

impl HyprEvents {
    /// Consume whole lines already buffered, returning the first display event among them.
    ///
    /// `None` means no complete line is buffered (the caller should read) or only
    /// non-display lines were consumed; either way the next thing to do is wait for more
    /// bytes, and a display event that arrived in the same read is found before then.
    fn poll_line(&mut self) -> Option<MonitorEvent> {
        loop {
            let position = self.pending.find('\n')?;
            let line: String = self.pending.drain(..=position).collect();
            if let Some(event) = parse_event(line.trim()) {
                return Some(event);
            }
        }
    }
}

/// `$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE/.socket2.sock`.
fn socket_path() -> Result<PathBuf, EventError> {
    let runtime = env::var("XDG_RUNTIME_DIR").map_err(|_| EventError {
        detail: "XDG_RUNTIME_DIR is not set; no way to name the compositor's socket".to_owned(),
    })?;
    let signature = env::var("HYPRLAND_INSTANCE_SIGNATURE").map_err(|_| EventError {
        detail: "HYPRLAND_INSTANCE_SIGNATURE is not set; no running Hyprland to watch".to_owned(),
    })?;
    Ok(PathBuf::from(runtime)
        .join("hypr")
        .join(signature)
        .join(".socket2.sock"))
}

/// One `name>>payload` line, or `None` for any event this does not care about.
///
/// The `v2` variants carry `id,name,description` -- the id first, the name second -- and the
/// plain ones carry the name alone. Only the name is read, and only the four monitor events
/// are recognised.
fn parse_event(line: &str) -> Option<MonitorEvent> {
    let (name, payload) = line.split_once(">>")?;
    match name {
        "monitoradded" => Some(MonitorEvent::Added(payload.to_owned())),
        "monitorremoved" => Some(MonitorEvent::Removed(payload.to_owned())),
        "monitoraddedv2" => Some(MonitorEvent::Added(field(payload, 1))),
        "monitorremovedv2" => Some(MonitorEvent::Removed(field(payload, 1))),
        _ => None,
    }
}

/// The `index`th comma-separated field, or the empty string when it is absent.
fn field(payload: &str, index: usize) -> String {
    payload.split(',').nth(index).unwrap_or_default().to_owned()
}

#[cfg(test)]
mod tests {
    use super::{field, parse_event};
    use garage_core::traits::MonitorEvent;

    #[test]
    fn the_plain_and_v2_monitor_events_both_yield_a_name() {
        assert_eq!(
            parse_event("monitoradded>>DP-2"),
            Some(MonitorEvent::Added("DP-2".to_owned()))
        );
        assert_eq!(
            parse_event("monitoraddedv2>>1,DP-2,Dell Inc. DELL U2422H 5C5CG83"),
            Some(MonitorEvent::Added("DP-2".to_owned()))
        );
        assert_eq!(
            parse_event("monitorremoved>>DP-1"),
            Some(MonitorEvent::Removed("DP-1".to_owned()))
        );
        assert_eq!(
            parse_event("monitorremovedv2>>0,DP-1,Dell Inc. DELL P2721Q FTHWTK3"),
            Some(MonitorEvent::Removed("DP-1".to_owned()))
        );
    }

    #[test]
    fn every_other_event_is_discarded() {
        assert_eq!(parse_event("activewindow>>kitty,opencode"), None);
        assert_eq!(parse_event("workspace>>2"), None);
        assert_eq!(parse_event("no separator here"), None);
    }

    #[test]
    fn a_v2_description_containing_a_comma_still_leaves_the_name_alone() {
        assert_eq!(field("1,DP-2,U2422H, rev A", 1), "DP-2");
        assert_eq!(field("only", 3), "");
    }
}
