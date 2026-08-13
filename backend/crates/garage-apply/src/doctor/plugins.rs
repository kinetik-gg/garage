//! `plugin_state()`: the live ABI comparison `garage-rebuild-plugins --check` makes,
//! reimplemented here rather than shelled out to.
//!
//! A deliberate choice, not a duplication for its own sake. `--check` is built to run
//! unattended from a login unit: it always exits `0` whether or not the plugins are stale,
//! since a red unit in the session is worse than a quiet one, and when they are stale it
//! raises a sticky desktop notification as a side effect. Neither behaviour belongs in a
//! read-only health check or in `update`'s skip decision -- one gives no answer worth
//! interpreting, the other paints the user's screen. What the check actually computes is four
//! `readlink`s and a string compare, all read-only and none of them worth a subprocess to
//! reach.
//!
//! "Never deployed here" is kept apart from "stale": Kinetik Glass's source is not published,
//! so most machines have no plugins at all and are not behind on anything, and folding that
//! case into "stale" would make every ordinary machine's report look broken.
//!
//! "Behind" is a second, separate condition from "stale": the deployed build loads fine, it
//! is just not the commit `system/plugin-pins` names -- what a local-only checkout looks like
//! after a pin bump with no pull for `update` to notice moving in.
//!
//! Doc-only: reads deployed plugin symlinks and the pins file, returns a state value, not
//! `Result<(), ApplyError>` over a [`SessionCx`](crate::cx::SessionCx).
