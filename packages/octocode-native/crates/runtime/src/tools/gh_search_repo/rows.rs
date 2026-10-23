//! One compact repository row.
use super::GhSearchRepoQuery;
use crate::response::result::remove_null_fields;
use serde_json::{Value, json};

/// A repository timestamp as its UTC day; text too short to name one
/// passes through.
pub(super) fn date(value: Option<String>) -> Option<String> {
    value.map(|v| crate::tools::gh_shared::dates::utc_day(&v).unwrap_or(v))
}

/// Lowercased query topics and keyword words: a row lists these topics first.
pub(super) fn wanted_topics(query: &GhSearchRepoQuery) -> Vec<String> {
    query
        .topics
        .iter()
        .map(|topic| topic.to_lowercase())
        .chain(query.keywords.iter().flat_map(|keyword| {
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

/// One compact repository row: `owner`, `repo` and `defaultBranch` (the
/// next hop's coordinates; a page whose rows agree states the branch once in
/// `shared`), the decision facts by default (stars, language, last push,
/// `archived`/`fork` when set) and the whole description. The license, the
/// creation date and every topic (query matches first) are `details`
/// extras; the fork count and the metadata-update date are `stats` extras
/// (the contract's extra paths decide what a response keeps).
pub(super) fn repository_row(
    item: octocode_github::RepositorySearchItem,
    wanted: &[String],
) -> Value {
    let mut topics = item.topics;
    // A topic equal to the repository's own name repeats `repo`; one equal
    // to its language repeats `language`.
    topics.retain(|topic| {
        !topic.eq_ignore_ascii_case(&item.name)
            && item
                .language
                .as_deref()
                .is_none_or(|language| !topic.eq_ignore_ascii_case(language))
    });
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
    if !topics.is_empty() {
        row["topics"] = json!(topics);
    }
    row["forks"] = json!(item.forks_count);
    row["updatedAt"] = json!(date(item.updated_at));
    remove_null_fields(&mut row);
    row
}
