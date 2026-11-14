//! One compact repository row.
use crate::response::result::remove_null_fields;
use octocode_contract_types::GhSearchRepoQuery;
use serde_json::{Value, json};

/// A repository timestamp as its UTC day; text too short to name one
/// passes through.
pub(super) fn date(value: Option<String>) -> Option<String> {
    value.map(|v| crate::tools::gh_shared::dates::utc_day(&v).unwrap_or(v))
}

/// Lowercased query topics and keyword words: a row lists these topics first.
pub(super) fn wanted_topics(query: &GhSearchRepoQuery) -> Vec<String> {
    super::access::filters(query)
        .into_iter()
        .flat_map(|filters| &filters.topics)
        .map(|topic| topic.to_lowercase())
        .chain(super::access::keywords(query).iter().flat_map(|keyword| {
            let keyword = keyword.to_lowercase();
            let hyphenated = keyword.split_whitespace().collect::<Vec<_>>().join("-");
            keyword
                .split_whitespace()
                .map(str::to_owned)
                .chain(std::iter::once(hyphenated))
                .collect::<Vec<_>>()
        }))
        .collect()
}

/// A repository only some accounts can see (`private`, `internal`): its
/// visibility, so a row the token's own access listed is told from public
/// evidence. `None` for a public one.
fn not_public(visibility: &str, private: bool) -> Option<&str> {
    match visibility {
        "" if private => Some("private"),
        "" | "public" => None,
        other => Some(other),
    }
}

/// One repository row, whole: `owner` and `repo`, `defaultBranch`, stars,
/// language, license, dates, the description, forks, `visibility` when not
/// public, its topics (query matches first), its homepage, open issue count
/// and size. `updatedAt` is the whole timestamp: a search sorted by it
/// orders rows within a day. What a default
/// row keeps of it, and what a `fields` pick names back, is the caller's
/// ([`super::LISTED_DETAIL`], [`super::EXACT_DETAIL`]).
pub(super) fn repository_row(
    item: octocode_github::RepositorySearchItem,
    wanted: &[String],
) -> Value {
    // Every topic GitHub returns: a topic is a tag the owner chose, even
    // when it spells the repository's name or language.
    let mut topics = item.topics;
    // Stable: matching topics keep GitHub's order, then the rest.
    topics.sort_by_key(|topic| !wanted.contains(&topic.to_lowercase()));
    let (owner, repo) = item
        .full_name
        .split_once('/')
        .map_or((None, item.name.clone()), |(owner, repo)| {
            (Some(owner.to_owned()), repo.to_owned())
        });
    let default_branch = Some(item.default_branch).filter(|branch| !branch.is_empty());
    let mut row = json!({
        "owner": owner,
        "repo": repo,
        "defaultBranch": default_branch,
        "stars": item.stargazers_count,
        "language": item.language,
        "license": item.license.and_then(|license| license.spdx_id),
        "pushedAt": date(item.pushed_at),
        "createdAt": date(item.created_at),
        "description": item.description,
    });
    if item.archived {
        row["archived"] = json!(true);
    }
    // A fork in an owner listing is not the owner's own source.
    if item.fork {
        row["fork"] = json!(true);
    }
    if let Some(visibility) = not_public(&item.visibility, item.private.unwrap_or(false)) {
        row["visibility"] = json!(visibility);
    }
    if !topics.is_empty() {
        row["topics"] = json!(topics);
    }
    row["forks"] = json!(item.forks_count);
    row["updatedAt"] = json!(
        item.updated_at
            .map(|at| crate::tools::gh_shared::dates::utc_date(Some(&at)))
    );
    row["homepage"] = json!(item.homepage.filter(|page| !page.trim().is_empty()));
    row["openIssuesCount"] = json!(item.open_issues_count);
    row["sizeKiB"] = json!(item.size);
    remove_null_fields(&mut row);
    row
}
