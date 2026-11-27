//! One compact repository row.
use octocode_contract_types::GhSearchRepoQuery;
use octocode_github::{ProviderError, ProviderErrorKind};
use octocode_runtime_base::response::result::remove_null_fields;
use serde_json::{Value, json};

/// A repository timestamp as its UTC day; text too short to name one
/// passes through.
pub(super) fn date(value: Option<String>) -> Option<String> {
    value.map(|v| octocode_tools_shared::gh_shared::dates::utc_day(&v).unwrap_or(v))
}

/// A repository timestamp whole, in UTC: rows ordered by it are told apart
/// within a day.
fn instant(value: Option<String>) -> Option<String> {
    value.map(|at| octocode_tools_shared::gh_shared::dates::utc_date(Some(&at)))
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

/// The visibility GitHub states: its `visibility`, else its `private`
/// flag; `None` when the server sent neither (some GitHub Enterprise
/// servers), which is not the same as public.
fn stated_visibility(visibility: &str, private: Option<bool>) -> Option<&str> {
    match (visibility, private) {
        ("", None) => None,
        ("", Some(true)) => Some("private"),
        ("", Some(false)) => Some("public"),
        (stated, _) => Some(stated),
    }
}

/// One repository row, whole: `owner` and `repo`, `defaultBranch`, stars,
/// language, license, dates, the description, forks, `visibility` when not
/// public, its topics (query matches first), its homepage, open issue count
/// and size. `pushedAt` and `updatedAt` are whole timestamps: an owner
/// listing is in push order to the second, while a search sorted by
/// `updatedAt` keeps GitHub's own order, which need not follow either. What
/// a default row keeps of it, and what a `fields` pick names back, is the
/// caller's ([`super::LISTED_DETAIL`], [`super::EXACT_DETAIL`]).
pub(super) fn repository_row(
    item: octocode_github::RepositorySearchItem,
    wanted: &[String],
    fields: &[String],
) -> Result<Value, ProviderError> {
    // Every topic GitHub returns: a topic is a tag the owner chose, even
    // when it spells the repository's name or language.
    let mut topics = item.topics;
    // Stable: matching topics keep GitHub's order, then the rest.
    topics.sort_by_key(|topic| !wanted.contains(&topic.to_lowercase()));
    let (owner, repo) = item
        .full_name
        .split_once('/')
        .filter(|(owner, repo)| !owner.is_empty() && !repo.is_empty() && !repo.contains('/'))
        .ok_or_else(|| {
            ProviderError::new(
                ProviderErrorKind::Decode,
                "GitHub returned a repository without a valid owner/repo name",
            )
        })?;
    let default_branch = Some(item.default_branch).filter(|branch| !branch.is_empty());
    let mut row = json!({
        "owner": owner,
        "repo": repo,
        "defaultBranch": default_branch,
        "stars": item.stargazers_count,
        "language": item.language,
        "license": item.license.and_then(|license| license.spdx_id),
        "pushedAt": instant(item.pushed_at),
        "createdAt": date(item.created_at),
        "description": item.description,
    });
    // A mark rides a row only when set (and a visibility only when not
    // public), unless `fields` picks it: false, public and no topics are
    // facts too, so a pick states them.
    let picked = |name: &str| fields.iter().any(|field| field == name);
    if item.archived || picked("archived") {
        row["archived"] = json!(item.archived);
    }
    // A fork in an owner listing is not the owner's own source.
    if item.fork || picked("fork") {
        row["fork"] = json!(item.fork);
    }
    let visibility = if picked("visibility") {
        stated_visibility(&item.visibility, item.private)
    } else {
        not_public(&item.visibility, item.private.unwrap_or(false))
    };
    if let Some(visibility) = visibility {
        row["visibility"] = json!(visibility);
    }
    if !topics.is_empty() || picked("topics") {
        row["topics"] = json!(topics);
    }
    row["forks"] = json!(item.forks_count);
    row["updatedAt"] = json!(instant(item.updated_at));
    row["homepage"] = json!(item.homepage.filter(|page| !page.trim().is_empty()));
    row["openIssuesCount"] = json!(item.open_issues_count);
    row["sizeKiB"] = json!(item.size);
    remove_null_fields(&mut row);
    Ok(row)
}
