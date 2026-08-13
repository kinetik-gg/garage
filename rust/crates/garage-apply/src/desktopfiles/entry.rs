//! `application_dirs()`, `desktop_file()` and `desktop_fields()`: finding and parsing a
//! `.desktop` file.
//!
//! `application_dirs()` follows the XDG lookup order -- `$XDG_DATA_HOME/applications` first,
//! then each of `$XDG_DATA_DIRS`'s `applications` subdirectories -- and `desktop_file()`
//! walks that list in order and returns the first match, which is what makes the resolution
//! agree with what every other XDG-aware tool on the system would resolve to.
//!
//! `desktop_fields()` reads `[Desktop Entry]`'s keys by hand rather than through a general
//! `.ini` reader: an `Exec` value legitimately carries `%` and `;`, which a reader that does
//! percent-interpolation would choke on, and a desktop file legitimately repeats a key in
//! localised forms (`Name[fr]`, `Name[de]`, ...) that some `.ini` readers reject outright as
//! duplicates. Only the first value seen for any given key is kept, which for the
//! unlocalised keys this reads is simply "first line wins".
//!
//! Doc-only: returns paths and field maps, not `Result<(), ApplyError>`.
