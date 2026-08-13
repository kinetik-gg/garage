//! The ten probes `doctor_checks()` walks: Hyprland's version, packages, fonts, stow links,
//! dead links, systemd units, plugins, generated fragments, preferences, and the compositor.
//!
//! Each is a small, independent read: `check_hyprland()` compares the running version against
//! `MINIMUM_HYPRLAND`, the support floor below which the tracked `hyprland.lua` fails in ways
//! that read as Garage bugs rather than as an old compositor, compared numerically rather
//! than as a string so `0.56.10` correctly outranks `0.56.9`. `check_packages()` runs one
//! `pacman -Q` for the whole named set rather than one per package. `check_fonts()` asks
//! `fc-list` by family name, not by package name, because a font can be installed and still
//! not be found by fontconfig. `check_units()` treats "not enabled" as a failure outright and
//! "enabled but not active" as a failure only when a graphical session is actually up, since
//! nothing is expected to be running from a bare TTY. `check_compositor()` and
//! `check_fragments()`' `luac` step are both informational, not failing, when the thing they
//! would check is simply absent (no compositor answering, no `luac` installed) -- a health
//! check that fails outside the situation it is meant to catch is a health check nobody
//! trusts.
//!
//! `check_preferences()` is the one probe whose hint names a Garage command rather than a
//! `pacman` or `systemctl` one: `garage repair`, because an unparseable `preferences.toml` is
//! the one problem this product can fix itself.
//!
//! Doc-only: every probe returns `(status, detail, hint)`, not `Result<(), ApplyError>` over
//! a [`SessionCx`](crate::cx::SessionCx).
