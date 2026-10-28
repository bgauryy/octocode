//! Call-link statistics and the reason a call stayed unlinked.
use super::*;

/// Call-linking tallies surfaced in the manifest.
#[derive(Clone, Debug, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CallStats {
    pub sites: u64,
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
pub(super) const INTERNAL_REASONS: &[&str] =
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

fn linkage(linked: u64, missed: u64, excluded: u64) -> serde_json::Value {
    let eligible = linked + missed;
    let ratio = (eligible != 0).then(|| linked as f64 / eligible as f64);
    serde_json::json!({"linked":linked, "eligible":eligible, "excluded":excluded,
        "ratio":ratio, "population":"syntax-call-sites"})
}

/// Classifies an unlinked call site for `CallStats::unresolved_by_reason`.
pub(super) fn unlinked_reason(
    bindings: &FileBindings,
    qualifier: &str,
    name: &str,
    global_names: &HashMap<&str, Vec<u32>>,
    callable: &dyn Fn(u32) -> bool,
) -> &'static str {
    let receiver = matches!(qualifier, "this" | "self" | "Self" | "cls" | "super");
    let head = first_segment(qualifier);
    let in_repo = global_names
        .get(name)
        .map_or(0, |ids| ids.iter().filter(|id| callable(**id)).count());
    if (qualifier.is_empty() && bindings.external.contains(name))
        || (!qualifier.is_empty() && !receiver && bindings.external.contains(head))
    {
        "external-package"
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
