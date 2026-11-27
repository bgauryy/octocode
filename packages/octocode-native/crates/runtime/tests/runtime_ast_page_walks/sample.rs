//! The ways a walk is started that the gate walks, and all of them.

/// One way to start a walk: over the MCP channel or the CLI, from the
/// absolute spelling of the path (`0`) or the workspace-relative one (`1`).
pub(super) type Start = (bool, usize);

/// The starts a walk compares. The gate takes two that differ in both
/// dimensions: a transport-only or a spelling-only difference still shows
/// between them, and each start's rows must be every row once. The full
/// matrix (ignored tests) takes all four.
pub(super) fn starts(full: bool) -> Vec<Start> {
    if full {
        vec![(false, 0), (false, 1), (true, 0), (true, 1)]
    } else {
        vec![(false, 0), (true, 1)]
    }
}
