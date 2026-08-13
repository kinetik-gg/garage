//! `default_apps_snapshot()`: the handler in force and the candidates, for every role the
//! pane offers.
//!
//! Read live rather than stored: the mime defaults belong to `mimeapps.list`, and a second
//! copy in `preferences.toml` would be nothing but a copy to drift. The terminal is the one
//! exception -- nothing on the system records a default terminal -- so it is a real
//! preference, resolved here against what is actually installed rather than read from a mime
//! association. See [`crate::desktopfiles::roles`] for `role_applications()` and
//! `resolve_terminal()`, which this composes per role.
//!
//! Doc-only: returns a snapshot value for [`crate::snapshot`]'s JSON envelope, not
//! `Result<(), ApplyError>`, and is reached from `make_snapshot()` rather than from
//! `Route::steps()`.

use serde_json::{json, Map, Value};

use crate::cx::SessionCx;
use crate::desktopfiles::roles::{
    resolve_terminal, role_applications, terminal_candidates, DEFAULT_APP_ROLES,
};

/// `default_apps_snapshot()` (garage:5074-5087): the handler in force and the candidates.
pub(crate) fn default_apps_snapshot(cx: &SessionCx<'_>) -> Value {
    let mut apps = Map::new();
    for (role, types) in DEFAULT_APP_ROLES {
        let (current, candidates) = role_applications(cx, types);
        apps.insert(
            (*role).to_owned(),
            json!({ "current": current, "candidates": candidates }),
        );
    }
    // The exception: nothing on the system records a default terminal, so it is a real
    // preference, resolved here against what is actually installed.
    let paths = cx.render().paths();
    apps.insert(
        "terminal".to_owned(),
        json!({
            "current": resolve_terminal(paths, cx.render().prefs().general.terminal.as_str()),
            "candidates": terminal_candidates(paths),
        }),
    );
    Value::Object(apps)
}
