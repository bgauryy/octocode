use octocode_contract_types::{
    CaseMode, DefaultExcludes, LocalSearchQuery, LocalSearchQueryContent, LocalSearchQueryFiles,
    LocalSearchQueryFilesView, LocalSearchQueryMatchOnly, LocalSearchSort,
    LocalSearchSortAnyOrderBy, RegexEngine, TextMatchTerms,
};
use octocode_engine::num::saturating_u32;
use octocode_engine::types::{SearchContext, SearchSort};
use octocode_runtime_base::num::u32_of_signed;
use serde_json::json;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum View {
    Content,
    Files,
    FilesWithout,
    CountLines,
    CountMatches,
    MatchOnly,
}
pub(crate) use octocode_contract_types::LocalSearchQueryMatchOnlyUnique as Unique;
pub(crate) use octocode_contract_types::LocalSearchQueryMultiline as Multiline;

/// Binds `$field` from whichever view variant `$q` is.
macro_rules! common {
    ($q:expr,$field:ident,$value:expr) => {
        octocode_tools_shared::variant_field::variant_field!($q, newtype [
            LocalSearchQuery::Content => LocalSearchQueryContent,
            LocalSearchQuery::Files => LocalSearchQueryFiles,
            LocalSearchQuery::MatchOnly => LocalSearchQueryMatchOnly,
        ] $field => $value)
    };
}
pub(crate) fn path(q: &LocalSearchQuery) -> &str {
    common!(q, path, path.as_str())
}
pub(crate) fn test_code(q: &LocalSearchQuery) -> Option<octocode_contract_types::TestCode> {
    common!(q, test_code, *test_code)
}
pub(crate) fn case_mode(q: &LocalSearchQuery) -> CaseMode {
    common!(q, case_mode, *case_mode)
}
pub(crate) fn regex_mode(q: &LocalSearchQuery) -> RegexEngine {
    common!(q, regex_engine, *regex_engine)
}
pub(crate) fn patterns(q: &LocalSearchQuery) -> &TextMatchTerms {
    common!(q, match_string, match_string)
}
pub(crate) fn include(q: &LocalSearchQuery) -> &[String] {
    common!(q, include, include.as_slice())
}
pub(crate) fn exclude(q: &LocalSearchQuery) -> &[String] {
    common!(q, exclude, exclude.as_slice())
}
pub(crate) fn include_hidden(q: &LocalSearchQuery) -> Option<bool> {
    common!(q, include_hidden, *include_hidden)
}
pub(crate) fn include_ignored(q: &LocalSearchQuery) -> Option<bool> {
    common!(q, include_ignored, *include_ignored)
}
pub(crate) fn invert_match(q: &LocalSearchQuery) -> Option<bool> {
    common!(q, invert_match, *invert_match)
}
pub(crate) fn whole_word(q: &LocalSearchQuery) -> Option<bool> {
    common!(q, whole_word, Some(*whole_word))
}
pub(crate) fn default_excludes(q: &LocalSearchQuery) -> &Option<DefaultExcludes> {
    common!(q, default_excludes, default_excludes)
}
pub(crate) fn page(q: &LocalSearchQuery) -> u32 {
    common!(q, page, saturating_u32(page.get()))
}
/// Rows per page (`limit`): hit rows in content and matchOnly views, files
/// in the others; `None` cuts pages by the budget alone.
pub(crate) fn limit(q: &LocalSearchQuery) -> Option<usize> {
    common!(
        q,
        limit,
        limit.map(|limit| octocode_engine::num::saturating_usize(limit.get()))
    )
}
pub(crate) fn max_depth(q: &LocalSearchQuery) -> Option<u32> {
    common!(
        q,
        max_depth,
        max_depth.map(|depth| saturating_u32(depth.get()).saturating_sub(1))
    )
}
/// Candidates, in walk order, the paused windows of a PCRE2 search searched
/// (see [`super::window`]).
pub(crate) fn scan_offset(q: &LocalSearchQuery) -> Option<u32> {
    common!(
        q,
        scan_offset,
        scan_offset.map(|offset| saturating_u32(offset.get()))
    )
}
/// Candidates, in walk order, before the first file a streamed page has not
/// shown whole (see [`super::streamed`](super::executor::streamed)).
pub(crate) fn walk_offset(q: &LocalSearchQuery) -> Option<u32> {
    common!(
        q,
        walk_offset,
        walk_offset.map(|offset| saturating_u32(offset.get()))
    )
}
/// Rows of that file the earlier pages showed.
pub(crate) fn row_offset(q: &LocalSearchQuery) -> Option<u32> {
    common!(
        q,
        row_offset,
        row_offset.map(|offset| saturating_u32(offset.get()))
    )
}
pub(crate) fn snapshot(q: &LocalSearchQuery) -> Option<&str> {
    common!(q, snapshot, snapshot.as_deref().map(String::as_str))
}
pub(crate) fn file_type(q: &LocalSearchQuery) -> Option<&str> {
    common!(q, file_type, file_type.as_deref().map(String::as_str))
}
pub(crate) fn file_has(q: &LocalSearchQuery) -> Vec<String> {
    common!(
        q,
        file_has,
        file_has.iter().map(ToString::to_string).collect()
    )
}
pub(crate) fn file_lacks(q: &LocalSearchQuery) -> Vec<String> {
    common!(
        q,
        file_lacks,
        file_lacks.iter().map(ToString::to_string).collect()
    )
}
pub(crate) fn has_classification(q: &LocalSearchQuery) -> bool {
    match q {
        LocalSearchQuery::Content(LocalSearchQueryContent { classification, .. }) => {
            classification.is_some()
        }
        LocalSearchQuery::MatchOnly(LocalSearchQueryMatchOnly { classification, .. }) => {
            classification.is_some()
        }
        _ => false,
    }
}
pub(crate) fn selection(q: &LocalSearchQuery) -> Option<&str> {
    match q {
        LocalSearchQuery::Content(LocalSearchQueryContent { selection, .. }) => {
            selection.as_deref().map(String::as_str)
        }
        LocalSearchQuery::MatchOnly(LocalSearchQueryMatchOnly { selection, .. }) => {
            selection.as_deref().map(String::as_str)
        }
        _ => None,
    }
}
pub(crate) fn context(q: &LocalSearchQuery) -> SearchContext {
    match q {
        LocalSearchQuery::Content(LocalSearchQueryContent {
            context_lines: Some(context),
            ..
        }) => SearchContext {
            before: u32_of_signed(context.before),
            after: u32_of_signed(context.after),
        },
        _ => SearchContext::symmetric(0),
    }
}
pub(crate) fn view(q: &LocalSearchQuery) -> View {
    match q {
        LocalSearchQuery::Content(LocalSearchQueryContent { .. }) => View::Content,
        LocalSearchQuery::MatchOnly(LocalSearchQueryMatchOnly { .. }) => View::MatchOnly,
        LocalSearchQuery::Files(LocalSearchQueryFiles { view, .. }) => match view {
            LocalSearchQueryFilesView::Files => View::Files,
            LocalSearchQueryFilesView::FilesWithout => View::FilesWithout,
            LocalSearchQueryFilesView::CountLines => View::CountLines,
            LocalSearchQueryFilesView::CountMatches => View::CountMatches,
        },
    }
}
/// The value a matchOnly row leaves out of its spans: the query's one
/// `matchString` term, so a span equal to it states only where it is and a
/// reader takes the text from the request. `None` for another view, several
/// terms, or a term the output gate would not serve as written (a span
/// showing it would be redacted, never the term).
pub(crate) fn derived_value(q: &LocalSearchQuery) -> Option<&str> {
    let [term] = patterns(q).0.as_slice() else {
        return None;
    };
    (view(q) == View::MatchOnly
        && octocode_runtime_base::security::ContentSecurity::new()
            .sanitize_leaf(term.as_str())
            .is_none())
    .then_some(term.as_str())
}
pub(crate) fn multiline(q: &LocalSearchQuery) -> Multiline {
    common!(q, multiline, *multiline)
}
/// The `keyPath` scope: hits kept to the value at a key path of JSON and
/// TOML documents ([`super::key_scope`]).
pub(crate) fn key_path(q: &LocalSearchQuery) -> Option<&str> {
    common!(q, key_path, key_path.as_deref().map(String::as_str))
}
pub(crate) fn unique(q: &LocalSearchQuery) -> Unique {
    match q {
        LocalSearchQuery::MatchOnly(LocalSearchQueryMatchOnly { unique, .. }) => *unique,
        _ => Unique::Off,
    }
}
/// The query's `sort`, shared by every view.
fn sort_of(q: &LocalSearchQuery) -> &LocalSearchSort {
    common!(q, sort, sort)
}
pub(crate) fn sort(q: &LocalSearchQuery) -> SearchSort {
    match sort_of(q) {
        LocalSearchSort::AscOnly(_) => SearchSort::Traversal,
        LocalSearchSort::AnyOrder(any) => match any.by {
            LocalSearchSortAnyOrderBy::MatchCount => SearchSort::MatchCount,
            LocalSearchSortAnyOrderBy::Path => SearchSort::Path,
            LocalSearchSortAnyOrderBy::ModifiedAt => SearchSort::Modified,
            LocalSearchSortAnyOrderBy::AccessedAt => SearchSort::Accessed,
            LocalSearchSortAnyOrderBy::CreatedAt => SearchSort::Created,
        },
    }
}
pub(crate) fn reverse(q: &LocalSearchQuery) -> Option<bool> {
    let descending = match sort_of(q) {
        LocalSearchSort::AnyOrder(any) => any.order == octocode_contract_types::SortOrder::Desc,
        LocalSearchSort::AscOnly(_) => false,
    };
    Some(descending != (sort(q) == SearchSort::MatchCount))
}
pub(crate) fn pattern(q: &LocalSearchQuery) -> &str {
    patterns(q).0.first().map_or("", |value| value.as_str())
}
/// Each `matchString` entry of a list query as the alternative
/// [`normalized`] joins it as, in order; `None` for a single entry.
pub(crate) fn term_patterns(q: &LocalSearchQuery) -> Option<Vec<String>> {
    (patterns(q).0.len() > 1).then(|| {
        patterns(q)
            .0
            .iter()
            .map(|entry| alternation_of(q, std::iter::once(entry.as_str())))
            .collect()
    })
}

/// Whether `term`, a `matchString` entry of `q`, matches case-sensitively
/// by `q`'s case rule: `smart` only when the term holds an uppercase letter.
pub(crate) fn case_sensitive(q: &LocalSearchQuery, term: &str) -> bool {
    match case_mode(q) {
        CaseMode::Smart => term.chars().any(char::is_uppercase),
        CaseMode::Sensitive => true,
        CaseMode::Insensitive => false,
    }
}

/// The alternation of `entries`, each keeping the query's case rule for
/// itself and escaped unless the query is a regex.
fn alternation_of<'a>(q: &LocalSearchQuery, entries: impl Iterator<Item = &'a str>) -> String {
    octocode_tools_shared::match_list::alternation(
        entries.map(|entry| (entry, case_sensitive(q, entry))),
        regex_mode(q) != RegexEngine::Literal,
    )
}

pub(crate) fn normalized(
    q: &LocalSearchQuery,
) -> Result<
    std::borrow::Cow<'_, LocalSearchQuery>,
    octocode_runtime_base::response::result::ToolError,
> {
    if patterns(q).0.len() == 1 {
        return Ok(std::borrow::Cow::Borrowed(q));
    }
    let alternation = alternation_of(q, patterns(q).0.iter().map(|entry| entry.as_str()));
    let dialect = if regex_mode(q) == RegexEngine::Pcre2 {
        RegexEngine::Pcre2
    } else {
        RegexEngine::Rust
    };
    let mut query = serde_json::to_value(q).map_err(|error| {
        octocode_runtime_base::response::result::ToolError::new(
            octocode_contract_types::ErrorCode::InvalidInput,
            error.to_string(),
        )
    })?;
    query["matchString"] = json!([alternation]);
    query["regexEngine"] = json!(dialect);
    query["caseMode"] = json!(CaseMode::Sensitive);
    serde_json::from_value(query)
        .map(std::borrow::Cow::Owned)
        .map_err(|error| {
            octocode_runtime_base::response::result::ToolError::new(
                octocode_contract_types::ErrorCode::InvalidInput,
                error.to_string(),
            )
        })
}
/// The terms of every fresh call in `calls` (a page's `next`, an error's
/// calls) built from `joined`, the [`normalized`] form of `written`, as the
/// caller wrote them: a list runs as one alternation, which is how the scan
/// matches it, never what a follow-up sends. A row holding the joined terms
/// gets the written terms, and the written engine and case rule where it
/// kept the joined ones (a default one left out, as a built call leaves it):
/// an engine or case rule the call sets itself, as the case hint's search
/// with case ignored does, is the call's. Any other row, and saved work (its
/// cursor seals it), stays as built.
pub(crate) fn as_written(
    calls: &mut serde_json::Value,
    written: &LocalSearchQuery,
    joined: &LocalSearchQuery,
) {
    use serde_json::Value;
    const TERMS: [&str; 3] = ["matchString", "regexEngine", "caseMode"];
    fn each_fresh_row(value: &mut Value, restate: &dyn Fn(&mut Value)) {
        match value {
            Value::Object(fields) => {
                if fields.get("kind").and_then(Value::as_str) == Some("related") {
                    let tool = fields
                        .get("tool")
                        .and_then(Value::as_str)
                        .and_then(octocode_runtime_base::id::ToolId::from_name);
                    let rows = fields
                        .get_mut("input")
                        .and_then(|input| input.get_mut("queries"))
                        .and_then(Value::as_array_mut);
                    for row in rows.into_iter().flatten() {
                        restate(row);
                        // A restated field at its default is its omission,
                        // as every built call states it.
                        if let Some(tool) = tool {
                            octocode_runtime_base::response::result::canonical_row(tool, row);
                        }
                    }
                    return;
                }
                fields
                    .values_mut()
                    .for_each(|field| each_fresh_row(field, restate));
            }
            Value::Array(items) => items
                .iter_mut()
                .for_each(|item| each_fresh_row(item, restate)),
            _ => {}
        }
    }
    if std::ptr::eq(written, joined) {
        return;
    }
    let (Ok(written), Ok(joined)) = (serde_json::to_value(written), serde_json::to_value(joined))
    else {
        return;
    };
    let restate = |row: &mut Value| {
        if row.get(TERMS[0]) != joined.get(TERMS[0]) {
            return;
        }
        for field in TERMS {
            if row.get(field) != joined.get(field) {
                continue;
            }
            match written.get(field).filter(|value| !value.is_null()) {
                Some(value) => row[field] = value.clone(),
                None => {
                    if let Some(fields) = row.as_object_mut() {
                        fields.remove(field);
                    }
                }
            }
        }
    };
    each_fresh_row(calls, &restate);
}

pub(super) fn uses_context(view: View) -> bool {
    view == View::Content
}
pub(super) fn shows_span_columns(q: &LocalSearchQuery) -> bool {
    view(q) == View::MatchOnly
}

/// `query` with `changes` applied (a `null` drops that field) and decoded
/// again as a search query: the edit of fields chosen at run time.
pub(crate) fn edited(
    query: &LocalSearchQuery,
    changes: &serde_json::Value,
) -> Result<LocalSearchQuery, octocode_runtime_base::response::result::ToolError> {
    octocode_runtime_base::response::result::edited_query(query, changes).map_err(|error| {
        octocode_runtime_base::response::result::ToolError::new(
            octocode_contract_types::ErrorCode::ExecutionFailed,
            format!("the continuation is no valid localSearch query: {error}"),
        )
    })
}
