use super::ast_search_query_symbols;
use super::names::{NameEntry, NameFilter, row_names};
use crate::id::ToolId;
use octocode_contract_types::AstSearchQuerySymbols;
use serde_json::{Value, json};

/// The declarations an exact `symbolName` entry left out because they only
/// contain it (`prune` equals `Prune`, so `prunes` is not listed): their
/// count and distinct names, so a prefix or substring search is not
/// answered by the one exact name alone. The notice names every one; the
/// `broadenSearch` lead runs them.
#[derive(Clone, serde::Deserialize, serde::Serialize)]
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

    /// The same query over the entries plus every suppressed name, each an
    /// exact entry now: the substring family the exact entries cut, one
    /// lead per schema item bound of names.
    pub(super) fn leads(&self, q: &AstSearchQuerySymbols) -> Vec<Value> {
        let entries = ast_search_query_symbols::name_filter(q)
            .map(|filter| filter.entries.into_iter().map(|entry| entry.text))
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        let names = entries
            .iter()
            .cloned()
            .chain(
                self.names
                    .iter()
                    .filter(|name| !entries.contains(name))
                    .cloned(),
            )
            .collect::<Vec<_>>();
        names
            .chunks(crate::contracts::query_limits::ast_search::symbols::SYMBOL_NAME_MAX_ITEMS)
            .map(|chunk| {
                let mut nq = crate::response::result::query_row(q, &["snapshot", "page"]);
                nq["symbolName"] = json!(chunk);
                crate::response::result::Continuation::new(ToolId::AstSearch, nq).build()
            })
            .collect()
    }
}

/// The exact `symbolName` entries that `kinds` leaves without a single
/// declaration although other kinds declare them: the kinds that do and how
/// many declarations each holds, so an empty or substring-free answer is not
/// read as absence. The `broadenSearch` lead adds those kinds.
#[derive(Clone, serde::Deserialize, serde::Serialize)]
pub(super) struct KindsLeftOut {
    /// The entries, in query order.
    names: Vec<String>,
    /// Declarations of those names `kinds` left out.
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
            q.kinds.join(", "),
            self.names.join(", "),
            self.count,
            if self.kinds.len() == 1 { "" } else { "s" },
            if one { "it" } else { "them" },
        )
    }

    /// The same query with the left-out kinds added.
    pub(super) fn lead(&self, q: &AstSearchQuerySymbols) -> Value {
        let mut nq = crate::response::result::query_row(q, &["snapshot", "page"]);
        let mut kinds = q.kinds.clone();
        kinds.extend(
            self.kinds
                .iter()
                .filter(|kind| !q.kinds.contains(kind))
                .cloned(),
        );
        nq["kinds"] = json!(kinds);
        crate::response::result::Continuation::new(ToolId::AstSearch, nq).build()
    }
}
