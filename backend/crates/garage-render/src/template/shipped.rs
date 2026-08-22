//! Every template this build ships, and the tests that keep them honest.
//!
//! One constant per file, each pairing the name the session looks for under
//! `~/.config/garage/templates` with the copy `include_str!` took from
//! `desktop/.config/garage/templates` at build time. Same arrangement as
//! [`garage_core::schema::defaults`]'s `COMPILED`, and for the same reason: the file in
//! the dotfiles tree is the source of truth, and the compiled copy exists so a machine
//! that has lost its config still renders rather than refusing to start a session.
//!
//! The tests at the bottom are the half an expansion cannot check. They read each
//! template's compiled text, pull out its `{{name}}`s, and compare them against the
//! `names()` of the [`TemplateVars`](super::TemplateVars) its renderer actually hands
//! over -- in both directions, so a placeholder with no variable behind it and a variable
//! no template uses are both build failures.

use super::Shipped;

/// One shipped template: the constant, and the file it is compiled from.
///
/// The file name is written once and used twice -- as the name looked up at runtime and
/// as the `include_str!` path -- so the two cannot drift apart.
macro_rules! shipped {
    ($(#[$note:meta])* $konst:ident, $file:literal) => {
        $(#[$note])*
        pub(crate) const $konst: Shipped = Shipped {
            file: $file,
            compiled: include_str!(concat!(
                "../../../../../desktop/.config/garage/templates/",
                $file
            )),
        };
    };
}

shipped!(
    /// `hypridle.conf`'s shell: the header and the `general` block, ending on the blank
    /// line the first listener sits after.
    HYPRIDLE_BASE,
    "hypridle-base.tmpl"
);

shipped!(
    /// `lock.lock_timeout`'s listener.
    HYPRIDLE_LISTENER_LOCK,
    "hypridle-listener-lock.tmpl"
);

shipped!(
    /// `lock.display_off_timeout`'s listener: DPMS off on timeout, back on on resume.
    HYPRIDLE_LISTENER_DPMS,
    "hypridle-listener-dpms.tmpl"
);

shipped!(
    /// `lock.suspend_timeout`'s listener.
    HYPRIDLE_LISTENER_SUSPEND,
    "hypridle-listener-suspend.tmpl"
);

shipped!(
    /// The whole of `hyprpaper.conf`.
    HYPRPAPER,
    "hyprpaper.tmpl"
);

shipped!(
    /// `locale.env`'s header, which is the whole file when there is no override.
    LOCALE_ENV,
    "locale-env.tmpl"
);

shipped!(
    /// The `export LANG=` line an override adds.
    LOCALE_ENV_EXPORT,
    "locale-env-export.tmpl"
);

shipped!(
    /// The GTK3 palette's header line.
    GTK3_PALETTE_HEAD,
    "gtk3-palette-head.tmpl"
);

shipped!(
    /// One `@define-color` line of the GTK3 palette.
    GTK3_PALETTE_TOKEN,
    "gtk3-palette-token.tmpl"
);

shipped!(
    /// The GTK4 sheet the two blocks of custom properties are wrapped in.
    GTK4_PALETTE,
    "gtk4-palette.tmpl"
);

shipped!(
    /// One custom property of the GTK4 palette's light block.
    GTK4_PALETTE_LIGHT_TOKEN,
    "gtk4-palette-light-token.tmpl"
);

shipped!(
    /// One custom property of the GTK4 palette's dark block, which the media query
    /// indents one level further.
    GTK4_PALETTE_DARK_TOKEN,
    "gtk4-palette-dark-token.tmpl"
);

shipped!(
    /// The whole of rofi's palette.
    ROFI_PALETTE,
    "rofi-palette.tmpl"
);

shipped!(
    /// The Qt palette's two header lines. Between them and the body sits the wrapped role
    /// comment, which is computed rather than written -- see [`crate::palette::qt`].
    QT_PALETTE_HEAD,
    "qt-palette-head.tmpl"
);

shipped!(
    /// The Qt palette's `[ColorScheme]` section and its three positional rows.
    QT_PALETTE_BODY,
    "qt-palette-body.tmpl"
);

shipped!(
    /// The whole of swayosd's palette.
    SWAYOSD_PALETTE,
    "swayosd-palette.tmpl"
);

shipped!(
    /// The whole of kitty's theme.
    KITTY_THEME,
    "kitty-theme.tmpl"
);

shipped!(
    /// btop's theme header.
    BTOP_THEME_HEAD,
    "btop-theme-head.tmpl"
);

shipped!(
    /// One `theme[key]="colour"` line of btop's theme.
    BTOP_THEME_LINE,
    "btop-theme-line.tmpl"
);

shipped!(
    /// The whole of the generated `hyprlock-theme.conf`.
    HYPRLOCK_THEME,
    "hyprlock-theme.tmpl"
);

shipped!(
    /// The whole of `xsettingsd.conf`.
    XSETTINGSD,
    "xsettingsd.tmpl"
);

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    use super::Shipped;
    use crate::idle::IdleListenerVars;
    use crate::palette::gtk::{Gtk3HeadVars, Gtk4Vars, TokenVars};
    use crate::palette::qt::{QtBodyVars, QtHeadVars};
    use crate::palette::rofi::RofiVars;
    use crate::palette::swayosd::SwayosdVars;
    use crate::palette::toolkits::{BtopLineVars, HyprlockVars, KittyVars, XsettingsdVars};
    use crate::region::LocaleExportVars;
    use crate::template::TemplateVars;
    use crate::wallpaper::WallpaperVars;

    /// Every template above, for the checks that are about the set rather than about one
    /// renderer's own.
    const ALL: &[Shipped] = &[
        super::HYPRIDLE_BASE,
        super::HYPRIDLE_LISTENER_LOCK,
        super::HYPRIDLE_LISTENER_DPMS,
        super::HYPRIDLE_LISTENER_SUSPEND,
        super::HYPRPAPER,
        super::LOCALE_ENV,
        super::LOCALE_ENV_EXPORT,
        super::GTK3_PALETTE_HEAD,
        super::GTK3_PALETTE_TOKEN,
        super::GTK4_PALETTE,
        super::GTK4_PALETTE_LIGHT_TOKEN,
        super::GTK4_PALETTE_DARK_TOKEN,
        super::ROFI_PALETTE,
        super::QT_PALETTE_HEAD,
        super::QT_PALETTE_BODY,
        super::SWAYOSD_PALETTE,
        super::KITTY_THEME,
        super::BTOP_THEME_HEAD,
        super::BTOP_THEME_LINE,
        super::HYPRLOCK_THEME,
        super::XSETTINGSD,
    ];

    /// The directory the compiled copies were taken from, resolved from this crate rather
    /// than from the working directory so the test runs from anywhere.
    fn templates_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../desktop/.config/garage/templates")
    }

    /// Every `{{name}}` in a template, in the engine's own reading of it.
    fn placeholders(text: &str) -> BTreeSet<String> {
        let mut found = BTreeSet::new();
        let mut rest = text;
        while let Some((_, opened)) = rest.split_once("{{") {
            let (name, after) = opened
                .split_once("}}")
                .expect("a shipped template closes every placeholder it opens");
            found.insert(name.to_owned());
            rest = after;
        }
        found
    }

    /// Both directions, for one renderer's templates: no placeholder without a variable,
    /// no variable without a placeholder.
    fn check<V: TemplateVars>(family: &str, templates: &[Shipped]) {
        let supplied: BTreeSet<&str> = V::names().iter().copied().collect();
        let mut used: BTreeSet<String> = BTreeSet::new();
        for template in templates {
            for name in placeholders(template.compiled) {
                assert!(
                    supplied.contains(name.as_str()),
                    "{}: {{{{{name}}}}} is not one of {family}'s variables {supplied:?}",
                    template.file
                );
                used.insert(name);
            }
        }
        let unused: Vec<&&str> = supplied
            .iter()
            .filter(|name| !used.contains(**name))
            .collect();
        assert!(
            unused.is_empty(),
            "{family} supplies {unused:?}, which no template of its own uses -- \
             a variable left behind by an edit to the text"
        );
    }

    #[test]
    fn hypridle_placeholders_and_variables_agree() {
        check::<IdleListenerVars>(
            "hypridle",
            &[
                super::HYPRIDLE_BASE,
                super::HYPRIDLE_LISTENER_LOCK,
                super::HYPRIDLE_LISTENER_DPMS,
                super::HYPRIDLE_LISTENER_SUSPEND,
            ],
        );
    }

    #[test]
    fn hyprpaper_placeholders_and_variables_agree() {
        check::<WallpaperVars>("hyprpaper", &[super::HYPRPAPER]);
    }

    #[test]
    fn region_placeholders_and_variables_agree() {
        check::<LocaleExportVars>("locale.env", &[super::LOCALE_ENV, super::LOCALE_ENV_EXPORT]);
    }

    #[test]
    fn gtk_placeholders_and_variables_agree() {
        check::<Gtk3HeadVars>("the GTK3 palette header", &[super::GTK3_PALETTE_HEAD]);
        check::<Gtk4Vars>("the GTK4 palette", &[super::GTK4_PALETTE]);
        check::<TokenVars>(
            "a palette token",
            &[
                super::GTK3_PALETTE_TOKEN,
                super::GTK4_PALETTE_LIGHT_TOKEN,
                super::GTK4_PALETTE_DARK_TOKEN,
            ],
        );
    }

    #[test]
    fn rofi_and_swayosd_placeholders_and_variables_agree() {
        check::<RofiVars>("the rofi palette", &[super::ROFI_PALETTE]);
        check::<SwayosdVars>("the swayosd palette", &[super::SWAYOSD_PALETTE]);
    }

    #[test]
    fn qt_placeholders_and_variables_agree() {
        check::<QtHeadVars>("the Qt palette header", &[super::QT_PALETTE_HEAD]);
        check::<QtBodyVars>("the Qt palette body", &[super::QT_PALETTE_BODY]);
    }

    #[test]
    fn toolkit_placeholders_and_variables_agree() {
        check::<XsettingsdVars>("xsettingsd", &[super::XSETTINGSD]);
        check::<KittyVars>("kitty", &[super::KITTY_THEME]);
        check::<BtopLineVars>("btop", &[super::BTOP_THEME_HEAD, super::BTOP_THEME_LINE]);
        check::<HyprlockVars>("hyprlock", &[super::HYPRLOCK_THEME]);
    }

    /// The compiled copy is the shipped file, byte for byte.
    ///
    /// `include_str!` makes that true by construction at build time; what this catches is
    /// a template deleted or renamed under a `target/` that still has the old bytes
    /// cached, which is exactly the state a machine would then be shipped in.
    #[test]
    fn every_compiled_copy_matches_the_file_it_was_taken_from() {
        for template in ALL {
            let path = templates_dir().join(template.file);
            let found = std::fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            assert_eq!(found, template.compiled, "{}", template.file);
        }
    }

    /// Nothing in the templates directory is unreachable from Rust.
    ///
    /// An orphan is a file someone edits expecting the desktop to change, and nothing
    /// happens -- the worst failure mode this arrangement has, because it is silent.
    #[test]
    fn every_template_file_is_registered() {
        let registered: BTreeSet<&str> = ALL.iter().map(|template| template.file).collect();
        let mut orphans = Vec::new();
        for entry in std::fs::read_dir(templates_dir()).expect("the templates directory exists") {
            let name = entry.expect("a directory entry is readable").file_name();
            let name = name.to_string_lossy().into_owned();
            if !registered.contains(name.as_str()) {
                orphans.push(name);
            }
        }
        assert!(
            orphans.is_empty(),
            "templates no renderer names: {orphans:?}"
        );
    }
}
