//! `PALETTE`: the two schemes' semantic roles, and the vocabulary every toolkit writer reads.
//!
//! One visual identity, one table: every colour the desktop draws with is named here once,
//! per appearance, and every toolkit config that carries colour is rendered from it by
//! `render_toolkits()` -- GTK3, GTK4, Qt, rofi, swayosd, waybar, kitty, btop and hyprlock.
//! Nothing downstream holds a hex of its own. Before this table the same identity was
//! maintained by hand in thirteen files across six languages, and it had already drifted:
//! the Qt dark palette shared exactly two colours with the GTK dark palette it was supposed
//! to match, and the notification daemon this desktop replaced carried a structural sheet
//! with dark-only literals that made notification body text unreadable on a light desktop.
//! A colour that exists in one place cannot drift from itself. (`PALETTE`, garage:362-585)
//!
//! Each [`PALETTE`] row carries both schemes at once -- `bg`, `fg_strong`, `accent`,
//! `border_active`, and around sixty more. Most roles are opaque `#rrggbb`; some are
//! composited CSS `rgba(r, g, b, a)`, for a surface meant to let the glass or the wallpaper
//! show through. The two forms are not interchangeable: the Qt and hyprlock renderers can
//! only spell an opaque colour, so a role they read has to be a hex -- [`crate::theme`]'s
//! `opaque()` is what refuses to hand a composited role to a toolkit that can only spell one,
//! so the failure happens where this table is rather than silently on the next login.
//!
//! Naming: `bg*` are bodies from sunken to floating, `fg*` are text from loudest to
//! faintest, `line*` are 1px separators, `bevel_*` are Qt's four shading roles, and `on_bg`
//! is the colour that tints *onto* a body -- white on dark, black on light -- which is what
//! every hairline, sheen and hover is a fraction of.
//!
//! Doc-only beyond the tables below: this module is data, not a renderer.

use garage_core::schema::enums::Scheme;

/// The desktop's whole visual identity, one `(name, light, dark)` row per role. A plain tuple
/// rather than a named-field struct: `rustfmt`'s `struct_lit_width` (18, not overridable per
/// crate without a workspace-wide style change) forces every `Struct { .. }` literal here onto
/// four lines regardless of length, which is what would have pushed this table well past the
/// file's line budget; a tuple keeps every row that fits in 100 columns on one line, which is
/// most of them. (`PALETTE`, garage:388-572)
pub(crate) const PALETTE: &[(&str, &str, &str)] = &[
    // The tint direction, and the one colour that is never itself a body.
    ("on_bg", "#000000", "#ffffff"),
    ("accent_fg", "#ffffff", "#ffffff"),
    // Bodies, sunken to floating.
    ("bg_sunken", "#ffffff", "#161617"),
    ("bg", "#f2f2f7", "#1c1c1e"),
    ("bg_alt2", "#f7f7fa", "#202022"),
    ("bg_sidebar", "#ebebf0", "#202022"),
    ("bg_raised", "#f5f5f7", "#242426"),
    ("bg_float", "#ffffff", "#2c2c2e"),
    ("bg_lifted", "#d1d1d6", "#3a3a3c"),
    ("bg_well", "#ffffff", "#111113"),
    // The body a *floating* surface starts from, as opposed to a window's. The shell's
    // panels, the bar and the OSD all sit on the compositor's glass rather than in a
    // window, and take the same step of the ramp.
    ("body_base", "#f5f5f7", "#1c1c1e"),
    // The fully opaque body, at the far end of the ramp from the window one. Two surfaces
    // want it: the terminal, which paints its own body over the glass so text has maximum
    // contrast whatever the compositor is doing, and the glass tint itself, which is this
    // colour at 20%.
    ("body_opaque", "#ffffff", "#1c1c1e"),
    ("row_selected", "#d1d1d6", "#2c2c2e"),
    // A drop shadow is black in both appearances; only its opacity moves.
    ("shadow_base", "#000000", "#000000"),
    // The edge of a locked group. The two appearances disagree by a hair -- #888888 here
    // against #8e8e93 on light -- and the dark value is the one config/decorations.lua also
    // carries as its static default, so the pair is preserved as it stands rather than
    // unified from one side only.
    ("border_locked", "#8e8e93", "#888888"),
    ("meter_track", "#e5e5ea", "#2c2c2e"),
    // Qt's four shading roles, which are read as a bevel around Button. Light is lighter
    // than the body and Dark darker, in both appearances.
    ("bevel_light", "#ffffff", "#3a3a3c"),
    ("bevel_midlight", "#f7f7fa", "#2c2c2e"),
    ("bevel_mid", "#d1d1d6", "#111113"),
    ("bevel_dark", "#b0b0b5", "#000000"),
    ("bevel_faint", "#d8d8dd", "#202022"),
    // Separators. line_faint is softer than line_box, not harder: btop draws a box edge and
    // a divider inside it, and the divider is the quieter of the two in both appearances.
    ("line", "#c7c7cc", "#3a3a3c"),
    ("line_box", "#b0b0b5", "#3a3a3c"),
    ("line_faint", "#c7c7cc", "#48484a"),
    // The window edge. Deliberately off the neutral ramp: a window border reads against
    // the wallpaper, not against another surface.
    ("border_active", "#b0b0b5", "#666666"),
    ("border_inactive", "#e5e5ea", "#181818"),
    // Text, loudest to faintest.
    ("fg_bright", "#0b0b0d", "#f5f5f7"),
    ("fg_strong", "#1c1c1e", "#f5f5f7"),
    ("fg", "#1c1c1e", "#d1d1d6"),
    ("fg_backdrop", "#3a3a3c", "#bcbcbc"),
    ("fg_secondary", "#45454a", "#a8a8ad"),
    ("fg_muted", "#6c6c70", "#8e8e93"),
    ("fg_placeholder", "#8e8e93", "#8e8e93"),
    ("fg_disabled", "#8a8a90", "#6e6e73"),
    ("fg_faint", "#b0b0b5", "#6e6e73"),
    ("fg_placeholder_dim", "#aeaeb2", "#6e6e73"),
    ("knob", "#ffffff", "#e5e5e7"),
    // Accent and links. The accent itself is the appearance's own blue; the nine
    // *choosable* accents are `accents::ACCENTS`, a different axis.
    ("accent", "#007aff", "#0a84ff"),
    ("accent_soft", "#007aff", "#64aaff"),
    ("link", "#0060df", "#64aaff"),
    ("link_visited", "#5856d6", "#5e5ce6"),
    // Signal colours, shared by both appearances: a temperature graph and a failed unlock
    // mean the same thing whatever the desktop looks like.
    ("ok", "#32d74b", "#32d74b"),
    ("warn", "#ffd60a", "#ffd60a"),
    ("danger", "#ff453a", "#ff453a"),
    ("info", "#5ac8fa", "#5ac8fa"),
    ("caution", "#ff9f0a", "#ff9f0a"),
    ("violet", "#af52de", "#af52de"),
    // Status pairs, for the toolkits that ask for a background as well.
    ("error_bg", "#ffdad6", "#3a0000"),
    ("error_fg", "#93000a", "#ffffff"),
    ("warning_bg", "#fff4d6", "#2a220f"),
    ("warning_fg", "#614700", "#d8d0b8"),
    ("success_bg", "#d9f2e3", "#13231a"),
    ("success_fg", "#0b3d24", "#bfd0c5"),
    // Composited: a fraction of on_bg over whatever is behind it, or a translucent body
    // that lets the glass through.
    (
        "hairline",
        "rgba(0, 0, 0, 0.12)",
        "rgba(255, 255, 255, 0.12)",
    ),
    ("edge", "rgba(0, 0, 0, 0.12)", "rgba(255, 255, 255, 0.16)"),
    ("shade", "rgba(0, 0, 0, 0.12)", "rgba(0, 0, 0, 0.45)"),
    ("bar_dot", "#1c1c1e", "#ffffff"),
    (
        "bar_bg",
        "rgba(245, 245, 247, 0.42)",
        "rgba(28, 28, 30, 0.42)",
    ),
    ("bar_edge", "rgba(0, 0, 0, 0.12)", "rgba(0, 0, 0, 0.30)"),
    (
        "bar_sheen",
        "rgba(255, 255, 255, 0.60)",
        "rgba(255, 255, 255, 0.07)",
    ),
    (
        "tooltip_bg",
        "rgba(255, 255, 255, 0.95)",
        "rgba(44, 44, 46, 0.90)",
    ),
    (
        "urgent",
        "rgba(255, 69, 58, 0.95)",
        "rgba(255, 69, 58, 0.95)",
    ),
    (
        "osd_bg",
        "rgba(245, 245, 247, 0.94)",
        "rgba(28, 28, 30, 0.94)",
    ),
    (
        "osd_fg",
        "rgba(28, 28, 30, 0.92)",
        "rgba(245, 245, 247, 0.92)",
    ),
    (
        "osd_fg_muted",
        "rgba(28, 28, 30, 0.82)",
        "rgba(245, 245, 247, 0.82)",
    ),
    (
        "osd_track",
        "rgba(28, 28, 30, 0.20)",
        "rgba(245, 245, 247, 0.24)",
    ),
    (
        "osd_fill",
        "rgba(28, 28, 30, 0.85)",
        "rgba(245, 245, 247, 0.90)",
    ),
];

/// The colour a named role resolves to for one scheme, or [`None`] if `name` is not a
/// `PALETTE` role. The general-purpose lookup every per-toolkit writer built from
/// [`GTK3_TOKENS`], [`GTK4_TOKENS`] and [`QT_ROLES`] is expected to read `PALETTE` through,
/// so a role rename is one row here rather than an edit in every writer.
#[must_use]
pub(crate) fn role(scheme: Scheme, name: &str) -> Option<&'static str> {
    PALETTE
        .iter()
        .find(|(entry_name, _, _)| *entry_name == name)
        .map(|&(_, light, dark)| match scheme {
            Scheme::Light => light,
            Scheme::Dark => dark,
        })
}

/// Which palette role each GTK3 token name reads, in the order the generated file writes
/// them. GTK3 needs the pre-libadwaita `theme_*` tokens `adw-gtk3` still reads; the list is
/// not padded out to match [`GTK4_TOKENS`] -- a token a toolkit does not define today is a
/// token its stylesheet is currently getting from its own theme, and defining it here would
/// silently restyle the desktop. (`GTK3_TOKENS`, garage:611-653)
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "consumed by the toolkit emitters, task 3.4b")
)]
pub(crate) const GTK3_TOKENS: &[(&str, &str)] = &[
    ("accent_color", "accent_soft"),
    ("accent_bg_color", "accent"),
    ("accent_fg_color", "accent_fg"),
    ("window_bg_color", "bg"),
    ("window_fg_color", "fg"),
    ("view_bg_color", "bg_sunken"),
    ("view_fg_color", "fg"),
    ("headerbar_bg_color", "bg_raised"),
    ("headerbar_fg_color", "fg_strong"),
    ("headerbar_backdrop_color", "bg"),
    ("sidebar_bg_color", "bg_sidebar"),
    ("sidebar_fg_color", "fg"),
    ("sidebar_backdrop_color", "bg"),
    ("sidebar_border_color", "hairline"),
    ("popover_bg_color", "bg_float"),
    ("popover_fg_color", "fg_strong"),
    ("card_bg_color", "bg_float"),
    ("card_fg_color", "fg"),
    ("dialog_bg_color", "bg_raised"),
    ("dialog_fg_color", "fg_strong"),
    ("theme_bg_color", "bg"),
    ("theme_fg_color", "fg"),
    ("theme_base_color", "bg_sunken"),
    ("theme_text_color", "fg"),
    ("theme_selected_bg_color", "accent"),
    ("theme_selected_fg_color", "accent_fg"),
    ("theme_unfocused_bg_color", "bg"),
    ("theme_unfocused_fg_color", "fg_muted"),
    ("theme_unfocused_selected_bg_color", "bg_lifted"),
    ("theme_unfocused_selected_fg_color", "fg_backdrop"),
    ("destructive_bg_color", "error_bg"),
    ("destructive_fg_color", "error_fg"),
    ("error_bg_color", "error_bg"),
    ("error_fg_color", "error_fg"),
];

/// Which palette role each GTK4 token name reads. GTK4 needs the status pairs and the shade
/// libadwaita draws with, which GTK3 does not carry -- see [`GTK3_TOKENS`] for why neither
/// list is padded out to the other. (`GTK4_TOKENS`, garage:654-684)
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "consumed by the toolkit emitters, task 3.4b")
)]
pub(crate) const GTK4_TOKENS: &[(&str, &str)] = &[
    ("accent-color", "accent_soft"),
    ("accent-bg-color", "accent"),
    ("accent-fg-color", "accent_fg"),
    ("window-bg-color", "bg"),
    ("window-fg-color", "fg"),
    ("view-bg-color", "bg_sunken"),
    ("view-fg-color", "fg"),
    ("headerbar-bg-color", "bg_raised"),
    ("headerbar-fg-color", "fg_strong"),
    ("headerbar-backdrop-color", "bg"),
    ("sidebar-bg-color", "bg_sidebar"),
    ("sidebar-fg-color", "fg"),
    ("sidebar-backdrop-color", "bg"),
    ("sidebar-border-color", "hairline"),
    ("card-bg-color", "bg_float"),
    ("card-fg-color", "fg"),
    ("popover-bg-color", "bg_float"),
    ("popover-fg-color", "fg_strong"),
    ("dialog-bg-color", "bg_raised"),
    ("dialog-fg-color", "fg_strong"),
    ("destructive-bg-color", "error_bg"),
    ("destructive-fg-color", "error_fg"),
    ("error-bg-color", "error_bg"),
    ("error-fg-color", "error_fg"),
    ("warning-bg-color", "warning_bg"),
    ("warning-fg-color", "warning_fg"),
    ("success-bg-color", "success_bg"),
    ("success-fg-color", "success_fg"),
    ("shade-color", "shade"),
];

/// Qt's palette, positional: `QPalette::ColorRole` cannot be reordered, so each row names
/// the role it takes when the window is `active`, `inactive`, and `disabled`, in the same
/// fixed order `qt6ct` writes every time. Every role here has to resolve to an opaque hex --
/// `qt6ct`'s parser takes `#rrggbb` and nothing else, so a composited role would write the
/// literal string `rgba(...)` and the whole scheme would fail to load.
/// (`QT_ROLES`, garage:686-718)
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "consumed by the toolkit emitters, task 3.4b")
)]
pub(crate) const QT_ROLES: &[(&str, &str, &str, &str)] = &[
    ("WindowText", "fg", "fg_backdrop", "fg_faint"),
    ("Button", "bg", "bg", "bg"),
    ("Light", "bevel_light", "bevel_light", "bevel_light"),
    (
        "Midlight",
        "bevel_midlight",
        "bevel_midlight",
        "bevel_midlight",
    ),
    ("Dark", "bevel_dark", "bevel_dark", "line"),
    ("Mid", "bevel_mid", "bevel_mid", "bevel_faint"),
    ("Text", "fg", "fg_backdrop", "fg_faint"),
    ("BrightText", "accent_fg", "accent_fg", "accent_fg"),
    ("ButtonText", "fg", "fg_backdrop", "fg_faint"),
    ("Base", "bg_sunken", "bg_sunken", "bg_alt2"),
    ("Window", "bg", "bg", "bg"),
    ("Shadow", "line", "line", "bevel_faint"),
    ("Highlight", "accent", "bg_lifted", "bg_lifted"),
    ("HighlightedText", "accent_fg", "fg", "bg_alt2"),
    ("Link", "accent", "accent", "fg_faint"),
    ("LinkVisited", "link_visited", "link_visited", "fg_faint"),
    ("AlternateBase", "bg_alt2", "bg_alt2", "bg"),
    ("NoRole", "bg_sunken", "bg_sunken", "bg_sunken"),
    ("ToolTipBase", "bg_float", "bg_float", "bg_alt2"),
    ("ToolTipText", "fg", "fg", "fg_faint"),
    (
        "PlaceholderText",
        "fg_placeholder",
        "fg_placeholder_dim",
        "line",
    ),
    ("Accent", "accent", "accent", "fg_faint"),
];
