//! [`casefold`] -- the case-insensitive key every stored name, every stored path, and every
//! search query is compared through.
//!
//! **Deviation, reported rather than shipped quietly:** the Python folds with
//! `str.casefold()`, Unicode's full case-folding table, which is a strict superset of
//! lowercasing -- it is the table that also maps `'ß'` to `"ss"` and a handful of other
//! multi-character or non-reciprocal foldings `str.lower()` does not perform. Rust's
//! standard library has no `casefold()`; implementing Unicode's `CaseFolding.txt` from
//! scratch, or pulling in a crate that ships it (`caseless`, built on
//! `unicode-normalization`), is a real cost for a table whose divergence from `to_lowercase`
//! is a few hundred code points that essentially never appear in a filename. This function
//! uses [`str::to_lowercase`] -- itself Unicode-aware, just not Unicode's *folding* table --
//! and both index time ([`crate::scan::index_rows`]) and query time
//! ([`crate::search::search_index`]) go through this one function, so search stays
//! self-consistent even where it does not match `CPython` bit for bit on exotic input.
#[must_use]
pub(crate) fn casefold(text: &str) -> String {
    text.to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::casefold;

    #[test]
    fn ascii_folds_like_lowercase() {
        assert_eq!(casefold("Budget 2026.ODS"), "budget 2026.ods");
    }

    #[test]
    fn unicode_letters_lowercase_as_expected() {
        assert_eq!(casefold("CAFÉ"), "café");
    }
}
