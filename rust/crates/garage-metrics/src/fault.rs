//! What went wrong reading a sensor, in the shape the Python's exceptions had.
//!
//! Two things about the Python force this to carry more than a message.
//!
//! The first is that the failure text is *product*. `mark_unavailable` puts `str(error)`
//! straight into the tooltip, so the strip that reads `n/a` explains itself with the
//! exception's own words -- `no default route`, `no stat for nvme0n1`,
//! `[Errno 2] No such file or directory: '/proc/stat'`. Reproducing those spellings is
//! reproducing the tooltip, so [`Fault`] holds enough to rebuild each one rather than
//! a Rust error's own phrasing.
//!
//! The second is that the two modes do not catch the same set. `bar_svg` catches
//! `(OSError, ValueError, KeyError, IndexError, subprocess.SubprocessError)`, which is
//! everything a sensor can raise, and degrades the widget. `stream` catches only
//! `(OSError, ValueError)`, so a `KeyError` from a `/proc/meminfo` with no `MemTotal`
//! line, or an `IndexError` from a truncated `/proc/stat`, ends the stream with a
//! traceback rather than an error object on the wire. That is not obviously deliberate
//! in the Python, but it is what the Python does, and this port is not the place to
//! decide otherwise -- so [`Fault::kind`] keeps the distinction and
//! [`Kind::caught_by_stream`] is where the two lists differ.
//!
//! `subprocess.SubprocessError` has no variant here, and its absence is the finding
//! rather than an omission. It is in `bar_svg`'s `except` tuple, but the only two
//! subprocesses in the script both go through the `run()` helper, which catches
//! `(OSError, subprocess.SubprocessError)` itself and returns `None`. So no
//! `SubprocessError` can ever reach that `except` -- a timed-out `nvidia-smi` is a
//! machine with no GPU for one tick, and never an exception. [`crate::exec::run`] folds
//! it the same way, and there is nothing left to represent.

use garage_core::pyrepr::py_str_repr;
use std::fmt;
use std::io;
use std::path::Path;

/// Which Python exception class a failure would have been.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    /// `OSError` -- a file that would not open, or one of the script's own
    /// `raise OSError("...")` refusals.
    Os,
    /// `ValueError` -- `int()` or `float()` on something that is not a number, or a
    /// line that would not unpack into the two halves a `split` promised.
    Value,
    /// `KeyError` -- a `/proc/meminfo` missing a key the code subscripts directly.
    Key,
    /// `IndexError` -- a `/proc` line with fewer fields than the code indexes.
    Index,
}

impl Kind {
    /// Whether `stream`'s narrower `except (OSError, ValueError)` would catch this.
    /// The other two escape the loop, exactly as they do in the Python.
    pub(crate) fn caught_by_stream(self) -> bool {
        matches!(self, Self::Os | Self::Value)
    }
}

/// One sensor failure, carrying the exception class it stands for and the text
/// `str(error)` would have produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Fault {
    kind: Kind,
    message: String,
}

impl Fault {
    /// One of the script's own `raise OSError("...")` calls, where the message is a
    /// sentence somebody wrote for the tooltip.
    pub(crate) fn os(message: impl Into<String>) -> Self {
        Self {
            kind: Kind::Os,
            message: message.into(),
        }
    }

    /// An `OSError` the operating system raised, spelled the way `CPython` spells one
    /// that carries a filename: `[Errno 2] No such file or directory: '/proc/stat'`.
    ///
    /// The strerror text comes from the platform, via a Rust `io::Error` built from the
    /// same errno -- so it is the same C string `CPython` would have formatted, and it
    /// follows the machine's locale in both. Rust appends ` (os error 2)` to its
    /// `Display`, which `CPython` does not, so that tail is trimmed. The filename is
    /// `repr()`-quoted, matching `CPython`'s use of `%R` for it.
    pub(crate) fn errno(path: &Path, error: &io::Error) -> Self {
        let code = error.raw_os_error().unwrap_or(0);
        let rendered = error.to_string();
        let strerror = rendered
            .split_once(" (os error ")
            .map_or(rendered.as_str(), |(head, _)| head);
        Self {
            kind: Kind::Os,
            message: format!(
                "[Errno {code}] {strerror}: {}",
                py_str_repr(&path.to_string_lossy())
            ),
        }
    }

    /// `int(text)` on something that is not an integer.
    pub(crate) fn bad_int(text: &str) -> Self {
        Self {
            kind: Kind::Value,
            message: format!(
                "invalid literal for int() with base 10: {}",
                py_str_repr(text)
            ),
        }
    }

    /// `int(text, 16)` on something that is not hexadecimal -- the one base-16 parse in
    /// the script, on `/proc/net/route`'s flags column.
    pub(crate) fn bad_hex(text: &str) -> Self {
        Self {
            kind: Kind::Value,
            message: format!(
                "invalid literal for int() with base 16: {}",
                py_str_repr(text)
            ),
        }
    }

    /// A tuple unpack that did not get the arity it asked for -- `key, value =
    /// line.split(":", 1)` on a `/proc/meminfo` line with no colon.
    pub(crate) fn bad_unpack(got: usize) -> Self {
        Self {
            kind: Kind::Value,
            message: format!("not enough values to unpack (expected 2, got {got})"),
        }
    }

    /// `dict[key]` on a key that is not there. `CPython`'s `str(KeyError)` is the
    /// key's `repr`, quotes and all, which is why an unavailable memory widget reads
    /// `MEMORY unavailable | 'MemTotal'`.
    pub(crate) fn key(name: &str) -> Self {
        Self {
            kind: Kind::Key,
            message: py_str_repr(name),
        }
    }

    /// `sequence[index]` past the end.
    pub(crate) fn index() -> Self {
        Self {
            kind: Kind::Index,
            message: "list index out of range".to_string(),
        }
    }

    /// Which `except` clause would have claimed this.
    pub(crate) fn kind(&self) -> Kind {
        self.kind
    }
}

/// `str(error)`, which is what reaches the tooltip and the stream's error object.
impl fmt::Display for Fault {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for Fault {}

#[cfg(test)]
mod tests {
    use super::{Fault, Kind};
    use std::io;
    use std::path::Path;

    #[test]
    fn an_errno_failure_reads_the_way_cpython_prints_one() {
        let error = io::Error::from_raw_os_error(2);
        let fault = Fault::errno(Path::new("/proc/stat"), &error);
        assert_eq!(
            fault.to_string(),
            "[Errno 2] No such file or directory: '/proc/stat'"
        );
        assert_eq!(fault.kind(), Kind::Os);
    }

    #[test]
    fn a_key_error_keeps_the_quotes_cpython_puts_round_it() {
        assert_eq!(Fault::key("MemTotal").to_string(), "'MemTotal'");
        assert_eq!(Fault::key("MemTotal").kind(), Kind::Key);
    }

    #[test]
    fn the_scripts_own_refusals_are_their_own_sentences() {
        assert_eq!(
            Fault::os("no default route").to_string(),
            "no default route"
        );
        assert_eq!(
            Fault::os("no stat for nvme0n1").to_string(),
            "no stat for nvme0n1"
        );
    }

    #[test]
    fn value_errors_quote_the_offending_text() {
        assert_eq!(
            Fault::bad_int("x").to_string(),
            "invalid literal for int() with base 10: 'x'"
        );
        assert_eq!(
            Fault::bad_hex("zz").to_string(),
            "invalid literal for int() with base 16: 'zz'"
        );
        assert_eq!(
            Fault::bad_unpack(1).to_string(),
            "not enough values to unpack (expected 2, got 1)"
        );
    }

    #[test]
    fn stream_catches_two_of_the_four_kinds() {
        assert!(Kind::Os.caught_by_stream());
        assert!(Kind::Value.caught_by_stream());
        assert!(!Kind::Key.caught_by_stream());
        assert!(!Kind::Index.caught_by_stream());
    }
}
