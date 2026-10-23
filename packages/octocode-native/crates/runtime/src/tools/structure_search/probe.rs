//! Probes of an empty listing: the skipped paths and relaxed name filters
//! that would list something, within one shared probe budget.

use super::*;
use crate::contracts::listing_roles::ListingRole;
use octocode_contract_types::ErrorCode;

/// What a walk left out of a listing, root-relative (a directory's with a
/// trailing `/` where noted): `.gitignore`d entries (`/`), entries the path
/// policy withheld, dot entries skipped without `hidden` (`/`), the
/// directories the default prune skipped, and the entries it could not read
/// (how many, and how many of them for want of permission).
#[derive(Clone, Debug, Default)]
pub(super) struct Uncovered {
    pub(super) ignored: Vec<String>,
    pub(super) withheld: crate::policy::discovery::Withheld,
    pub(super) hidden: Vec<String>,
    pub(super) pruned: Vec<String>,
    pub(super) unreadable: (u32, u32),
}

impl Uncovered {
    /// The distinct names of the default-pruned directories, sorted.
    pub(super) fn pruned_names(&self) -> Option<Vec<String>> {
        if self.pruned.is_empty() {
            return None;
        }
        let mut names = self
            .pruned
            .iter()
            .map(|dir| dir.rsplit('/').next().unwrap_or(dir).to_owned())
            .collect::<Vec<_>>();
        names.sort_unstable();
        names.dedup();
        Some(names)
    }

    /// Whether the walk skipped anything a relaxed walk could list.
    pub(super) fn saw_skips(&self) -> bool {
        !self.ignored.is_empty() || !self.hidden.is_empty() || !self.pruned.is_empty()
    }

    /// The causes of `walk` this walk skipped entries by, in [`Cause`] order.
    pub(super) fn skipped(&self, walk: Walk) -> Vec<Cause> {
        walk.skipping()
            .into_iter()
            .filter(|cause| match cause {
                Cause::Ignored => !self.ignored.is_empty(),
                Cause::Hidden => !self.hidden.is_empty(),
                Cause::DefaultExcluded => !self.pruned.is_empty(),
            })
            .collect()
    }

    /// The causes a lead must lift to reach what this walk skipped when no
    /// probe says where entries sit: [`Self::skipped`], plus the default
    /// prune when ignored or dot entries were skipped, since a pruned
    /// directory behind them (a `.gitignore`d `target/`) is never seen as
    /// pruned.
    pub(super) fn unproven(&self, walk: Walk) -> Vec<Cause> {
        let behind = !self.ignored.is_empty() || !self.hidden.is_empty();
        walk.skipping()
            .into_iter()
            .filter(|cause| match cause {
                Cause::Ignored => !self.ignored.is_empty(),
                Cause::Hidden => !self.hidden.is_empty(),
                Cause::DefaultExcluded => behind || !self.pruned.is_empty(),
            })
            .collect()
    }

    /// `causes` as a hint names them: the pruned directories by name, the
    /// rest by label.
    pub(super) fn named(&self, causes: &[Cause]) -> String {
        causes
            .iter()
            .map(|cause| match (cause, self.pruned_names()) {
                (Cause::DefaultExcluded, Some(names)) => names
                    .iter()
                    .map(|name| format!("{name}/"))
                    .collect::<Vec<_>>()
                    .join(", "),
                _ => cause.label().to_owned(),
            })
            .collect::<Vec<_>>()
            .join("; ")
    }
}

/// Entries one probe lists before it stops: enough to name the skip
/// reasons and to tell "some" from "none".
pub(super) const PROBE_HITS: usize = crate::tools::empty_walk::SAMPLE_FILES + 1;

/// Size of a directory's own listing (its entry table) past which a probe
/// leaves it unread: one such directory (`target/debug/deps`, ~255k
/// entries, ~8 MB) takes seconds to list, past any budget.
const PROBE_MAX_DIR_BYTES: u64 = 1024 * 1024;

/// The bound of an empty listing's relaxed probe walks: past `deadline`
/// they admit nothing more, and a directory too large to list in time is
/// left unread. Either makes the probe's finding a lower bound (`cut`).
pub(super) struct ProbeBound {
    pub(super) deadline: std::time::Instant,
    pub(super) cut: std::sync::atomic::AtomicBool,
}

impl ProbeBound {
    pub(super) fn new(budget: std::time::Duration) -> Self {
        Self {
            deadline: octocode_engine::scan_budget::walk_now() + budget,
            cut: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Whether a probe walk may admit `path`: not once its deadline passed,
    /// nor a directory whose listing is too large to read in time.
    pub(super) fn admits(
        &self,
        path: &std::path::Path,
        file_type: Option<std::fs::FileType>,
    ) -> bool {
        use std::sync::atomic::Ordering::Relaxed;
        let refused = octocode_engine::scan_budget::walk_now() >= self.deadline
            || (file_type.is_some_and(|kind| kind.is_dir())
                && std::fs::symlink_metadata(path)
                    .is_ok_and(|meta| meta.len() > PROBE_MAX_DIR_BYTES));
        if refused {
            self.cut.store(true, Relaxed);
        }
        !refused
    }

    /// Whether the probe left something unread.
    pub(super) fn cut(&self) -> bool {
        self.cut.load(std::sync::atomic::Ordering::Relaxed)
    }
}

/// An empty listing cannot prove absence in what its walk skipped: when the
/// same listing over some of it (`probe` re-walks with the given flags)
/// finds entries, the shared disclosure ([`crate::tools::leads::skipped`])
/// names the rule, entry or directory and leads to the listing with exactly
/// those causes lifted. A probe cut before it found anything proves nothing:
/// the lead lifts every cause the walk skipped by, and the warning says
/// absence there is unproven (the listing is partial).
pub(super) fn note_empty(
    out: &mut Value,
    query: &impl serde::Serialize,
    (root, paths): (&std::path::Path, &crate::policy::path::PathPolicy),
    seen: &Uncovered,
    walk: Walk,
    probe: impl FnMut(Walk) -> Result<Hits, ToolError>,
) -> Result<(), ToolError> {
    if out["status"] != "empty" || !seen.saw_skips() {
        return Ok(());
    }
    // Rules and entries are named like the rows: workspace-relative.
    let shown = |path: &std::path::Path| {
        paths
            .workspace_relative(path)
            .unwrap_or_else(|| path.to_string_lossy().into_owned())
    };
    let verdict = diagnose_shown(root, walk, &shown, probe)?;
    // A probe cut before it found anything proved nothing about what the
    // walk skipped: the lead walks everything this walk may have skipped.
    let unproven = seen.unproven(walk);
    let named = seen.named(&unproven);
    let Some(disclosure) = crate::tools::leads::skipped::disclose(
        &verdict,
        ["entry", "entries"],
        crate::tools::leads::skipped::Unchecked {
            causes: &unproven,
            named: &named,
        },
    ) else {
        return Ok(());
    };
    let lead = crate::tools::leads::skipped::lead(
        crate::id::ToolId::StructureSearch,
        query,
        &disclosure.causes,
    );
    crate::tools::leads::skipped::attach(out, &disclosure, lead);
    Ok(())
}

/// A non-empty first page whose walk left out entries leads to the same
/// listing with the causes it skipped by lifted, in one call: every
/// ignored or dot entry its `skippedPaths` names is reachable. Default-pruned
/// directories (dependencies, build output) are lifted only for a `filtered`
/// listing (one looking for names, types or metadata), where more of what it
/// looks for may sit.
/// An empty page gets its lead from [`note_empty`], which proves the
/// skipped paths hold entries.
pub(super) fn lead_to_skipped(
    out: &mut Value,
    query: &impl serde::Serialize,
    seen: &Uncovered,
    walk: Walk,
    filtered: bool,
) {
    if out["status"] == "empty" || out["next"].get("includeSkipped").is_some() {
        return;
    }
    let mut causes = seen.skipped(walk);
    if !filtered {
        causes.retain(|cause| *cause != Cause::DefaultExcluded);
    }
    if causes.is_empty() {
        return;
    }
    out["next"]["includeSkipped"] =
        crate::tools::leads::skipped::lead(crate::id::ToolId::StructureSearch, query, &causes);
}

/// Entries the one probe walk of an empty listing tags at most: enough to
/// tell the skip causes that hold matches apart, within the probe budget.
pub(super) const TAGGED_PROBE_ENTRIES: usize = 256;

/// [`note_empty`] for an empty [`Listing`]: one walk of `query`'s filters
/// with every skip its walk took lifted (`walk` of the relaxed query, with
/// its entry cap, within the probe budget), each entry tagged with the
/// causes that hid it ([`Tagged::listed`]). Every relaxed walk the
/// diagnosis asks about is answered from the tags, so an empty listing walks
/// once more, not once per subset of skip causes.
pub(super) fn probe_listing<Q: Listing>(
    out: &mut Value,
    query: &Q,
    at: (&std::path::Path, &crate::policy::path::PathPolicy),
    seen: &Uncovered,
    walk: impl Fn(&Q, usize, Walking<'_>) -> Result<Walked, ToolError>,
) -> Result<(), ToolError> {
    let shown = query.walk();
    let mut tagged = None::<Tagged>;
    note_empty(out, query, at, seen, shown, |relaxed| {
        if tagged.is_none() {
            let bound = ProbeBound::new(crate::tools::empty_walk::probe_budget());
            let lifted = walk(
                &query.relaxed(shown.lifting(&shown.skipping())),
                TAGGED_PROBE_ENTRIES,
                Walking::Probe(&bound),
            )?;
            let cut = lifted.meta.was_capped || bound.cut();
            tagged = Some(Tagged::listed(
                at.0,
                lifted
                    .rows
                    .iter()
                    .map(|row| (row.source.clone(), row.kind == EntryKind::Directory)),
                cut,
            ));
        }
        Ok(tagged
            .as_ref()
            .map_or_else(Hits::default, |tagged| tagged.find(relaxed, Some(shown))))
    })
}

/// An empty listing that skipped nothing worth lifting (no
/// `includeSkipped`) and filtered by name: when the same listing without its
/// name filters (`walk` of the relaxed query, within the probe budget) finds
/// entries below `root` (the listed directory itself is no find), lead to
/// it (`next.broadenSearch`) and say how many it lists, so a name miss gets
/// a runnable next step, never prose alone. `query` names every default the
/// relaxed listing must keep (the walked depth).
pub(super) fn broaden_empty<Q>(
    out: &mut Value,
    query: &Q,
    root: &std::path::Path,
    walk: impl FnOnce(&Q, usize, Walking<'_>) -> Result<Walked, ToolError>,
) -> Result<(), ToolError>
where
    Q: serde::Serialize + serde::de::DeserializeOwned,
{
    if out["status"] != "empty" || out["next"].get("includeSkipped").is_some() {
        return Ok(());
    }
    let row = serde_json::to_value(query).unwrap_or_else(|_| json!({}));
    // The fields that keep entries by name or path (contract `x-listing`):
    // an empty listing they alone emptied is broadened by dropping them.
    let dropped = crate::contracts::listing_roles::structure_search::FIELDS
        .iter()
        .filter(|(field, role)| {
            *role == ListingRole::Name && crate::response::output::has_value(&row, field)
        })
        .map(|(field, _)| *field)
        .collect::<Vec<_>>();
    if dropped.is_empty() {
        return Ok(());
    }
    let mut changes = json!({"page": 1, "snapshot": null, "walkOffset": null, "scanOffset": null});
    for field in &dropped {
        changes[*field] = Value::Null;
    }
    let lead = continuation(query, changes);
    let Ok(relaxed) = serde_json::from_value::<Q>(lead["query"]["queries"][0].clone()) else {
        return Ok(());
    };
    let bound = ProbeBound::new(crate::tools::empty_walk::probe_budget());
    let found = walk(&relaxed, PROBE_HITS, Walking::Probe(&bound))?;
    let count = found.rows.iter().filter(|row| row.source != root).count();
    if count == 0 {
        return Ok(());
    }
    let cut = found.meta.was_capped || bound.cut();
    let hint = format!(
        "No entry matches {}; {}{count} {} without {}: the lead lists {}.",
        dropped.join("/"),
        if cut { "at least " } else { "" },
        if count == 1 {
            "entry matches"
        } else {
            "entries match"
        },
        if dropped.len() == 1 { "it" } else { "them" },
        if count == 1 { "it" } else { "them" },
    );
    out["next"]["broadenSearch"] = lead;
    crate::tools::leads::push_hint(out, hint);
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
            || self
                .0
                .lists(path, &crate::tools::display_name(path), false, true)
    }
}
