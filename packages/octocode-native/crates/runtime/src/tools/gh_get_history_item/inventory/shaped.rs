/// What shaping a pull request's changed-file page found.
pub(in crate::tools::gh_get_history_item) struct ShapedFiles {
    /// The page's review pick (`next.readSelectedPatches`).
    pub(in crate::tools::gh_get_history_item) review: Vec<String>,
    /// Paths of the patch rows the window delivers; `None` for an inventory
    /// that lists every file of the page.
    pub(in crate::tools::gh_get_history_item) shown: Option<Vec<String>>,
    /// In-scope text files a `matchString` could not search because GitHub
    /// sent no patch for them, as compact `!reason path` rows.
    pub(in crate::tools::gh_get_history_item) unsearched: Vec<String>,
    /// The first of those paths (the source-read template), and whether
    /// the change removed it (its text is at the base).
    pub(in crate::tools::gh_get_history_item) first_unsearched: Option<(String, bool)>,
    /// `(dropped, listed)`: in-scope searched files a `matchString` dropped
    /// for holding no hit, beside files it kept; `None` when it dropped none
    /// or kept none (an unmatched literal is stated on its own).
    pub(in crate::tools::gh_get_history_item) match_dropped: Option<(usize, usize)>,
    /// Files whose `matchString` context the hunks cut short, with the
    /// new-side windows to read at the head.
    pub(in crate::tools::gh_get_history_item) clipped: Vec<(String, Vec<(usize, usize)>)>,
    /// Files of the patch window not fully delivered (a working count, never
    /// an output field).
    pub(in crate::tools::gh_get_history_item) unfinished_patches: usize,
}
