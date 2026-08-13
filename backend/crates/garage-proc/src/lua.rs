//! `write_lua()`'s preflight: `luac -p` over a candidate fragment, and the chunk-name rewrite.

use std::path::Path;

use garage_core::traits::{LuaCheckError, LuaSyntaxCheck, Output, Runner, DEFAULT_RUN_TIMEOUT};

use crate::run::which;

/// The `luac -p` syntax check, run through whatever [`Runner`] the caller hands it.
///
/// Borrowed rather than owned so the same [`crate::System`] backs the runner a session uses
/// and the one a render's preflight uses -- one process boundary, not two.
#[derive(Clone, Copy)]
pub struct Luac<'a> {
    runner: &'a dyn Runner,
}

/// Hand-written because the one field is a trait object, which carries no `Debug`; requiring
/// one from [`Runner`] would buy nothing, since a runner has no state worth printing.
impl std::fmt::Debug for Luac<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Luac").finish_non_exhaustive()
    }
}

impl<'a> Luac<'a> {
    /// Wrap a runner.
    #[must_use]
    pub const fn new(runner: &'a dyn Runner) -> Self {
        Self { runner }
    }
}

impl LuaSyntaxCheck for Luac<'_> {
    /// # Errors
    ///
    /// [`LuaCheckError`] when `luac` runs and rejects the candidate. A machine with no
    /// `luac` is `Ok(())`: the check is a safety net over generated output, not a
    /// dependency of the feature, and refusing a setting because a developer tool is
    /// absent would be the safety net doing more harm than the thing it guards against.
    fn check(&self, candidate: &Path) -> Result<(), LuaCheckError> {
        if which("luac").is_none() {
            return Ok(());
        }
        let path = candidate.to_string_lossy();
        // A `run()` that could not run at all is `CompletedProcess(command, 1, "",
        // str(error))` on the Python side, and `write_lua()` reads that as a *failed check*
        // -- returncode 1, with the OS error where `luac`'s complaint would be. Mapping the
        // error back onto that shape is what keeps the two agreeing about a `luac` that
        // exists but cannot be executed.
        let output = self
            .runner
            .run(&["luac", "-p", &path], DEFAULT_RUN_TIMEOUT)
            .unwrap_or_else(|error| Output {
                status: 1,
                stdout: String::new(),
                stderr: error.detail,
            });
        if output.status == 0 {
            return Ok(());
        }
        // `(check.stderr or check.stdout).strip()`: stderr unless it is empty.
        let detail = if output.stderr.is_empty() {
            output.stdout
        } else {
            output.stderr
        };
        Err(LuaCheckError {
            detail: rewrite_chunk(detail.trim()),
        })
    }
}

/// `re.sub(r"^luac: .*?:(\d+): ", r"line \1: ", detail)`.
///
/// `luac` reports `luac: <chunk>:<line>: <complaint>`, where the chunk is the temporary
/// file's name -- elided from the middle once it is long enough, so it is not even a usable
/// path. Neither the chunk nor the `luac:` prefix means anything to whoever sees the
/// message, so both are replaced by `line <line>: `.
///
/// Hand-rolled rather than a regex dependency, and it is the same match: `^` anchors to the
/// start of the whole string (no `MULTILINE`), `.` does not cross a newline, and `.*?` is
/// non-greedy -- so this walks the colons of the first line in order and takes the first one
/// followed by digits and `": "`.
fn rewrite_chunk(detail: &str) -> String {
    let Some(rest) = detail.strip_prefix("luac: ") else {
        return detail.to_owned();
    };
    let head = match rest.find('\n') {
        Some(at) => rest.get(..at).unwrap_or(rest),
        None => rest,
    };
    for (at, _) in head.char_indices().filter(|(_, letter)| *letter == ':') {
        let after = rest.get(at + 1..).unwrap_or_default();
        let digits: String = after.chars().take_while(char::is_ascii_digit).collect();
        if digits.is_empty() {
            continue;
        }
        if let Some(remainder) = after
            .get(digits.len()..)
            .and_then(|tail| tail.strip_prefix(": "))
        {
            return format!("line {digits}: {remainder}");
        }
    }
    detail.to_owned()
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::path::Path;
    use std::time::Duration;

    use garage_core::traits::{LuaSyntaxCheck as _, Output, RunError, Runner};

    use super::{rewrite_chunk, Luac};

    /// A runner that answers with one canned result and remembers what it was asked.
    struct Canned {
        answer: RefCell<Option<Result<Output, RunError>>>,
        seen: RefCell<Vec<String>>,
    }

    impl Canned {
        fn new(answer: Result<Output, RunError>) -> Self {
            Self {
                answer: RefCell::new(Some(answer)),
                seen: RefCell::new(Vec::new()),
            }
        }
    }

    impl Runner for Canned {
        fn run(&self, command: &[&str], _timeout: Duration) -> Result<Output, RunError> {
            self.seen.borrow_mut().push(command.join(" "));
            self.answer.borrow_mut().take().unwrap_or(Ok(Output {
                status: 0,
                stdout: String::new(),
                stderr: String::new(),
            }))
        }

        fn spawn_detached(&self, _command: &[&str]) -> Result<(), RunError> {
            Ok(())
        }

        fn run_streamed(&self, _command: &[&str], _cwd: Option<&Path>) -> Result<i32, RunError> {
            Ok(0)
        }
    }

    fn output(status: i32, stderr: &str) -> Output {
        Output {
            status,
            stdout: String::new(),
            stderr: stderr.to_owned(),
        }
    }

    #[test]
    fn a_clean_fragment_passes_and_the_command_is_the_pythons() {
        let runner = Canned::new(Ok(output(0, "")));
        let verdict = Luac::new(&runner).check(Path::new("/tmp/.binds.lua.abcd1234"));
        if crate::which("luac").is_some() {
            assert!(verdict.is_ok());
            assert_eq!(
                runner.seen.borrow().first().map(String::as_str),
                Some("luac -p /tmp/.binds.lua.abcd1234")
            );
        } else {
            // A machine with no luac never asks, which is the policy under test.
            assert!(verdict.is_ok());
            assert!(runner.seen.borrow().is_empty());
        }
    }

    #[test]
    fn a_rejected_fragment_keeps_only_the_line_number() {
        assert_eq!(
            rewrite_chunk("luac: /home/x/.local/.../.binds.lua.k3f9x_2q:12: '=' expected"),
            "line 12: '=' expected"
        );
    }

    #[test]
    fn a_complaint_that_is_not_luacs_is_left_alone() {
        assert_eq!(
            rewrite_chunk("bash: luac: command not found"),
            "bash: luac: command not found"
        );
        assert_eq!(
            rewrite_chunk("luac: nothing to say"),
            "luac: nothing to say"
        );
    }

    #[test]
    fn the_first_colon_that_can_match_is_the_one_taken() {
        // A chunk name carrying its own colon: the non-greedy match stops at the first
        // `:<digits>: ` rather than the last.
        assert_eq!(rewrite_chunk("luac: a:1: b:2: boom"), "line 1: b:2: boom");
    }

    #[test]
    fn the_rewrite_does_not_cross_a_newline() {
        assert_eq!(
            rewrite_chunk("luac: chunk\nsecond:4: later"),
            "luac: chunk\nsecond:4: later"
        );
    }
}
