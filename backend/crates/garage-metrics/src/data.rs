//! Layouts
//!
//! 22px tall to match the bar's other modules, laid out left to right as
//! icon / graph / value / extra. The first column used to be the metric's name in text --
//! CPU, MEM, TEMP -- and is a Phosphor glyph now, from the same icon set the bar's bell
//! and the shell's buttons already draw, so the width the strip spends on saying which
//! metric it is goes to the numbers instead. Network is the exception: its individual
//! down/up arrows already label both figures, so a third bidirectional glyph would repeat
//! rather than identify them.
//!
//! Only the widgets with a genuine second number carry an `extra` column, and only the
//! widgets with a genuine second series draw one -- GPU, VRAM under load. Network has no
//! graph at all: it is an icon and two throughput figures, because one log-scaled line
//! shared by up and down said less than the figures beside it, and a widget carrying two
//! numbers is the one that can least afford the room.
//!
//! The geometry is arithmetic rather than taste. The icon box is `ICON_SIZE` wide at
//! x=0, every column starts 6px after the one before it ends, and each width is the last
//! column's x plus the widest string that column can hold, rounded up, plus 1px so the
//! final glyph is not clipped at the viewBox edge. The x values are spelled out below
//! rather than computed because [`LAYOUTS`] has to stay a reviewable literal alongside
//! the bar renderer's matching width table. Measured in the strip's own
//! face, Plus Jakarta Sans 600 at 11.5px: "100%" 33px, "100°" 27px, "999.9M" 42px,
//! "24.0G" 36px, "↓999.9M" 50px. The one string that runs a character past that is a
//! rate in the 1000-1023 MiB/s band, "1023.9M", which was over the old widths too; 7px of
//! permanent emptiness in every strip is the worse side of that trade.
//!
//! Kept in a file of its own because the two tables are most of it and neither is code:
//! the rendering that reads them is easier to follow when it is not preceded by six
//! Phosphor path strings, and a path string is easier to check against upstream when
//! nothing is interleaved with it.

/// One widget's geometry. Every field is an integer because the Python's are, and the
/// difference shows: `{graph_x}` interpolates as `20`, and a float would put `20.0` in
/// the `d` attribute.
///
/// `None` means the column is absent rather than zero, preserving the source data model's
/// distinction between a missing column and a zero value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Layout {
    /// The viewBox width, and the width Waybar reserves for the strip.
    pub(crate) width: i32,
    /// The Phosphor glyph identifying the metric, for the five widgets that have one.
    pub(crate) icon: Option<&'static str>,
    /// The graph's left edge, for the five widgets that draw one.
    pub(crate) graph_x: Option<i32>,
    /// The graph's width in pixels.
    pub(crate) graph_width: Option<i32>,
    /// Where the primary reading starts.
    pub(crate) value_x: i32,
    /// Where the secondary reading starts, for the two widgets that carry one.
    pub(crate) extra_x: Option<i32>,
    /// Network only: the two direction glyphs that stand in for a metric icon.
    pub(crate) direction_icons: Option<(i32, i32)>,
}

/// The layout table, keyed the way `LAYOUTS[widget]` is.
///
/// Also the list of valid `--bar-svg` arguments: the Python's `if widget not in LAYOUTS`
/// is the only validation there is, so a widget with no layout is a usage error rather
/// than a widget that renders blank.
pub(crate) const LAYOUTS: [(&str, Layout); 6] = [
    (
        "cpu",
        Layout {
            width: 82,
            icon: Some("cpu"),
            graph_x: Some(20),
            graph_width: Some(22),
            value_x: 48,
            extra_x: None,
            direction_icons: None,
        },
    ),
    (
        "memory",
        Layout {
            width: 82,
            icon: Some("memory"),
            graph_x: Some(20),
            graph_width: Some(22),
            value_x: 48,
            extra_x: None,
            direction_icons: None,
        },
    ),
    (
        "temp",
        Layout {
            width: 76,
            icon: Some("thermometer-simple"),
            graph_x: Some(20),
            graph_width: Some(22),
            value_x: 48,
            extra_x: None,
            direction_icons: None,
        },
    ),
    (
        "disk",
        Layout {
            width: 91,
            icon: Some("hard-drives"),
            graph_x: Some(20),
            graph_width: Some(22),
            value_x: 48,
            extra_x: None,
            direction_icons: None,
        },
    ),
    (
        "gpu",
        Layout {
            width: 124,
            icon: Some("graphics-card"),
            graph_x: Some(20),
            graph_width: Some(22),
            value_x: 48,
            extra_x: Some(87),
            direction_icons: None,
        },
    ),
    // Two same-size direction glyphs label the individual readings; there is no third,
    // redundant network glyph in front of the pair.
    (
        "network",
        Layout {
            width: 130,
            icon: None,
            graph_x: None,
            graph_width: None,
            value_x: 20,
            extra_x: Some(88),
            direction_icons: Some((0, 68)),
        },
    ),
];

/// `LAYOUTS[widget]`, or nothing for a name that is not a widget.
pub(crate) fn layout(widget: &str) -> Option<Layout> {
    LAYOUTS
        .iter()
        .find(|(name, _)| *name == widget)
        .map(|(_, layout)| *layout)
}

/// The icon box. `ICON_SIZE` is a little taller than the text's cap height on purpose:
/// at 14px in a 22px strip the glyph reads as a label at a glance.
pub(crate) const ICON_SIZE: f64 = 14.0;

/// The `.secondary` fill the strip's own second number uses, which is what keeps the
/// icon a label rather than the thing the eye lands on first.
pub(crate) const ICON_OPACITY: &str = "0.75";

/// The glyphs themselves, as the `d` of the single path each icon in Phosphor's regular
/// weight is drawn with, on Phosphor's own 256x256 box. Held here rather than read from
/// the shell's `icons/` directory so a strip still draws on a machine where only this
/// binary is installed; the same six paths are checked in there as house assets, in that
/// directory's format, so the popovers can draw them too.
/// Source: github.com/phosphor-icons/core, assets/regular, MIT.
pub(crate) const ICON_PATHS: [(&str, &str); 7] = [
    (
        "cpu",
        "M152,96H104a8,8,0,0,0-8,8v48a8,8,0,0,0,8,8h48a8,8,0,0,0,8-8V104A8,8,0,0,0,152,96Zm-8,\
         48H112V112h32Zm88,0H216V112h16a8,8,0,0,0,0-16H216V56a16,16,0,0,0-16-16H160V24a8,8,0,0,\
         0-16,0V40H112V24a8,8,0,0,0-16,0V40H56A16,16,0,0,0,40,56V96H24a8,8,0,0,0,0,16H40v32H24a8,\
         8,0,0,0,0,16H40v40a16,16,0,0,0,16,16H96v16a8,8,0,0,0,16,0V216h32v16a8,8,0,0,0,16,\
         0V216h40a16,16,0,0,0,16-16V160h16a8,8,0,0,0,0-16Zm-32,56H56V56H200v95.87s0,.09,0,.13,0,\
         .09,0,.13V200Z",
    ),
    (
        "memory",
        "M232,56H24A16,16,0,0,0,8,72V200a8,8,0,0,0,16,0V184H40v16a8,8,0,0,0,16,0V184H72v16a8,8,0,\
         0,0,16,0V184h16v16a8,8,0,0,0,16,0V184h16v16a8,8,0,0,0,16,0V184h16v16a8,8,0,0,0,16,\
         0V184h16v16a8,8,0,0,0,16,0V184h16v16a8,8,0,0,0,16,0V72A16,16,0,0,0,232,56ZM24,\
         72H232v96H24Zm88,80a8,8,0,0,0,8-8V96a8,8,0,0,0-8-8H48a8,8,0,0,0-8,8v48a8,8,0,0,0,8,\
         8ZM56,104h48v32H56Zm88,48h64a8,8,0,0,0,8-8V96a8,8,0,0,0-8-8H144a8,8,0,0,0-8,8v48A8,8,0,\
         0,0,144,152Zm8-48h48v32H152Z",
    ),
    (
        "thermometer-simple",
        "M136,153V88a8,8,0,0,0-16,0v65a32,32,0,1,0,16,0Zm-8,47a16,16,0,1,1,16-16A16,16,0,0,1,128,\
         200Zm40-66V48a40,40,0,0,0-80,0v86a64,64,0,1,0,80,0Zm-40,98a48,48,0,0,1-27.42-87.4A8,8,0,\
         0,0,104,138V48a24,24,0,0,1,48,0v90a8,8,0,0,0,3.42,6.56A48,48,0,0,1,128,232Z",
    ),
    (
        "hard-drives",
        "M208,136H48a16,16,0,0,0-16,16v48a16,16,0,0,0,16,16H208a16,16,0,0,0,16-16V152A16,16,0,0,\
         0,208,136Zm0,64H48V152H208v48Zm0-160H48A16,16,0,0,0,32,56v48a16,16,0,0,0,16,16H208a16,\
         16,0,0,0,16-16V56A16,16,0,0,0,208,40Zm0,64H48V56H208v48ZM192,80a12,12,0,1,1-12-12A12,12,\
         0,0,1,192,80Zm0,96a12,12,0,1,1-12-12A12,12,0,0,1,192,176Z",
    ),
    (
        "graphics-card",
        "M232,48H16a8,8,0,0,0-8,8V208a8,8,0,0,0,16,0V192H40v16a8,8,0,0,0,16,0V192H72v16a8,8,0,0,\
         0,16,0V192h16v16a8,8,0,0,0,16,0V192H232a16,16,0,0,0,16-16V64A16,16,0,0,0,232,48Zm0,\
         128H24V64H232Zm-56-16a40,40,0,1,0-40-40A40,40,0,0,0,176,160Zm-24-40a23.74,23.74,0,0,1,\
         2.35-10.34l32,32A23.74,23.74,0,0,1,176,144,24,24,0,0,1,152,120Zm48,0a23.74,23.74,0,0,\
         1-2.35,10.34l-32-32A23.74,23.74,0,0,1,176,96,24,24,0,0,1,200,120ZM80,160a40,40,0,1,\
         0-40-40A40,40,0,0,0,80,160ZM56,120a23.74,23.74,0,0,1,2.35-10.34l32,32A23.74,23.74,0,0,1,\
         80,144,24,24,0,0,1,56,120Zm48,0a23.74,23.74,0,0,1-2.35,10.34l-32-32A23.74,23.74,0,0,1,\
         80,96,24,24,0,0,1,104,120Z",
    ),
    (
        "arrow-down",
        "M205.66,122.34l-72,72a8,8,0,0,1-11.32,0l-72-72a8,8,0,0,1,11.32-11.32L120,169.37V32\
         a8,8,0,0,1,16,0V169.37L194.34,111a8,8,0,0,1,11.32,11.32Z",
    ),
    (
        "arrow-up",
        "M205.66,133.66a8,8,0,0,1-11.32,0L136,75.31V224a8,8,0,0,1-16,0V75.31L61.66,133.66\
         a8,8,0,0,1-11.32-11.32l72-72a8,8,0,0,1,11.32,0l72,72A8,8,0,0,1,205.66,133.66Z",
    ),
];

/// `ICON_PATHS[name]`. The Python subscripts this directly, so a missing name is a
/// `KeyError` -- unreachable, since every name comes from [`LAYOUTS`] or is one of the
/// two arrow literals, and the test below pins that.
pub(crate) fn icon_path(name: &str) -> &'static str {
    ICON_PATHS
        .iter()
        .find(|(key, _)| *key == name)
        .map_or("", |(_, path)| *path)
}

/// The graph's top edge, 3px down from the top of the 22px strip.
pub(crate) const GRAPH_TOP: i32 = 3;

/// The graph's height, leaving 3px of air below it as well.
pub(crate) const GRAPH_HEIGHT: i32 = 16;

/// The graph's baseline -- where a reading of zero is drawn.
pub(crate) const GRAPH_BOTTOM: i32 = GRAPH_TOP + GRAPH_HEIGHT;

/// 120 points at the bar's 2s interval is four minutes of history -- long enough that a
/// compile or a game launch is still on screen when you look up, short enough that the
/// strip's graph is 22px wide and still readable.
pub(crate) const POINTS: usize = 120;

#[cfg(test)]
mod tests {
    use super::{icon_path, layout, ICON_PATHS, LAYOUTS, POINTS};
    use crate::dirs::WIDGETS;

    #[test]
    fn every_widget_has_a_layout_and_every_layout_is_a_widget() {
        for widget in WIDGETS {
            assert!(layout(widget).is_some(), "{widget} has no layout");
        }
        assert_eq!(LAYOUTS.len(), WIDGETS.len());
        for (name, _) in LAYOUTS {
            assert!(WIDGETS.contains(&name), "{name} is not a widget");
        }
    }

    #[test]
    fn the_widths_are_the_ones_the_bars_own_config_is_pinned_against() {
        // The bar renderer carries these widths too; a mismatch makes strips overlap.
        let widths: Vec<(&str, i32)> = LAYOUTS
            .iter()
            .map(|(name, layout)| (*name, layout.width))
            .collect();
        assert_eq!(
            widths,
            [
                ("cpu", 82),
                ("memory", 82),
                ("temp", 76),
                ("disk", 91),
                ("gpu", 124),
                ("network", 130),
            ]
        );
    }

    #[test]
    fn network_is_the_one_widget_with_no_icon_and_no_graph() {
        let network = layout("network").expect("network is a widget");
        assert_eq!(network.icon, None);
        assert_eq!(network.graph_width, None);
        assert_eq!(network.direction_icons, Some((0, 68)));
        for widget in ["cpu", "memory", "temp", "disk", "gpu"] {
            let other = layout(widget).expect("is a widget");
            assert!(other.icon.is_some());
            assert_eq!(other.graph_x, Some(20));
            assert_eq!(other.graph_width, Some(22));
            assert_eq!(other.direction_icons, None);
        }
    }

    #[test]
    fn gpu_and_network_are_the_only_widgets_with_a_second_column() {
        let with_extra: Vec<&str> = LAYOUTS
            .iter()
            .filter(|(_, layout)| layout.extra_x.is_some())
            .map(|(name, _)| *name)
            .collect();
        assert_eq!(with_extra, ["gpu", "network"]);
    }

    #[test]
    fn every_icon_a_layout_names_is_in_the_path_table() {
        for (_, layout) in LAYOUTS {
            if let Some(icon) = layout.icon {
                assert!(!icon_path(icon).is_empty(), "{icon} has no path");
            }
        }
        assert!(!icon_path("arrow-down").is_empty());
        assert!(!icon_path("arrow-up").is_empty());
    }

    #[test]
    fn every_path_is_one_phosphor_path_starting_at_a_moveto() {
        for (name, path) in ICON_PATHS {
            assert!(path.starts_with('M'), "{name} does not start with a moveto");
            assert!(path.ends_with('Z'), "{name} is not closed");
            assert!(
                !path.contains(' '),
                "{name} has whitespace in its path data"
            );
        }
    }

    #[test]
    fn the_history_length_is_four_minutes_of_two_second_ticks() {
        assert_eq!(POINTS, 120);
    }
}
