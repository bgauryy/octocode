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
    // Every page reads the whole tree of the pinned commit, so a rank is
    // over the whole listing before its page is cut.
    let ranked = rank(&mut traversal.entries, &query.sort);
    let page = ListingPage::of(query, &traversal.entries);
    let mut value = json!({
        "owner": query.owner,
        "repo": query.repo,
        "files": if ranked {
            ranked_structure(page.entries)
        } else {
            build_structure(page.entries)
        },
        "summary": summary(&traversal.entries),
    });
    // `commitSha` is the listed commit on every page; `ref` names the branch
    // or tag that resolved to it, never the commit itself, so a later page
    // (its cursor pins the commit) renames nothing.
    if !resolved_branch.eq_ignore_ascii_case(&commit_sha) {
        value["ref"] = json!(resolved_branch);
    }
    value["commitSha"] = json!(commit_sha);
    page.paginate(&mut value);
    let mut output = ToolData::from(Value::Null);
    let materialize_resume = if query.materialize == Some(true) {
        materialize_page(
            provider,
            query,
            &scope,
            &commit_sha,
            &traversal.entries,
            home,
            context,
            &mut value,
        )
        .await?
    } else {
        None
    };
    disclose_limits(&traversal, &page, &mut value);
    // A rank over a walk that missed entries is a rank of what it listed.
    if ranked && (traversal.contents_limit || traversal.failed_subtrees > 0) {
        crate::response::paging::push_warnings(
            &mut value,
            [
                "sort ranks only the entries this listing read; an unread entry may rank higher."
                    .to_owned(),
            ],
        );
    }
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
        if let Some((broader, rebased_under)) = broaden_listing(&pinned) {
            crate::tools::leads::broaden::set_broadened(
                &mut value,
                &query_row(&pinned, &[]),
                broader,
            );
            // Said only with the request it explains.
            if let Some(root) = rebased_under {
                crate::response::result::add_hints(
                    &mut value,
                    [format!(
                        "include globs match paths from the repository root, also with path set: the related[] ghStructure request's first query asks them under {root}/."
                    )],
                );
            }
        }
    }
    crate::response::result::finish_calls(&mut value);
    output.data = value;
    Ok(output)
}

/// An empty filtered listing, broadened ([`crate::tools::leads::broaden::broaden`]):
/// the same commit's listing without its `include` globs, so what the tree
/// holds names why they matched nothing; first, when a glob is written
/// relative to the listed `path` ([`under_path`]), the same globs asked
/// under it, and the listed path it names. Never a materialization.
fn broaden_listing(pinned: &GhStructureQueryTree) -> Option<(Value, Option<String>)> {
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
    let mut rebased_under = None;
    if let Some((root, rebased_globs)) = under_path(pinned)
        && let Some(queries) = lead
            .pointer_mut("/input/queries")
            .and_then(Value::as_array_mut)
    {
        let mut rebased = queries[0].clone();
        rebased["include"] = rebased_globs.into();
        queries.insert(0, rebased);
        rebased_under = Some(root);
    }
    Some((lead, rebased_under))
}

/// The listed `path` and the `include` globs asked under it, when a glob
/// reads as written relative to `path`: globs match repository-relative
/// paths also with `path` set (which carry no leading `/`), so a glob
/// neither under `path` nor starting at any depth (`**`) matches nothing
/// below it. Globs already under `path`, or at any depth, stay as written,
/// less one leading `/`.
fn under_path(pinned: &GhStructureQueryTree) -> Option<(String, Vec<String>)> {
    let root = pinned
        .path
        .as_deref()
        .and_then(|path| crate::tools::gh_shared::listing_path(path).ok())
        .filter(|root| !root.is_empty())?;
    let prefix = format!("{root}/");
    // `None` when the glob stands as written.
    let rebased = |glob: &str| {
        let bare = glob.strip_prefix('/').unwrap_or(glob);
        if bare.starts_with(&prefix) || bare.starts_with("**") {
            (bare != glob).then(|| bare.to_owned())
        } else {
            Some(format!("{prefix}{bare}"))
        }
    };
    if !pinned
        .include
        .iter()
        .any(|glob| rebased(glob.as_str()).is_some())
    {
        return None;
    }
    let globs = pinned
        .include
        .iter()
        .map(|glob| rebased(glob.as_str()).unwrap_or_else(|| glob.as_str().to_owned()))
        .collect();
    Some((root, globs))
}
