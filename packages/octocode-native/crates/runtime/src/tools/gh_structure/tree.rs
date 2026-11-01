//! One tree listing: the walk, its page, its dating and the broadened lead of an empty filtered listing.
use super::*;

pub(super) async fn execute_tree(
    provider: &GitHubProvider,
    query: &GhStructureQueryTree,
    context: &RequestContext,
    home: &Path,
) -> Result<ToolData, ProviderError> {
    let scope = Scope::of(query)?;
    let (resolved_branch, commit_sha) = resolve_listing_ref(provider, query, context).await?;
    // A listing is read, dated and continued under the canonical name:
    // later requests skip the redirect, and every lead names the
    // repository as it is now.
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
    let listing = Listing {
        owner: &query.owner,
        repo: &query.repo,
        branch: &commit_sha,
        root: &scope.path,
        max_depth: scope.depth,
    };
    let mut traversal = traverse(provider, &listing, context).await?;
    scope.select(&mut traversal);
    let page = ListingPage::of(query, &traversal.entries);
    let page_dates = date_page(provider, query, &commit_sha, &page, context).await;
    let dating = PageDating {
        undated: page_dates.undated,
        failed: page_dates.warning.is_some(),
    };
    let date_offset = query
        .date_offset
        .map_or(0, crate::num::usize_of)
        .min(page.entries.len());
    let shown_entries = if query.materialize != Some(true) {
        &page.entries[date_offset
            ..date_offset
                .saturating_add(dates::DATED_PER_PAGE)
                .min(page.entries.len())]
    } else {
        page.entries
    };
    let mut value = json!({
        "owner": query.owner,
        "repo": query.repo,
        "files": build_structure(shown_entries, &page_dates.entries),
        "summary": summary(&traversal.entries),
        "resolvedRef": resolved_branch,
    });
    // A caller-supplied full SHA is not restated.
    if !resolved_branch.eq_ignore_ascii_case(&commit_sha) {
        value["commitSha"] = json!(commit_sha);
    }
    page.paginate(&mut value);
    dates::attach(&mut value, page_dates);
    let mut output = ToolData::from(Value::Null);
    let materialize_resume = if query.materialize == Some(true) {
        materialize_page(
            provider,
            query,
            &scope,
            &commit_sha,
            &page,
            home,
            context,
            &mut value,
        )
        .await?
    } else {
        None
    };
    disclose_limits(&traversal, &page, &mut value, &mut output);
    if traversal.contents_limit {
        value["calls"]["cloneRepo"] = clone_lead(query, &scope, &commit_sha);
    }
    if let Some(warning) = renamed_warning {
        push_warning_front(&mut value, warning);
    }
    // Continuations read the same commit.
    let pinned = GhStructureQueryTree {
        ref_: Some(commit_sha.parse().map_err(
            |error: octocode_contract_types::error::ConversionError| {
                ProviderError::new(ProviderErrorKind::Decode, error.to_string())
            },
        )?),
        ..query.clone()
    };
    attach_continuations(
        &mut value,
        &pinned,
        &page,
        traversal.failed_subtrees > 0 || dating.failed,
        query.materialize == Some(true),
        materialize_resume,
    )?;
    dates::expand(&mut value, &pinned, &page, dating.undated)?;
    // A resumed materialize page (offset > 0) is anchored by `location`:
    // its first call already listed these entries.
    let resumed = query.materialize == Some(true)
        && query.materialize_offset.is_some_and(|offset| offset > 0)
        && value.get("location").is_some();
    if resumed && let Some(map) = value.as_object_mut() {
        map.remove("files");
    }
    if !resumed && structure_is_empty(&value) && value.get("isPartial").is_none() {
        output.status = Some("empty");
        if let Some(broader) = broaden_listing(&pinned) {
            crate::tools::leads::broaden::set_broadened(
                &mut value,
                &query_row(&pinned, &[]),
                broader,
            );
        }
    } else if value.pointer("/location/localPath").is_some() {
        // A materialized listing continues with the local tools at
        // `location.localPath`; re-listing the tree it just returned adds
        // nothing.
    } else if date_offset == 0
        && page.current == 1
        && let Some(read) = entry_read(&pinned, &traversal.entries, &commit_sha)
    {
        value["calls"]["read"] = read;
    }
    crate::response::result::finish_calls(&mut value);
    output.data = value;
    Ok(output)
}

/// An empty filtered listing, broadened ([`crate::tools::leads::broaden::broaden`]):
/// the same commit's listing without its `include` globs, so what the tree
/// holds names why they matched nothing; first, when a glob repeats the
/// listed `path` (globs match relative to it), the same globs rebased onto
/// it. Never a materialization.
fn broaden_listing(pinned: &GhStructureQueryTree) -> Option<Value> {
    let mut lead = crate::tools::leads::broaden::broaden(
        ToolId::GhStructure,
        &query_row(pinned, &[]),
        &crate::tools::leads::broaden::Broaden {
            strip: &["materialize"],
            groups: &[&["include"]],
            split_lone_keyword: false,
        },
        1,
    )?;
    let root = pinned
        .path
        .as_deref()
        .and_then(|path| crate::tools::gh_shared::listing_path(path).ok())
        .filter(|root| !root.is_empty());
    if let Some(root) = root
        && let Some(queries) = lead
            .pointer_mut("/input/queries")
            .and_then(Value::as_array_mut)
    {
        let prefix = format!("{root}/");
        let globs = pinned.include.iter().map(|glob| glob.as_str());
        if globs.clone().any(|glob| glob.starts_with(&prefix)) {
            let mut rebased = queries[0].clone();
            rebased["include"] = globs
                .map(|glob| glob.strip_prefix(&prefix).unwrap_or(glob))
                .collect();
            queries.insert(0, rebased);
        }
    }
    Some(lead)
}

/// What dating a listing page left: entries no date reached, and whether
/// the dating request failed (the same page retries it).
#[derive(Default)]
struct PageDating {
    undated: usize,
    failed: bool,
}

/// A listing includes last-change dates in bounded GraphQL batches.
/// Saved `dateOffset` progress reaches entries beyond the current batch.
/// A materialized page hands its files to local tools and stays undated.
async fn date_page(
    provider: &GitHubProvider,
    query: &GhStructureQueryTree,
    commit_sha: &str,
    page: &ListingPage<'_>,
    context: &RequestContext,
) -> dates::PageDates {
    if query.materialize == Some(true) {
        return dates::PageDates::default();
    }
    dates::page_dates(
        provider,
        &query.owner,
        &query.repo,
        commit_sha,
        &page.entries[query
            .date_offset
            .map_or(0, crate::num::usize_of)
            .min(page.entries.len())..],
        context,
    )
    .await
}
