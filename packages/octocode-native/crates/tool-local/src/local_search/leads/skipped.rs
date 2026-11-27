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
    /// directory at the top of its walk ([`Self::discloses_skipped`]).
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
