//! `operation:"refs"` (branches and tags) and `operation:"languages"`.
use super::*;
use crate::response::paging::push_warnings;
use octocode_contract_types::ExtraKind;
use octocode_github::MAX_PER_PAGE;
use octocode_github::{NamedRef, RefKind, RefMatch};

/// Refs per page when `pageSize` is omitted (contract
/// `ghStructure.semanticPolicy.refsPageSize`, GitHub's page maximum).
const DEFAULT_REFS_PER_PAGE: usize = crate::id::gh_structure_policy::REFS_PAGE_SIZE;
const _: () = assert!(DEFAULT_REFS_PER_PAGE <= MAX_PER_PAGE);

/// The default branch and its head commit, and one page of tag names,
/// newest version first ([`super::tag_order`]); with
/// `extras: ["details"]` also the same page of branches (`{name: sha}`),
/// read only when asked. A page continues while a listed kind has more, so
/// every listed tag (and branch) is named exactly once. A tag name is
/// itself a ref, so tags carry no SHA. With `ref`: that ref's commit and its `refKind` only.
pub(super) async fn refs(
    provider: &GitHubProvider,
    query: &GhStructureQueryRefs,
    context: &RequestContext,
) -> Result<ToolData, ProviderError> {
    let (owner, repo) = (query.owner.as_str(), query.repo.as_str());
    // One named ref resolves to its commit (gh `api git/ref/tags/X`), not a
    // page of every tag.
    if let Some(reference) = query.ref_.as_deref() {
        let sha = match provider
            .resolve_reference(owner, repo, Some(reference), false, context)
            .await
        {
            Ok(sha) => sha,
            Err(error) => {
                // The failure names a renamed repository as it is now.
                let renamed = crate::tools::gh_shared::canonical_query(
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
        let mut value = json!({ "commitSha": sha, "refKind": kind.as_str() });
        if let Some((warning, _)) = crate::tools::gh_shared::canonical_query(
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
    let page = query.page.map_or(1, crate::num::usize_of);
    let requested = query.page_size.map_or(DEFAULT_REFS_PER_PAGE, |size| {
        crate::num::usize_of(size.get())
    });
    let per_page = requested.clamp(1, MAX_PER_PAGE);
    let with_branches = query.extras.contains(&ExtraKind::Details);
    let transport = &provider.transport;
    let branches = async {
        if with_branches {
            transport
                .repository_refs(owner, repo, RefKind::Branches, page, per_page, context)
                .await
                .map(Some)
        } else {
            Ok(None)
        }
    };
    let (default_branch, head, branches, tags) = tokio::try_join!(
        default_branch(provider, owner, repo, context),
        provider.resolve_reference(owner, repo, None, false, context),
        branches,
        tag_page(transport, owner, repo, page, per_page, context),
    )?;
    let mut value = json!({
        "defaultBranch": default_branch,
        "commitSha": head,
        "tags": tags.names,
    });
    if tags.github_order {
        push_warnings(
            &mut value,
            ["Tags are in GitHub's order (reverse-lexicographic, not newest first): the whole tag list could not be read to order it.".to_owned()],
        );
    }
    // The default-branch read named the canonical repository: leads use it.
    let (renamed_warning, canonical) = crate::tools::gh_shared::canonical_query(
        provider,
        query,
        query.owner.as_str(),
        query.repo.as_str(),
        context,
    )
    .await?
    .unzip();
    let query = canonical.as_ref().unwrap_or(query);
    if let Some(warning) = renamed_warning {
        push_warning_front(&mut value, warning);
    }
    // The rows a lead or the next page copies: from page 1, and with the
    // size GitHub reads, so a clamped `pageSize` warns on this call only.
    let mut continued = query_row(query, &["page"]);
    if requested > per_page {
        push_warnings(
            &mut value,
            [format!(
                "pageSize {requested} reads as {MAX_PER_PAGE}, GitHub's ref page."
            )],
        );
        continued["pageSize"] = json!(per_page);
    }
    // A page past the last lists no ref of any listed kind: the shared
    // response stage words it and restarts from page 1.
    let past_end = page > 1
        && tags.names.is_empty()
        && branches
            .as_ref()
            .is_none_or(|branches| branches.refs.is_empty());
    if !with_branches && !past_end {
        // Branches are read only when asked, from their first page: the
        // lead adds the extra that asks for them to the caller's own.
        let mut listed = continued.clone();
        let mut extras = query.extras.clone();
        extras.push(ExtraKind::Details);
        listed["extras"] = json!(extras);
        value["next"]["viewRefs"] = continuation(listed);
    }
    if let Some(branches) = &branches {
        // `{name: sha}`: one key per branch instead of repeated `name`/`sha` keys.
        value["branches"] = Value::Object(
            branches
                .refs
                .iter()
                .map(|named: &NamedRef| (named.name.clone(), json!(named.sha)))
                .collect(),
        );
    }
    let more = tags.has_more || branches.as_ref().is_some_and(|branches| branches.has_more);
    let pages = crate::response::pages::PageFacts::open(page, Some(per_page), more && !past_end);
    if past_end {
        value["pagination"] = pages.out_of_range(true).to_value();
    } else if more {
        value["pagination"] = pages.to_value();
        if let Some(next_page) = crate::response::paging::next_cursor(
            &mut value,
            crate::id::ToolId::GhStructure,
            None,
            "page",
            page + 1,
        ) {
            // `pageSize` rides along only when the caller set it.
            continued["page"] = json!(next_page);
            value["next"]["nextPage"] = continuation(continued);
        }
    } else if page > 1 {
        // The last page of a paged listing says it is the last.
        value["pagination"] = pages.to_value();
    }
    Ok(ToolData::from(value))
}

/// One page of a repository's tag names.
struct TagPage {
    names: Vec<String>,
    has_more: bool,
    /// GitHub's own tag page (reverse-lexicographic), read when the whole
    /// tag list could not be.
    github_order: bool,
}

/// Page `page` of the repository's tags, newest version first: the whole
/// tag list is read (one git-refs listing) and ordered, so every page
/// continues one order. When that listing fails, GitHub's tag page in its
/// own order stands in.
async fn tag_page(
    transport: &octocode_github::GitHubTransport,
    owner: &str,
    repo: &str,
    page: usize,
    per_page: usize,
    context: &RequestContext,
) -> Result<TagPage, ProviderError> {
    match transport.repository_tag_names(owner, repo, context).await {
        Ok(mut names) => {
            super::tag_order::newest_first(&mut names);
            let start = page.saturating_sub(1).saturating_mul(per_page);
            let has_more = names.len() > start.saturating_add(per_page);
            Ok(TagPage {
                names: names.into_iter().skip(start).take(per_page).collect(),
                has_more,
                github_order: false,
            })
        }
        Err(error) if error.is_cancelled_or_timed_out() => Err(error),
        Err(_) => {
            let tags = transport
                .repository_refs(owner, repo, RefKind::Tags, page, per_page, context)
                .await?;
            Ok(TagPage {
                names: tags.refs.into_iter().map(|named| named.name).collect(),
                has_more: tags.has_more,
                github_order: true,
            })
        }
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
    let mut value = json!({ "languages": languages });
    if let Some((warning, _)) = crate::tools::gh_shared::canonical_query(
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
    Ok(ToolData::from(value))
}
