//! `qt_palette_conf()`: the Qt palette `qt6ct` loads, as three positional rows of colours.
//!
//! `QPalette::ColorRole` is positional and cannot be reordered, so the row for each of
//! `active`, `inactive` and `disabled` is built by walking the same fixed role list in the
//! same order every time, and a wrapped comment naming every role is written into the file's
//! own header -- the role order is the file's only documentation of what each column is, and
//! it is what makes a hand-read of this file possible at all.
//!
//! Every role goes through [`crate::theme`]'s `opaque()`: `qt6ct` can only spell `#rrggbb`,
//! and a composited role handed to it would write the literal text `rgba(...)` into a file
//! whose parser takes neither -- Qt would then silently fall back to Fusion's own grey rather
//! than failing loudly. Better to fail here, where the role table is, than on the next login.
//!
//! Returns a `String`, written by `render_toolkits()` rather than by this module.

use garage_core::paths::Paths;
use garage_core::schema::enums::Scheme;

use crate::error::RenderError;
use crate::palette::table::QT_ROLES;
use crate::template::shipped::{QT_PALETTE_BODY, QT_PALETTE_HEAD};
use crate::template::vars::template_vars;
use crate::template::Template;
use crate::theme::opaque;

template_vars!(
    /// Which appearance the header names.
    QtHeadVars { scheme: Scheme }
);

template_vars!(
    /// The three rows, each already joined. `QPalette` has exactly these three states and
    /// no more, so they are three named variables rather than a list: the row names are
    /// fixed text and live in `qt-palette-body.tmpl` beside `[ColorScheme]`.
    QtBodyVars {
        active: String,
        inactive: String,
        disabled: String,
    }
);

/// `textwrap.wrap()`'s width for the role-order comment, and its two indents, exactly as the
/// Python passes them.
const WRAP_WIDTH: usize = 76;
const WRAP_INDENT: &str = "; ";

/// The Qt palette `qt6ct` loads, as three positional rows of twenty-two (garage:3818-3830).
///
/// # Errors
///
/// [`RenderError::CompositedRole`] if any `QT_ROLES` entry resolves to a composited colour
/// -- see [`opaque`] for why that is a refusal rather than a fallback -- or
/// [`RenderError::Template`] if either template names a variable this does not supply.
///
/// # The wrapped comment is not a template
///
/// Between the header and the body sits `textwrap.wrap()`'s output over the role names,
/// which is computed from the table's own contents and rewraps whenever the table changes.
/// There is no fixed text in it at all, so it stays in code -- see [`wrap_roles`].
pub(crate) fn qt_palette_conf(paths: &Paths, scheme: Scheme) -> Result<String, RenderError> {
    let mut out = Template::load(paths, QT_PALETTE_HEAD).expand(&QtHeadVars { scheme })?;
    // The role order is the file's only documentation of what each column is, and it is what
    // makes a hand-read of this file possible at all.
    let names: Vec<&str> = QT_ROLES.iter().map(|&(name, ..)| name).collect();
    for line in wrap_roles(&names) {
        out.push_str(&line);
        out.push('\n');
    }
    out.push_str(&Template::load(paths, QT_PALETTE_BODY).expand(&QtBodyVars {
        active: row(scheme, 0)?,
        inactive: row(scheme, 1)?,
        disabled: row(scheme, 2)?,
    })?);
    Ok(out)
}

/// One positional row of colours: every `QT_ROLES` entry's colour for one state, in the
/// table's order, `", "` apart.
fn row(scheme: Scheme, state: usize) -> Result<String, RenderError> {
    let mut row = String::new();
    for &(_, active, inactive, disabled) in QT_ROLES {
        let role = match state {
            0 => active,
            1 => inactive,
            _ => disabled,
        };
        if !row.is_empty() {
            row.push_str(", ");
        }
        row.push_str(opaque(scheme, role)?);
    }
    Ok(row)
}

/// `textwrap.wrap(", ".join(names) + ".", width=76, initial_indent="; ", subsequent_indent=
/// "; ")`, for the one shape of input this file ever hands it.
///
/// `textwrap` splits on whitespace and greedily fills each line to `width` counting the
/// indent, dropping the space it would otherwise have ended on. With no hyphens, no long
/// words and single spaces throughout -- which is every role name Qt has -- that reduces to
/// the greedy fill below. A role name longer than the 74 columns left after the indent would
/// be broken mid-word by `textwrap` and is placed on a line of its own here instead; the
/// longest `QPalette::ColorRole` name is `HighlightedText`, so the two cannot disagree on any
/// input this function can be given.
fn wrap_roles(names: &[&str]) -> Vec<String> {
    let available = WRAP_WIDTH - WRAP_INDENT.len();
    let last = names.len().saturating_sub(1);
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    for (index, name) in names.iter().enumerate() {
        let word = if index == last {
            format!("{name}.")
        } else {
            format!("{name},")
        };
        let would_be = if current.is_empty() {
            word.len()
        } else {
            current.len() + 1 + word.len()
        };
        if !current.is_empty() && would_be > available {
            lines.push(format!("{WRAP_INDENT}{current}"));
            current = word;
        } else {
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(&word);
        }
    }
    if !current.is_empty() {
        lines.push(format!("{WRAP_INDENT}{current}"));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::{wrap_roles, QT_ROLES};

    /// The four lines `textwrap.wrap()` produces for the shipped role list, taken from the
    /// Python itself rather than reasoned about.
    #[test]
    fn the_role_comment_wraps_where_textwrap_wraps_it() {
        let names: Vec<&str> = QT_ROLES.iter().map(|&(name, ..)| name).collect();
        assert_eq!(
            wrap_roles(&names),
            vec![
                "; WindowText, Button, Light, Midlight, Dark, Mid, Text, BrightText,",
                "; ButtonText, Base, Window, Shadow, Highlight, HighlightedText, Link,",
                "; LinkVisited, AlternateBase, NoRole, ToolTipBase, ToolTipText,",
                "; PlaceholderText, Accent.",
            ]
        );
    }

    #[test]
    fn a_single_role_needs_no_wrapping_at_all() {
        assert_eq!(wrap_roles(&["Accent"]), vec!["; Accent."]);
    }
}
