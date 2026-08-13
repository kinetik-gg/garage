//! `render_toolkits()`: every toolkit config the resolved scheme decides, for one scheme.
//!
//! GTK3 and GTK4 settings and CSS, `xsettingsd.conf`, rofi's `config.rasi` and palette,
//! swayosd's style, kitty's theme, btop's theme, micro's colourscheme, the generated
//! `hyprlock-theme.conf`, and `qt6ct.conf` -- one function that writes all of them for a
//! single resolved scheme, called once per render with `scheme = resolve_theme(config)`.
//!
//! Both appearances' palette files are written on every call, not just the resolved one:
//! each toolkit's entry point names its own palette file by scheme, and a render interrupted
//! between the two would otherwise leave a stylesheet importing a file that is not there --
//! which in GTK's case is a silent failure that drops the whole palette, not an error anyone
//! would see. Writing both halves every time is what makes that interruption harmless.
//!
//! Written in place rather than atomically, unlike most of layer 3: waybar and kitty watch
//! these paths with `inotify`, and an atomic rename-into-place leaves their watch pointing at
//! the replaced inode. Every file here is small enough that a torn write is not a real risk,
//! which is the trade that makes in-place writing acceptable.
//!
//! The material still supplies the surface everywhere else, but the terminal carries some
//! body opacity of its own so text has something to sit on -- near-opaque would hide the
//! glass entirely, which is the trade `render_toolkits()` avoids by fixing the terminal's
//! opacity independently of the glass slider.
//!
//! Writes many small files through the per-toolkit builders in [`crate::palette::gtk`],
//! [`crate::palette::rofi`], [`crate::palette::qt`], [`crate::palette::swayosd`] and
//! [`crate::palette::waybar`], and is itself reached only from
//! [`crate::theme::render_theme`].
//!
//! # One write is still owed
//!
//! `waybar/style.css` is the twenty-first file, and the only one whose contents read a
//! preference rather than only the palette. It is written from `waybar_style_css()`, which
//! composes [`crate::bar::style`]'s `bar_background()` with [`crate::bar::spacing`]'s
//! `waybar_spacing_css()` -- the scaled padding table, which is a port of its own and belongs
//! to the bar task rather than to this one. Its call sits below in the Python's position,
//! commented rather than written, so the file the bar task has to fill in is the one line it
//! reads.

use std::fmt::Write as _;

use garage_core::fs::atomic::atomic_write;
use garage_core::fs::marker::write_marker;
use garage_core::paths::Paths;
use garage_core::schema::enums::Scheme;

use crate::error::RenderError;
use crate::palette::gtk::{gtk3_palette_css, gtk4_palette_css};
use crate::palette::qt::qt_palette_conf;
use crate::palette::rofi::rofi_palette_rasi;
use crate::palette::swayosd::swayosd_palette_css;
use crate::palette::table::role;
use crate::theme::opaque;

/// What one appearance is called by each toolkit that names a theme rather than reading a
/// palette (`THEME_TOOLKITS`, garage:284-289).
///
/// The portal's own `color-scheme` value (`prefer-dark` / `prefer-light`) is the fifth key of
/// the Python's table and is deliberately absent here: it is read by `push_theme()`, on the
/// apply side, and nothing this crate writes carries it.
struct Look {
    gtk: &'static str,
    icons: &'static str,
    prefer_dark: u8,
    qt_colors: &'static str,
}

const fn look(scheme: Scheme) -> Look {
    match scheme {
        Scheme::Dark => Look {
            gtk: "adw-gtk3-dark",
            icons: "Papirus-Dark",
            prefer_dark: 1,
            qt_colors: "vanta.conf",
        },
        Scheme::Light => Look {
            gtk: "adw-gtk3",
            icons: "Papirus-Light",
            prefer_dark: 0,
            qt_colors: "vanta-light.conf",
        },
    }
}

/// The two appearances, in the Python's `SCHEMES` order (garage:332).
const SCHEMES: [Scheme; 2] = [Scheme::Light, Scheme::Dark];

/// The terminal's own body opacity, `f"{0.5:g}"`. The material still supplies the surface,
/// but the terminal carries some body of its own so text has something to sit on.
const TERM_OPACITY: &str = "0.5";

/// Write every toolkit config the resolved scheme decides (garage:4022-4233).
///
/// Split into one helper per group only to keep each under the workspace's function-length
/// lint; the write order is the Python's, unbroken, and each helper is called exactly once.
///
/// # Errors
///
/// [`RenderError::Marker`] if any of the in-place writes failed, [`RenderError::Atomic`] if
/// the generated `hyprlock-theme.conf` could not be replaced, or
/// [`RenderError::CompositedRole`] if a role Qt or hyprlock reads is not an opaque hex.
pub(crate) fn render_toolkits(paths: &Paths, scheme: Scheme) -> Result<(), RenderError> {
    let look = look(scheme);
    write_gtk_settings(paths, scheme, &look)?;
    write_palettes(paths)?;
    write_xsettingsd_and_rofi(paths, scheme, &look)?;
    // The bar's colours, its spacing and the base sheet it imports; see waybar_style_css(),
    // which render_bar_style() also writes on its own. Not ported -- see the module docs:
    //     write(paths, "waybar/style.css", &waybar_style_css(scheme, prefs))?;
    write_apps(paths, scheme)?;
    write_hyprlock(paths, scheme)?;
    write(
        paths,
        "qt6ct/qt6ct.conf",
        &format!(
            "[Appearance]
color_scheme_path={home}/.config/qt6ct/colors/{qt_colors}
custom_palette=true
icon_theme={icons}
standard_dialogs=gtk3
style=Fusion

[Fonts]
fixed=\"CaskaydiaMono Nerd Font Mono,10,-1,5,50,0,0,0,0,0\"
general=\"Plus Jakarta Sans,10,-1,5,50,0,0,0,0,0\"
",
            home = paths.home.display(),
            qt_colors = look.qt_colors,
            icons = look.icons
        ),
    )
}

/// One config file, in place rather than atomically: waybar and kitty watch these paths, and
/// a rename leaves their inotify watch pointing at the replaced inode. All of these are small
/// enough that a torn write is not a real risk.
fn write(paths: &Paths, relative: &str, text: &str) -> Result<(), RenderError> {
    write_marker(&paths.config_home.join(relative), text)?;
    Ok(())
}

/// A `PALETTE` role for one scheme. Every name below is one of the table's fixed roles --
/// `palette::parity::palette_matches_the_python_dump` pins the whole table against the Python
/// source -- so the empty fallback is unreachable and would be visible in the first generated
/// file if it ever were not.
fn colour(scheme: Scheme, name: &str) -> &'static str {
    role(scheme, name).unwrap_or_default()
}

/// The two `settings.ini` files and the two `gtk.css` entry points (garage:4045-4079).
fn write_gtk_settings(paths: &Paths, scheme: Scheme, look: &Look) -> Result<(), RenderError> {
    let shared = format!(
        "gtk-font-name=Plus Jakarta Sans 11\n\
         gtk-icon-theme-name={}\n\
         gtk-cursor-theme-name=macOS\n\
         gtk-cursor-theme-size=24\n\
         gtk-application-prefer-dark-theme={}\n",
        look.icons, look.prefer_dark
    );
    write(
        paths,
        "gtk-3.0/settings.ini",
        &format!(
            "# Generated by garage.
[Settings]
gtk-theme-name={}
{shared}gtk-toolbar-style=GTK_TOOLBAR_ICONS
gtk-toolbar-icon-size=GTK_ICON_SIZE_LARGE_TOOLBAR
gtk-button-images=0
gtk-menu-images=0
gtk-enable-event-sounds=1
gtk-enable-input-feedback-sounds=0
gtk-xft-antialias=1
gtk-xft-hinting=1
gtk-xft-hintstyle=hintslight
gtk-xft-rgba=rgb
",
            look.gtk
        ),
    )?;
    write(
        paths,
        "gtk-4.0/settings.ini",
        &format!("# Generated by garage.\n[Settings]\n{shared}"),
    )?;
    // gtk.css itself has to be generated: the palette it imports is the thing that changes,
    // and GTK offers no way to redirect an @import at runtime.
    write(
        paths,
        "gtk-3.0/gtk.css",
        &format!(
            "/* Generated by garage. */\n@import url(\"apple-{scheme}.css\");\n\
             @import url(\"thunar.css\");\n"
        ),
    )?;
    // GTK4 carries both schemes in one file behind a prefers-color-scheme query, so running
    // apps follow the portal instead of freezing the palette they launched with. GTK3 has no
    // media queries, so it still imports per scheme.
    write(
        paths,
        "gtk-4.0/gtk.css",
        "/* Generated by garage. */\n@import url(\"apple.css\");\n",
    )
}

/// Both appearances' palette files, written on every render rather than only the resolved one
/// (garage:4081-4092) -- see the module docs for the interrupted render that rule is for.
fn write_palettes(paths: &Paths) -> Result<(), RenderError> {
    for appearance in SCHEMES {
        write(
            paths,
            &format!("gtk-3.0/apple-{appearance}.css"),
            &gtk3_palette_css(appearance),
        )?;
        write(
            paths,
            &format!("rofi/apple-{appearance}.rasi"),
            &rofi_palette_rasi(appearance),
        )?;
        write(
            paths,
            &format!("swayosd/swayosd-{appearance}.css"),
            &swayosd_palette_css(appearance),
        )?;
    }
    write(paths, "gtk-4.0/apple.css", &gtk4_palette_css())?;
    for appearance in SCHEMES {
        write(
            paths,
            &format!("qt6ct/colors/{}", look(appearance).qt_colors),
            &qt_palette_conf(appearance)?,
        )?;
    }
    Ok(())
}

/// `XSettings`, which is what re-themes GTK3 and `XWayland` apps without restarting them, and
/// rofi's own configuration (garage:4094-4125).
fn write_xsettingsd_and_rofi(
    paths: &Paths,
    scheme: Scheme,
    look: &Look,
) -> Result<(), RenderError> {
    write(
        paths,
        "xsettingsd/xsettingsd.conf",
        &format!(
            "Net/ThemeName \"{}\"
Gtk/FontName \"Plus Jakarta Sans 11\"
Net/IconThemeName \"{}\"
Gtk/CursorThemeName \"macOS\"
Net/EnableEventSounds 1
EnableInputFeedbackSounds 0
Xft/Antialias 1
Xft/Hinting 1
Xft/HintStyle \"hintslight\"
Xft/RGBA \"rgb\"
",
            look.gtk, look.icons
        ),
    )?;
    write(
        paths,
        "rofi/config.rasi",
        &format!(
            "/* Generated by garage. */
configuration {{
    modes: \"drun\";
    show-icons: true;
    icon-theme: \"Adwaita\";
    display-drun: \"\";
    drun-display-format: \"{{name}}\";
    matching: \"fuzzy\";
    sort: true;
    click-to-exit: true;
    steal-focus: true;
}}

@theme \"apple-{scheme}\"
"
        ),
    )
}

/// swayosd's entry point, kitty's theme, btop's theme and micro's colourscheme
/// (garage:4130-4211).
fn write_apps(paths: &Paths, scheme: Scheme) -> Result<(), RenderError> {
    write(
        paths,
        "swayosd/style.css",
        &format!("/* Generated by garage. */\n@import \"swayosd-{scheme}.css\";\n"),
    )?;
    write(paths, "kitty/theme.conf", &kitty_theme(scheme))?;
    write(paths, "btop/themes/vanta.theme", &btop_theme(scheme))?;
    write(
        paths,
        "micro/settings.json",
        &format!(
            "{{\n    \"colorscheme\": \"{}\"\n}}\n",
            match scheme {
                Scheme::Light => "catppuccin-latte",
                Scheme::Dark => "catppuccin-macchiato",
            }
        ),
    )
}

/// The generated lock theme (garage:4212-4222), the one file here that lands under
/// `generated/` rather than in `~/.config` and therefore the one written atomically.
///
/// hyprlang spells a colour `rgba(rrggbbaa)` or `rgb(rrggbb)`, with no `#`, and the
/// placeholder is interpolated into Pango markup where it needs the bare hex. Only the
/// no-alpha form is used, so `lock_color()`'s `rgba` branch has no call site to port.
fn write_hyprlock(paths: &Paths, scheme: Scheme) -> Result<(), RenderError> {
    let digits = |name: &str| opaque(scheme, name).map(|hex| hex.trim_start_matches('#'));
    atomic_write(
        &paths.generated.join("hyprlock-theme.conf"),
        &format!(
            "# Generated by garage.
$font_color = rgb({})
$placeholder_hex = {}
$check_hex = {}
$fail_hex = {}
",
            digits("fg_strong")?,
            digits("fg_muted")?,
            digits("accent")?,
            digits("danger")?
        ),
    )?;
    Ok(())
}

/// kitty's theme (garage:4144-4159). kitty paints its own background over the glass, so the
/// terminal stays dark until this changes no matter what the compositor does: `body_opaque`
/// is the one body that is not the window body, the highest-contrast step of the ramp, so
/// body text has as much of it as the appearance allows.
fn kitty_theme(scheme: Scheme) -> String {
    let hex = |name: &str| colour(scheme, name);
    format!(
        "# Generated by garage.
# Fully transparent: the glass underlay supplies the surface.
background_opacity          {TERM_OPACITY}
background                  {body}
foreground                  {fg}
cursor                      {fg_strong}
cursor_text_color           {body}
selection_background        {accent}
selection_foreground        {accent_fg}
url_color                   {link}
active_border_color         {accent}
inactive_border_color       {line}
active_tab_foreground       {fg_strong}
active_tab_background       {bg_lifted}
inactive_tab_foreground     {fg_muted}
inactive_tab_background     {bg}
",
        body = hex("body_opaque"),
        fg = hex("fg"),
        fg_strong = hex("fg_strong"),
        accent = hex("accent"),
        accent_fg = hex("accent_fg"),
        link = hex("link"),
        line = hex("line"),
        bg_lifted = hex("bg_lifted"),
        fg_muted = hex("fg_muted"),
        bg = hex("bg"),
    )
}

/// Which palette role each `btop` theme key reads, in the order the generated file writes
/// them (garage:4168-4207). An empty role is an empty value: `main_bg` keeps the terminal's
/// own background, which is the glass, and each `*_mid` left empty makes btop interpolate a
/// two-stop gradient rather than a three.
///
/// The gradients run through the signal colours, which are the same in both appearances: a
/// CPU at 90 degrees is not a lighter shade of hot on a light desktop.
const BTOP_KEYS: &[(&str, &str)] = &[
    ("main_bg", ""),
    ("main_fg", "fg"),
    ("title", "fg"),
    ("hi_fg", "accent"),
    ("selected_bg", "row_selected"),
    ("selected_fg", "fg"),
    ("inactive_fg", "fg_muted"),
    ("graph_text", "fg_muted"),
    ("meter_bg", "meter_track"),
    ("proc_misc", "accent"),
    ("cpu_box", "line_box"),
    ("mem_box", "line_box"),
    ("net_box", "line_box"),
    ("proc_box", "line_box"),
    ("div_line", "line_faint"),
    ("temp_start", "ok"),
    ("temp_mid", "warn"),
    ("temp_end", "danger"),
    ("cpu_start", "ok"),
    ("cpu_mid", "warn"),
    ("cpu_end", "danger"),
    ("free_start", "fg_muted"),
    ("free_mid", ""),
    ("free_end", "fg"),
    ("cached_start", "info"),
    ("cached_mid", ""),
    ("cached_end", "accent"),
    ("available_start", "warn"),
    ("available_mid", ""),
    ("available_end", "caution"),
    ("used_start", "ok"),
    ("used_mid", "warn"),
    ("used_end", "danger"),
    ("download_start", "info"),
    ("download_mid", ""),
    ("download_end", "accent"),
    ("upload_start", "caution"),
    ("upload_mid", ""),
    ("upload_end", "danger"),
    ("process_start", "accent"),
    ("process_mid", ""),
    ("process_end", "violet"),
];

/// btop's theme. btop reads a named theme file once at startup, so the file's contents are
/// swapped rather than the config's `color_theme` -- that keeps `btop.conf` stowed.
fn btop_theme(scheme: Scheme) -> String {
    let mut out = String::from(
        "# Generated by garage.\n\
         # An empty main_bg keeps the terminal's own background, which is the glass.\n",
    );
    for &(key, name) in BTOP_KEYS {
        let value = if name.is_empty() {
            ""
        } else {
            colour(scheme, name)
        };
        let _ = writeln!(out, "theme[{key}]=\"{value}\"");
    }
    out
}
