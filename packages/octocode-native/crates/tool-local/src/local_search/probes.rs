//! What an empty page offers instead of searching again: the call that
//! searches the paths its walk skipped, and the other readings of a literal
//! miss ([`alternates`]). Nothing here reads a file: an empty answer states
//! what it covered, and widening it is the caller's next call, as ripgrep
//! leaves `-u` and `-i` to its user.
use octocode_contract_types::LocalSearchQuery;
use octocode_runtime_base::id::ToolId;
use octocode_tools_shared::empty_walk::Cause;
use octocode_tools_shared::leads::skipped::{lead, unsearched};
use serde_json::{Value, json};

mod alternates;
pub(super) use alternates::Alternates;

/// Whether a walk's ledger names nothing: no skipped path a flag lifts, nothing
/// the path policy withheld. An empty page then has nothing to add about
/// skipped paths.
pub(super) fn names_nothing(skipped: &octocode_runtime_base::policy::discovery::WalkSkips) -> bool {
    skipped.counts().is_empty() && skipped.withheld.total() == 0
}

/// An empty page's one recovery: the first cause that explains the miss
/// states one sentence and the one call that applies its fix. Causes are
/// offered in order, the proven ones (hits a filter removed, a file never
/// read) before the possible ones (skipped paths, another reading of the
/// term), so the page never lists every way it could have gone wrong.
#[derive(Default)]
pub(super) struct Recovery {
    pub(super) hint: Option<String>,
    /// The call, by its lead name.
    pub(super) call: Option<(&'static str, Value)>,
}

impl Recovery {
    /// States `hint` and `call` when no earlier cause did.
    pub(super) fn offer(&mut self, hint: String, call: Option<(&'static str, Value)>) {
        if self.hint.is_none() {
            self.hint = Some(hint);
            self.call = call;
        }
    }

    /// Writes the recovery on the row: its hint, and its call in `next`
    /// beside the continuations already there.
    pub(super) fn state(self, hints: &mut Vec<String>, next: &mut Option<Value>) {
        hints.extend(self.hint);
        if let Some((name, call)) = self.call
            && let Some(map) = next.get_or_insert_with(|| json!({})).as_object_mut()
        {
            map.insert(name.to_owned(), call);
        }
    }
}

/// An empty page's word on the paths its walk skipped by `causes` (a flag
/// lifts each): `skippedPaths` names them, they were not searched, and the
/// one call searches them too with exactly those causes lifted. No file is
/// read for it. `None` when the walk skipped nothing a flag lifts.
pub(super) fn skipped_recovery(
    query: &LocalSearchQuery,
    walk: &octocode_runtime_base::policy::discovery::WalkSkips,
    causes: &[Cause],
) -> Option<(String, (&'static str, Value))> {
    if causes.is_empty() || walk.counts().is_empty() {
        return None;
    }
    let disclosure = unsearched(causes)?;
    Some((
        disclosure.text,
        ("includeSkipped", lead(ToolId::LocalSearch, query, causes)),
    ))
}
