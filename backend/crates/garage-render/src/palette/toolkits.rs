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
//! Doc-only: this writes many small files through the per-toolkit builders in
//! [`crate::palette::gtk`], [`crate::palette::rofi`], [`crate::palette::qt`],
//! [`crate::palette::swayosd`] and [`crate::palette::waybar`], and is itself reached only
//! from [`crate::theme::render_theme`], which carries the stub.
