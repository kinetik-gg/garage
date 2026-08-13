//! [`search_index`] -- the read path the launcher's `search QUERY [LIMIT]` answers through,
//! and the ranking ([`match_score`]) that decides what a query's top results are.

use crate::config::configuration;
use crate::db::connect_readonly;
use crate::error::FileIndexError;
use crate::fold::casefold;
use crate::json::Json;
use crate::paths::IndexPaths;

/// One row of `SELECT path,name,name_fold,parent,path_fold,kind,modified_ns`.
struct Candidate {
    path: String,
    name: String,
    name_fold: String,
    parent: String,
    path_fold: String,
    kind: String,
    modified_ns: i64,
}

/// One search hit, in the shape the launcher's QML reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SearchHit {
    pub kind: String,
    pub title: String,
    pub subtitle: String,
    pub path: String,
}

impl SearchHit {
    #[must_use]
    pub(crate) fn to_json(&self) -> Json {
        Json::Object(vec![
            ("kind".to_string(), Json::str(self.kind.clone())),
            ("title".to_string(), Json::str(self.title.clone())),
            ("subtitle".to_string(), Json::str(self.subtitle.clone())),
            ("path".to_string(), Json::str(self.path.clone())),
        ])
    }
}

/// Answer a query against the last committed index.
///
/// Empty input, indexing turned off, or no database file yet, are all "no results" rather
/// than an error -- and so, distinctly, is every `SQLite` failure reading the snapshot: a
/// corrupt or briefly-unreadable database degrades the launcher to no file results, never to
/// a visible error, matching the Python's own `except sqlite3.Error: return []` around this
/// one read.
///
/// # Errors
///
/// [`FileIndexError`] only if reading the `[indexing]` configuration itself fails for a
/// reason [`configuration`] does not already tolerate (a preferences file that exists but
/// cannot be read for a reason other than "missing" or "permission denied").
pub(crate) fn search_index(
    paths: &IndexPaths,
    query: &str,
    limit: i64,
) -> Result<Vec<SearchHit>, FileIndexError> {
    let text = normalise(query);
    if text.is_empty() {
        return Ok(Vec::new());
    }
    if !configuration(paths)?.enabled {
        return Ok(Vec::new());
    }
    if !paths.database_path.exists() {
        return Ok(Vec::new());
    }
    let tokens: Vec<&str> = text.split(' ').collect();
    let Ok(candidates) = fetch_candidates(paths, &tokens) else {
        return Ok(Vec::new());
    };
    let mut scored: Vec<(Key, Candidate)> = candidates
        .into_iter()
        .map(|candidate| (score(&candidate, &text, &tokens), candidate))
        .collect();
    scored.sort_by(|left, right| left.0.cmp(&right.0));
    let take = i64::max(1, i64::min(limit, 50));
    let take = usize::try_from(take).unwrap_or(1);
    let home = paths.home.to_string_lossy().into_owned();
    Ok(scored
        .into_iter()
        .take(take)
        .map(|(_, candidate)| SearchHit {
            kind: candidate.kind,
            title: candidate.name,
            subtitle: display_path(&candidate.parent, &home),
            path: candidate.path,
        })
        .collect())
}

/// `" ".join(str(query).casefold().split())` -- casefold, then collapse every run of
/// whitespace to one space and drop leading/trailing whitespace entirely.
fn normalise(query: &str) -> String {
    casefold(query)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn fetch_candidates(
    paths: &IndexPaths,
    tokens: &[&str],
) -> Result<Vec<Candidate>, rusqlite::Error> {
    let database = connect_readonly(paths)?;
    let clauses: Vec<String> = tokens
        .iter()
        .map(|_| "path_fold LIKE ? ESCAPE '\\'".to_string())
        .collect();
    let sql = format!(
        "SELECT path,name,name_fold,parent,path_fold,kind,modified_ns FROM files WHERE {} LIMIT ?",
        clauses.join(" AND ")
    );
    let mut statement = database.prepare(&sql)?;
    let mut parameters: Vec<Box<dyn rusqlite::ToSql>> = tokens
        .iter()
        .map(|token| Box::new(like_pattern(token)) as Box<dyn rusqlite::ToSql>)
        .collect();
    parameters.push(Box::new(500i64));
    let params: Vec<&dyn rusqlite::ToSql> =
        parameters.iter().map(std::convert::AsRef::as_ref).collect();
    let rows = statement.query_map(params.as_slice(), row_to_candidate)?;
    rows.collect::<Result<Vec<_>, _>>()
}

fn row_to_candidate(row: &rusqlite::Row<'_>) -> rusqlite::Result<Candidate> {
    Ok(Candidate {
        path: row.get(0)?,
        name: row.get(1)?,
        name_fold: row.get(2)?,
        parent: row.get(3)?,
        path_fold: row.get(4)?,
        kind: row.get(5)?,
        modified_ns: row.get(6)?,
    })
}

/// Escape a token for `LIKE ... ESCAPE '\'` and wrap it as a substring match: `\` first
/// (so the escapes added next are not themselves escaped again), then `%` and `_`.
fn like_pattern(token: &str) -> String {
    let escaped = token
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("%{escaped}%")
}

/// The ranking key [`search_index`] sorts ascending on: an exact `name_fold` match first,
/// then a prefix match, then a substring match, then "every token appears somewhere in the
/// name", then everything else -- each tier then broken by the folded name's length
/// (shorter first), then by modification time (newest first, hence the negation), and
/// finally by the folded path, which is unique per row and so fully orders any tie the first
/// three fields leave open.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Key {
    quality: u8,
    name_length: usize,
    reverse_modified_ns: i64,
    path_fold: String,
}

fn score(candidate: &Candidate, query: &str, tokens: &[&str]) -> Key {
    let name = &candidate.name_fold;
    let quality = if name == query {
        0
    } else if name.starts_with(query) {
        1
    } else if name.contains(query) {
        2
    } else if tokens.iter().all(|token| name.contains(token)) {
        3
    } else {
        4
    };
    Key {
        quality,
        name_length: name.chars().count(),
        reverse_modified_ns: -candidate.modified_ns,
        path_fold: candidate.path_fold.clone(),
    }
}

/// `"~" + path[len(home):]` when `path` is `home` or a descendant of it, else `path`
/// unchanged.
fn display_path(path: &str, home: &str) -> String {
    if path == home {
        return "~".to_string();
    }
    let prefix = format!("{home}/");
    path.strip_prefix(&prefix)
        .map_or_else(|| path.to_string(), |rest| format!("~/{rest}"))
}

#[cfg(test)]
mod tests {
    use super::{display_path, like_pattern, score, Candidate};

    fn candidate(name_fold: &str, path_fold: &str, modified_ns: i64) -> Candidate {
        Candidate {
            path: format!("/home/tester/{path_fold}"),
            name: name_fold.to_string(),
            name_fold: name_fold.to_string(),
            parent: "/home/tester".to_string(),
            path_fold: path_fold.to_string(),
            kind: "file".to_string(),
            modified_ns,
        }
    }

    /// A fixed corpus, mirroring the real one the byte-parity check ran against: sorting by
    /// [`score`] must put every row in exactly this order, tier by tier and then tie by
    /// tie, matching `search_index("foo")`'s ranking against the same six names.
    #[test]
    fn a_fixed_corpus_sorts_in_the_documented_order() {
        let query = "foo";
        let tokens = ["foo"];
        let mut rows = [
            candidate("foo", "a/foo", 100),                     // exact
            candidate("foo.txt", "b/foo.txt", 100),             // prefix, len 7
            candidate("food.pdf", "c/food.pdf", 100),           // prefix, len 8
            candidate("foofoofoo.txt", "d/foofoofoo.txt", 200), // prefix, len 13, newer
            candidate("foo_readme.md", "e/foo_readme.md", 100), // prefix, len 13, older
            candidate("myfoo.txt", "f/myfoo.txt", 100),         // substring
            candidate("unrelated.txt", "g/unrelated.txt", 100), // no match at all
        ];
        rows.sort_by_key(|row| score(row, query, &tokens));
        let order: Vec<&str> = rows.iter().map(|row| row.name_fold.as_str()).collect();
        assert_eq!(
            order,
            vec![
                "foo",
                "foo.txt",
                "food.pdf",
                // Tie at (prefix, len 13): newer modification wins.
                "foofoofoo.txt",
                "foo_readme.md",
                "myfoo.txt",
                "unrelated.txt",
            ]
        );
    }

    #[test]
    fn like_pattern_escapes_wildcards_and_the_escape_character() {
        assert_eq!(like_pattern("abc"), "%abc%");
        assert_eq!(like_pattern("50%_x\\y"), "%50\\%\\_x\\\\y%");
    }

    /// Mirrors the ranking tiers the Python's `match_score` documents in its own if/elif
    /// chain: exact, prefix, substring, all-tokens, everything else.
    #[test]
    fn quality_tiers_rank_in_the_documented_order() {
        let exact = score(&candidate("budget", "x/budget", 0), "budget", &["budget"]);
        let prefix = score(
            &candidate("budget2026", "x/budget2026", 0),
            "budget",
            &["budget"],
        );
        let substring = score(
            &candidate("myBudget2026".to_lowercase().as_str(), "x", 0),
            "budget",
            &["budget"],
        );
        // "kinetik launch" is not a *substring* of this name (the tokens appear out of
        // order), so this must fall to the all-tokens tier rather than the substring one.
        let all_tokens = score(
            &candidate("launch notes for kinetik", "x", 0),
            "kinetik launch",
            &["kinetik", "launch"],
        );
        let none = score(&candidate("unrelated", "x", 0), "budget", &["budget"]);
        assert!(exact.quality < prefix.quality);
        assert!(prefix.quality < substring.quality);
        assert!(substring.quality < all_tokens.quality);
        assert!(all_tokens.quality < none.quality);
    }

    #[test]
    fn ties_break_by_length_then_recency_then_path() {
        let short = score(&candidate("ab", "z/ab", 100), "a", &["a"]);
        let long = score(&candidate("abcdef", "a/abcdef", 100), "a", &["a"]);
        assert!(short < long, "shorter folded name should sort first");

        let newer = score(&candidate("ab", "z/ab", 200), "a", &["a"]);
        let older = score(&candidate("ab", "z/ab", 100), "a", &["a"]);
        assert!(newer < older, "more recent modification should sort first");
    }

    #[test]
    fn display_path_replaces_the_home_prefix_with_a_tilde() {
        assert_eq!(display_path("/home/tester", "/home/tester"), "~");
        assert_eq!(
            display_path("/home/tester/Documents", "/home/tester"),
            "~/Documents"
        );
        assert_eq!(display_path("/etc", "/home/tester"), "/etc");
        // Not a path separator boundary -- must not be treated as a descendant.
        assert_eq!(
            display_path("/home/tester-other", "/home/tester"),
            "/home/tester-other"
        );
    }
}
