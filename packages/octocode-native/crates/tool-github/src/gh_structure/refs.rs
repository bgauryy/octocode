//! `operation:"refs"` (branches and tags) and `operation:"languages"`.
use super::*;
use octocode_github::MAX_PER_PAGE;
use octocode_github::{NamedRef, RefKind, RefMatch, TagRef};

/// Refs per provider page are the query's `limit`; the contract's largest
/// is GitHub's own page maximum.
const _: () = assert!(
    octocode_runtime_base::contracts::query_limits::gh_structure::refs::LIMIT_MAXIMUM
        <= MAX_PER_PAGE
);

/// The default branch, one page of branches and one of tags ordered newest
/// version first ([`super::tag_order`]), each by its name alone: every read
/// takes a name as `ref` and returns its commit, so a listing reads no tag's
/// commit. Saved cursor progress continues while either list has more.
/// A listing resolves no ref, so it states no `commitSha`: one beside the
/// tags would read as theirs. With `ref`,
/// return only that ref's commit and `refKind`; with a glob `ref`
/// ([`ref_pattern`], `"*"` for every ref), the listing of the tags and
/// branches it matches, each `"<name> <commitSha>"` with the commit it names
/// (an annotated tag's target, every one on the page read together).
pub(super) async fn refs(
    provider: &GitHubProvider,
    query: &GhStructureQueryRefs,
    context: &RequestContext,
) -> Result<ToolData, ProviderError> {
    let (owner, repo) = (query.owner.as_str(), query.repo.as_str());
    let pattern = query
        .ref_
        .as_deref()
        .map(|reference| ref_pattern(reference))
        .transpose()?
        .flatten();
    // One named ref resolves to its commit (gh `api git/ref/tags/X`), not a
    // page of every tag.
    if let Some(reference) = query.ref_.as_deref().filter(|_| pattern.is_none()) {
        let sha = match provider
            .resolve_reference(owner, repo, Some(reference), context)
            .await
        {
            Ok(sha) => sha,
            Err(error) => {
                // The failure names a renamed repository as it is now.
                let renamed = octocode_tools_shared::gh_shared::canonical_query(
                    provider,
                    query,
                    query.owner.as_str(),
                    query.repo.as_str(),
                    context,
                )
                .await
                .ok()
                .flatten();
                let at = renamed.as_ref().map_or(query, |(_, canonical)| canonical);
                return Err(super::traverse::missing_ref(
                    error,
                    at.owner.as_str(),
                    at.repo.as_str(),
                    reference,
                ));
            }
        };
        // A full SHA names itself: a commit, no ref lookup needed.
        let kind = if sha.eq_ignore_ascii_case(reference) {
            RefMatch::Commit
        } else {
            provider
                .transport
                .reference_kind(owner, repo, reference, &sha, context)
                .await?
        };
        let mut value =
            json!({ "owner": owner, "repo": repo, "commitSha": sha, "refKind": kind.as_str() });
        if let Some((warning, _)) = octocode_tools_shared::gh_shared::canonical_query(
            provider,
            query,
            query.owner.as_str(),
            query.repo.as_str(),
            context,
        )
        .await?
        {
            push_warning_front(&mut value, warning);
        }
        return Ok(ToolData::from(value));
    }
    let page = query
        .page
        .map_or(1, |page| octocode_engine::num::saturating_usize(page.get()));
    let per_page = octocode_engine::num::saturating_usize(query.limit.get());
    let transport = &provider.transport;
    // Copy the semantic request before adding the next saved page position.
    let asked = query_row(query, &["page"]);
    let wanted = RefLists::of(&asked);
    // A failed whole-list read remains a typed failure: substituting
    // GitHub's page order would change the cursor's ordering. A glob's
    // literal start is GitHub's to match: only refs that can match are read,
    // branches and tags alike.
    let prefix = match (&pattern, query.ref_.as_deref()) {
        (Some(_), Some(glob)) => literal_prefix(glob),
        _ => "",
    };
    let branches = async {
        match (wanted.branches, &pattern) {
            (false, _) => Ok(None),
            (true, None) => transport
                .repository_refs(owner, repo, RefKind::Branches, page, per_page, context)
                .await
                .map(Some),
            // A pattern selects from the branches under its literal start,
            // in name order: a page of the matches is cut from them.
            (true, Some(pattern)) => transport
                .branch_refs(owner, repo, prefix, context)
                .await
                .map(|all| Some(matching_page(all, pattern, page, per_page))),
        }
    };
    let tags = async {
        if wanted.tags {
            transport
                .repository_tag_refs(owner, repo, prefix, context)
                .await
                .map(Some)
        } else {
            Ok(None)
        }
    };
    // The default branch's commit travels with an unfiltered listing that
    // reads branches: the branch page may not hold it, and every
    // default-branch read lands there. A tags-only listing spends no request
    // on it.
    let default_sha = async {
        if pattern.is_some() || !wanted.branches {
            return Ok(None);
        }
        // An empty repository has no commit: the listing stays whole.
        provider
            .transport
            .commit_sha(owner, repo, "HEAD", context)
            .await
            .map_or(Ok(None), |sha| Ok::<_, ProviderError>(Some(sha)))
    };
    let (default_branch, branches, tags, default_sha) = tokio::try_join!(
        default_branch(provider, owner, repo, context),
        branches,
        tags,
        default_sha,
    )?;
    let mut value = json!({
        "owner": owner,
        "repo": repo,
        "defaultBranch": default_branch,
    });
    if let Some(sha) = default_sha {
        value["defaultBranchSha"] = Value::from(sha);
    }
    // The default-branch read named the canonical repository: leads use it,
    // and the tag order knows the repository's own tags by its name.
    let (renamed_warning, canonical) = octocode_tools_shared::gh_shared::canonical_query(
        provider,
        query,
        query.owner.as_str(),
        query.repo.as_str(),
        context,
    )
    .await?
    .unzip();
    let query = canonical.as_ref().unwrap_or(query);
    let tags = tags.map(|mut refs| {
        if let Some(pattern) = &pattern {
            refs.retain(|tag| pattern.is_match(&tag.name));
        }
        tag_page(refs, query.repo.as_str(), page, per_page)
    });
    // Only a glob listing states commits: the names of a plain listing cost
    // no read of each tag's commit.
    let with_commits = pattern.is_some();
    let mut listed_tags = 0;
    if let Some(tags) = &tags {
        let rows = if with_commits {
            let commits = transport
                .tag_commits(
                    query.owner.as_str(),
                    query.repo.as_str(),
                    &tags.refs,
                    context,
                )
                .await?;
            // A tag deleted since the list was read is no tag of the repository.
            tags.refs
                .iter()
                .zip(commits)
                .filter_map(|(tag, sha)| Some(json!(named_ref(&tag.name, &sha?))))
                .collect::<Vec<_>>()
        } else {
            tags.refs.iter().map(|tag| json!(tag.name)).collect()
        };
        listed_tags = rows.len();
        value["tags"] = Value::Array(rows);
    }
    let tags = tags.unwrap_or_default();
    if let Some(warning) = renamed_warning {
        push_warning_front(&mut value, warning);
    }
    let continued = query_row(query, &["page"]);
    let listed_branches = branches.as_ref().map_or(0, |branches| branches.refs.len());
    let past_end = page > 1 && tags.refs.is_empty() && listed_branches == 0;
    let more = tags.has_more || branches.as_ref().is_some_and(|branches| branches.has_more);
    // Rows, never `{name: commitSha}`: a branch name is data, and in a JSON
    // key the output secret gate reads `"…-token-x":"<sha>"` as a credential.
    if let Some(branches) = branches {
        value["branches"] = branches
            .refs
            .iter()
            .map(|named: &NamedRef| {
                if with_commits {
                    json!(named_ref(&named.name, &named.sha))
                } else {
                    json!(named.name)
                }
            })
            .collect();
    }
    // The refs each list shows on this page, from the start of its provider
    // page: `limit` counts every list, so offset and length share one unit.
    let shown = listed_tags.max(listed_branches);
    let pages = octocode_runtime_base::response::pages::PageFacts::open(
        octocode_runtime_base::response::pages::PageFacts::start_of(page, per_page),
        shown,
        more && !past_end,
    );
    if past_end {
        value["pagination"] = pages.out_of_range(true).to_value();
    } else if more {
        value["pagination"] = pages.to_value();
        // The central continuation store seals this private page position.
        if let Some(call) = octocode_runtime_base::response::collection::continued_page(
            ToolId::GhStructure,
            continued,
            "page",
            page + 1,
            &[],
        ) {
            octocode_runtime_base::response::collection::insert_row_walk(
                &mut value,
                ToolId::GhStructure,
                call,
            );
        }
    } else if page > 1 {
        // The last page of a paged listing says it is the last.
        value["pagination"] = pages.to_value();
    }
    // A glob reads every ref, so an empty first page is the answer: the
    // empty status and the lists state it, and the listing without the glob
    // is the recovery. No warning restates them.
    let unmatched = pattern.is_some() && page == 1 && listed_tags == 0 && listed_branches == 0;
    if unmatched {
        let mut unfiltered = query_row(query, &["page"]);
        if let Some(row) = unfiltered.as_object_mut() {
            row.remove("ref");
        }
        value["calls"]["listRefs"] =
            octocode_runtime_base::response::result::Continuation::related(
                ToolId::GhStructure,
                unfiltered,
            )
            .build();
    }
    octocode_runtime_base::response::result::finish_calls(&mut value);
    let mut output = ToolData::from(value);
    if unmatched {
        output.status = Some("empty");
    }
    Ok(output)
}

/// A `ref` holding a glob character (`*`, `?`, `[`) names no ref (git's
/// `check-ref-format` refuses each in a ref name): it selects the listed
/// tags and branches whose whole name it matches (`v4.*`, `release/*`), so
/// one release line is one call. `None`: a ref name, resolved to its commit.
fn ref_pattern(reference: &str) -> Result<Option<globset::GlobMatcher>, ProviderError> {
    if !reference.contains(['*', '?', '[']) {
        return Ok(None);
    }
    globset::GlobBuilder::new(reference)
        .literal_separator(false)
        .build()
        .map(|glob| Some(glob.compile_matcher()))
        .map_err(|error| {
            ProviderError::new(
                octocode_github::ProviderErrorKind::Validation,
                format!("ref {reference:?} is neither a ref name nor a valid glob: {error}"),
            )
        })
}

/// The start of glob `glob` every name it matches begins with: up to its
/// first glob character (`*`, `?`, `[`, `{`, or a `\\` escape).
fn literal_prefix(glob: &str) -> &str {
    glob.split_once(['*', '?', '[', '{', '\\'])
        .map_or(glob, |(prefix, _)| prefix)
}

/// Page `page` of the refs in `all` whose name `pattern` matches, in
/// GitHub's (name) order.
fn matching_page(
    all: Vec<NamedRef>,
    pattern: &globset::GlobMatcher,
    page: usize,
    per_page: usize,
) -> octocode_github::RefPage {
    let matched = all
        .into_iter()
        .filter(|named| pattern.is_match(&named.name))
        .collect::<Vec<_>>();
    let start = page.saturating_sub(1).saturating_mul(per_page);
    let has_more = matched.len() > start.saturating_add(per_page);
    octocode_github::RefPage {
        refs: matched.into_iter().skip(start).take(per_page).collect(),
        has_more,
    }
}

/// The ref lists one read returns: the request's `refKinds` (`"tags"`,
/// `"branches"`), else both. A list the request leaves out is neither read
/// nor shown, so "the newest tags" costs no page of branches. Read from the
/// request's wire row: the field takes effect when the contract publishes it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct RefLists {
    pub(super) tags: bool,
    pub(super) branches: bool,
}

impl RefLists {
    pub(super) fn of(asked: &Value) -> Self {
        let named = asked.get("refKinds").and_then(Value::as_array);
        let Some(kinds) = named.filter(|kinds| !kinds.is_empty()) else {
            return Self {
                tags: true,
                branches: true,
            };
        };
        let names = |kind: &str| kinds.iter().any(|named| named == kind);
        Self {
            tags: names("tags"),
            branches: names("branches"),
        }
    }
}

/// One page of a repository's tags.
#[derive(Default)]
struct TagPage {
    refs: Vec<TagRef>,
    has_more: bool,
}

/// Page `page` of repository `repo`'s tags, newest version first: the
/// whole tag list (one git-refs listing) is ordered, so every page
/// continues one order.
fn tag_page(refs: Vec<TagRef>, repo: &str, page: usize, per_page: usize) -> TagPage {
    let mut names = refs.iter().map(|tag| tag.name.clone()).collect::<Vec<_>>();
    super::tag_order::newest_first(&mut names, repo);
    let mut by_name = refs
        .into_iter()
        .map(|tag| (tag.name.clone(), tag))
        .collect::<std::collections::HashMap<_, _>>();
    let start = page.saturating_sub(1).saturating_mul(per_page);
    let has_more = names.len() > start.saturating_add(per_page);
    TagPage {
        refs: names
            .into_iter()
            .skip(start)
            .take(per_page)
            .filter_map(|name| by_name.remove(&name))
            .collect(),
        has_more,
    }
}

/// Bytes of code per language on the default branch, largest first, with
/// the rename warning a renamed repository's other operations carry.
pub(super) async fn languages(
    provider: &GitHubProvider,
    query: &GhStructureQueryLanguages,
    context: &RequestContext,
) -> Result<ToolData, ProviderError> {
    let languages = provider
        .transport
        .repository_languages(query.owner.as_str(), query.repo.as_str(), context)
        .await?;
    let languages: Map<String, Value> = languages
        .into_iter()
        .map(|(name, bytes)| (name, json!(bytes)))
        .collect();
    let mut value = json!({ "owner": query.owner, "repo": query.repo, "languages": languages });
    if let Some((warning, _)) = octocode_tools_shared::gh_shared::canonical_query(
        provider,
        query,
        query.owner.as_str(),
        query.repo.as_str(),
        context,
    )
    .await?
    {
        push_warning_front(&mut value, warning);
    }
    octocode_runtime_base::response::result::finish_calls(&mut value);
    Ok(ToolData::from(value))
}

#[cfg(test)]
mod ref_list_tests;

/// A ref as a glob listing states it: `"<name> <commitSha>"`. A git ref name
/// holds no space (git-check-ref-format), so the first space splits it back;
/// the name stays a value, never a JSON key the output secret gate would read
/// as an assignment.
fn named_ref(name: &str, sha: &str) -> String {
    format!("{name} {sha}")
}
