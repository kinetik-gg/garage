//! `render_toolkits()`: every toolkit config the resolved scheme decides, for one scheme.
//!
//! GTK3 and GTK4 settings and CSS, `xsettingsd.conf`, kitty's theme, btop's theme,
//! micro's colourscheme, the generated
//! `hyprlock-theme.conf`, and `qt6ct.conf` -- one function that writes all of them for a
//! single resolved scheme, called once per render with `scheme = resolve_theme(config)`.
//!
//! Both appearances' palette files are written on every call, not just the resolved one:
//! each toolkit's entry point names its own palette file by scheme, and a render interrupted
//! between the two would otherwise leave a stylesheet importing a file that is not there --
//! which in GTK's case is a silent failure that drops the whole palette, not an error anyone
//! would see. Writing both halves every time is what makes that interruption harmless.
//!
//! Kitty's theme is written in place rather than atomically: it watches its path with
//! `inotify`, and an atomic rename-into-place leaves that watch pointing at the replaced
//! inode. Every file here is small enough that a torn write is not a real risk, which is
//! the trade that makes in-place writing acceptable.
//!
//! The material still supplies the surface everywhere else, but the terminal carries some
//! body opacity of its own so text has something to sit on -- near-opaque would hide the
//! glass entirely, which is the trade `render_toolkits()` avoids by fixing the terminal's
//! opacity independently of the glass slider.
//!
//! Writes many small files through the per-toolkit builders in [`crate::palette::gtk`] and
//! [`crate::palette::qt`], and is itself reached only from [`crate::theme::render_theme`].
//!
//! # Which of these are templates
//!
//! The four writes this module makes on its own that carry real text -- `xsettingsd.conf`,
//! kitty's theme, btop's theme and the generated `hyprlock-theme.conf` -- are
//! [`crate::template`] files now, as are all three palette builders it calls. What is left
//! as a literal here is the short, structural half: the two `settings.ini` files and the
//! two `gtk.css` entry points, micro's `settings.json` and `qt6ct.conf`. Each of those is
//! one `@import` or a handful of keys
//! naming a file this same function just wrote, so their text is a statement about this
//! module's own output layout rather than about how anything looks -- editing one without
//! editing the write beside it produces a config pointing at a file that is not there.

use garage_core::fs::atomic::atomic_write;
use garage_core::fs::marker::write_marker;
use garage_core::paths::Paths;
use garage_core::schema::enums::Scheme;

use crate::error::RenderError;
use crate::palette::gtk::{gtk3_palette_css, gtk4_palette_css};
use crate::palette::qt::qt_palette_conf;
use crate::palette::table::role;
use crate::template::shipped::{
    BTOP_THEME_HEAD, BTOP_THEME_LINE, HYPRLOCK_THEME, KITTY_THEME, XSETTINGSD,
};
use crate::template::vars::template_vars;
use crate::template::{NoVars, Template};
use crate::theme::opaque;

template_vars!(
    /// The two names `XSettings` republishes to X11 clients. Everything else in
    /// `xsettingsd.tmpl` -- the font, the cursor, the hinting -- is fixed text that no
    /// preference reaches.
    XsettingsdVars {
        gtk_theme: &'static str,
        icon_theme: &'static str,
    }
);

template_vars!(
    /// kitty's ten roles and its own body opacity.
    KittyVars {
        opacity: &'static str,
        body: &'static str,
        fg: &'static str,
        fg_strong: &'static str,
        accent: &'static str,
        accent_fg: &'static str,
        link: &'static str,
        line: &'static str,
        bg_lifted: &'static str,
        fg_muted: &'static str,
        bg: &'static str,
    }
);

template_vars!(
    /// One `btop` theme key and the colour behind it. Which keys exist, and which role each
    /// reads, is [`BTOP_KEYS`] -- a table, walked in code.
    BtopLineVars {
        key: &'static str,
        value: &'static str,
    }
);

template_vars!(
    /// The four colours hyprlock is handed, each already reduced to bare hex digits: the
    /// `rgb()` around the first is hyprlang syntax and lives in the template, and the other
    /// three are interpolated into Pango markup that wants the digits alone.
    HyprlockVars {
        font_color: &'static str,
        placeholder_hex: &'static str,
        check_hex: &'static str,
        fail_hex: &'static str,
    }
);

/// What one appearance is called by each toolkit that names a theme rather than reading a
/// palette (`THEME_TOOLKITS`, garage:284-289).
///
/// All five keys of the Python's table, including the portal's own `color-scheme` value
/// (`prefer-dark` / `prefer-light`) -- which nothing this crate writes carries, but which
/// `push_theme()` on the apply side reads out of the same row. Kept together rather than
/// split across the two crates so a future appearance is named once.
#[derive(Copy, Clone, Debug)]
pub struct Look {
    /// The GTK theme name `adw-gtk3` ships under, for `gsettings set ... gtk-theme`.
    pub gtk: &'static str,
    /// The icon theme name, for `gsettings set ... icon-theme`.
    pub icons: &'static str,
    /// `gtk-application-prefer-dark-theme`, which is an integer in the settings.ini it lands
    /// in rather than a boolean.
    pub prefer_dark: u8,
    /// The qt6ct colour file this appearance selects.
    pub qt_colors: &'static str,
    /// The XDG portal's `color-scheme`, read only by `push_theme()`.
    pub portal: &'static str,
}

/// `THEME_TOOLKITS[scheme]` (garage:284-289).
#[must_use]
pub const fn look(scheme: Scheme) -> Look {
    match scheme {
        Scheme::Dark => Look {
            gtk: "adw-gtk3-dark",
            icons: "Papirus-Dark",
            prefer_dark: 1,
            qt_colors: "vanta.conf",
            portal: "prefer-dark",
        },
        Scheme::Light => Look {
            gtk: "adw-gtk3",
            icons: "Papirus-Light",
            prefer_dark: 0,
            qt_colors: "vanta-light.conf",
            portal: "prefer-light",
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
/// the generated `hyprlock-theme.conf` could not be replaced,
/// [`RenderError::CompositedRole`] if a role Qt or hyprlock reads is not an opaque hex, or
/// [`RenderError::Template`] if a template on disk names a variable no renderer supplies.
pub(crate) fn render_toolkits(paths: &Paths, scheme: Scheme) -> Result<(), RenderError> {
    let look = look(scheme);
    write_gtk_settings(paths, scheme, &look)?;
    write_palettes(paths)?;
    write_xsettingsd(paths, &look)?;
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

/// One config file, in place rather than atomically: kitty watches its theme's path, and a
/// rename leaves that inotify watch pointing at the replaced inode. All of these are small
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
            &gtk3_palette_css(paths, appearance)?,
        )?;
    }
    write(paths, "gtk-4.0/apple.css", &gtk4_palette_css(paths)?)?;
    for appearance in SCHEMES {
        write(
            paths,
            &format!("qt6ct/colors/{}", look(appearance).qt_colors),
            &qt_palette_conf(paths, appearance)?,
        )?;
    }
    Ok(())
}

/// `XSettings`, which re-themes GTK3 and `XWayland` apps without restarting them.
fn write_xsettingsd(paths: &Paths, look: &Look) -> Result<(), RenderError> {
    write(
        paths,
        "xsettingsd/xsettingsd.conf",
        &Template::load(paths, XSETTINGSD).expand(&XsettingsdVars {
            gtk_theme: look.gtk,
            icon_theme: look.icons,
        })?,
    )
}

/// Kitty's theme, btop's theme and micro's colourscheme.
fn write_apps(paths: &Paths, scheme: Scheme) -> Result<(), RenderError> {
    write(paths, "kitty/theme.conf", &kitty_theme(paths, scheme)?)?;
    write(
        paths,
        "btop/themes/vanta.theme",
        &btop_theme(paths, scheme)?,
    )?;
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
    let theme = Template::load(paths, HYPRLOCK_THEME).expand(&HyprlockVars {
        font_color: digits("fg_strong")?,
        placeholder_hex: digits("fg_muted")?,
        check_hex: digits("accent")?,
        fail_hex: digits("danger")?,
    })?;
    atomic_write(&paths.generated.join("hyprlock-theme.conf"), &theme)?;
    Ok(())
}

/// kitty's theme (garage:4144-4159). kitty paints its own background over the glass, so the
/// terminal stays dark until this changes no matter what the compositor does: `body_opaque`
/// is the one body that is not the window body, the highest-contrast step of the ramp, so
/// body text has as much of it as the appearance allows.
fn kitty_theme(paths: &Paths, scheme: Scheme) -> Result<String, RenderError> {
    let hex = |name: &str| colour(scheme, name);
    let theme = Template::load(paths, KITTY_THEME).expand(&KittyVars {
        opacity: TERM_OPACITY,
        body: hex("body_opaque"),
        fg: hex("fg"),
        fg_strong: hex("fg_strong"),
        accent: hex("accent"),
        accent_fg: hex("accent_fg"),
        link: hex("link"),
        line: hex("line"),
        bg_lifted: hex("bg_lifted"),
        fg_muted: hex("fg_muted"),
        bg: hex("bg"),
    })?;
    Ok(theme)
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
fn btop_theme(paths: &Paths, scheme: Scheme) -> Result<String, RenderError> {
    let mut out = Template::load(paths, BTOP_THEME_HEAD).expand(&NoVars)?;
    let line = Template::load(paths, BTOP_THEME_LINE);
    for &(key, name) in BTOP_KEYS {
        let value = if name.is_empty() {
            ""
        } else {
            colour(scheme, name)
        };
        out.push_str(&line.expand(&BtopLineVars { key, value })?);
    }
    Ok(out)
}
