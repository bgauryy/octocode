//! What an empty page offers instead of searching again: the call that
//! searches the paths its walk skipped, and the other readings of a literal
//! miss ([`alternates`]). Nothing here reads a file: an empty answer states
//! what it covered, and widening it is the caller's next call, as ripgrep
//! leaves `-u` and `-i` to its user.
use octocode_contract_types::LocalSearchQuery;
use octocode_runtime_base::id::ToolId;
use octocode_tools_shared::empty_walk::Cause;
use octocode_tools_shared::leads::skipped::{lead, walks_all_skipped};
use serde_json::{Value, json};

mod alternates;
pub(super) use alternates::Alternates;

/// Whether a walk's ledger names nothing: no skipped path a flag lifts, nothing
/// the path policy withheld. An empty page then has nothing to add about
/// skipped paths.
pub(super) fn names_nothing(skipped: &octocode_runtime_base::policy::discovery::WalkSkips) -> bool {
    skipped.counts().is_empty() && skipped.withheld.total() == 0
}

/// What a page's walk skipped, by cause, for the empty page's word on it.
pub(super) struct Skipped<'a> {
    pub(super) walk: &'a octocode_runtime_base::policy::discovery::WalkSkips,
    /// The causes it skipped paths by: what a lead over them lifts.
    pub(super) causes: Vec<Cause>,
}

/// What an empty page said of the paths its walk skipped.
#[derive(Default)]
pub(super) struct Unsearched {
    /// Paths a flag lifts were skipped and not searched: they may hold a match.
    pub(super) may_hold_matches: bool,
}

/// An empty page's word on the paths its walk skipped: they were not searched,
/// so absence there is unproven, and `next.includeSkipped` searches them with
/// exactly their causes lifted. No file is read for it.
pub(super) fn disclose_skipped(
    query: &LocalSearchQuery,
    skipped: &Skipped<'_>,
    scoped: Vec<Value>,
    (next, hints): (&mut Option<Value>, &mut Vec<String>),
) -> Unsearched {
    let named = skipped.walk.counts();
    if skipped.causes.is_empty() || named.is_empty() {
        return Unsearched::default();
    }
    let aimed = !scoped.is_empty();
    if let Some(map) = next.get_or_insert_with(|| json!({})).as_object_mut() {
        // Each search of one skipped directory first, then the one over
        // everything skipped.
        for (index, lead) in scoped.into_iter().enumerate() {
            map.insert(
                octocode_runtime_base::id::numbered("searchSkipped", index),
                lead,
            );
        }
        map.insert(
            "includeSkipped".into(),
            lead(ToolId::LocalSearch, query, &skipped.causes),
        );
    }
    let aimed = if aimed {
        "The related[] searches at one skippedPaths path search only there. "
    } else {
        ""
    };
    hints.insert(
        0,
        format!(
            "Skipped paths ({named}) were not searched, so absence there is unproven. {aimed}{}",
            walks_all_skipped(&skipped.causes)
        ),
    );
    Unsearched {
        may_hold_matches: true,
    }
}
