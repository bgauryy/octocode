//! One tree listing: the walk, its page and the broadened lead of an empty filtered listing.
use super::*;

pub(super) async fn execute_tree(
    provider: &GitHubProvider,
    query: &GhStructureQueryTree,
    context: &RequestContext,
    home: &Path,
    read_commit: &mut Option<String>,
) -> Result<ToolData, ProviderError> {
    let scope = Scope::of(query)?;
    let (resolved_branch, commit_sha) = resolve_listing_ref(provider, query, context).await?;
    *read_commit = Some(commit_sha.clone());
    // A listing is read and continued under the canonical name:
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
    let mut value = json!({
        "owner": query.owner,
        "repo": query.repo,
        "files": build_structure(page.entries),
        "summary": summary(&traversal.entries),
        "resolvedRef": resolved_branch,
    });
    // A caller-supplied full SHA is not restated.
    if !resolved_branch.eq_ignore_ascii_case(&commit_sha) {
        value["commitSha"] = json!(commit_sha);
    }
    page.paginate(&mut value);
    let mut output = ToolData::from(Value::Null);
    let materialize_resume = if query.materialize == Some(true) {
        materialize_page(
            provider,
            query,
            &scope,
            &commit_sha,
            &page,
            &traversal.entries,
            home,
            context,
            &mut value,
        )
        .await?
    } else {
        None
    };
    disclose_limits(&traversal, &page, &mut value, &mut output);
    // A walk that missed entries wrote what it saw: never a complete checkout.
    if (traversal.contents_limit || traversal.failed_subtrees > 0)
        && let Some(state) = value.pointer_mut("/location/coverage/state")
    {
        *state = json!("partial");
    }
    // A selection one call cannot finish, or a listing GitHub capped, is what
    // a clone is for.
    if traversal.contents_limit || materialize_resume.is_some() {
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
        &scope,
        &page,
        traversal.failed_subtrees > 0,
        materialize_resume,
    )?;
    // A resumed materialize is anchored by `location`: its first call already
    // listed these entries, and its `nextPage` reads the rest of the listing.
    let resumed = query.materialize == Some(true)
        && query.materialize_offset.is_some()
        && value.get("location").is_some();
    if resumed && let Some(map) = value.as_object_mut() {
        map.remove("files");
        map.remove("pagination");
        if let Some(calls) = map.get_mut("calls").and_then(Value::as_object_mut) {
            calls.remove("nextPage");
        }
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
    } else if page.current == 1
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
