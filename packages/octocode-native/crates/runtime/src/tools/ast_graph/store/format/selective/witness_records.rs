//! The witness records behind a topology answer's edges, for counting each
//! edge's source occurrences (`sites`): the `WITN` section is authenticated
//! and checked structurally; no evidence string or source block is read.
use super::*;
use crate::tools::ast_graph::store::format::evidence::{Checks, witness_records};

impl SnapshotReader {
    /// Every witness record of the snapshot, checked against the nodes of
    /// `t` (tables from [`Self::topology_tables`]).
    pub(crate) fn witnesses(
        &mut self,
        t: &GraphTables,
        cancel: &dyn CancellationCheck,
    ) -> Result<Vec<WitnessRec>, String> {
        let strings = u32::try_from(self.blocks[b"STRE"].0)
            .ok()
            .and_then(|evidence| t.string_bound().ok()?.checked_add(evidence))
            .ok_or("graph string dictionary exceeds its id space")?;
        let bytes = self.section(*b"WITN", cancel)?;
        let mut r = Reader::new(&bytes, "WITN");
        let witnesses = witness_records(&mut r, t, strings, Checks::Structural)?;
        if r.at != bytes.len() {
            return Err("graph section WITN has trailing data".into());
        }
        self.decoded_occurrences += witnesses.len();
        Ok(witnesses)
    }
}
