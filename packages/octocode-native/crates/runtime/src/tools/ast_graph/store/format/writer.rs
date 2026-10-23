//! Encoding: varint writers and the deterministic, sectioned snapshot writer.
use super::*;

// ── Encoding ────────────────────────────────────────────────────────────────

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

/// Serializes the snapshot. Returns `(bytes, body SHA-256 hex)`; identical
/// tables always produce identical bytes.
pub(crate) fn encode(tables: &GraphTables) -> (Vec<u8>, String) {
    let encoder = Encoder::new(tables);
    let (t, sid) = (tables, |old: u32| encoder.sid(old));
    let mut sections = vec![
        Section {
            tag: *b"STRS",
            bytes: encoder.strings(),
        },
        Section {
            tag: *b"NODE",
            bytes: encoder.nodes(),
        },
        Section {
            tag: *b"EDGE",
            bytes: encoder.edges(),
        },
        Section {
            tag: *b"KEYX",
            bytes: varints(&t.key_index),
        },
        Section {
            tag: *b"NAMX",
            bytes: varints(&t.name_index),
        },
        Section {
            tag: *b"DIAG",
            bytes: encoder.diagnostics(),
        },
    ];
    let digests = t
        .digests
        .iter()
        .flat_map(|(node, digest)| [*node, sid(*digest)])
        .collect::<Vec<_>>();
    let components = t
        .components
        .iter()
        .flat_map(|(node, dir, name, meta)| [*node, sid(*dir), sid(*name), sid(*meta)])
        .collect::<Vec<_>>();
    let entries = t
        .entries
        .iter()
        .flat_map(|(node, rule)| [*node, sid(*rule)])
        .collect::<Vec<_>>();
    sections.extend([
        Section {
            tag: *b"FDIG",
            bytes: varints(&digests),
        },
        Section {
            tag: *b"FCMP",
            bytes: varints(&components),
        },
        Section {
            tag: *b"ENTR",
            bytes: varints(&entries),
        },
    ]);
    if !t.stamps.is_empty() {
        sections.push(Section {
            tag: *b"FSTM",
            bytes: stamp_section(&t.stamps),
        });
    }
    assemble(sections)
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

/// The string table every section references by id.
pub(super) struct Encoder<'t> {
    pub(super) tables: &'t GraphTables,
    pub(super) texts: Vec<&'t str>,
    pub(super) ids: HashMap<&'t str, u32>,
}

impl<'t> Encoder<'t> {
    pub(super) fn new(tables: &'t GraphTables) -> Self {
        let mut texts = Vec::<&str>::new();
        for (index, node) in tables.nodes.iter().enumerate() {
            texts.push(key_text(tables, index).0);
            texts.push(tables.str(node.name));
            texts.push(tables.str(node.detail));
        }
        texts.extend(tables.edges.iter().map(|edge| tables.str(edge.detail)));
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
        let ids = texts
            .iter()
            .enumerate()
            .map(|(index, text)| (*text, index as u32))
            .collect();
        Self { tables, texts, ids }
    }

    pub(super) fn id(&self, text: &str) -> u32 {
        self.ids.get(text).copied().unwrap_or(0)
    }

    pub(super) fn sid(&self, old: u32) -> u32 {
        self.id(self.tables.str(old))
    }

    /// Front-coded strings: shared prefix length, suffix length, suffix.
    pub(super) fn strings(&self) -> Vec<u8> {
        let mut strs = Vec::new();
        put(&mut strs, self.texts.len() as u64);
        let mut previous: &[u8] = &[];
        for text in &self.texts {
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

    pub(super) fn nodes(&self) -> Vec<u8> {
        let tables = self.tables;
        let mut nodes = Vec::new();
        put(&mut nodes, tables.nodes.len() as u64);
        for (index, node) in tables.nodes.iter().enumerate() {
            let (key, suffix) = key_text(tables, index);
            nodes.push(node.kind as u8 | if suffix { KEY_SUFFIX } else { 0 });
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

/// A node's key text: symbol keys shrink to their suffix after `path#`.
pub(super) fn key_text(tables: &GraphTables, index: usize) -> (&str, bool) {
    let node = &tables.nodes[index];
    let key = tables.str(node.key);
    if node.kind == NodeKind::Symbol && node.file != NONE {
        let file = tables.str(tables.nodes[node.file as usize].key);
        if let Some(suffix) = key
            .strip_prefix(file)
            .and_then(|rest| rest.strip_prefix('#'))
        {
            return (suffix, true);
        }
    }
    (key, false)
}

/// Header, section table and 8-byte-aligned section bodies; returns the
/// file bytes and the hex body digest.
pub(super) fn assemble(sections: Vec<Section>) -> (Vec<u8>, String) {
    let section_count = sections.len() as u32;
    let table_len = sections.len() * TABLE_ENTRY_LEN;
    let mut body = Vec::new();
    let mut offset = ((HEADER_LEN + table_len) as u64).next_multiple_of(8);
    let padding_before_body = offset as usize - (HEADER_LEN + table_len);
    for section in &sections {
        body.extend(section.tag);
        body.extend(offset.to_le_bytes());
        body.extend((section.bytes.len() as u64).to_le_bytes());
        offset += (section.bytes.len() as u64).next_multiple_of(8);
    }
    body.extend(vec![0u8; padding_before_body]);
    for section in sections {
        let pad = section.bytes.len().next_multiple_of(8) - section.bytes.len();
        body.extend(section.bytes);
        body.extend(vec![0u8; pad]);
    }
    let digest = Sha256::digest(&body);
    let mut out = Vec::with_capacity(HEADER_LEN + body.len());
    out.extend(MAGIC);
    out.extend(FORMAT_VERSION.to_le_bytes());
    out.extend(section_count.to_le_bytes());
    out.extend(digest);
    out.extend(body);
    (out, hex::encode(digest))
}
