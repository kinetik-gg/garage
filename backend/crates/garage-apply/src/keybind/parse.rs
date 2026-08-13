//! `parse_combination()` and its derived spellings: splitting and comparing a bind string.
//!
//! Splitting on `+` is safe because the key that character stands for is spelled `"plus"` in
//! every keysym table this reads against; a bare `+` on its own is an empty part, and empty
//! parts are refused outright.
//!
//! Three spellings come out of the same parse, each for a different reader:
//! [`canonical_combination`] is the pane's own list spelling -- case is the writer's taste,
//! since the compositor matches keysyms case-insensitively, so storing whichever case arrived
//! would show one shortcut two ways in the Keybindings pane. [`combination_id`] strips
//! whitespace and lowercases, and must agree exactly with `bind_id()` in `config/binds.lua`,
//! which is the side that hands ids out and what `hl.unbind` matches on.
//! [`combination_signature`] is what the compositor actually matches on for telling two binds
//! apart -- an unordered set of modifiers plus a lowercase key -- because modifiers reach
//! Hyprland as a mask, so `SUPER+SHIFT+A` and `shift + super + a` are one shortcut and a set
//! holding both would fire both handlers off a single press.
//!
//! [`require_bindable`] checks against the canonical spelling, not the one that arrived --
//! what is stored and handed to the compositor is the canonical form, so checking anything
//! else would refuse `"f5"` while allowing the `"F5"` it normalises to. It also refuses a
//! bare key with no modifier unless that key is in the small set safe to bind standalone,
//! because a bare `F5` would be swallowed everywhere rather than reaching this shortcut.
//!
//! # The two regexes, hand-rolled
//!
//! The Python spells `KEY_NAME` and `KEY_STANDALONE` as `re` patterns and matches them with
//! `fullmatch`. Both are small enumerations rather than real grammars, so they are matched
//! here by hand rather than by taking a regex engine as a dependency for two patterns.
//! One deliberate narrowing: Python's `\d` matches every Unicode decimal digit, so
//! `code:٣` is a keycode there and is not one here. Hyprland's own parser reads those
//! digits with `strtol`, which is ASCII-only, so the narrower reading is the one that
//! matches what the compositor would accept.

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::keybind::KeybindError;

/// The modifiers Hyprland's bind parser knows.
pub const KEY_MODIFIERS: [&str; 8] = [
    "SUPER", "SHIFT", "CONTROL", "ALT", "CAPS", "MOD2", "MOD3", "MOD5",
];

/// The spellings people reach for that mean the same thing. MOD4 is SUPER under another
/// name; ALTGR is MOD5.
pub const KEY_MODIFIER_ALIASES: [(&str, &str); 5] = [
    ("CTRL", "CONTROL"),
    ("META", "SUPER"),
    ("WIN", "SUPER"),
    ("MOD4", "SUPER"),
    ("ALTGR", "MOD5"),
];

/// The order a combination is written back in, so two shortcuts that mean the same thing to
/// the compositor also read the same way in the pane.
pub const KEY_MODIFIER_ORDER: [&str; 8] = [
    "SUPER", "CONTROL", "ALT", "SHIFT", "CAPS", "MOD2", "MOD3", "MOD5",
];

/// The named keys the pane offers, after the letters, the digits and F1-F12.
const KEY_NAMED: [&str; 30] = [
    "Return",
    "Space",
    "Tab",
    "Escape",
    "BackSpace",
    "Delete",
    "Insert",
    "Home",
    "End",
    "Page_Up",
    "Page_Down",
    "Left",
    "Right",
    "Up",
    "Down",
    "minus",
    "plus",
    "equal",
    "bracketleft",
    "bracketright",
    "backslash",
    "semicolon",
    "apostrophe",
    "grave",
    "comma",
    "period",
    "slash",
    "Print",
    "Pause",
    "Menu",
];

/// Ceilings, so a runaway UI or a hand-edited file cannot produce a shortcut the pane can no
/// longer show or a command line nothing will run. `KEYBIND_LIMITS`.
#[must_use]
pub const fn keybind_limit(field: &str) -> usize {
    match field.as_bytes() {
        b"keys" => 64,
        b"description" => 120,
        // "command", and the only other field any caller asks about.
        _ => 1024,
    }
}

/// `CUSTOM_KEYBIND_MAX`: how many shortcuts the user may invent.
pub const CUSTOM_KEYBIND_MAX: usize = 64;

/// The witness `config/binds.lua` writes as the last line of the catalog it publishes: this
/// word, a tab, and the number of rows above it. It is what lets a reader tell a whole
/// catalog from the leading fragment of one -- see
/// [`read_keybind_catalog`](crate::keybind::catalog::read_keybind_catalog), which is the only
/// thing that may act on the difference.
pub const KEYBIND_CATALOG_SENTINEL: &str = "#end";

/// What the pane offers as the key half of a combination. Held here rather than in QML so
/// there is one whitelist: anything the list cannot express is also something
/// [`parse_combination`] would refuse on the way in.
#[must_use]
pub fn key_choices() -> Vec<String> {
    let mut choices: Vec<String> = ('A'..='Z').map(|letter| letter.to_string()).collect();
    choices.extend((0..10).map(|digit| digit.to_string()));
    choices.extend((1..=12).map(|number| format!("F{number}")));
    choices.extend(KEY_NAMED.iter().map(|name| (*name).to_owned()));
    choices
}

/// The whitelist's own spelling of each key, found case-insensitively. `KEY_CANONICAL`.
///
/// Keysym names are mixed case on purpose -- `minus`, `Page_Up`, `BackSpace` -- so a key is
/// normalised by adopting the spelling above rather than by upper- or lowercasing it.
/// Anything not offered there (`code:NN`, `mouse:NNN`, the XF86 keys) has no canonical
/// spelling to adopt and keeps the one it arrived in.
fn key_canonical() -> &'static HashMap<String, String> {
    static TABLE: OnceLock<HashMap<String, String>> = OnceLock::new();
    TABLE.get_or_init(|| {
        key_choices()
            .into_iter()
            .map(|name| (name.to_lowercase(), name))
            .collect()
    })
}

/// Split a Hyprland bind string into its modifiers and its key.
///
/// # Errors
///
/// [`KeybindError::NoKey`] for nothing at all, [`KeybindError::NotAShortcut`] for an empty
/// part, [`KeybindError::NotAModifier`] for a leading part that is not one, and
/// [`KeybindError::NotAKeyName`] for a key half Hyprland could not read.
pub fn parse_combination(value: Option<&str>) -> Result<(Vec<String>, String), KeybindError> {
    let text = value.unwrap_or_default();
    if text.trim().is_empty() {
        return Err(KeybindError::NoKey);
    }
    let parts: Vec<&str> = text.split('+').map(str::trim).collect();
    if parts.iter().any(|part| part.is_empty()) {
        return Err(KeybindError::NotAShortcut(text.trim().to_owned()));
    }
    let Some((key, leading)) = parts.split_last() else {
        return Err(KeybindError::NoKey);
    };
    let mut modifiers: Vec<String> = Vec::new();
    for part in leading {
        let name = modifier_name(&part.to_uppercase())
            .ok_or_else(|| KeybindError::NotAModifier((*part).to_owned()))?;
        if !modifiers.iter().any(|held| held == name) {
            modifiers.push(name.to_owned());
        }
    }
    if !is_key_name(key) {
        return Err(KeybindError::NotAKeyName((*key).to_owned()));
    }
    Ok((modifiers, (*key).to_owned()))
}

/// One uppercased part as a modifier: itself, its alias's target, or nothing.
fn modifier_name(upper: &str) -> Option<&'static str> {
    if let Some((_, target)) = KEY_MODIFIER_ALIASES
        .iter()
        .find(|(alias, _)| *alias == upper)
    {
        return Some(target);
    }
    KEY_MODIFIERS.iter().find(|name| **name == upper).copied()
}

/// `KEY_NAME`: an xkb keysym name, or one of the two forms Hyprland resolves itself --
/// `code:NN` is a raw keycode and `mouse:NNN` a button.
fn is_key_name(text: &str) -> bool {
    if !text.is_empty()
        && text
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return true;
    }
    if let Some(rest) = text.strip_prefix("code:") {
        return digits(rest, 1, 3);
    }
    if let Some(rest) = text.strip_prefix("mouse:") {
        return digits(rest, 1, 4);
    }
    false
}

/// `\d{min,max}` over ASCII digits, anchored at both ends.
fn digits(text: &str, least: usize, most: usize) -> bool {
    let count = text.chars().count();
    count >= least && count <= most && text.chars().all(|ch| ch.is_ascii_digit())
}

/// `KEY_STANDALONE`: the keys that may be bound with no modifier at all.
///
/// Everything else swallows the key across the whole desktop -- a bare `A` makes the letter
/// untypable in every window -- which is a trap worth refusing rather than documenting.
fn is_standalone(text: &str) -> bool {
    if let Some(rest) = text.strip_prefix("XF86") {
        return !rest.is_empty()
            && rest
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_');
    }
    if let Some(rest) = text.strip_prefix("F") {
        if digits(rest, 1, 2) {
            return true;
        }
    }
    if let Some(rest) = text.strip_prefix("mouse:") {
        return digits(rest, 1, 4);
    }
    matches!(
        text,
        "Print" | "Pause" | "Menu" | "Scroll_Lock" | "mouse_up" | "mouse_down"
    )
}

/// A key half spelled the way the pane's own list spells it.
///
/// Case is the writer's taste here exactly as it is for the modifiers: the compositor matches
/// keysyms case-insensitively, so `a` and `A` are one key and storing whichever arrived left
/// the Keyboard pane showing one shortcut two ways. Not an uppercase, which would invent
/// `MINUS` and `PAGE_UP` -- the spelling adopted is the whitelist's, looked up
/// case-insensitively, and a key the list does not offer keeps the spelling it came with.
#[must_use]
pub fn canonical_key(key: &str) -> String {
    key_canonical()
        .get(&key.to_lowercase())
        .cloned()
        .unwrap_or_else(|| key.to_owned())
}

/// The combination as the pane spells it: modifiers in their fixed order, then the key.
///
/// # Errors
///
/// Whatever [`parse_combination`] refuses.
pub fn canonical_combination(value: Option<&str>) -> Result<String, KeybindError> {
    let (modifiers, key) = parse_combination(value)?;
    let mut parts: Vec<String> = KEY_MODIFIER_ORDER
        .iter()
        .filter(|name| modifiers.iter().any(|held| held == *name))
        .map(|name| (*name).to_owned())
        .collect();
    parts.push(canonical_key(&key));
    Ok(parts.join(" + "))
}

/// The id a bind is known by: its default combination, normalised.
///
/// Must agree exactly with `bind_id()` in `config/binds.lua`, which is the side that hands
/// the ids out. It is also what `hl.unbind` matches on.
#[must_use]
pub fn combination_id(value: &str) -> String {
    value
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect::<String>()
        .to_lowercase()
}

/// What the compositor actually matches on, for telling two binds apart.
///
/// Order and case are the writer's taste, not the compositor's: modifiers reach Hyprland as a
/// mask. So `SUPER+SHIFT+A` and `shift + super + a` are one shortcut, and a set holding both
/// would fire both handlers off a single press. The Python's `frozenset` is a sorted `Vec`
/// here, which compares and hashes the same way for the same reason a set does: the order the
/// modifiers arrived in has been thrown away.
///
/// # Errors
///
/// Whatever [`parse_combination`] refuses.
pub fn combination_signature(value: &str) -> Result<(Vec<String>, String), KeybindError> {
    let (mut modifiers, key) = parse_combination(Some(value))?;
    modifiers.sort();
    Ok((modifiers, key.to_lowercase()))
}

/// A combination the user may take over, canonically spelled.
///
/// # Errors
///
/// Whatever [`parse_combination`] refuses, plus [`KeybindError::Standalone`] for a bare key
/// that would be swallowed everywhere.
pub fn require_bindable(value: Option<&str>) -> Result<String, KeybindError> {
    let (modifiers, key) = parse_combination(value)?;
    // Against the canonical spelling, not the one that arrived: what is stored and handed to
    // the compositor is the canonical form, so checking anything else would refuse "f5" while
    // allowing the "F5" it normalises to.
    if modifiers.is_empty() && !is_standalone(&canonical_key(&key)) {
        return Err(KeybindError::Standalone(key));
    }
    canonical_combination(value)
}

/// `KEYBIND_CONTROL`: control characters have no business in a shortcut.
///
/// Tab and newline are the exceptions: a shell command legitimately carries both, and they
/// survive the round trip through TOML and JSON as escapes.
#[must_use]
pub fn has_control_character(text: &str) -> bool {
    text.chars().any(|ch| {
        matches!(ch, '\u{0}'..='\u{8}') || matches!(ch, '\u{b}'..='\u{1f}') || ch == '\u{7f}'
    })
}

#[cfg(test)]
mod tests {
    use super::{
        canonical_combination, canonical_key, combination_id, combination_signature,
        has_control_character, key_choices, parse_combination, require_bindable,
    };
    use crate::keybind::KeybindError;

    #[test]
    fn the_key_whitelist_is_the_python_one() {
        let choices = key_choices();
        assert_eq!(choices.len(), 26 + 10 + 12 + 30);
        assert_eq!(choices.first().map(String::as_str), Some("A"));
        assert_eq!(choices.get(26).map(String::as_str), Some("0"));
        assert_eq!(choices.get(36).map(String::as_str), Some("F1"));
        assert_eq!(choices.last().map(String::as_str), Some("Menu"));
    }

    #[test]
    fn a_combination_normalises_to_the_panes_spelling() {
        assert_eq!(
            canonical_combination(Some("shift + super + a")).unwrap(),
            "SUPER + SHIFT + A"
        );
        assert_eq!(canonical_key("page_up"), "Page_Up");
        assert_eq!(canonical_key("XF86AudioPlay"), "XF86AudioPlay");
        assert_eq!(combination_id(" SUPER  +  Return "), "super+return");
    }

    #[test]
    fn two_spellings_of_one_shortcut_share_a_signature() {
        assert_eq!(
            combination_signature("SUPER+SHIFT+A").unwrap(),
            combination_signature("shift + super + a").unwrap()
        );
    }

    #[test]
    fn a_bare_key_is_refused_unless_it_is_safe_to_bind_alone() {
        assert!(matches!(
            require_bindable(Some("A")),
            Err(KeybindError::Standalone(_))
        ));
        assert_eq!(require_bindable(Some("f5")).unwrap(), "F5");
        assert_eq!(
            require_bindable(Some("XF86AudioPlay")).unwrap(),
            "XF86AudioPlay"
        );
        assert_eq!(require_bindable(Some("mouse:272")).unwrap(), "mouse:272");
    }

    #[test]
    fn the_refusals_name_the_part_that_was_wrong() {
        assert!(matches!(parse_combination(None), Err(KeybindError::NoKey)));
        assert!(matches!(
            parse_combination(Some("SUPER+")),
            Err(KeybindError::NotAShortcut(_))
        ));
        assert!(matches!(
            parse_combination(Some("HYPER+A")),
            Err(KeybindError::NotAModifier(_))
        ));
        assert!(matches!(
            parse_combination(Some("SUPER+code:9999")),
            Err(KeybindError::NotAKeyName(_))
        ));
    }

    #[test]
    fn tab_and_newline_are_not_control_characters_here() {
        assert!(!has_control_character("echo hi\n\tthere"));
        assert!(has_control_character("echo \u{7} hi"));
        assert!(has_control_character("echo \u{7f} hi"));
    }
}
