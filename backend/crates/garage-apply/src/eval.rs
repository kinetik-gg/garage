//! `eval_config()`: set live Hyprland options from a Lua table body, without a reload.
//!
//! Hyprland 0.56 parses its config with Lua, and `hyprctl keyword` refuses to work there --
//! "keyword can't work with non-legacy parsers. Use eval." -- so the only way to reach a live
//! option is `hyprctl eval`. That is enough on its own: options are dereferenced per frame
//! and cached nowhere, so no reload is needed for a new value to be picked up. The renderer
//! has already written the fragment by the time this runs, which is what makes the value
//! survive a real reload or a restart even though this call never touches the file.
//!
//! `eval` sets values but damages nothing -- it does not emit `config.reloaded`, which is the
//! only event the glass plugin repaints on -- so a second call writes
//! `decoration:blur:size` back to its own value. A core option set dynamically schedules the
//! refresh it declares, and blur's forces a full frame on every monitor without re-running a
//! single monitor, window or layer rule. Writing it back to itself keeps the nudge free of
//! any visible side effect. It has to be its own `hl.config()` call: a second `decoration`
//! key in the same table constructor is a plain Lua overwrite, and the options it was meant
//! to accompany would be dropped without any error.
//!
//! Doc-only: this returns a captured subprocess result, not `Result<(), ApplyError>`, and is
//! called from inside [`crate::glass`], [`crate::corner`] and [`crate::border`]'s real
//! implementations rather than being a dispatch target of its own. [`crate::motion`] is the
//! one sibling that deliberately does not use it: its per-leaf speeds are top-level
//! `hl.animation()` calls rather than a single `hl.config()` table, and it needs none of the
//! blur write-back this appends.

use garage_core::traits::Output;

use crate::command::run;
use crate::cx::SessionCx;

/// Set live Hyprland options from a Lua table body, without a reload (garage:4703-4727).
///
/// Returns the captured `hyprctl eval` result rather than a `Result`, exactly as the Python
/// does: every caller has its own fallback for a refused eval -- three of them reload the
/// compositor instead -- and only one of them turns the refusal into an error at all. Handing
/// back the outcome is what lets each decide.
pub(crate) fn eval_config(cx: &SessionCx<'_>, body: &str) -> Output {
    let code = format!(
        "hl.config({{{body}}}) hl.config({{decoration = {{blur = \
         {{size = hl.get_config(\"decoration:blur:size\")}}}}}})"
    );
    run(cx, &["hyprctl", "eval", &code])
}

#[cfg(test)]
mod tests {
    use super::eval_config;
    use crate::testing::{Script, World};

    #[test]
    fn the_blur_write_back_is_its_own_config_call() {
        // A second `decoration` key in the same table constructor is a plain Lua overwrite,
        // and the options it was meant to accompany would be dropped without any error. The
        // two `hl.config(` occurrences are what says the nudge is separate.
        let world = World::plain("eval-writeback", Script::new());
        world.with(|cx| drop(eval_config(cx, "general = {border_size = 2}")));
        let trace = world.trace();
        let code = trace.first().expect("one eval was issued");
        assert!(code.starts_with("hyprctl eval hl.config({general = {border_size = 2}}) "));
        assert_eq!(code.matches("hl.config(").count(), 2);
        assert!(code.ends_with(
            "hl.config({decoration = {blur = {size = hl.get_config(\"decoration:blur:size\")}}})"
        ));
    }
}
