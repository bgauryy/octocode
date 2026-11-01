//! Evidence pages (`witnesses`, `unresolved`) over a file-backed snapshot:
//! nodes, record strings and one occurrence section are decoded; evidence
//! strings are read and authenticated one `STRE` block at a time.
use super::*;
use crate::tools::ast_graph::store::format::evidence::{Checks, read_unresolved, read_witnesses};

/// Decoded `STRE` blocks, keyed by block index.
#[derive(Debug)]
pub(super) struct EvidenceStrings {
    /// Record strings (`STRS`); evidence string ids follow them.
    records: u32,
    blocks: BTreeMap<usize, Vec<String>>,
}

impl SnapshotReader {
    /// Whether per-node degrees are loaded ([`Self::degrees`]).
    pub(crate) fn has_degrees(&self) -> bool {
        !self.degrees.is_empty()
    }

    /// Per-node degrees from the lookup section, read on first use: tables
    /// without an edge index (evidence, candidates) still rank a reference.
    pub(crate) fn load_degrees(
        &mut self,
        t: &GraphTables,
        cancel: &dyn CancellationCheck,
    ) -> Result<(), String> {
        if self.has_degrees() {
            return Ok(());
        }
        let (_, degrees) = lookup::read_lookup(&self.section(*b"LOOK", cancel)?, t, true)?;
        self.degrees = degrees;
        Ok(())
    }

    /// The tables an evidence page needs: record strings, every node,
    /// the lookup indexes, the per-file tables and the requested occurrence
    /// section. Occurrences are checked structurally; their evidence strings
    /// and origins are authenticated when a row renders them.
    pub(crate) fn evidence_tables(
        &mut self,
        witnesses: bool,
        cancel: &dyn CancellationCheck,
    ) -> Result<GraphTables, String> {
        let mut t = GraphTables::default();
        read_strings(
            &mut Reader::new(&self.section(*b"STRS", cancel)?, "STRS"),
            &mut t,
            &LIMITS,
        )?;
        let nodes = blocks::unpack(b"NODE", &self.section(*b"NODE", cancel)?)?;
        for index in 0..self.blocks[b"NODE"].1.len() {
            self.checked.insert((*b"NODE", Some(index)));
        }
        read_nodes(&mut Reader::new(&nodes, "NODE"), &mut t)?;
        t.key_index = Reader::new(&self.section(*b"KEYX", cancel)?, "KEYX").var_vec()?;
        t.name_index = Reader::new(&self.section(*b"NAMX", cancel)?, "NAMX").var_vec()?;
        lookup::validate_indexes(&t)?;
        let mut files = BTreeMap::new();
        for tag in [*b"DIAG", *b"FDIG", *b"FSTM"] {
            if tag != *b"FSTM" || self.sections.contains_key(&tag) {
                files.insert(tag, std::borrow::Cow::Owned(self.section(tag, cancel)?));
            }
        }
        let records = t.string_bound()?;
        read_file_tables(&Sections(files), &mut t, records)?;
        let strings = u32::try_from(self.blocks[b"STRE"].0)
            .ok()
            .and_then(|evidence| records.checked_add(evidence))
            .ok_or("graph string dictionary exceeds its id space")?;
        let (tag, what) = if witnesses {
            (*b"WITN", "WITN")
        } else {
            (*b"UNRS", "UNRS")
        };
        let bytes = self.section(tag, cancel)?;
        let mut r = Reader::new(&bytes, what);
        if witnesses {
            read_witnesses(&mut r, &mut t, strings, Checks::Structural)?;
        } else {
            read_unresolved(&mut r, &mut t, strings, Checks::Structural)?;
        }
        if r.at != bytes.len() {
            return Err(format!("graph section {what} has trailing data"));
        }
        self.decoded_nodes = t.nodes.len();
        self.decoded_occurrences = t.witnesses.len() + t.unresolved.len();
        self.evidence = Some(EvidenceStrings {
            records,
            blocks: BTreeMap::new(),
        });
        Ok(t)
    }

    /// Evidence string `id` (an id at or past the record strings), read from
    /// its authenticated `STRE` block; `None` when this reader is not in
    /// evidence mode or `id` is a record string.
    pub(crate) fn evidence_string(
        &mut self,
        id: u32,
        cancel: &dyn CancellationCheck,
    ) -> Option<Result<String, String>> {
        let records = self.evidence.as_ref()?.records;
        let local = id.checked_sub(records)?;
        Some(self.load_evidence_string(local, cancel))
    }

    fn load_evidence_string(
        &mut self,
        local: u32,
        cancel: &dyn CancellationCheck,
    ) -> Result<String, String> {
        let index = self.blocks[b"STRE"]
            .1
            .binary_search_by(|block| {
                if block.last < local {
                    std::cmp::Ordering::Less
                } else if block.first > local {
                    std::cmp::Ordering::Greater
                } else {
                    std::cmp::Ordering::Equal
                }
            })
            .map_err(|_| "graph evidence string reference is invalid".to_owned())?;
        let first = self.blocks[b"STRE"].1[index].first;
        let cached = self
            .evidence
            .as_ref()
            .is_some_and(|strings| strings.blocks.contains_key(&index));
        if !cached {
            let bytes = self.record_block(*b"STRE", index, cancel)?;
            let mut block = GraphTables::default();
            // A block restarts its front coding: decoding it alone is exact.
            read_strings(&mut Reader::new(&bytes, "STRE"), &mut block, &LIMITS)?;
            if let Some(strings) = &mut self.evidence {
                strings.blocks.insert(index, block.strings);
            }
        }
        self.evidence
            .as_ref()
            .and_then(|strings| strings.blocks.get(&index))
            .and_then(|block| block.get((local - first) as usize))
            .cloned()
            .ok_or_else(|| "graph evidence string block is inconsistent".to_owned())
    }
}
