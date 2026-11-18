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
    /// walked scope.
    /// An empty page's `observed` entries (the skipped paths a re-run found
    /// matches in, each with its rule) replace the walk's for their causes.
    pub(super) fn skipped_paths(&self) -> Result<Option<SkippedPaths>, ToolError> {
        use octocode_contract_types::SkippedPathCause::{Hidden, Ignored, OctocodeArtifact};
        let (root, skipped) = (self.root, self.skipped);
        let walked = under(root, Ignored, &skipped.ignored)
            .chain(octocode_tools_shared::leads::skipped::default_excluded(
                root,
                &skipped.pruned,
                &skipped.generated,
            ))
            .chain(under(root, Hidden, &skipped.hidden))
            // No flag lifts these: they stay out of the includeSkipped lead.
            .chain(under(root, OctocodeArtifact, &skipped.artifacts))
            .chain(octocode_tools_shared::leads::skipped::unlifted(
                &self.root.to_string_lossy(),
                &skipped.withheld,
                Default::default(),
            ));
        octocode_tools_shared::leads::skipped::skipped_paths(walked, false, !self.walk_complete)
    }
}
