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
        GhSearchRepoQuery::Search(q) => crate::num::usize_of(q.page),
        GhSearchRepoQuery::List(q) => crate::num::usize_of(q.page),
        GhSearchRepoQuery::Get(_) => 1,
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
        GhSearchRepoQuery::List(q) => q.filters.as_ref().map(|f| f.archive),
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
        GhSearchRepoQuery::List(q) => match q
            .sort
            .as_ref()
            .map_or(octocode_contract_types::SortOrder::Desc, |s| s.order)
        {
            octocode_contract_types::SortOrder::Asc => "asc",
            octocode_contract_types::SortOrder::Desc => "desc",
        },
        _ => "desc",
    }
}
pub(super) fn search_sort(q: &GhSearchRepoQuery) -> Option<String> {
    let GhSearchRepoQuery::Search(q) = q else {
        return None;
    };
    let value = serde_json::to_value(&q.sort).ok()?;
    let by = match value["by"].as_str()? {
        "bestMatch" => return None,
        "stars" => "stars",
        "forkCount" => "forks",
        "helpWantedIssueCount" => "help-wanted-issues",
        "updatedAt" => "updated",
        _ => return None,
    };
    Some(format!("{by}:{}", value["order"].as_str()?))
}
