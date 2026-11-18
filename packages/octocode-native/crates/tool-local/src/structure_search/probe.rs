//! What an empty listing offers instead of walking again: the listing of
//! the paths its walk skipped, and the same listing without the name filters
//! or the depth bound that may have emptied it. Each is a call built from the
//! query alone; nothing more is walked.
use super::listing::Listing;
use octocode_contract_types::StructureSearchQuery;
use octocode_runtime_base::response::result::{Continuation, ToolError};
use octocode_tools_shared::empty_walk::{Cause, Walk};
use serde_json::{Value, json};

use octocode_contract_types::ErrorCode;
use octocode_runtime_base::contracts::listing_roles::ListingRole;

mod uncovered;

pub(super) use uncovered::Uncovered;

/// An empty listing cannot prove absence in what its walk skipped: the
/// shared disclosure ([`octocode_tools_shared::leads::skipped::unsearched`])
/// says those paths were not listed and leads to the listing with every cause
/// the walk skipped by lifted.
pub(super) fn note_empty(
    out: &mut Value,
    query: &impl serde::Serialize,
    seen: &Uncovered,
    walk: Walk,
) {
    if out["status"] != "empty" || !seen.saw_skips() {
        return;
    }
    let causes = seen.unproven(walk);
    let Some(disclosure) =
        octocode_tools_shared::leads::skipped::unsearched(&causes, &seen.named(&causes))
    else {
        return;
    };
    let lead = octocode_tools_shared::leads::skipped::lead(
        octocode_runtime_base::id::ToolId::StructureSearch,
        query,
        &disclosure.causes,
    );
    octocode_tools_shared::leads::skipped::attach(out, &disclosure, lead);
}

/// A non-empty first page whose walk left out entries leads to the same
/// listing with the causes it skipped by lifted, in one call: every
/// ignored or dot entry its `skippedPaths` names is reachable. Default-pruned
/// directories (dependencies, build output) are lifted only for a `filtered`
/// listing (one looking for names, types or metadata), where more of what it
/// looks for may sit. An empty page gets its lead from [`note_empty`].
pub(super) fn lead_to_skipped(
    out: &mut Value,
    query: &impl serde::Serialize,
    seen: &Uncovered,
    walk: Walk,
    filtered: bool,
) {
    if out["status"] == "empty" || out["calls"].get("includeSkipped").is_some() {
        return;
    }
    let mut causes = seen.skipped(walk);
    if !filtered {
        causes.retain(|cause| *cause != Cause::DefaultExcluded);
    }
    if causes.is_empty() {
        return;
    }
    out["calls"]["includeSkipped"] = octocode_tools_shared::leads::skipped::lead(
        octocode_runtime_base::id::ToolId::StructureSearch,
        query,
        &causes,
    );
}

/// An empty listing that skipped nothing worth lifting (no `includeSkipped`)
/// and filtered by name, else by metadata, leads to the same listing without
/// those filters (`next.broadenSearch`), so a name miss gets a runnable next
/// step, never prose alone. `query` names every default the relaxed listing
/// must keep (the walked depth).
pub(super) fn broaden_empty<Q: Listing>(out: &mut Value, query: &Q) -> Result<(), ToolError> {
    if out["status"] != "empty"
        || out["calls"].get("includeSkipped").is_some()
        || out["calls"].get("broadenSearch").is_some()
    {
        return Ok(());
    }
    let row = serde_json::to_value(query).unwrap_or_else(|_| json!({}));
    // The fields that keep entries by name or path (contract `x-listing`):
    // an empty listing they alone emptied is broadened by dropping them; one
    // with none, by dropping the fields that keep entries by metadata.
    let set = |wanted: ListingRole| {
        octocode_runtime_base::contracts::listing_roles::structure_search::FIELDS
            .iter()
            .filter(|(field, role)| {
                *role == wanted && octocode_runtime_base::response::output::has_value(&row, field)
            })
            .map(|(field, _)| *field)
            .collect::<Vec<_>>()
    };
    let dropped = Some(set(ListingRole::Name))
        .filter(|names| !names.is_empty())
        .unwrap_or_else(|| set(ListingRole::Metadata));
    if dropped.is_empty() {
        return Ok(());
    }
    let mut changes = json!({"page": 1, "snapshot": null, "walkOffset": null, "scanOffset": null});
    for field in &dropped {
        changes[*field] = Value::Null;
    }
    let relaxed = octocode_runtime_base::response::result::edited_query(query, &changes).map_err(
        |error| {
            ToolError::new(
                ErrorCode::ExecutionFailed,
                format!("the broadened listing is no valid query: {error}"),
            )
        },
    )?;
    let named: StructureSearchQuery = relaxed.into();
    out["calls"]["broadenSearch"] = Continuation::related_query(named).build();
    octocode_runtime_base::response::result::add_hints(
        out,
        [format!(
            "No entry matches {}; the related[] structureSearch request lists without {}.",
            dropped.join("/"),
            if dropped.len() == 1 { "it" } else { "them" },
        )],
    );
    Ok(())
}

/// An empty filtered listing a depth bound may have emptied (`tree` lists one
/// level unless told otherwise) leads to the same filters without the bound
/// (`next.broadenSearch`), since no name filter is at fault. An empty listing
/// that skipped something worth lifting has its lead.
pub(super) fn deepen_empty<Q: Listing>(out: &mut Value, query: &Q) -> Result<(), ToolError> {
    if out["status"] != "empty" || out["calls"].get("includeSkipped").is_some() || !query.filtered()
    {
        return Ok(());
    }
    let mut row = octocode_runtime_base::response::result::query_row(
        query,
        &["maxDepth", "page", "snapshot", "scanOffset", "walkOffset"],
    );
    let bound = octocode_runtime_base::response::result::query_value(query)["maxDepth"].clone();
    if !bound.is_u64() {
        return Ok(());
    }
    row["operation"] = json!("files");
    row["page"] = json!(1);
    let deeper: octocode_contract_types::StructureSearchQueryFiles = serde_json::from_value(row)
        .map_err(|error| {
            ToolError::new(
                ErrorCode::ExecutionFailed,
                format!("the deepened listing is no valid query: {error}"),
            )
        })?;
    let named: StructureSearchQuery = deeper.into();
    out["calls"]["broadenSearch"] = Continuation::related_query(named).build();
    octocode_runtime_base::response::result::add_hints(
        out,
        [format!(
            "No entry matches within {bound} level(s) of path; the related[] structureSearch request lists every depth."
        )],
    );
    Ok(())
}

/// Whether a filtered listing could have listed `path` had the policy
/// admitted it: a directory may hold matches; a file must pass the name
/// filters (include globs, extensions, basename regex) as the walk applies
/// them, in the walk's one glob dialect.
pub(super) struct FilterProbe(octocode_engine::search::fs_query::NameFilter);

impl FilterProbe {
    /// The probe of a walk rooted at `root`. A glob or regex that does not
    /// compile fails the call (`invalidGlob`, `invalidPattern`), as the
    /// walk itself does: a filter is never silently dropped.
    pub(super) fn new(
        root: &std::path::Path,
        include: &[String],
        extensions: &[String],
        name_regex: Option<&str>,
    ) -> Result<Self, ToolError> {
        octocode_engine::search::fs_query::NameFilter::new(root, include, extensions, name_regex)
            .map(Self)
            .map_err(|error| {
                let code = match error.kind() {
                    octocode_engine::error::ErrorKind::InvalidPattern => ErrorCode::InvalidPattern,
                    _ => ErrorCode::InvalidGlob,
                };
                ToolError::new(code, error.to_string())
            })
    }

    /// A directory, or a link to one (the walk follows none), may hold
    /// matches.
    pub(super) fn could_list(&self, path: &std::path::Path) -> bool {
        path.is_dir()
            || self.0.lists(
                path,
                &octocode_tools_shared::display_name(path),
                false,
                true,
            )
    }
}
