//! Newtypes for values that carry meaning: workspace ids, blocks, connectors,
//! colours, clock times. A bare integer can never become one by assignment.

use core::fmt;
use core::str::FromStr;

/// The width of the id block each display owns, whatever its count. Ten (the
/// workspace count ceiling) is the natural size: a display can never want
/// more slots than there are number keys, so a block that wide never has to
/// be resized and no display's ids can ever depend on another display's
/// count. (`WORKSPACE_BLOCK`, garage:842-849)
pub const WORKSPACE_BLOCK: u32 = 10;

/// A Hyprland workspace id, as assigned by `per_display_groups()`
/// (garage:2624-2650): `block.first() * WORKSPACE_BLOCK + 1` and up. Ids are
/// never zero -- the block arithmetic starts every block's first id at 1 --
/// so the guarded constructor below is what a value parsed from outside the
/// module has to go through; there is no `From<u32>`.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct WorkspaceId(u32);

impl WorkspaceId {
    /// Rejects zero: no Hyprland workspace is ever numbered 0.
    #[must_use]
    pub const fn new(id: u32) -> Option<Self> {
        if id > 0 {
            Some(Self(id))
        } else {
            None
        }
    }

    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Which block of workspace ids a display owns. `workspace_blocks()`
/// (garage:2592-2618) hands out indices starting at 0 -- "one seen for the
/// first time takes the lowest block nothing else holds" -- and never
/// reclaims one, so every `u32` is a legitimate block index and there is no
/// guarded constructor to fail. There is still no `From<u32>`: a block index
/// is not interchangeable with any other count the schema carries.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct BlockId(u32);

impl BlockId {
    #[must_use]
    pub const fn new(index: u32) -> Self {
        Self(index)
    }

    #[must_use]
    pub const fn index(self) -> u32 {
        self.0
    }

    /// The first workspace id in this block: `block * WORKSPACE_BLOCK + 1`,
    /// matching `per_display_groups()`'s `blocks[output] * WORKSPACE_BLOCK +
    /// 1` exactly. Always >= 1, so constructing the result directly (rather
    /// than through the guarded `WorkspaceId::new`) is sound for every
    /// `BlockId` that exists.
    #[must_use]
    pub const fn first(self) -> WorkspaceId {
        WorkspaceId(self.0 * WORKSPACE_BLOCK + 1)
    }
}

/// A compositor output name (a connector, in Hyprland's sense: `DP-1`,
/// `eDP-1`, ...). Unlike the workspace ids, any string a compositor reports
/// is a valid connector, so `From<&str>` is the constructor rather than a
/// fallible parse.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct ConnectorName(Box<str>);

impl ConnectorName {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for ConnectorName {
    fn from(value: &str) -> Self {
        Self(value.into())
    }
}

impl fmt::Display for ConnectorName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Returned when a string does not match `is_hex_color`'s `#rrggbb` shape.
#[derive(Debug, thiserror::Error)]
#[error("invalid hex color: {0:?}")]
pub struct ParseHexColorError(Box<str>);

/// A `#rrggbb` colour, stored with both the string it was parsed from and
/// the decoded channels. Kept apart because writers pass the stored string
/// through verbatim -- magick gets it as-is -- so normalizing the spelling
/// (case-folding it, for instance) would change generated files even though
/// the colour is unchanged. (garage:1958, `is_hex_color`)
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct HexColor {
    raw: Box<str>,
    rgb: [u8; 3],
}

impl HexColor {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.raw
    }

    #[must_use]
    pub const fn rgb(&self) -> [u8; 3] {
        self.rgb
    }
}

const fn hex_nibble(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        b'A'..=b'F' => byte - b'A' + 10,
        // Unreachable in practice: callers only reach this after confirming
        // every byte is an ASCII hex digit. 0 is a harmless, silent fallback
        // rather than a panic for a byte that should never arrive.
        _ => 0,
    }
}

impl FromStr for HexColor {
    type Err = ParseHexColorError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // is_hex_color (garage:1958): exactly `#` followed by six hex
        // digits, either case. No short form, no alpha channel.
        let Some(digits) = s.strip_prefix('#') else {
            return Err(ParseHexColorError(s.into()));
        };
        if digits.len() != 6 || !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(ParseHexColorError(s.into()));
        }
        let mut rgb = [0u8; 3];
        for (channel, pair) in rgb.iter_mut().zip(digits.as_bytes().chunks(2)) {
            let [hi, lo] = pair else {
                return Err(ParseHexColorError(s.into()));
            };
            *channel = hex_nibble(*hi) * 16 + hex_nibble(*lo);
        }
        Ok(Self { raw: s.into(), rgb })
    }
}

impl fmt::Display for HexColor {
    // Re-emits the raw spelling it was parsed from, not a normalized one --
    // see the struct comment on why that matters to the writers that consume
    // this.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.raw)
    }
}

/// Returned when a string does not match `is_clock_time`'s `HH:MM` shape.
#[derive(Debug, thiserror::Error)]
#[error("invalid clock time: {0:?}")]
pub struct ParseClockTimeError(Box<str>);

/// A time of day at minute resolution, stored the way `night_shift_start`,
/// `theme_light_at` and the rest of the schema's `"time"`-kind keys are:
/// zero-padded 24-hour `HH:MM`. (garage:1954, `is_clock_time`)
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ClockTime {
    hour: u8,
    minute: u8,
}

impl ClockTime {
    #[must_use]
    pub const fn hour(self) -> u8 {
        self.hour
    }

    #[must_use]
    pub const fn minute(self) -> u8 {
        self.minute
    }

    /// Mirrors `minutes()` (garage:3677-3679), which every night-shift and
    /// theme-schedule comparison in the Python source goes through so the two
    /// clock-shaped keys are compared as plain integers rather than strings.
    #[must_use]
    #[allow(clippy::cast_lossless)] // u16::from(u8) is not yet a const fn on stable
    pub const fn minutes_from_midnight(self) -> u16 {
        self.hour as u16 * 60 + self.minute as u16
    }
}

impl FromStr for ClockTime {
    type Err = ParseClockTimeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // is_clock_time (garage:1954): `(?:[01]\d|2[0-3]):[0-5]\d` exactly --
        // two digits, a colon, two digits. "7:30" (no leading zero) and
        // "24:00" (hour out of range) both fail; so does any string that
        // isn't precisely five bytes long.
        let [h1, h2, b':', m1, m2] = *s.as_bytes() else {
            return Err(ParseClockTimeError(s.into()));
        };
        if ![h1, h2, m1, m2].iter().all(u8::is_ascii_digit) {
            return Err(ParseClockTimeError(s.into()));
        }
        let hour = (h1 - b'0') * 10 + (h2 - b'0');
        let minute = (m1 - b'0') * 10 + (m2 - b'0');
        if hour > 23 || minute > 59 {
            return Err(ParseClockTimeError(s.into()));
        }
        Ok(Self { hour, minute })
    }
}

impl fmt::Display for ClockTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:02}:{:02}", self.hour, self.minute)
    }
}

#[cfg(test)]
mod tests {
    use super::{BlockId, ClockTime, HexColor, WorkspaceId};

    #[test]
    fn workspace_id_rejects_zero() {
        assert!(WorkspaceId::new(0).is_none());
        assert_eq!(WorkspaceId::new(1).unwrap().get(), 1);
    }

    #[test]
    fn block_id_first_arithmetic() {
        assert_eq!(BlockId::new(0).first(), WorkspaceId::new(1).unwrap());
        assert_eq!(BlockId::new(1).first(), WorkspaceId::new(11).unwrap());
        assert_eq!(BlockId::new(2).first(), WorkspaceId::new(21).unwrap());
    }

    #[test]
    fn hex_color_preserves_raw_spelling() {
        let color: HexColor = "#3584E4".parse().unwrap();
        assert_eq!(color.as_str(), "#3584E4");
        assert_eq!(color.to_string(), "#3584E4");
        assert_eq!(color.rgb(), [0x35, 0x84, 0xE4]);

        let lower: HexColor = "#3584e4".parse().unwrap();
        assert_eq!(lower.to_string(), "#3584e4");
        assert_eq!(lower.rgb(), color.rgb());
    }

    #[test]
    fn hex_color_rejects_malformed_input() {
        assert!("3584e4".parse::<HexColor>().is_err());
        assert!("#3584e".parse::<HexColor>().is_err());
        assert!("#3584e44".parse::<HexColor>().is_err());
        assert!("#3584eg".parse::<HexColor>().is_err());
    }

    #[test]
    fn clock_time_accepts_exactly_what_python_accepts() {
        assert!("00:00".parse::<ClockTime>().is_ok());
        assert!("23:59".parse::<ClockTime>().is_ok());
        assert!("07:30".parse::<ClockTime>().is_ok());

        // Rejected by is_clock_time's regex, verified against the Python
        // source directly (garage:1954) before writing this test.
        assert!("24:00".parse::<ClockTime>().is_err());
        assert!("7:30".parse::<ClockTime>().is_err());
        assert!("23:60".parse::<ClockTime>().is_err());
        assert!("1:00".parse::<ClockTime>().is_err());
        assert!(" 07:30".parse::<ClockTime>().is_err());
        assert!("07:30 ".parse::<ClockTime>().is_err());
    }

    #[test]
    fn clock_time_round_trips_and_converts_to_minutes() {
        let time: ClockTime = "18:30".parse().unwrap();
        assert_eq!(time.to_string(), "18:30");
        assert_eq!(time.minutes_from_midnight(), 18 * 60 + 30);
        assert_eq!(
            "00:00"
                .parse::<ClockTime>()
                .unwrap()
                .minutes_from_midnight(),
            0
        );
        assert_eq!(
            "23:59"
                .parse::<ClockTime>()
                .unwrap()
                .minutes_from_midnight(),
            23 * 60 + 59
        );
    }
}
