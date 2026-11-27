//! Test-only diagnostics for complete and selectively authenticated snapshots.
use super::*;

pub(crate) fn full_validation(
    bytes: &[u8],
    tables: &GraphTables,
) -> Result<serde_json::Value, String> {
    let mut header = Reader::new(&bytes[8..HEADER_LEN], "header");
    header.fixed_u32()?;
    let count = usize::try_from(header.fixed_u32()?).map_err(|_| "graph header overflows")?;
    let mut total = 1 + count;
    let mut directory = Reader::new(&bytes[HEADER_LEN..], "directory");
    let span = |value: u64| usize::try_from(value).map_err(|_| "graph directory overflows");
    for _ in 0..count {
        let tag = directory.take(4)?;
        let (offset, len) = (span(directory.fixed_u64()?)?, span(directory.fixed_u64()?)?);
        directory.take(32)?;
        if tag == b"NODE" || tag == b"DECL" {
            let (_, blocks) = blocks::parse(
                bytes
                    .get(offset..offset + len)
                    .ok_or("graph block section is truncated")?,
                len as u64,
            )?;
            total += blocks.len();
        }
    }
    Ok(
        serde_json::json!({"scope":"fullSnapshot", "checkedBlocks":total, "totalBlocks":total,
        "bytesRead":bytes.len(), "snapshotBytes":bytes.len(), "decodedNodes":tables.nodes.len(),
        "decodedEdges":tables.edges.len(), "decodedWitnesses":tables.witnesses.len()}),
    )
}

impl SnapshotReader {
    pub(crate) fn validation(&self) -> serde_json::Value {
        let total = 1
            + self.sections.len()
            + self
                .blocks
                .values()
                .map(|(_, blocks)| blocks.len())
                .sum::<usize>();
        serde_json::json!({"scope":"accessedBlocks", "checkedBlocks":1 + self.checked.len(), "totalBlocks":total,
            "bytesRead":self.bytes_read, "snapshotBytes":self.snapshot_bytes,
            "sourceBlocksRead":self.checked.iter().filter(|(tag, block)| tag == b"SRCO" && block.is_some()).count(),
            "declarationBlocksRead":self.checked.iter().filter(|(tag, block)| tag == b"DECL" && block.is_some()).count(),
            "decodedNodes":self.decoded_nodes, "decodedEdges":self.decoded_edges, "decodedWitnesses":self.decoded_occurrences})
    }
}
