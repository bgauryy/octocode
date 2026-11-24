//! Where a repository page leads: the top repository, archived rows, a broadened empty search.
use super::{FILTERS, GhSearchRepoQuery};
use crate::qualifier_keywords::{Qualifier, qualifier};
use octocode_runtime_base::id::ToolId;
use octocode_runtime_base::response::result::{Continuation, ToolData, query_row};
use octocode_tools_shared::leads::broaden::{Broaden, broaden, set_broadened, set_no_broader};
use serde_json::{Value, json};

/// The search a lead copies, from its first page: defaults and the brief
/// stay out of the copied call.
pub(super) fn copied_search(query: &GhSearchRepoQuery) -> Value {
    query_row(query, &["page"])
}

/// The same search with archived repositories included, from its first page.
pub(super) fn include_archived(query: &GhSearchRepoQuery) -> Value {
    let mut row = copied_search(query);
    row["filters"]["archive"] = json!("include");
    Continuation::related(ToolId::GhSearchRepo, row).build()
}

/// An empty search, broadened ([`broaden`]): one variant per set filter,
/// then one per dropped keyword; a lone keyword of an `owner` search drops
/// to that owner's repositories (of an `owner`+`repo` search, to that
/// repository's own row). A variant without any keyword and without an
/// owner keeps nothing of what was asked (every Rust repository), so none
/// is offered.
pub(super) fn broaden_search(query: &GhSearchRepoQuery) -> Option<Value> {
    let mut lead = broaden(
        ToolId::GhSearchRepo,
        &copied_search(query),
        &Broaden {
            strip: &[],
            groups: FILTERS,
            split_lone_keyword: false,
        },
        octocode_runtime_base::response::result::rows_per_call(ToolId::GhSearchRepo),
    )?;
    let variants = lead.pointer_mut("/input/queries")?.as_array_mut()?;
    variants.retain(|variant| variant.get("keywords").is_some() || variant.get("owner").is_some());
    (!variants.is_empty()).then_some(lead)
}

/// When GitHub's first repositories were created (October 2007): no
/// repository is older, so a creation-date split weighs its lower half from
/// here (that half itself stays open below).
const FIRST_REPOSITORY_SECS: i64 = 1_191_196_800;

/// A search past GitHub's 1,000-result window, split in two by when the
/// repositories were created: the same search before and after the middle
/// of its creation window (its own `createdAt` bounds, else GitHub's first
/// repository and `now_secs`). The halves are disjoint, at whole seconds,
/// and together hold every match; a half still past the window offers its
/// own split, so the walk reaches every match. `None` for a listing, or a
/// window of one second.
pub(super) fn split_by_created(query: &GhSearchRepoQuery, now_secs: i64) -> Option<Value> {
    let GhSearchRepoQuery::Search(_) = query else {
        return None;
    };
    let created = super::access::filters(query).and_then(|filters| filters.created_at.as_ref());
    let secs = |bound: Option<&str>| {
        bound
            .and_then(octocode_runtime_base::civil_date::parse_rfc3339)
            .map(|(secs, _)| secs)
    };
    let low = secs(created.and_then(|range| range.from.as_deref().map(String::as_str)))
        .unwrap_or(FIRST_REPOSITORY_SECS);
    let high =
        secs(created.and_then(|range| range.to.as_deref().map(String::as_str))).unwrap_or(now_secs);
    if high.saturating_sub(low) < 1 {
        return None;
    }
    let middle = low + (high - low) / 2;
    let half = |bound: &str, secs: i64| {
        let mut row = copied_search(query);
        row["filters"]["createdAt"][bound] =
            json!(octocode_runtime_base::civil_date::iso8601_secs(secs));
        row
    };
    Some(
        Continuation::related_input(
            ToolId::GhSearchRepo,
            json!({"queries": [half("to", middle), half("from", middle + 1)]}),
        )
        // The only call that reaches the matches past the window: the row
        // needs it.
        .required()
        .build(),
    )
}

pub(super) fn repository_output(
    value: Value,
    query: &GhSearchRepoQuery,
    repositories_empty: bool,
    provider_incomplete: bool,
    provider_capped: bool,
) -> ToolData {
    let mut output = ToolData::from(value);
    // The matches past the window are reached by splitting the search: the
    // lead rides the first page, where the cap is first stated. The cap
    // warning states the count; the required call is the route, unexplained.
    if provider_capped
        && super::access::page(query) == 1
        && let Some(split) = split_by_created(query, octocode_runtime_base::civil_date::unix_secs())
    {
        output.data["calls"]["splitByCreated"] = split;
    }
    // No rows is empty whether or not GitHub finished the search, as
    // ghSearchCode reports it: an incomplete page keeps its retry (the CLI
    // exits 1, nothing printed), and only a complete one is worth broadening.
    if repositories_empty && !provider_capped {
        output.status = Some("empty");
        if !provider_incomplete {
            match broaden_search(query) {
                Some(broader) => set_broadened(&mut output.data, &copied_search(query), broader),
                // A lone phrase can lose words; a lone word has nothing to
                // drop, and the empty status is the answer.
                None if lone_phrase(query) => set_no_broader(&mut output.data),
                None => {}
            }
        }
    }
    octocode_runtime_base::response::result::finish_calls(&mut output.data);
    output
}

/// The one keyword of a search, when it holds several words (one exact
/// phrase): the only empty search with nothing to broaden whose next query
/// changes the words.
fn lone_phrase(query: &GhSearchRepoQuery) -> bool {
    matches!(
        super::access::keywords(query).as_slice(),
        [lone] if lone.split_whitespace().nth(1).is_some()
    )
}

/// The repository-search qualifiers a keyword can spell, with the field that
/// carries each: a keyword matches text, so `language:rust` or
/// `stars:>1000` finds nothing the filter would.
const QUALIFIER_FIELDS: &[Qualifier] = &[
    qualifier!(["language", "lang"] => "filters.language"),
    qualifier!(["stars"] => range "filters.stars"),
    qualifier!(["forks"] => range "filters.forks"),
    qualifier!(["size"] => range "filters.sizeKiB"),
    qualifier!(["good-first-issues"] => range "filters.goodFirstIssueCount"),
    qualifier!(["help-wanted-issues"] => range "filters.helpWantedIssueCount"),
    qualifier!(["topic"] => "filters.topics"),
    qualifier!(["license"] => "filters.license"),
    qualifier!(["created"] => "filters.createdAt" ("`{from, to}` date-times")),
    qualifier!(["pushed"] => "filters.pushedAt" ("`{from, to}` date-times")),
    qualifier!(["archived"] => "filters.archive" ("`exclude`, `include` or `only`")),
    qualifier!(["is", "visibility"] => "filters.visibility" ("`public`, `private` or `internal`")),
    qualifier!(["in"] => "matchIn" ("`name`, `description`, `readme` or `topics`")),
    qualifier!(["user", "org"] => "owner"),
];

/// A hint per keyword spelled as a repository-search qualifier that names the
/// field carrying it.
fn qualifier_keyword_hints(keywords: &[&str]) -> Vec<String> {
    crate::qualifier_keywords::hints(keywords, QUALIFIER_FIELDS)
}

/// A keyword spelled as a repository-search qualifier (`language:rust`)
/// matches text, so the search returns repositories that merely mention it,
/// with no signal: the call is refused, naming the field that carries it.
pub(super) fn reject_qualifier_keywords<S: AsRef<str>>(
    keywords: &[S],
) -> Result<(), octocode_github::ProviderError> {
    let keywords: Vec<&str> = keywords.iter().map(AsRef::as_ref).collect();
    let hints = qualifier_keyword_hints(&keywords);
    if hints.is_empty() {
        return Ok(());
    }
    Err(octocode_github::ProviderError::new(
        octocode_github::ProviderErrorKind::Validation,
        hints.join(" "),
    ))
}
