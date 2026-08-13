//! `garage-waybar-module media [--activate] [PREFERRED]` -- the whole of
//! `media-status.py`, which has no `main()`: everything at module scope runs on
//! import, dispatching on `sys.argv[1] == "--activate"`.
//!
//! CLI shape note (deviation, reported): the Python's dispatch reads only
//! `sys.argv[1] == "--activate"`; when that check fails it falls through to
//! `render()` called with NO argument, even though `render(preferred="")` and
//! `primary_player(players, preferred)` both accept one -- no current caller (the bare
//! exec in the generated fragment, or `--activate` from an on-click) ever supplies a
//! second argument, so in the field this is dead code. This port wires an optional
//! `PREFERRED` positional straight into that existing parameter instead of reproducing
//! the dead wiring literally, matching the CLI contract this task specifies.

mod browser;
mod hyprctl;
mod playerctl;
mod render;
mod source;
mod state;

use browser::BrowserTitleCache;
use garage_core::paths::Paths;

use crate::waybar::Payload;

/// `payload()`'s full shape, `player` included -- `output()` in the Python strips
/// `player` back out right before printing; [`MediaPayload::visible`] is that step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MediaPayload {
    pub(crate) text: String,
    pub(crate) tooltip: String,
    pub(crate) class: String,
    pub(crate) player: String,
}

impl MediaPayload {
    #[must_use]
    fn idle() -> Self {
        Self {
            text: String::new(),
            tooltip: String::new(),
            class: "idle".to_string(),
            player: String::new(),
        }
    }

    #[must_use]
    fn visible(&self) -> Payload {
        Payload {
            text: self.text.clone(),
            tooltip: self.tooltip.clone(),
            class: self.class.clone(),
        }
    }
}

/// The whole of `media-status.py`'s module-level try/except: `--activate` runs
/// [`state::activate`] and prints nothing on success; anything else renders and (on
/// success) saves the sticky primary-player cache before printing. A failure anywhere
/// in either path -- a missing `playerctl`/`hyprctl`, a timeout, or (render only) a
/// failure to write the sticky cache -- lands on the same idle payload the Python's
/// `output(payload())` prints, even for `--activate`, which the bare Python does too:
/// its own `activate()` call is inside the same outer try, so a `SubprocessError`
/// raised from within it is caught right there and turns into a printed idle payload.
pub(crate) fn dispatch(args: &[String]) {
    let paths = Paths::from_env();
    let browser_cache = BrowserTitleCache::new();
    if args.first().map(String::as_str) == Some("--activate") {
        run_activate(&paths, &browser_cache);
    } else {
        let preferred = args.first().map_or("", String::as_str);
        run_render(&paths, &browser_cache, preferred);
    }
}

fn run_activate(paths: &Paths, browser_cache: &BrowserTitleCache) {
    if state::activate(paths, browser_cache).is_err() {
        Payload::idle().emit();
    }
}

fn run_render(paths: &Paths, browser_cache: &BrowserTitleCache, preferred: &str) {
    let payload =
        render_and_save(paths, browser_cache, preferred).unwrap_or_else(|_| MediaPayload::idle());
    payload.visible().emit();
}

/// `current = render(); save_primary(current); output(current)` -- in that order, so
/// a `save_primary` failure (an I/O error the Python does not guard locally) skips
/// `output(current)` exactly as an uncaught exception propagating past it would.
fn render_and_save(
    paths: &Paths,
    browser_cache: &BrowserTitleCache,
    preferred: &str,
) -> Result<MediaPayload, state::Failure> {
    let current = render::render(preferred, browser_cache)?;
    state::save_primary(paths, &current)?;
    Ok(current)
}
