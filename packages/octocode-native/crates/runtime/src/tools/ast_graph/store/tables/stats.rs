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
    /// Why call sites stayed unlinked; `internalRecall` counts only sites
    /// that could plausibly target code in this repository.
    pub unresolved_by_reason: BTreeMap<&'static str, u64>,
    /// Per language: `[linked, unlinked sites naming an in-repo declaration]`.
    #[serde(skip)]
    pub by_language: BTreeMap<String, [u64; 2]>,
}

/// Unlinked reasons that still name a declaration in this repository.
pub(super) const INTERNAL_REASONS: &[&str] =
    &["ambiguous", "member-unknown-receiver-in-repo", "other"];

impl CallStats {
    /// Internal recall per language (see [`CallStats::internal_recall`]).
    pub(crate) fn recall_by_language(&self) -> BTreeMap<String, f64> {
        self.by_language
            .iter()
            .map(|(language, [linked, missed])| {
                let total = linked + missed;
                let recall = if total == 0 {
                    1.0
                } else {
                    *linked as f64 / total as f64
                };
                (language.clone(), (recall * 1000.0).round() / 1000.0)
            })
            .collect()
    }

    /// Linked / (linked + unlinked sites that name an in-repo declaration).
    pub(crate) fn internal_recall(&self) -> f64 {
        let internal = self.linked
            + INTERNAL_REASONS
                .iter()
                .map(|reason| self.unresolved_by_reason.get(reason).copied().unwrap_or(0))
                .sum::<u64>();
        if internal == 0 {
            1.0
        } else {
            self.linked as f64 / internal as f64
        }
    }
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
