//! `browser_window_titles()`: the window titles of every open browser, used to tell
//! "`YouTube` Music" and "`YouTube`" apart from a browser tab that carries no useful
//! URL in its MPRIS metadata.
//!
//! Ported with its 2-second cache intact even though, in this binary, a single
//! invocation calls it at most once (`playing_players()` is the only caller, and it
//! is called once per process). The cache is harmless dead weight here for the same
//! reason it was in the Python: nothing about porting the *behaviour* license
//! dropping a stateful detail on the grounds that this particular process happens
//! not to exercise it twice.

use std::cell::{Cell, RefCell};
use std::time::{Duration, Instant};

use crate::exec::RunError;
use crate::media::hyprctl;

/// The window classes `is_browser`/`browser_window_titles()` both recognise.
const BROWSER_MARKERS: [&str; 6] = ["chromium", "chrome", "firefox", "brave", "vivaldi", "zen"];

/// How long a fetched answer stays valid before the next call re-asks `hyprctl`.
const CACHE_TTL: Duration = Duration::from_secs(2);

/// `_browser_titles_cache = (0.0, "")` at module scope, made an owned value instead
/// of a global so tests can hold one of their own.
#[derive(Debug)]
pub(crate) struct BrowserTitleCache {
    cached_at: Cell<Option<Instant>>,
    titles: RefCell<String>,
}

impl Default for BrowserTitleCache {
    fn default() -> Self {
        Self::new()
    }
}

impl BrowserTitleCache {
    #[must_use]
    pub(crate) fn new() -> Self {
        Self {
            cached_at: Cell::new(None),
            titles: RefCell::new(String::new()),
        }
    }

    /// `browser_window_titles()`. Returns the space-joined `title` of every open
    /// browser window; `Err` only for the spawn failure / timeout `hyprctl` can raise,
    /// same as [`hyprctl::clients`].
    pub(crate) fn titles(&self) -> Result<String, RunError> {
        if self.is_fresh() {
            return Ok(self.titles.borrow().clone());
        }
        let Some(clients) = hyprctl::clients(Duration::from_secs(2))? else {
            return Ok(self.titles.borrow().clone());
        };
        let joined = join_browser_titles(&clients);
        self.cached_at.set(Some(Instant::now()));
        self.titles.borrow_mut().clone_from(&joined);
        Ok(joined)
    }

    fn is_fresh(&self) -> bool {
        self.cached_at
            .get()
            .is_some_and(|at| at.elapsed() < CACHE_TTL)
    }
}

fn join_browser_titles(clients: &[serde_json::Value]) -> String {
    clients
        .iter()
        .filter(|client| {
            is_browser(&hyprctl::client_identity(
                client,
                &["class", "initialClass"],
            ))
        })
        .map(|client| {
            client
                .get("title")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_browser(identity: &str) -> bool {
    BROWSER_MARKERS
        .iter()
        .any(|marker| identity.contains(marker))
}

#[cfg(test)]
mod tests {
    use super::join_browser_titles;
    use serde_json::json;

    #[test]
    fn only_browser_windows_contribute_their_title() {
        let clients = vec![
            json!({"class": "firefox", "initialClass": "firefox", "title": "Cat video - YouTube"}),
            json!({"class": "org.kde.dolphin", "initialClass": "dolphin", "title": "Downloads"}),
        ];
        assert_eq!("Cat video - YouTube", join_browser_titles(&clients));
    }

    #[test]
    fn multiple_browser_windows_are_space_joined_in_order() {
        let clients = vec![
            json!({"class": "chromium", "initialClass": "chromium", "title": "A"}),
            json!({"class": "brave-browser", "initialClass": "brave-browser", "title": "B"}),
        ];
        assert_eq!("A B", join_browser_titles(&clients));
    }

    #[test]
    fn no_browser_windows_yields_an_empty_string() {
        let clients = vec![json!({"class": "kitty", "initialClass": "kitty", "title": "zsh"})];
        assert_eq!("", join_browser_titles(&clients));
    }
}
