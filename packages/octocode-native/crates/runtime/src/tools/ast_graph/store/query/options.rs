//! The options of one graph query: a graphQuery row, decoded.
use std::path::PathBuf;

#[derive(Clone, Debug, Default)]
pub struct QueryOptions {
    pub op: String,
    /// The graphQuery `path` that selected the snapshot: every
    /// continuation, verify step and hint repeats it with the snapshot id.
    pub path: Option<String>,
    /// Full origin objects and `derivation` on every row; lean rows (the
    /// default) mark proven identity origins `"identity"` and omit
    /// `derivation`.
    pub include_provenance: bool,
    pub check_freshness: bool,
    /// Node reference: key, absolute path, name, or `Class.method`.
    pub target: Option<String>,
    /// `witnesses`/`unresolved`: source references beside `target`; rows
    /// come from any of them.
    pub targets: Vec<String>,
    /// `unresolved`: keep only rows with these reasons, before paging.
    pub reasons: Vec<String>,
    /// Characters this answer may fill: the row's share of the response
    /// page. A list page ends at `limit` rows or this budget, whichever
    /// comes first. `None`: `limit` alone.
    pub response_window: Option<usize>,
    /// Keep the decoded snapshot open for later queries of this process.
    pub reuse_snapshot: bool,
    /// Second reference for `path`.
    pub to: Option<String>,
    /// Snapshot directory, `graph.bin` path, id, scope, or a substring of
    /// exactly one id.
    pub graph: Option<String>,
    pub workspace: Option<PathBuf>,
    /// Edge kinds to follow (`contains`, `imports`, `uses`, `calls`, `inherits`).
    pub edges: Vec<String>,
    /// Node kind filter (`file`, `symbol`, `package`).
    pub kind: Option<String>,
    /// `out`, `in`, or `both`.
    pub direction: Option<String>,
    pub depth: Option<u32>,
    /// Weakest call-edge confidence to follow (`high`, `medium`, `low`).
    pub confidence: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    /// The call count of a walk: 1 for its first call, each continuation's
    /// one more (`pagination.page`).
    pub page: Option<usize>,
    /// Evidence list in `summary` or `baseline` to return as paged results.
    pub list: Option<String>,
    /// `dependents`/`callers`: `files` groups the edges per file.
    pub view: Option<String>,
    /// `issues`: detectors to run (default all).
    pub detectors: Vec<String>,
    /// `issues`: snapshot to diff findings against (new / existing / resolved).
    pub baseline: Option<String>,
    /// `issues`: drop findings scoring below this.
    pub min_score: Option<f64>,
    /// `impact`: additional changed references (files or symbols).
    pub changed: Vec<String>,
    /// `impact`: git revision; files changed since it (plus untracked) seed
    /// the blast radius.
    pub since: Option<String>,
    /// `issues`: lowest finding tier to show (100/90/60; default 90).
    pub min_tier: Option<u8>,
    /// `issues`: entrypoints (node references) in place of the inferred ones.
    pub entries: Vec<String>,
}

impl QueryOptions {
    /// `target` then `targets`: the source references an evidence page keeps.
    pub(crate) fn sources(&self) -> impl Iterator<Item = &str> {
        self.target.iter().chain(&self.targets).map(String::as_str)
    }
}
