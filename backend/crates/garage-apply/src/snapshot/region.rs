//! `region_snapshot()`: the three `LANG`s the pane needs to tell the truth about a locale
//! change.
//!
//! Three, because they genuinely differ after a change: the system one the override sits on
//! top of (`/etc/locale.conf`), the one the systemd user manager will hand the next
//! application (`systemctl --user show-environment`), and the one this process was started
//! with -- which, since this binary is a child of the shell, is the locale the running
//! session is actually in (`$LANG` from the process environment). A pane that only showed
//! one of the three could not explain why a just-applied locale has not reached the terminal
//! it is running in yet.
//!
//! Doc-only: returns a snapshot value for [`crate::snapshot`]'s JSON envelope, not
//! `Result<(), ApplyError>`, and is reached from `make_snapshot()` rather than from
//! `Route::steps()`.

use serde_json::{json, Value};

use crate::cx::SessionCx;
use crate::locale::{installed_locales, session_locale, system_locale};

/// `region_snapshot()` (garage:5113-5126): the three `LANG`s, and every installed locale.
pub(crate) fn region_snapshot(cx: &SessionCx<'_>) -> Value {
    json!({
        "locales": installed_locales(cx),
        "system": system_locale(),
        "session": session_locale(cx),
        // The locale this process was started with -- and since this binary is a child of the
        // shell, that is the locale the running session is actually in.
        "active": std::env::var("LANG").unwrap_or_default(),
    })
}
