use super::names::{NameEntry, NameFilter, row_names};
use octocode_contract_types::AstSearchQuerySymbols;
use octocode_runtime_base::id::ToolId;
use serde_json::{Value, json};

/// The declarations an exact `symbolFilter.terms` entry left out because they only
/// contain it (`prune` equals `Prune`, so `prunes` is not listed): their
/// count and distinct names, so a prefix or substring search is not
/// answered by the one exact name alone. The notice names every one; the
/// `broadenSearch` lead runs them.
#[derive(Clone, serde::Serialize)]
pub(super) struct Suppressed {
    /// The exact entries that suppressed them.
    exact: Vec<String>,
    /// Declarations left out.
    count: usize,
    /// Their distinct names, sorted: `broadenSearch` lists them.
    names: Vec<String>,
}

impl Suppressed {
    pub(super) fn of<'a>(
        filter: &NameFilter,
        rows: impl Iterator<Item = &'a Value>,
    ) -> Option<Self> {
        let mut count = 0;
        let names = rows
            .inspect(|_| count += 1)
            .filter_map(|row| row["name"].as_str().map(str::to_owned))
            .collect::<std::collections::BTreeSet<_>>();
        (count > 0).then(|| Self {
            exact: filter
                .entries
                .iter()
                .filter(|entry| entry.exact)
                .map(|entry| entry.text.clone())
                .collect(),
            count,
            names: names.into_iter().collect(),
        })
    }

    /// Every suppressed name, so none is lost: passing them lists them.
    pub(super) fn notice(&self) -> String {
        let one = self.count == 1;
        format!(
            "symbolName {} equals a declaration, so {} declaration{} that only contain{} it {} not listed: {}. Pass {} to list {}.",
            self.exact.join(", "),
            self.count,
            if one { "" } else { "s" },
            if one { "s" } else { "" },
            if one { "is" } else { "are" },
            self.names.join(", "),
            if self.names.len() == 1 {
                "this name"
            } else {
                "these names"
            },
            if one { "it" } else { "them" },
        )
    }

    /// Broaden the exact name filter to include its substring family.
    pub(super) fn lead(q: &AstSearchQuerySymbols) -> Value {
        let mut row = octocode_runtime_base::response::result::query_row(q, &[]);
        row["symbolFilter"]["mode"] = json!("substring");
        octocode_runtime_base::response::result::restart_query(ToolId::AstSearch, &row)
    }
}

/// The exact `symbolFilter.terms` entries that `symbolKinds` leaves without a single
/// declaration although other kinds declare them: the kinds that do and how
/// many declarations each holds, so an empty or substring-free answer is not
/// read as absence. The `broadenSearch` lead adds those kinds.
#[derive(Clone, serde::Serialize)]
pub(super) struct KindsLeftOut {
    /// The entries, in query order.
    names: Vec<String>,
    /// Declarations of those names `symbolKinds` left out.
    count: usize,
    /// Their distinct kinds, sorted.
    kinds: Vec<String>,
}

impl KindsLeftOut {
    pub(super) fn of<'a>(
        filter: &NameFilter,
        kept: impl Iterator<Item = &'a Value> + Clone,
        off_kind: &[Value],
    ) -> Option<Self> {
        let declares =
            |entry: &NameEntry, row: &Value| row_names(row).any(|name| entry.equals(name));
        let names = filter
            .entries
            .iter()
            .filter(|entry| entry.exact && !kept.clone().any(|row| declares(entry, row)))
            .collect::<Vec<_>>();
        let left_out = off_kind
            .iter()
            .filter(|row| names.iter().any(|entry| declares(entry, row)))
            .collect::<Vec<_>>();
        (!left_out.is_empty()).then(|| Self {
            names: names
                .iter()
                .filter(|entry| left_out.iter().any(|row| declares(entry, row)))
                .map(|entry| entry.text.clone())
                .collect(),
            count: left_out.len(),
            kinds: left_out
                .iter()
                .filter_map(|row| row["kind"].as_str().map(str::to_owned))
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect(),
        })
    }

    pub(super) fn notice(&self, q: &AstSearchQuerySymbols) -> String {
        let one = self.count == 1;
        let kinds = self.kinds.join(", ");
        format!(
            "kinds {} leaves out every declaration named {}: {} of kind{} {kinds}. Add {kinds} to kinds to list {}.",
            q.symbol_kinds
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", "),
            self.names.join(", "),
            self.count,
            if self.kinds.len() == 1 { "" } else { "s" },
            if one { "it" } else { "them" },
        )
    }

    /// The same query with the left-out kinds added.
    pub(super) fn lead(&self, q: &AstSearchQuerySymbols) -> Value {
        let mut nq = octocode_runtime_base::response::result::query_row(q, &[]);
        let mut kinds = q
            .symbol_kinds
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        kinds.extend(
            self.kinds
                .iter()
                .filter(|kind| {
                    !q.symbol_kinds
                        .iter()
                        .any(|selected| selected.as_str() == kind.as_str())
                })
                .cloned(),
        );
        nq["symbolKinds"] = json!(kinds);
        octocode_runtime_base::response::result::restart_query(ToolId::AstSearch, &nq)
    }
}

#[cfg(test)]
mod tests;
