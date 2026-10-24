//! What a page discloses of the paths its walk skipped.

use super::*;

impl Found<'_> {
    /// The causes the walk skipped paths by, so the lead over them lifts
    /// each in one call: a dot directory the default prune also names
    /// (`.github`, `.cache`), or a dot file the default excludes match, sits
    /// behind both.
    pub(super) fn skipped_causes(&self) -> Vec<Cause> {
        let skipped = self.skipped;
        let excluded = skipped.hidden_excluded
            || skipped.ignored_excluded
            || !skipped.generated.is_empty()
            || !skipped.pruned.is_empty();
        [
            (!skipped.ignored.is_empty(), Cause::Ignored),
            (
                !skipped.hidden.is_empty() || skipped.ignored_hidden,
                Cause::Hidden,
            ),
            (excluded, Cause::DefaultExcluded),
        ]
        .into_iter()
        .filter_map(|(hit, cause)| hit.then_some(cause))
        .collect()
    }

    /// Every ignored, default-excluded and hidden path on the first page;
    /// policy-withheld names stay hidden, counted by reason at the walked scope.
    /// An empty page's `observed` entries (the skipped paths a re-run found
    /// matches in, each with its rule) replace the walk's for their causes.
    pub(super) fn skipped_paths(
        &self,
        observed: Option<SkippedPaths>,
    ) -> Result<Option<SkippedPaths>, ToolError> {
        let named = |cause: SkippedPathCause| {
            observed
                .as_ref()
                .is_some_and(|observed| observed.entries.iter().any(|entry| entry.cause == cause))
        };
        use octocode_contract_types::SkippedPathCause::{DefaultExcluded, Hidden, Ignored};
        let (root, skipped) = (self.root, self.skipped);
        let walked = under(root, Ignored, &skipped.ignored)
            .chain(under(root, DefaultExcluded, &skipped.pruned))
            .chain(under(root, DefaultExcluded, &skipped.generated))
            .chain(under(root, Hidden, &skipped.hidden))
            .filter(|skip| !named(skip.cause))
            .chain(crate::tools::leads::skipped::unlifted(
                &self.root.to_string_lossy(),
                &skipped.withheld,
                Default::default(),
            ));
        let disclosure =
            crate::tools::leads::skipped::skipped_paths(walked, true, !self.walk_complete)?;
        Ok(match (disclosure, observed) {
            (Some(mut disclosure), Some(observed)) => {
                disclosure.entries.extend(observed.entries);
                disclosure.sampled |= observed.sampled;
                Some(disclosure)
            }
            (disclosure, observed) => disclosure.or(observed),
        })
    }
}
