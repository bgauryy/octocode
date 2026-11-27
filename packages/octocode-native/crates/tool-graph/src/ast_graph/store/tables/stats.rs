//! Call-link statistics and the reason a call stayed unlinked.
use super::*;

/// Call-linking tallies surfaced in the manifest.
#[derive(Clone, Debug, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CallStats {
    pub sites: u64,
    /// Stated as `linkage.linked`.
    #[serde(skip)]
    pub linked: u64,
    pub unresolved: u64,
    pub by_resolution: BTreeMap<String, u64>,
    /// Why call sites stayed unlinked; linkage eligibility selects reasons
    /// that could plausibly target code in this repository.
    pub unresolved_by_reason: BTreeMap<&'static str, u64>,
    /// Per language: `[linked, eligible unresolved, excluded unresolved]`.
    #[serde(skip)]
    pub by_language: BTreeMap<String, [u64; 3]>,
}

/// Unlinked reasons that still name a declaration in this repository.
pub(in crate::ast_graph::store) const INTERNAL_REASONS: &[&str] =
    &["ambiguous", "member-unknown-receiver-in-repo", "other"];

impl CallStats {
    pub(crate) fn linkage_by_language(&self) -> BTreeMap<String, serde_json::Value> {
        self.by_language
            .iter()
            .map(|(language, [linked, missed, excluded])| {
                (language.clone(), linkage(*linked, *missed, *excluded))
            })
            .collect()
    }

    pub(crate) fn linkage(&self) -> serde_json::Value {
        let missed = INTERNAL_REASONS
            .iter()
            .map(|reason| self.unresolved_by_reason.get(reason).copied().unwrap_or(0))
            .sum::<u64>();
        linkage(self.linked, missed, self.unresolved.saturating_sub(missed))
    }
}

/// The call sites of one population, partitioned: `eligible` (the ratio's
/// denominator) is `linked` plus the unlinked sites whose name still has a
/// declaration in this repository ([`INTERNAL_REASONS`]); `excluded` is the
/// unlinked rest (external packages, names declared nowhere in it), outside
/// the ratio. Every call site is in exactly one of `eligible`, `excluded`.
fn linkage(linked: u64, missed: u64, excluded: u64) -> serde_json::Value {
    let eligible = linked + missed;
    // A share, not a measurement: 4 significant digits.
    let ratio =
        (eligible != 0).then(|| super::super::detect::significant(linked as f64 / eligible as f64));
    serde_json::json!({"linked":linked, "eligible":eligible, "excluded":excluded,
        "sites":eligible + excluded, "ratio":ratio, "population":"syntax-call-sites"})
}

/// Classifies an unlinked call site for `CallStats::unresolved_by_reason`;
/// `in_repo` counts the site's acceptable declarations of `name` in the
/// caller's language family ([`Linker::named`]).
pub(super) fn unlinked_reason(
    bindings: &FileBindings,
    qualifier: &str,
    name: &str,
    in_repo: usize,
) -> &'static str {
    let receiver = matches!(qualifier, "this" | "self" | "Self" | "cls" | "super");
    let head = first_segment(qualifier);
    if (qualifier.is_empty() && bindings.external.contains(name))
        || (!qualifier.is_empty() && !receiver && bindings.external.contains(head))
    {
        "external-package"
    } else if (qualifier.is_empty() && bindings.unknown.contains(name))
        || (!qualifier.is_empty() && !receiver && bindings.unknown.contains(head))
    {
        // Bound by an import that did not link: nothing says what it is.
        "other"
    } else if in_repo == 0 {
        // std/builtins/macros/methods of foreign types: nothing to link to.
        "not-declared-in-repo"
    } else if !qualifier.is_empty()
        && !receiver
        && head.starts_with(|c: char| c.is_ascii_uppercase())
    {
        "type-qualified-unmatched"
    } else if !qualifier.is_empty() && !receiver {
        "member-unknown-receiver-in-repo"
    } else if in_repo > 1 {
        "ambiguous"
    } else {
        "other"
    }
}
