//! `{{name}}` substitution, and nothing else: the whole template engine.
//!
//! A generated file is text with a few values in it. The text is data -- it belongs in a
//! file a person can open and edit -- and the values are logic, computed from the
//! preferences by the renderer that owns them. This module is the seam: a renderer names
//! a template and hands over a [`TemplateVars`], and gets the text back with the values
//! in it.
//!
//! # Deliberately not a template language
//!
//! `{{name}}` is the entire syntax. No conditionals, no loops, no filters, no escaping
//! hooks. Anything that *decides* something stays in Rust, where the schema types and the
//! tests are: which listeners `hypridle.conf` gets is a condition on three timeouts, so
//! the code picks the fragments and the fragments carry the text. A template that could
//! branch would be a second place where the desktop's behaviour is defined, in a language
//! with no type checker and no coverage.
//!
//! A single `{` is literal, which is why no shipped template needs an escape: every
//! target format here -- hyprlang, CSS, `rasi`, JSONC -- spells its own blocks with one
//! brace, and none of them ever writes two in a row.
//!
//! # Where a template comes from
//!
//! The same shape as [`garage_core::schema::defaults`]: the file under
//! `desktop/.config/garage/templates` is the source of truth, the session reads it from
//! `~/.config/garage/templates` (the stow link to it), and a copy is compiled in with
//! `include_str!` so a machine whose config was deleted still renders. See
//! [`Template::load`] for the one asymmetry -- an absent template falls back, a present
//! but broken one does not.

use std::borrow::Cow;

use garage_core::paths::Paths;

use crate::error::TemplateError;

pub(crate) mod shipped;
pub(crate) mod vars;

/// The values one expansion is given, by name.
///
/// An expansion only ever asks [`get`](TemplateVars::get) about the names a template
/// actually used, so [`names`](TemplateVars::names) exists for the two moments where the
/// whole list is what is wanted: [`TemplateError::Unknown`] prints it, because the reader
/// of that message has just misspelled one of them by hand; and the tests in [`shipped`]
/// check it in both directions -- every `{{name}}` in a shipped template is a name its
/// renderer supplies, and every name a renderer supplies is used by at least one of its
/// templates. The second is what catches drift: a variable left behind by an edit to the
/// text.
pub(crate) trait TemplateVars {
    /// This variable's value, or `None` if the renderer has no such variable -- which
    /// the engine turns into [`TemplateError::Unknown`].
    fn get(&self, name: &str) -> Option<Cow<'_, str>>;

    /// Every name this renderer answers to, for the drift check in [`shipped`].
    fn names() -> &'static [&'static str]
    where
        Self: Sized;
}

/// The variables of a template that has none: the fixed halves, like `hypridle.conf`'s
/// `general` block, which are text all the way through and still worth having in a file.
#[derive(Debug)]
pub(crate) struct NoVars;

impl TemplateVars for NoVars {
    fn get(&self, _name: &str) -> Option<Cow<'_, str>> {
        None
    }

    fn names() -> &'static [&'static str] {
        &[]
    }
}

/// One shipped template: the file name under `templates/`, and the copy compiled in.
#[derive(Copy, Clone, Debug)]
pub(crate) struct Shipped {
    /// The file's name, which is also what an error names.
    pub(crate) file: &'static str,
    /// The build-time copy of that same file, from `include_str!`.
    pub(crate) compiled: &'static str,
}

/// A template's text, from wherever it was found.
#[derive(Debug)]
pub(crate) struct Template {
    file: &'static str,
    text: Cow<'static, str>,
}

impl Template {
    /// Read the template, preferring the session's own copy over the compiled one.
    ///
    /// A template that cannot be read -- absent, or unreadable -- falls back to the
    /// compiled copy, because a deleted config must still boot a desktop. A template that
    /// *is* read is then used as it stands: if it names a variable that does not exist,
    /// [`Template::expand`] fails and the render fails with it. That asymmetry is the
    /// point. An absent file is a machine missing its dotfiles; a broken one is an edit
    /// someone made, and silently rendering the old text instead would hide it until the
    /// next login.
    pub(crate) fn load(paths: &Paths, shipped: Shipped) -> Self {
        let path = paths.root.join("templates").join(shipped.file);
        let text =
            std::fs::read_to_string(&path).map_or(Cow::Borrowed(shipped.compiled), Cow::Owned);
        Self {
            file: shipped.file,
            text,
        }
    }

    /// The template's text with every `{{name}}` replaced by its value.
    ///
    /// # Errors
    ///
    /// [`TemplateError::Unknown`] naming the template and the variable, or
    /// [`TemplateError::Unterminated`] for a `{{` with no `}}` after it.
    pub(crate) fn expand<V: TemplateVars>(&self, vars: &V) -> Result<String, TemplateError> {
        let mut rest: &str = &self.text;
        let mut out = String::with_capacity(rest.len());
        while let Some((before, opened)) = rest.split_once("{{") {
            out.push_str(before);
            let Some((name, after)) = opened.split_once("}}") else {
                return Err(TemplateError::Unterminated { file: self.file });
            };
            let Some(value) = vars.get(name) else {
                return Err(TemplateError::Unknown {
                    file: self.file,
                    variable: name.to_owned(),
                    given: V::names().join(", "),
                });
            };
            out.push_str(&value);
            rest = after;
        }
        out.push_str(rest);
        Ok(out)
    }

    /// [`Template::expand`], less the file's own final newline.
    ///
    /// For the fragments that are spliced into the middle of another template rather than
    /// concatenated at its end. Every text file ends with a newline and these are text
    /// files; the block itself does not carry one, and the caller that splices it is the
    /// one that knows what follows.
    ///
    /// # Errors
    ///
    /// Whatever [`Template::expand`] returns.
    pub(crate) fn expand_block<V: TemplateVars>(&self, vars: &V) -> Result<String, TemplateError> {
        let mut text = self.expand(vars)?;
        if text.ends_with('\n') {
            text.pop();
        }
        Ok(text)
    }
}
