//! Which files' rows state the byte provenance a snapshot recorded of them.
//!
//! A build parses each source's own text, so its digests, byte ranges,
//! origins and the occurrence ids made from them count that text, hidden
//! values included: shown, a digest or a length would test a guess of a
//! value every read hides. The build judges each source whole, as every
//! read does, and flags the file a read hides a value of
//! ([`ROLE_HIDDEN_VALUE`]); that file's rows are placed by path and line
//! alone. The snapshot keeps its digests for staleness checks; no row of
//! such a file shows them.
use super::Graph;
use super::super::format::NodeRec;
use crate::tools::ast_graph::classify::ROLE_HIDDEN_VALUE;

impl Graph {
    /// Whether rows of `file`, a file node's record, state the byte
    /// provenance the snapshot recorded of it: only when a read of it hid
    /// nothing when it was built. A point answer passes the record it read
    /// ([`Graph::node_record`]); a page has every node decoded.
    pub(super) fn shows_provenance(file: &NodeRec) -> bool {
        file.flags & ROLE_HIDDEN_VALUE == 0
    }
}
