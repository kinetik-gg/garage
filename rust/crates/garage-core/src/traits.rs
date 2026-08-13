//! Capability traits whose implementations live elsewhere (`garage-proc`), so that
//! pure crates can name the need without gaining the power to spawn processes.

use std::path::Path;
use std::time::Duration;

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

/// One display, carrying only what the render path reads off it.
///
/// Deliberately not a model of `hyprctl monitors -j`. That record also carries `id`,
/// `make`, `model`, `serial`, `refreshRate`, `scale`, `x`/`y`, `disabled`, `mirrorOf`,
/// `focused` and `activeWorkspace`, and every one of those is read by a caller on the
/// apply side: `display_snapshot()` builds the Displays pane out of them, and
/// `active_workspaces()`/`restore_active_workspaces()` use `focused` and
/// `activeWorkspace` to put each screen back on what it was showing across a workspace
/// reload. None of that is a render. Three fields are:
///
/// * [`name`](Monitor::name) -- the connector, which is all `monitor_names()` takes out
///   of the JSON. `workspace_outputs()` folds the live connectors in on top of the saved
///   layout so a display `displays.toml` has never seen still gets a workspace range,
///   and `render_workspaces()` is what asks.
/// * [`width`](Monitor::width) and [`height`](Monitor::height) -- read by
///   `solid_wallpaper()`, on the `render_wallpaper()` path. hyprpaper has no colour mode,
///   so a picked swatch has to reach it as an image, and the flat PNG is sized to the
///   largest monitor so the image never has to be scaled up. The two dimensions are
///   maximised independently, and a machine reporting no usable size falls back to
///   1920x1080.
///
/// A field belongs here once some renderer reads it. Adding one because hyprctl happens
/// to report it is how this becomes a second, drifting copy of the compositor's schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Monitor {
    /// The connector, as Hyprland names it: `DP-1`, `eDP-1`. Stable across sessions and
    /// needs nothing remembered to reproduce, which is why the workspace plan orders by
    /// it rather than by position -- dragging a display around in the Displays pane
    /// would otherwise renumber every workspace on the desktop.
    pub name: String,
    /// Pixel width of the current mode. Zero means the compositor reported nothing
    /// usable, and a caller sizing an image drops such a monitor rather than trusting it.
    pub width: u32,
    /// Pixel height of the current mode, with the same reading of zero.
    pub height: u32,
}

/// Why the compositor could not be asked which displays exist.
///
/// The Python has no such type: `json_command()` hands back its fallback -- an empty
/// list -- whether `hyprctl` is missing, times out, exits non-zero or prints something
/// that is not a JSON array, and every caller reads that empty list as "no monitors".
/// That conflation is fine for `workspace_outputs()`, which has the saved layout to fall
/// back on, and wrong for a caller that would rather know. Keeping the failure separate
/// here costs a caller one `unwrap_or_default()` to get the Python's behaviour back, and
/// gives the others the choice.
#[derive(Debug, Error)]
#[error("{detail}")]
pub struct MonitorError {
    /// What went wrong, in whatever terms the implementation has.
    pub detail: String,
}

/// Ask the compositor which displays exist.
///
/// This is a QUESTION to the compositor, and that is the whole reason a render may make
/// it. `render_all()`'s docstring draws the line: "`hyprctl monitors` still runs from
/// `render_workspaces()`, through `workspace_outputs()`. That is a question, not an
/// instruction: the plan cannot be written without knowing which displays exist. Nothing
/// here answers back." A render writes files and signals nothing -- no `gsettings`, no
/// service restart, no `pkill`, no `hyprctl eval` or `reload` -- so an outbound call is
/// only admissible when the answer feeds a file and the call moves nothing.
///
/// Together with [`LuaSyntaxCheck`] that is the complete list: two named questions, not a
/// general ability to run programs. [`crate::traits::Runner`] is the general ability, and
/// it lives on the apply side.
pub trait MonitorSource {
    /// Every display the compositor currently reports.
    ///
    /// # Errors
    ///
    /// [`MonitorError`] when the compositor cannot be reached or its answer cannot be
    /// read. A machine with no displays attached is `Ok(vec![])`, not an error.
    fn monitors(&self) -> Result<Vec<Monitor>, MonitorError>;
}

/// What a finished subprocess left behind.
///
/// `subprocess.CompletedProcess` minus the fields nothing reads: the Python's callers
/// look at `returncode`, `stdout` and `stderr` and never at `args`, which they already
/// hold. Text, not bytes, because `run()` passes `text=True` -- every consumer here is
/// parsing `hyprctl` JSON, splitting `timedatectl list-timezones` into lines, or putting
/// `stderr` in front of the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    /// The exit status. Zero is success; the caller decides what a non-zero one means,
    /// exactly as `check=False` leaves it to.
    pub status: i32,
    /// Everything the child wrote to stdout.
    pub stdout: String,
    /// Everything the child wrote to stderr. This is what reaches the user when a command
    /// fails with something to say -- `solid_wallpaper()` raises with
    /// `result.stderr.strip()` and only invents a message when that is empty.
    pub stderr: String,
}

/// Why a subprocess could not be run to completion.
///
/// The Python's `run()` catches `OSError` and `subprocess.SubprocessError` -- the binary
/// is missing, the fork fails, the timeout expires -- and then forks on `check`: it
/// raises `SettingsError` when the caller asked to be told, and otherwise synthesises
/// `CompletedProcess(command, 1, "", str(error))` so the caller sees an ordinary failed
/// run. There is no `check` flag here because `Result` is that flag. `Err` is the family
/// the Python catches; a command that ran and exited non-zero is `Ok` with
/// [`Output::status`] set, which is the same split the Python draws, made visible in the
/// type rather than in an argument.
#[derive(Debug, Error)]
#[error("{detail}")]
pub struct RunError {
    /// What went wrong, in whatever terms the implementation has. The Python puts
    /// `str(error)` here.
    pub detail: String,
}

/// The default timeout every captured run gets: `run()`'s `timeout: float = 4`.
///
/// Short on purpose. Nearly every command behind it is `hyprctl`, `gsettings` or
/// `timedatectl` answering a local socket or bus, and the settings path is synchronous --
/// the UI fires one `garage set` per wheel notch. A command that has not answered in four
/// seconds is not going to, and waiting on it would stall the session rather than the
/// command. The handful that legitimately take longer pass their own: `solid_wallpaper()`
/// gives `magick` fifteen.
pub const DEFAULT_RUN_TIMEOUT: Duration = Duration::from_secs(4);

/// Run a program. The process boundary for APPLY, and the reason apply and render are
/// separate crates rather than separate directories.
///
/// Everything that moves the running desktop goes through here: `hyprctl eval` and
/// `reload`, `gsettings`, `pkill -HUP xsettingsd`, `systemctl --user restart`,
/// `loginctl lock-session`. A renderer cannot name this trait, because its crate does not
/// depend on the crate that implements it and its context does not carry one.
///
/// Three methods, not one, because the Python has three shapes and the differences are
/// each load-bearing.
pub trait Runner {
    /// Run `command` to completion, capturing both streams.
    ///
    /// This is `run()`: `subprocess.run(command, text=True, capture_output=True,
    /// check=..., timeout=...)`. Pass [`DEFAULT_RUN_TIMEOUT`] unless the command has a
    /// documented reason to take longer.
    ///
    /// The command is an argument vector, never a string handed to a shell -- there is no
    /// `shell=True` anywhere in the Python, so a timezone name or a wallpaper path
    /// carrying a space, a quote or a `;` is an argument and cannot become syntax.
    ///
    /// # Errors
    ///
    /// [`RunError`] when the command could not be run or did not finish: the binary is
    /// absent, the spawn fails, or `timeout` expires. A command that ran and exited
    /// non-zero returns `Ok`; see [`RunError`] for why the line is drawn there.
    fn run(&self, command: &[&str], timeout: Duration) -> Result<Output, RunError>;

    /// Start `command` in a session of its own and do not wait for it.
    ///
    /// `subprocess.Popen(..., stdin/stdout/stderr=DEVNULL, start_new_session=True)`. The
    /// new session is the point: the child leaves the caller's process group, so it
    /// outlives the caller and is not taken down by the signals that reach it. Three
    /// callers, and both reasons for the shape:
    ///
    /// * `display_test()`'s `_display-watchdog`. A tested layout is applied at once but
    ///   is not written to `displays.toml` until it is confirmed, and if nobody confirms
    ///   it the watchdog reverts it fifteen seconds later. It has to survive the `garage`
    ///   process that started it -- that process returns a token immediately -- or a
    ///   layout that left no working input device would strand the user in it.
    /// * `timedatectl set-ntp` and `timedatectl set-timezone`. Neither reads a result, and
    ///   both hand the work to `systemd-timedated` over the bus. Detaching keeps the
    ///   settings path, which is synchronous, from waiting on a call whose answer it has
    ///   no use for.
    ///
    /// Detached means unobserved: nothing here reports the child's exit status, because
    /// by construction there is nobody left to report it to. All three discard the child's
    /// stdio for the same reason.
    ///
    /// # Errors
    ///
    /// [`RunError`] when the child could not be started at all. Once it is running, its
    /// fate is its own.
    fn spawn_detached(&self, command: &[&str]) -> Result<(), RunError>;

    /// Run `command` with this process's own stdin, stdout and stderr, and wait for it.
    ///
    /// Not through [`Runner::run`], and the reason is in `update`'s comment on the
    /// `bootstrap.sh` call: "this streams to the terminal, both because the output is a
    /// progress report and because pacman's sudo prompt needs the tty." Capturing either
    /// stream would break both halves -- a progress report that arrives all at once at the
    /// end is not a progress report, and a password prompt with no terminal cannot be
    /// answered. There is no timeout for the same reason: a system upgrade takes as long
    /// as it takes, and the user is watching it.
    ///
    /// Two callers, `bootstrap.sh` (which is why `cwd` exists -- it runs from the checkout
    /// root) and `garage-rebuild-plugins`, "streamed and interactive for the same reasons
    /// as bootstrap: it installs into /usr/lib and asks for sudo once."
    ///
    /// An implementation flushes this process's own stdout before handing the terminal
    /// over. Redirected to a file our stdout is block-buffered while the child writes
    /// straight to the descriptor, so without the flush the whole report lands after the
    /// output it was introducing.
    ///
    /// # Errors
    ///
    /// [`RunError`] when the child could not be started or could not be waited on. A
    /// non-zero exit comes back as `Ok`: both callers print their own explanation of what
    /// a failure means for the rest of the run and carry on.
    fn run_streamed(&self, command: &[&str], cwd: Option<&Path>) -> Result<i32, RunError>;
}
