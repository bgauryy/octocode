//! What a page discloses of the paths its walk skipped.

use super::*;

impl Found<'_> {
    /// The causes the walk skipped paths by, so the lead over them lifts
    /// each in one call: a dot directory the default prune also names
    /// (`.github`, `.cache`), or a dot file the default excludes match, sits
    /// behind both.
    pub(super) fn skipped_causes(&self) -> Vec<Cause> {
        octocode_tools_shared::leads::skipped::skipped_causes(self.skipped)
    }

    /// The ignored, default-excluded and hidden paths on the first page, one
    /// counted entry per cause and rule (the `includeSkipped` lead reaches
    /// each); policy-withheld names stay hidden, counted by reason at the
    /// walked scope. An empty page that discloses them names them per
    /// directory at the top of its walk ([`Self::discloses_skipped`]), so
    /// its leads can search one of them.
    pub(super) fn skipped_paths(&self) -> Result<Option<SkippedPaths>, ToolError> {
        if self.discloses_skipped() {
            return octocode_tools_shared::leads::skipped::scoped_ledger(
                self.root,
                self.skipped,
                Default::default(),
                false,
            );
        }
        let walked = octocode_tools_shared::leads::skipped::ledger_entries(
            self.root,
            self.skipped,
            Default::default(),
        );
        octocode_tools_shared::leads::skipped::skipped_paths(walked, false, !self.walk_complete)
    }

    /// Whether this page is the empty first page of a whole directory walk
    /// that skipped paths a flag lifts: it says they were not searched and
    /// offers the searches over them.
    pub(super) fn discloses_skipped(&self) -> bool {
        self.empty
            && self.walk_complete
            && self.first_page()
            && !self.root.is_file()
            && !names_nothing(self.skipped)
    }
}

/// Scoped searches an empty page offers ahead of the one over everything
/// skipped.
const SCOPED_LEADS: usize = 2;

/// The searches over single directories of `skipped` (an empty page's
/// [`Found::skipped_paths`]): `query` at an entry's path with only the flags
/// its cause needs, at most [`SCOPED_LEADS`]. A directory the default prune
/// names (`node_modules/`, `target/`: dependency and build output, where a
/// name absent from the sources most often sits) comes first, then entries
/// of one path, then the rest, each in the listing's order. An entry at the
/// walk's root, or one no flag lifts, has none.
pub(super) fn scoped_leads(
    query: &LocalSearchQuery,
    root: &std::path::Path,
    skipped: &SkippedPaths,
) -> Result<Vec<Value>, ToolError> {
    use octocode_contract_types::SkippedPathCause as Kind;
    let root = root.to_string_lossy();
    let rank = |entry: &octocode_contract_types::SkippedPath| {
        let pruned = matches!(
            entry.cause,
            Kind::IgnoredDefaultExcluded | Kind::DefaultExcluded
        ) && entry
            .reason
            .as_deref()
            .is_some_and(|reason| reason.ends_with('/'));
        match (pruned, entry.count.unwrap_or(1) == 1) {
            (true, true) => 0,
            (true, false) | (false, true) => 1,
            (false, false) => 2,
        }
    };
    // A lead must run as served: a path the output gate redacts cannot.
    let security = octocode_runtime_base::security::ContentSecurity::new();
    let mut scoped = skipped
        .entries
        .iter()
        .filter(|entry| entry.path.as_str() != root)
        .filter(|entry| {
            security.sanitize_text(entry.path.as_str(), None).content == entry.path.as_str()
        })
        .filter_map(|entry| {
            let mut causes = match entry.cause {
                Kind::Ignored => vec![Cause::Ignored],
                Kind::Hidden => vec![Cause::Hidden],
                Kind::DefaultExcluded => vec![Cause::DefaultExcluded],
                Kind::IgnoredDefaultExcluded => vec![Cause::Ignored, Cause::DefaultExcluded],
                _ => return None,
            };
            // A dot entry is hidden too, whatever skipped it first.
            if !causes.contains(&Cause::Hidden)
                && std::path::Path::new(entry.path.as_str())
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with('.'))
            {
                causes.push(Cause::Hidden);
            }
            Some((rank(entry), entry, causes))
        })
        .collect::<Vec<_>>();
    scoped.sort_by_key(|(rank, _, _)| *rank);
    let mut leads: Vec<Value> = Vec::new();
    for (_, entry, causes) in scoped {
        if leads.len() == SCOPED_LEADS {
            break;
        }
        let mut changes = json!({"path": entry.path.as_str()});
        for cause in &causes {
            let (field, value) = cause.flag();
            changes[field] = json!(value);
        }
        let lead = restart_lead(query, changes)?;
        if !leads.contains(&lead) {
            leads.push(lead);
        }
    }
    Ok(leads)
}
