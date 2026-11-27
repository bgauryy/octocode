//! Connection strength: the distinct source occurrences (witnesses) behind
//! each edge, counted once per loaded snapshot when an answer first needs it.
use super::*;
use std::collections::{HashMap, HashSet};

/// One stored edge: the snapshot keeps one per (src, kind, dst, detail).
type EdgeKey = (u32, u8, u32, u32);

fn key(edge: &EdgeRec) -> EdgeKey {
    (edge.src, edge.kind as u8, edge.dst, edge.detail)
}

/// Distinct witness occurrences per stored edge, split by test role.
#[derive(Debug, Default)]
pub(super) struct Sites {
    /// Per edge: `[production, test]` occurrences.
    by_edge: HashMap<EdgeKey, [u64; 2]>,
}

impl Sites {
    fn of(witnesses: &[super::super::format::WitnessRec]) -> Self {
        let mut seen = HashSet::new();
        let mut by_edge = HashMap::<EdgeKey, [u64; 2]>::new();
        for witness in witnesses {
            let edge = key(&witness.edge);
            if seen.insert((edge, witness.occurrence)) {
                by_edge.entry(edge).or_default()[usize::from(witness.edge.test)] += 1;
            }
        }
        Self { by_edge }
    }

    /// The occurrences behind `edge` a filter admitting test edges
    /// (`tests`) or not counts: a production edge may also be written in
    /// test code, which a production ranking leaves out.
    pub(super) fn behind(&self, edge: &EdgeRec, tests: bool) -> u64 {
        self.by_edge
            .get(&key(edge))
            .map_or(0, |[production, test]| {
                production + if tests { *test } else { 0 }
            })
    }
}

impl Graph {
    /// Each edge's source occurrences: from the decoded witness table, or
    /// read once from the snapshot when the loaded tables carry none.
    pub(super) fn sites(
        &self,
        cancel: &dyn octocode_runtime_base::execution::cancel::CancellationCheck,
    ) -> Result<&Sites, GraphOutput> {
        if let Some(sites) = self.sites.get() {
            return Ok(sites);
        }
        let sites = match &self.selective {
            Some(reader) if self.t.witnesses.is_empty() => {
                let witnesses = reader
                    .borrow_mut()
                    .witnesses(&self.t, cancel)
                    .map_err(|message| *decode_failure(message, &self.manifest, cancel))?;
                Sites::of(&witnesses)
            }
            _ => Sites::of(&self.t.witnesses),
        };
        Ok(self.sites.get_or_init(|| sites))
    }
}
