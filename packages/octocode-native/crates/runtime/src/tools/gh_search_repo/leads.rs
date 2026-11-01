//! Where a repository page leads: the top repository, archived rows, a broadened empty search.
use super::{FILTERS, GhSearchRepoQuery};
use crate::id::ToolId;
use crate::response::result::{Continuation, ToolData, query_row};
use crate::tools::leads::broaden::{Broaden, broaden, set_broadened};
use serde_json::{Value, json};

/// Where the top repository leads: its default-branch root listing. Repo
/// discovery words are not code keywords, so no code-search lead follows.
pub(super) fn top_leads(
    top: Option<&octocode_github::RepositorySearchItem>,
) -> Vec<(&'static str, Value)> {
    let Some((owner, repo)) = top.and_then(|item| item.full_name.split_once('/')) else {
        return Vec::new();
    };
    vec![(
        "viewRepo",
        crate::tools::gh_shared::RepoPath::root(owner, repo)
            .lead(ToolId::GhStructure)
            .build(),
    )]
}

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
/// then one per dropped keyword; a lone keyword of an `owner`+`repo` search
/// drops to that repository's own row.
pub(super) fn broaden_search(query: &GhSearchRepoQuery) -> Option<Value> {
    broaden(
        ToolId::GhSearchRepo,
        &copied_search(query),
        &Broaden {
            strip: &[],
            groups: FILTERS,
            split_lone_keyword: false,
        },
        crate::response::result::rows_per_call(ToolId::GhSearchRepo),
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
    // No rows is empty whether or not GitHub finished the search, as
    // ghSearchCode reports it: an incomplete page keeps its retry (the CLI
    // exits 1, nothing printed), and only a complete one is worth broadening.
    if repositories_empty && !provider_capped {
        output.status = Some("empty");
        if !provider_incomplete && let Some(broader) = broaden_search(query) {
            set_broadened(&mut output.data, &copied_search(query), broader);
        }
    }
    crate::response::result::finish_calls(&mut output.data);
    output
}
