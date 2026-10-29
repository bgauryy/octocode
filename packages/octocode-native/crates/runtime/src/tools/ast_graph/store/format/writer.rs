//! Encoding: varint writers and the deterministic, sectioned snapshot writer.
use super::*;
use crate::execution::cancel::CancellationCheck;
use crate::response::result::ToolError;
mod publication;
pub(crate) use publication::write_for_publish;
#[cfg(test)]
use publication::write_snapshot;

// ── Encoding ────────────────────────────────────────────────────────────────

#[cfg(test)]
pub(super) struct Section {
    pub(super) tag: [u8; 4],
    pub(super) bytes: Vec<u8>,
}

pub(super) fn put(out: &mut Vec<u8>, mut value: u64) {
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

/// `NONE` → 0, otherwise `value + 1`.
pub(super) fn put_opt(out: &mut Vec<u8>, value: u32) {
    put(
        out,
        if value == NONE {
            0
        } else {
            u64::from(value) + 1
        },
    );
}

pub(super) fn zigzag(value: i64) -> u64 {
    ((value << 1) ^ (value >> 63)) as u64
}

pub(super) fn varints(values: &[u32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(values.len() * 3 + 5);
    put(&mut out, values.len() as u64);
    for value in values {
        put(&mut out, u64::from(*value));
    }
    out
}

/// Serializes the snapshot. Returns `(bytes, directory SHA-256 hex)`; identical
/// tables always produce identical bytes.
#[cfg(test)]
pub(crate) fn encode(tables: &GraphTables) -> (Vec<u8>, String) {
    let encoder = Encoder::new(tables);
    encode_with(&encoder, &crate::execution::cancel::NeverCancel)
        .expect("fixture graph source correspondence encodes")
}

/// Publication must not create a snapshot that this format's reader rejects for size.
#[cfg(test)]
pub(crate) fn encode_for_publish(
    tables: &GraphTables,
    cancel: &dyn CancellationCheck,
) -> Result<(Vec<u8>, String), ToolError> {
    let encoder = checked_encoder(tables, &LIMITS, cancel)?;
    encode_with(&encoder, cancel)
}

#[cfg(test)]
pub(super) fn encode_with(
    encoder: &Encoder<'_>,
    cancel: &dyn CancellationCheck,
) -> Result<(Vec<u8>, String), ToolError> {
    let mut output = std::io::Cursor::new(Vec::new());
    let (_, digest) = write_snapshot(encoder, &mut output, &LIMITS, cancel)?;
    Ok((output.into_inner(), digest))
}

/// Count, then per file: node, size, modified, changed and inode varints.
pub(super) fn stamp_section(stamps: &[(u32, octocode_engine::stamp::SourceStamp)]) -> Vec<u8> {
    let mut out = Vec::with_capacity(stamps.len() * 24 + 5);
    put(&mut out, stamps.len() as u64);
    for (node, stamp) in stamps {
        for value in [
            u64::from(*node),
            stamp.size,
            stamp.modified_ns,
            stamp.changed_ns,
            stamp.inode,
        ] {
            put(&mut out, value);
        }
    }
    out
}

/// The string dictionary every section references by id: record strings
/// (`texts[..records]`, section `STRS`) sorted before the strings only
/// occurrence evidence references (`STRE`), so a reader that skips
/// evidence also skips its text.
pub(super) struct Encoder<'t> {
    pub(super) tables: &'t GraphTables,
    pub(super) texts: Vec<&'t str>,
    pub(super) records: usize,
    pub(super) ids: HashMap<&'t str, u32>,
}

impl<'t> Encoder<'t> {
    pub(super) fn new(tables: &'t GraphTables) -> Self {
        let mut texts = Vec::<&str>::new();
        for node in &tables.nodes {
            texts.push(tables.str(node.key));
            texts.push(tables.str(node.name));
            texts.push(tables.str(node.detail));
        }
        texts.extend(tables.edges.iter().map(|edge| tables.str(edge.detail)));
        for decl in &tables.declarations {
            texts.extend([
                tables.str(decl.occurrence),
                tables.str(decl.source_digest),
                tables.str(decl.test_identity),
                tables.str(decl.assessment),
            ]);
        }
        for diag in &tables.diagnostics {
            texts.extend([
                tables.str(diag.file),
                tables.str(diag.code),
                tables.str(diag.message),
            ]);
        }
        texts.extend(tables.digests.iter().map(|(_, digest)| tables.str(*digest)));
        for (_, dir, name, meta) in &tables.components {
            texts.extend([tables.str(*dir), tables.str(*name), tables.str(*meta)]);
        }
        texts.extend(tables.entries.iter().map(|(_, rule)| tables.str(*rule)));
        texts.sort_unstable();
        texts.dedup();
        let records = texts.len();
        let mut evidence = Vec::<&str>::new();
        for witness in &tables.witnesses {
            evidence.extend([
                tables.str(witness.edge.detail),
                tables.str(witness.occurrence),
                tables.str(witness.argument),
                tables.str(witness.provenance.source_digest),
                tables.str(witness.provenance.derivation_version),
            ]);
        }
        for site in &tables.unresolved {
            evidence.extend([
                tables.str(site.occurrence),
                tables.str(site.argument),
                tables.str(site.callee),
                tables.str(site.reason),
                tables.str(site.provenance.source_digest),
                tables.str(site.provenance.derivation_version),
            ]);
        }
        evidence.retain(|text| texts.binary_search(text).is_err());
        evidence.sort_unstable();
        evidence.dedup();
        texts.extend(evidence);
        let ids = texts
            .iter()
            .enumerate()
            .map(|(index, text)| (*text, index as u32))
            .collect();
        Self {
            tables,
            texts,
            records,
            ids,
        }
    }

    pub(super) fn id(&self, text: &str) -> u32 {
        self.ids.get(text).copied().unwrap_or(0)
    }

    pub(super) fn sid(&self, old: u32) -> u32 {
        self.id(self.tables.str(old))
    }

    /// `STRS`: the record strings.
    pub(super) fn record_strings(&self) -> Vec<u8> {
        front_coded(&self.texts[..self.records])
    }

    /// `STRE`: strings only witnesses and unresolved sites reference.
    pub(super) fn evidence_strings(&self) -> Vec<u8> {
        front_coded(&self.texts[self.records..])
    }

    pub(super) fn nodes(&self) -> Vec<u8> {
        let tables = self.tables;
        let mut nodes = Vec::new();
        put(&mut nodes, tables.nodes.len() as u64);
        for node in &tables.nodes {
            let key = tables.str(node.key);
            nodes.push(node.kind as u8);
            nodes.push(node.flags);
            put(&mut nodes, u64::from(self.id(key)));
            put(&mut nodes, u64::from(self.sid(node.name)));
            put(&mut nodes, u64::from(self.sid(node.detail)));
            put_opt(&mut nodes, node.file);
            put_opt(&mut nodes, node.parent);
            put_opt(&mut nodes, node.line);
            if node.line == NONE || node.end_line == NONE {
                put_opt(&mut nodes, node.end_line);
            } else {
                put(
                    &mut nodes,
                    zigzag(i64::from(node.end_line) - i64::from(node.line)) + 1,
                );
            }
        }
        nodes
    }

    pub(super) fn edges(&self) -> Vec<u8> {
        let mut edges = Vec::new();
        put(&mut edges, self.tables.edges.len() as u64);
        let mut last_src = 0u32;
        for edge in &self.tables.edges {
            put(&mut edges, u64::from(edge.src.saturating_sub(last_src)));
            last_src = edge.src;
            put(
                &mut edges,
                zigzag(i64::from(edge.dst) - i64::from(edge.src)),
            );
            let test = if edge.test { EDGE_TEST } else { 0 };
            edges.push(((edge.kind as u8) << 2) | edge.confidence as u8 | test);
            put(&mut edges, u64::from(self.sid(edge.detail)));
            put_opt(&mut edges, edge.line);
        }
        edges
    }

    pub(super) fn diagnostics(&self) -> Vec<u8> {
        let mut diags = Vec::new();
        put(&mut diags, self.tables.diagnostics.len() as u64);
        for diag in &self.tables.diagnostics {
            put(&mut diags, u64::from(self.sid(diag.file)));
            put_opt(&mut diags, diag.line);
            put(&mut diags, u64::from(self.sid(diag.code)));
            put(&mut diags, u64::from(self.sid(diag.message)));
            put_opt(&mut diags, diag.target);
        }
        diags
    }
}

/// Sorted strings, front-coded: shared prefix length, suffix length, suffix.
fn front_coded(texts: &[&str]) -> Vec<u8> {
    let mut strs = Vec::new();
    put(&mut strs, texts.len() as u64);
    let mut previous: &[u8] = &[];
    for text in texts {
        let bytes = text.as_bytes();
        let shared = previous
            .iter()
            .zip(bytes)
            .take_while(|(a, b)| a == b)
            .count();
        put(&mut strs, shared as u64);
        put(&mut strs, (bytes.len() - shared) as u64);
        strs.extend(&bytes[shared..]);
        previous = bytes;
    }
    strs
}

/// Header, section table and 8-byte-aligned section bodies; returns the
/// file bytes and the hex body digest.
#[cfg(test)]
pub(super) fn assemble(sections: Vec<Section>) -> (Vec<u8>, String) {
    let sections = sections
        .into_iter()
        .map(|mut section| {
            if blocks::blocked(&section.tag) {
                section.bytes =
                    blocks::pack(&section.tag, &section.bytes).expect("fixture record blocks");
            }
            section
        })
        .collect::<Vec<_>>();
    let count = sections.len();
    let start = (HEADER_LEN + count * TABLE_ENTRY_LEN).next_multiple_of(8);
    let mut out = vec![0; start];
    let mut directory = Vec::new();
    for section in sections {
        directory.extend(section.tag);
        directory.extend((out.len() as u64).to_le_bytes());
        directory.extend((section.bytes.len() as u64).to_le_bytes());
        directory.extend(blocks::digest(&section.tag, &section.bytes).unwrap());
        out.extend(section.bytes);
        out.resize(out.len().next_multiple_of(8), 0);
    }
    let digest = Sha256::digest(&directory);
    out[..8].copy_from_slice(MAGIC);
    out[8..12].copy_from_slice(&FORMAT_VERSION.to_le_bytes());
    out[12..16].copy_from_slice(&(count as u32).to_le_bytes());
    out[16..HEADER_LEN].copy_from_slice(&digest);
    out[HEADER_LEN..HEADER_LEN + directory.len()].copy_from_slice(&directory);
    (out, hex::encode(digest))
}

pub(super) fn checked_encoder<'a>(
    tables: &'a GraphTables,
    limits: &Limits,
    cancel: &dyn CancellationCheck,
) -> Result<Encoder<'a>, ToolError> {
    cancel.check().map_err(ToolError::cancelled)?;
    let encoder = Encoder::new(tables);
    tables
        .sources
        .validate_with_cancel(cancel)
        .map_err(ToolError::source)?;
    tables
        .sources
        .validate_records(tables, cancel)
        .map_err(|error| {
            cancel.check().map_or_else(ToolError::cancelled, |()| {
                ToolError::new(octocode_contract_types::ErrorCode::ScopeTooBroad, error)
            })
        })?;
    limits::validate(&encoder, limits).map_err(|error| {
        ToolError::new(octocode_contract_types::ErrorCode::ScopeTooBroad, error)
    })?;
    Ok(encoder)
}
