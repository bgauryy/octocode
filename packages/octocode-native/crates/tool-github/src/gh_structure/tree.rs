//! One tree listing: the walk, its page and the broadened lead of an empty filtered listing.
use super::*;

pub(super) async fn execute_tree(
    provider: &GitHubProvider,
    query: &GhStructureQueryTree,
    context: &RequestContext,
    home: &Path,
    blobs: Option<&BlobSource>,
    read_commit: &mut Option<String>,
) -> Result<ToolData, ProviderError> {
    let scope = Scope::of(query)?;
    let (resolved_branch, commit_sha) = resolve_listing_ref(provider, query, context).await?;
    *read_commit = Some(commit_sha.clone());
    // A listing is read and continued under the canonical name:
    // later requests skip the redirect, and every lead names the
    // repository as it is now.
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
    let listing = Listing {
        owner: &query.owner,
        repo: &query.repo,
        branch: &commit_sha,
        root: &scope.path,
        max_depth: scope.depth,
    };
    // Continuations read the same commit.
    let pinned = GhStructureQueryTree {
        ref_: Some(commit_sha.parse().map_err(
            |error: octocode_contract_types::error::ConversionError| {
                ProviderError::new(ProviderErrorKind::Decode, error.to_string())
            },
        )?),
        ..query.clone()
    };
    let pace = Pace::of(query.walk_offset, context);
    let mut traversal = match traverse(provider, &listing, pace, context).await? {
        Walked::Done(traversal) => traversal,
        Walked::Paused { reads } => {
            let mut value = paused_listing(&pinned, &resolved_branch, &commit_sha, reads);
            if let Some(warning) = renamed_warning {
                push_warning_front(&mut value, warning);
            }
            return Ok(ToolData::from(value));
        }
    };
    scope.select(&mut traversal);
    // Every page reads the whole tree of the pinned commit, so a rank is
    // over the whole listing before its page is cut.
    let ranked = rank(&mut traversal.entries, &query.sort);
    let page = ListingPage::of(query, &traversal.entries);
    // A materialize lists links and reads nothing of them.
    let targets = if query.materialize == Some(true) {
        LinkTargets::new()
    } else {
        link_targets(provider, &listing, page.entries, context).await?
    };
    let mut value = json!({
        "owner": query.owner,
        "repo": query.repo,
        "files": if ranked {
            ranked_structure(page.entries, &targets)
        } else {
            build_structure(page.entries, &targets)
        },
    });
    // The whole listing's counts are one fact of the pinned commit: its
    // first page states them, and a later page repeats them only when this
    // walk missed entries (its counts are then what this call saw).
    if page.current == 1 || traversal.contents_limit || traversal.missed() {
        value["summary"] = summary(&traversal.entries);
    }
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
            blobs,
            context,
            &mut value,
        )
        .await?
    } else {
        None
    };
    disclose_limits(&traversal, &page, &mut value);
    // A rank over a walk that missed entries is a rank of what it listed.
    if ranked && (traversal.contents_limit || traversal.missed()) {
        octocode_runtime_base::response::paging::push_warnings(
            &mut value,
            [
                "sort ranks only the entries this listing read; an unread entry may rank higher."
                    .to_owned(),
            ],
        );
    }
    // A walk that missed entries wrote what it saw: never a complete checkout.
    if (traversal.contents_limit || traversal.missed())
        && let Some(state) = value.pointer_mut("/location/coverage/state")
    {
        *state = json!("partial");
    }
    state_checkout_partial_once(&mut value);
    // A directory GitHub capped is complete only in a checkout: the clone
    // reaches the entries this listing cannot, so the row needs it. A
    // selection one call cannot finish is finished by its own continuation.
    if traversal.contents_limit {
        value["calls"]["cloneRepo"] = octocode_runtime_base::response::result::required_call(
            clone_lead(query, &scope, &commit_sha),
        );
    }
    if let Some(warning) = renamed_warning {
        push_warning_front(&mut value, warning);
    }
    attach_continuations(
        &mut value,
        &pinned,
        &scope,
        &page,
        traversal.missed(),
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
        map.remove("summary");
    }
    if resumed {
        octocode_runtime_base::response::collection::remove_row_walk(&mut value);
    }
    if !resumed && structure_is_empty(&value) && value.get("isPartial").is_none() {
        output.status = Some("empty");
        match broaden_listing(&pinned) {
            // The globs are only mis-written: the one repaired listing and
            // the hint that names the fix.
            Some(Broadened::Repaired { lead, hint }) => {
                octocode_runtime_base::response::result::attach_related(&mut value, lead);
                octocode_runtime_base::response::result::add_hints(&mut value, [hint]);
            }
            Some(Broadened::Dropped(lead)) => {
                octocode_tools_shared::leads::broaden::set_broadened(
                    &mut value,
                    &query_row(&pinned, &[]),
                    lead,
                );
            }
            None => {}
        }
    }
    octocode_runtime_base::response::result::finish_calls(&mut value);
    output.data = value;
    Ok(output)
}

/// The page of a walk the call's time paused ([`Walked::Paused`]): nothing
/// is listed yet, since a listing orders, counts and pages its whole
/// selection; `next.nextPage` replays the `reads` made so far at the same
/// commit and walks on, so walking every page lists each entry once.
pub(super) fn paused_listing(
    pinned: &GhStructureQueryTree,
    resolved_branch: &str,
    commit_sha: &str,
    reads: usize,
) -> Value {
    let mut value = json!({"owner": pinned.owner, "repo": pinned.repo});
    if !resolved_branch.eq_ignore_ascii_case(commit_sha) {
        value["ref"] = json!(resolved_branch);
    }
    value["commitSha"] = json!(commit_sha);
    octocode_runtime_base::response::paging::mark_partial(
        &mut value,
        octocode_runtime_base::response::paging::PartialReason::WalkDeadline,
        &[format!(
            "Walk paused at its time budget after {reads} folder reads: nothing is listed until next ends the walk."
        )],
    );
    if let Some(call) = octocode_runtime_base::response::collection::continued_page(
        ToolId::GhStructure,
        query_row(pinned, &[]),
        "walkOffset",
        reads,
        &[],
    ) {
        octocode_runtime_base::response::collection::insert_row_walk(
            &mut value,
            ToolId::GhStructure,
            call,
        );
    }
    octocode_runtime_base::response::result::finish_calls(&mut value);
    value
}

/// How an empty filtered listing recovers ([`broaden_listing`]).
enum Broadened {
    /// The same listing with its globs repaired, alone: they restate `path`
    /// ([`under_path`]), or name a bare word that matches only an exact name
    /// ([`as_substrings`]). `hint` names the fix.
    Repaired { lead: Value, hint: String },
    /// The same commit's listing without its `include` globs, so what the
    /// tree holds names why they matched nothing.
    Dropped(Value),
}

/// An empty filtered listing's recovery, never a materialization: when a
/// glob is written from the repository root though `path` is set, the same
/// globs asked relative to it; else, when an entry is a bare word, the same
/// globs with each such word as a substring; else the listing without its
/// `include` globs ([`octocode_tools_shared::leads::broaden::broaden`]).
fn broaden_listing(pinned: &GhStructureQueryTree) -> Option<Broadened> {
    let mut lead = octocode_tools_shared::leads::broaden::broaden(
        ToolId::GhStructure,
        &query_row(pinned, &[]),
        &octocode_tools_shared::leads::broaden::Broaden {
            strip: &["materialize"],
            groups: &[&["include"]],
            split_lone_keyword: false,
            loosen_phrase: false,
        },
        1,
    )?;
    let repair = under_path(pinned)
        .map(|(root, rebased)| {
            let hint = if rebased.restated.len() == 1 {
                format!(
                    "include globs are relative to path `{root}`: {} restates it, so it matches nothing below it.",
                    quoted(&rebased.restated)
                )
            } else {
                format!(
                    "include globs are relative to path `{root}`: {} restate it, so they match nothing below it.",
                    quoted(&rebased.restated)
                )
            };
            (rebased.globs, hint)
        })
        .or_else(|| {
            as_substrings(pinned).map(|(bare, globs)| {
                let wrapped = bare
                    .iter()
                    .map(|word| format!("*{word}*"))
                    .collect::<Vec<_>>();
                let hint = if bare.len() == 1 {
                    format!(
                        "include {} matches only that exact name; {} matches a substring.",
                        quoted(&bare),
                        quoted(&wrapped)
                    )
                } else {
                    format!(
                        "include {} match only those exact names; {} match substrings.",
                        quoted(&bare),
                        quoted(&wrapped)
                    )
                };
                (globs, hint)
            })
        });
    let Some((globs, hint)) = repair else {
        return Some(Broadened::Dropped(lead));
    };
    let queries = lead
        .pointer_mut("/input/queries")
        .and_then(Value::as_array_mut)?;
    let mut repaired = queries.first()?.clone();
    repaired["include"] = globs.into();
    // A repaired glob the contract refuses (one grown past its length)
    // falls back to dropping the globs.
    if octocode_runtime_base::contracts::validate(
        ToolId::GhStructure.as_str(),
        json!({ "queries": [&repaired] }),
    )
    .is_err()
    {
        return Some(Broadened::Dropped(lead));
    }
    *queries = vec![repaired];
    Some(Broadened::Repaired { lead, hint })
}

/// `"a"`, `"b"`: the globs a hint names, as written.
fn quoted(globs: &[String]) -> String {
    globs
        .iter()
        .map(|glob| format!("{glob:?}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The `include` entries that are bare words (no glob character, no `/`,
/// no negation), and the globs with each such word as a substring
/// (`index` → `*index*`): a bare word matches only an entry of exactly that
/// name. `None` when no entry is one.
fn as_substrings(pinned: &GhStructureQueryTree) -> Option<(Vec<String>, Vec<String>)> {
    let bare_word = |glob: &str| {
        !glob.is_empty()
            && !glob.starts_with(['!', '#'])
            && !glob.contains(['*', '?', '[', '{', '/'])
    };
    let bare = pinned
        .include
        .iter()
        .map(|glob| glob.as_str().to_owned())
        .filter(|glob| bare_word(glob))
        .collect::<Vec<_>>();
    if bare.is_empty() {
        return None;
    }
    let globs = pinned
        .include
        .iter()
        .map(|glob| {
            let glob = glob.as_str();
            if bare_word(glob) {
                format!("*{glob}*")
            } else {
                glob.to_owned()
            }
        })
        .collect();
    Some((bare, globs))
}

/// `include` globs asked relative to the listed path ([`under_path`]).
struct Rebased {
    /// Every glob, those that restated `path` without that prefix.
    globs: Vec<String>,
    /// The globs as written that restated `path`.
    restated: Vec<String>,
}

/// The listed `path` and the `include` globs asked relative to it, when a
/// glob restates `path`: globs read paths below the listed directory, as on
/// the local tools, so a glob that starts with `path` itself matches nothing
/// there. Such a glob loses that prefix (and one leading `/`); the others
/// stay as written.
fn under_path(pinned: &GhStructureQueryTree) -> Option<(String, Rebased)> {
    let root = pinned
        .path
        .as_deref()
        .and_then(|path| octocode_tools_shared::gh_shared::listing_path(path).ok())
        .filter(|root| !root.is_empty())?;
    let prefix = format!("{root}/");
    // `None` when the glob stands as written.
    let rebased = |glob: &str| {
        let bare = glob.strip_prefix('/').unwrap_or(glob);
        bare.strip_prefix(&prefix)
            .filter(|rest| !rest.is_empty())
            .map(str::to_owned)
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
    let restated = pinned
        .include
        .iter()
        .map(|glob| glob.as_str().to_owned())
        .filter(|glob| rebased(glob.as_str()).is_some())
        .collect();
    Some((root, Rebased { globs, restated }))
}
