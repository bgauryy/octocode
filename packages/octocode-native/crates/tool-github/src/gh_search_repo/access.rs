use octocode_contract_types::{GhSearchRepoQuery, GhSearchRepoQuerySearchFilters, GitHubArchive};

pub(super) fn owner(q: &GhSearchRepoQuery) -> Option<&str> {
    match q {
        GhSearchRepoQuery::Search(q) => q.owner.as_ref().map(|x| x.as_str()),
        GhSearchRepoQuery::List(q) => Some(&q.owner),
        GhSearchRepoQuery::Get(q) => Some(&q.owner),
    }
}
pub(super) fn repo(q: &GhSearchRepoQuery) -> Option<&str> {
    match q {
        GhSearchRepoQuery::Search(q) => q.repo.as_ref().map(|x| x.as_str()),
        GhSearchRepoQuery::Get(q) => Some(&q.repo),
        GhSearchRepoQuery::List(_) => None,
    }
}
pub(super) fn page(q: &GhSearchRepoQuery) -> usize {
    match q {
        GhSearchRepoQuery::Search(q) => octocode_engine::num::saturating_usize(q.page.get()),
        GhSearchRepoQuery::List(q) => octocode_engine::num::saturating_usize(q.page.get()),
        GhSearchRepoQuery::Get(_) => 1,
    }
}
/// Rows per page: the query's `limit`, its contract default when omitted. A
/// `get` reads one repository and pages nothing.
pub(super) fn limit(q: &GhSearchRepoQuery) -> usize {
    match q {
        GhSearchRepoQuery::Search(q) => octocode_engine::num::saturating_usize(q.limit.get()),
        GhSearchRepoQuery::List(q) => octocode_engine::num::saturating_usize(q.limit.get()),
        GhSearchRepoQuery::Get(_) => {
            octocode_runtime_base::contracts::query_limits::gh_search_repo::LIMIT_DEFAULT
        }
    }
}
pub(super) fn fields(q: &GhSearchRepoQuery) -> Vec<String> {
    match q {
        GhSearchRepoQuery::Search(q) => q.fields.iter().map(ToString::to_string).collect(),
        GhSearchRepoQuery::List(q) => q.fields.iter().map(ToString::to_string).collect(),
        GhSearchRepoQuery::Get(q) => q.fields.iter().map(ToString::to_string).collect(),
    }
}
pub(super) fn filters(q: &GhSearchRepoQuery) -> Option<&GhSearchRepoQuerySearchFilters> {
    match q {
        GhSearchRepoQuery::Search(q) => q.filters.as_ref(),
        _ => None,
    }
}
pub(super) fn archive(q: &GhSearchRepoQuery) -> GitHubArchive {
    match q {
        GhSearchRepoQuery::Search(q) => q.filters.as_ref().and_then(|f| f.archive),
        GhSearchRepoQuery::List(q) => q.filters.as_ref().and_then(|f| f.archive),
        GhSearchRepoQuery::Get(_) => Some(GitHubArchive::Include),
    }
    .unwrap_or(GitHubArchive::Exclude)
}
pub(super) fn keywords(q: &GhSearchRepoQuery) -> Vec<String> {
    match q {
        GhSearchRepoQuery::Search(q) => q.keywords.iter().map(ToString::to_string).collect(),
        _ => Vec::new(),
    }
}
pub(super) fn order(q: &GhSearchRepoQuery) -> &str {
    match q {
        GhSearchRepoQuery::List(q) => match q.sort.order {
            octocode_contract_types::SortOrder::Asc => "asc",
            octocode_contract_types::SortOrder::Desc => "desc",
        },
        _ => "desc",
    }
}
/// The row field the rows are ordered by, when a default row leaves it to
/// `fields`: an owner listing's push time, or a search's forks or update
/// time (`stars` is on every row; a help-wanted count is on none).
pub(super) fn sorted_detail(q: &GhSearchRepoQuery) -> Option<&'static str> {
    let q = match q {
        GhSearchRepoQuery::List(_) => return Some("pushedAt"),
        GhSearchRepoQuery::Search(q) => q,
        GhSearchRepoQuery::Get(_) => return None,
    };
    match serde_json::to_value(&q.sort).ok()?["by"].as_str()? {
        "forks" => Some("forks"),
        "updatedAt" => Some("updatedAt"),
        _ => None,
    }
}
pub(super) fn search_sort(q: &GhSearchRepoQuery) -> Option<String> {
    let GhSearchRepoQuery::Search(q) = q else {
        return None;
    };
    let value = serde_json::to_value(&q.sort).ok()?;
    let by = match value["by"].as_str()? {
        "relevance" => return None,
        "stars" => "stars",
        "forks" => "forks",
        "helpWantedIssueCount" => "help-wanted-issues",
        "updatedAt" => "updated",
        _ => return None,
    };
    Some(format!("{by}:{}", value["order"].as_str()?))
}
