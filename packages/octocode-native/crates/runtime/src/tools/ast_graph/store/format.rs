//! `graph.bin`: a sectioned, varint-coded snapshot.
//!
//! ```text
//! header   magic "OCGRAPH\0" · u32 format version · u32 section count
//!          · [u8; 32] SHA-256 of every byte after the header
//! table    per section: [u8; 4] tag · u64 offset · u64 length
//! sections 8-byte aligned, in table order
//! ```
//!
//! v2 layout (LEB128 varints throughout):
//! - `STRS` sorted unique strings, front-coded (shared-prefix length, suffix).
//!   A symbol stores only its key suffix (`Struct.member`); the reader rebuilds
//!   `path#Struct.member` from its file, so each path is stored once.
//! - `NODE` kind/flags bytes and varint columns (`+1` encodes an absent ref).
//! - `EDGE` sorted by source: source delta, zig-zag target delta, one byte for
//!   kind and confidence, detail, line.
//! - `KEYX`/`NAMX` lookup permutations; `DIAG`, `FDIG`, `FCMP`, `ENTR` extras.
//!
//! Adjacency (both CSR directions) is rebuilt on load in O(V + E) instead of
//! being stored. Readers reject an unknown major version and ignore unknown
//! section tags, so additive sections never break older binaries.
use sha2::{Digest, Sha256};
use std::collections::HashMap;

pub(crate) const MAGIC: &[u8; 8] = b"OCGRAPH\0";
pub(crate) const FORMAT_VERSION: u32 = 2;
const HEADER_LEN: usize = 8 + 4 + 4 + 32;
const TABLE_ENTRY_LEN: usize = 4 + 8 + 8;
/// Sentinel for an absent `u32` reference (no file, no parent, no line).
pub(crate) const NONE: u32 = u32::MAX;
/// Node kind byte bit: the key column holds only the symbol's key suffix.
const KEY_SUFFIX: u8 = 0x80;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
#[repr(u8)]
pub(crate) enum NodeKind {
    File = 0,
    Symbol = 1,
    Package = 2,
}

impl NodeKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Symbol => "symbol",
            Self::Package => "package",
        }
    }
    fn from_u8(value: u8) -> Option<Self> {
        Some(match value {
            0 => Self::File,
            1 => Self::Symbol,
            2 => Self::Package,
            _ => return None,
        })
    }
    pub(crate) fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "file" => Self::File,
            "symbol" => Self::Symbol,
            "package" => Self::Package,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[repr(u8)]
pub(crate) enum EdgeKind {
    /// file → top-level symbol, symbol → member.
    Contains = 0,
    /// file → file (linked) or file → package (external).
    Imports = 1,
    /// symbol|file → symbol, resolved from syntax call sites.
    Calls = 2,
    /// file → exported symbol it imports by name (through re-exports), or
    /// file → file for a namespace import (`import * as ns`).
    Uses = 3,
    /// symbol → symbol: `extends` / `implements` (detail).
    Inherits = 4,
}

impl EdgeKind {
    pub(crate) const ALL: [Self; 5] = [
        Self::Contains,
        Self::Imports,
        Self::Calls,
        Self::Uses,
        Self::Inherits,
    ];
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Contains => "contains",
            Self::Imports => "imports",
            Self::Calls => "calls",
            Self::Uses => "uses",
            Self::Inherits => "inherits",
        }
    }
    fn from_u8(value: u8) -> Option<Self> {
        Some(match value {
            0 => Self::Contains,
            1 => Self::Imports,
            2 => Self::Calls,
            3 => Self::Uses,
            4 => Self::Inherits,
            _ => return None,
        })
    }
    pub(crate) fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "contains" => Self::Contains,
            "imports" => Self::Imports,
            "calls" => Self::Calls,
            "uses" => Self::Uses,
            "inherits" => Self::Inherits,
            _ => return None,
        })
    }
}

/// How an edge was linked; ordered from strongest to weakest evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
#[repr(u8)]
pub(crate) enum Confidence {
    High = 0,
    Medium = 1,
    Low = 2,
}

impl Confidence {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::High => "high",
            Self::Medium => "medium",
            Self::Low => "low",
        }
    }
    fn from_u8(value: u8) -> Option<Self> {
        Some(match value {
            0 => Self::High,
            1 => Self::Medium,
            2 => Self::Low,
            _ => return None,
        })
    }
}

pub(crate) const FLAG_EXPORTED: u8 = 1;
/// Symbol is test code: in a test file, a Rust test module, or a pytest
/// `test_*` function / `Test*` class.
pub(crate) const FLAG_TEST: u8 = 1 << 1;
/// Symbol is referenced inside its own file (a value reference, not only its
/// declaration): an unused export of it is "could be un-exported", not dead.
pub(crate) const FLAG_LOCAL_USE: u8 = 1 << 2;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct NodeRec {
    pub kind: NodeKind,
    pub flags: u8,
    /// Stable, human-typable id (`src/a.ts`, `src/a.ts#Foo.bar`, `pkg:react`).
    pub key: u32,
    pub name: u32,
    /// Symbol kind, file language, or package ecosystem.
    pub detail: u32,
    /// Owning file node.
    pub file: u32,
    /// Containing symbol node.
    pub parent: u32,
    pub line: u32,
    pub end_line: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct EdgeRec {
    pub src: u32,
    pub dst: u32,
    pub kind: EdgeKind,
    pub confidence: Confidence,
    /// Import relation or call resolution strategy.
    pub detail: u32,
    pub line: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DiagRec {
    pub file: u32,
    pub line: u32,
    pub code: u32,
    pub message: u32,
}

/// The decoded snapshot. Edges are sorted by `(src, kind, dst, line)`;
/// `out_offsets[n]..out_offsets[n + 1]` are node `n`'s outgoing edges and
/// `in_edges[in_offsets[n]..in_offsets[n + 1]]` index its incoming ones.
#[derive(Debug, Default)]
pub(crate) struct GraphTables {
    pub strings: Vec<String>,
    pub nodes: Vec<NodeRec>,
    pub edges: Vec<EdgeRec>,
    pub out_offsets: Vec<u32>,
    pub in_offsets: Vec<u32>,
    pub in_edges: Vec<u32>,
    /// Node ids sorted by key (exact lookup by binary search).
    pub key_index: Vec<u32>,
    /// Node ids sorted by `(lowercase name, id)` (name/prefix search).
    pub name_index: Vec<u32>,
    pub diagnostics: Vec<DiagRec>,
    /// `(file node, digest string)` for staleness checks.
    pub digests: Vec<(u32, u32)>,
    /// `(file node, component dir, component name, meta)`: owning build
    /// unit; meta is `ecosystem[;library][;templates]`.
    pub components: Vec<(u32, u32, u32, u32)>,
    /// `(file node, rule)`: inferred production entrypoints.
    pub entries: Vec<(u32, u32)>,
}

impl GraphTables {
    pub(crate) fn str(&self, id: u32) -> &str {
        self.strings.get(id as usize).map_or("", String::as_str)
    }

    /// Computes both CSR directions and both lookup indexes. Call once after
    /// `edges` are sorted; the writer persists the lookup indexes.
    pub(crate) fn index(&mut self) {
        self.index_adjacency();
        self.index_lookups();
    }

    /// Both CSR directions from `edges` (sorted by source), in O(V + E).
    pub(crate) fn index_adjacency(&mut self) {
        let n = self.nodes.len();
        self.out_offsets = vec![0; n + 1];
        self.in_offsets = vec![0; n + 1];
        for edge in &self.edges {
            self.out_offsets[edge.src as usize + 1] += 1;
            self.in_offsets[edge.dst as usize + 1] += 1;
        }
        for i in 0..n {
            self.out_offsets[i + 1] += self.out_offsets[i];
            self.in_offsets[i + 1] += self.in_offsets[i];
        }
        let mut cursor = self.in_offsets.clone();
        self.in_edges = vec![0; self.edges.len()];
        // Edges are sorted by src, so each node's incoming list is sorted by
        // (src, kind) as well: deterministic without a second sort.
        for (index, edge) in self.edges.iter().enumerate() {
            let slot = &mut cursor[edge.dst as usize];
            self.in_edges[*slot as usize] = index as u32;
            *slot += 1;
        }
    }

    fn index_lookups(&mut self) {
        let n = self.nodes.len();
        let mut keys = (0..n as u32).collect::<Vec<_>>();
        keys.sort_by(|a, b| {
            self.str(self.nodes[*a as usize].key)
                .cmp(self.str(self.nodes[*b as usize].key))
        });
        self.key_index = keys;
        let lower = self
            .nodes
            .iter()
            .map(|node| self.str(node.name).to_lowercase())
            .collect::<Vec<_>>();
        let mut names = (0..n as u32).collect::<Vec<_>>();
        names.sort_by(|a, b| lower[*a as usize].cmp(&lower[*b as usize]).then(a.cmp(b)));
        self.name_index = names;
    }

    pub(crate) fn out(&self, node: u32) -> &[EdgeRec] {
        let start = self.out_offsets[node as usize] as usize;
        let end = self.out_offsets[node as usize + 1] as usize;
        &self.edges[start..end]
    }

    pub(crate) fn incoming(&self, node: u32) -> impl Iterator<Item = &EdgeRec> {
        let start = self.in_offsets[node as usize] as usize;
        let end = self.in_offsets[node as usize + 1] as usize;
        self.in_edges[start..end]
            .iter()
            .map(|index| &self.edges[*index as usize])
    }

    pub(crate) fn by_key(&self, key: &str) -> Option<u32> {
        self.key_index
            .binary_search_by(|id| self.str(self.nodes[*id as usize].key).cmp(key))
            .ok()
            .map(|at| self.key_index[at])
    }
}

// ── Encoding ────────────────────────────────────────────────────────────────

struct Section {
    tag: [u8; 4],
    bytes: Vec<u8>,
}

fn put(out: &mut Vec<u8>, mut value: u64) {
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
fn put_opt(out: &mut Vec<u8>, value: u32) {
    put(
        out,
        if value == NONE {
            0
        } else {
            u64::from(value) + 1
        },
    );
}

fn zigzag(value: i64) -> u64 {
    ((value << 1) ^ (value >> 63)) as u64
}

fn unzigzag(value: u64) -> i64 {
    ((value >> 1) as i64) ^ -((value & 1) as i64)
}

fn varints(values: &[u32]) -> Vec<u8> {
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
    assemble(sections)
}

/// The string table every section references by id.
struct Encoder<'t> {
    tables: &'t GraphTables,
    texts: Vec<&'t str>,
    ids: HashMap<&'t str, u32>,
}

impl<'t> Encoder<'t> {
    fn new(tables: &'t GraphTables) -> Self {
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

    fn id(&self, text: &str) -> u32 {
        self.ids.get(text).copied().unwrap_or(0)
    }

    fn sid(&self, old: u32) -> u32 {
        self.id(self.tables.str(old))
    }

    /// Front-coded strings: shared prefix length, suffix length, suffix.
    fn strings(&self) -> Vec<u8> {
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

    fn nodes(&self) -> Vec<u8> {
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

    fn edges(&self) -> Vec<u8> {
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
            edges.push(((edge.kind as u8) << 2) | edge.confidence as u8);
            put(&mut edges, u64::from(self.sid(edge.detail)));
            put_opt(&mut edges, edge.line);
        }
        edges
    }

    fn diagnostics(&self) -> Vec<u8> {
        let mut diags = Vec::new();
        put(&mut diags, self.tables.diagnostics.len() as u64);
        for diag in &self.tables.diagnostics {
            put(&mut diags, u64::from(self.sid(diag.file)));
            put_opt(&mut diags, diag.line);
            put(&mut diags, u64::from(self.sid(diag.code)));
            put(&mut diags, u64::from(self.sid(diag.message)));
        }
        diags
    }
}

/// A node's key text: symbol keys shrink to their suffix after `path#`.
fn key_text(tables: &GraphTables, index: usize) -> (&str, bool) {
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
fn assemble(sections: Vec<Section>) -> (Vec<u8>, String) {
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

// ── Decoding ────────────────────────────────────────────────────────────────

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
    what: &'static str,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8], what: &'static str) -> Self {
        Self { bytes, at: 0, what }
    }
    fn truncated(&self) -> String {
        format!("graph section {} is truncated", self.what)
    }
    fn take(&mut self, len: usize) -> Result<&'a [u8], String> {
        let end = self
            .at
            .checked_add(len)
            .filter(|end| *end <= self.bytes.len())
            .ok_or_else(|| self.truncated())?;
        let slice = &self.bytes[self.at..end];
        self.at = end;
        Ok(slice)
    }
    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
    fn fixed_u32(&mut self) -> Result<u32, String> {
        let raw = self.take(4)?;
        Ok(u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]))
    }
    fn fixed_u64(&mut self) -> Result<u64, String> {
        let raw = self.take(8)?;
        let mut buf = [0u8; 8];
        buf.copy_from_slice(raw);
        Ok(u64::from_le_bytes(buf))
    }
    fn var(&mut self) -> Result<u64, String> {
        let mut value = 0u64;
        for shift in (0..70).step_by(7) {
            let byte = self.u8()?;
            value |= u64::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err(format!(
            "graph section {} has an overlong varint",
            self.what
        ))
    }
    fn var_u32(&mut self) -> Result<u32, String> {
        u32::try_from(self.var()?)
            .map_err(|_| format!("graph section {} value overflows", self.what))
    }
    fn var_opt(&mut self) -> Result<u32, String> {
        match self.var()? {
            0 => Ok(NONE),
            value => u32::try_from(value - 1).map_err(|_| self.truncated()),
        }
    }
    /// A varint count checked against the bytes left (each item needs one).
    fn count(&mut self) -> Result<usize, String> {
        let count = self.var()? as usize;
        if count > self.bytes.len() - self.at {
            return Err(self.truncated());
        }
        Ok(count)
    }
    fn var_vec(&mut self) -> Result<Vec<u32>, String> {
        let count = self.count()?;
        (0..count).map(|_| self.var_u32()).collect()
    }
}

/// Decodes and fully validates a snapshot: magic, version, body digest, and
/// every cross-reference, so queries can index without bounds panics.
pub(crate) fn decode(bytes: &[u8]) -> Result<(GraphTables, String), String> {
    if bytes.len() < HEADER_LEN || &bytes[..8] != MAGIC {
        return Err("not an octocode graph (bad magic); re-run `octocode graph ingest`".into());
    }
    let mut header = Reader::new(&bytes[8..HEADER_LEN], "header");
    let version = header.fixed_u32()?;
    if version != FORMAT_VERSION {
        return Err(format!(
            "graph format v{version} is not supported by this octocode (v{FORMAT_VERSION}); re-run `octocode graph ingest`"
        ));
    }
    let section_count = header.fixed_u32()? as usize;
    let stored = header.take(32)?;
    let body = &bytes[HEADER_LEN..];
    let digest = Sha256::digest(body);
    if digest.as_slice() != stored {
        return Err("graph checksum mismatch (file is corrupt or truncated); re-run `octocode graph ingest`".into());
    }
    let sections = Sections(read_section_table(body, section_count)?);
    let mut tables = GraphTables::default();
    read_strings(&mut sections.get(b"STRS", "STRS")?, &mut tables)?;
    // String ids stored in the file; symbol keys expanded below come after.
    let strings = tables.strings.len() as u32;
    read_nodes(&mut sections.get(b"NODE", "NODE")?, &mut tables)?;
    read_edges(&mut sections.get(b"EDGE", "EDGE")?, &mut tables, strings)?;
    tables.index_adjacency();
    let node_count = tables.nodes.len();
    tables.key_index = sections.get(b"KEYX", "KEYX")?.var_vec()?;
    tables.name_index = sections.get(b"NAMX", "NAMX")?.var_vec()?;
    if tables.key_index.len() != node_count
        || tables.name_index.len() != node_count
        || !tables
            .key_index
            .iter()
            .chain(&tables.name_index)
            .all(|id| (*id as usize) < node_count)
    {
        return Err("graph index sections are inconsistent".into());
    }
    read_file_tables(&sections, &mut tables, strings)?;
    Ok((tables, hex::encode(digest)))
}

/// Section tag → its bytes in the body.
struct Sections<'b>(std::collections::BTreeMap<[u8; 4], &'b [u8]>);

impl<'b> Sections<'b> {
    fn get(&self, tag: &[u8; 4], what: &'static str) -> Result<Reader<'b>, String> {
        self.0
            .get(tag)
            .map(|bytes| Reader::new(bytes, what))
            .ok_or_else(|| format!("graph section {what} is missing"))
    }
}

fn read_section_table(
    body: &[u8],
    section_count: usize,
) -> Result<std::collections::BTreeMap<[u8; 4], &[u8]>, String> {
    let mut table = Reader::new(body, "table");
    let mut sections = std::collections::BTreeMap::new();
    for _ in 0..section_count {
        let tag = table.take(4)?;
        let offset = table.fixed_u64()? as usize;
        let len = table.fixed_u64()? as usize;
        let start = offset
            .checked_sub(HEADER_LEN)
            .ok_or("graph section offset is invalid")?;
        let end = start
            .checked_add(len)
            .filter(|end| *end <= body.len())
            .ok_or("graph section exceeds the file")?;
        let mut key = [0u8; 4];
        key.copy_from_slice(tag);
        sections.insert(key, &body[start..end]);
    }
    Ok(sections)
}

fn read_strings(strs: &mut Reader, tables: &mut GraphTables) -> Result<(), String> {
    let count = strs.count()?;
    tables.strings.reserve(count);
    let mut previous = Vec::<u8>::new();
    for _ in 0..count {
        let shared = strs.var()? as usize;
        let suffix = strs.var()? as usize;
        if shared > previous.len() {
            return Err("graph string table is corrupt".into());
        }
        previous.truncate(shared);
        previous.extend_from_slice(strs.take(suffix)?);
        tables.strings.push(
            std::str::from_utf8(&previous)
                .map_err(|_| "graph string table is not UTF-8")?
                .to_owned(),
        );
    }
    Ok(())
}

fn read_nodes(nodes: &mut Reader, tables: &mut GraphTables) -> Result<(), String> {
    let count = nodes.count()?;
    tables.nodes.reserve(count);
    let mut suffixed = Vec::new();
    for index in 0..count {
        let kind_byte = nodes.u8()?;
        let kind =
            NodeKind::from_u8(kind_byte & !KEY_SUFFIX).ok_or("graph node kind is unknown")?;
        let flags = nodes.u8()?;
        let key = nodes.var_u32()?;
        let name = nodes.var_u32()?;
        let detail = nodes.var_u32()?;
        let file = nodes.var_opt()?;
        let parent = nodes.var_opt()?;
        let line = nodes.var_opt()?;
        let end_line = if line == NONE {
            nodes.var_opt()?
        } else {
            match nodes.var()? {
                0 => NONE,
                delta => u32::try_from(i64::from(line) + unzigzag(delta - 1))
                    .map_err(|_| "graph node range is invalid")?,
            }
        };
        if kind_byte & KEY_SUFFIX != 0 {
            suffixed.push(index);
        }
        tables.nodes.push(NodeRec {
            kind,
            flags,
            key,
            name,
            detail,
            file,
            parent,
            line,
            end_line,
        });
    }
    let strings = tables.strings.len() as u32;
    let node_count = tables.nodes.len() as u32;
    let optional_node_ok = |id: u32| id == NONE || id < node_count;
    if !tables.nodes.iter().all(|node| {
        node.key < strings
            && node.name < strings
            && node.detail < strings
            && optional_node_ok(node.file)
            && optional_node_ok(node.parent)
    }) {
        return Err("graph node references are out of range".into());
    }
    // Rebuild `path#suffix` keys for symbols stored as suffixes.
    for index in suffixed {
        let node = tables.nodes[index];
        if node.file == NONE {
            return Err("graph symbol key has no file".into());
        }
        let full = format!(
            "{}#{}",
            tables.str(tables.nodes[node.file as usize].key),
            tables.str(node.key)
        );
        tables.nodes[index].key = tables.strings.len() as u32;
        tables.strings.push(full);
    }
    Ok(())
}

fn read_edges(edges: &mut Reader, tables: &mut GraphTables, strings: u32) -> Result<(), String> {
    let node_count = tables.nodes.len() as u32;
    let count = edges.count()?;
    tables.edges.reserve(count);
    let mut src = 0u32;
    for _ in 0..count {
        src = src
            .checked_add(edges.var_u32()?)
            .ok_or("graph edge source overflows")?;
        let dst = u32::try_from(i64::from(src) + unzigzag(edges.var()?))
            .map_err(|_| "graph edge references are out of range")?;
        let packed = edges.u8()?;
        let kind = EdgeKind::from_u8(packed >> 2).ok_or("graph edge kind is unknown")?;
        let confidence =
            Confidence::from_u8(packed & 0b11).ok_or("graph edge confidence is unknown")?;
        let detail = edges.var_u32()?;
        let line = edges.var_opt()?;
        if src >= node_count || dst >= node_count || detail >= strings {
            return Err("graph edge references are out of range".into());
        }
        tables.edges.push(EdgeRec {
            src,
            dst,
            kind,
            confidence,
            detail,
            line,
        });
    }
    Ok(())
}

/// Diagnostics, file digests, components and entries.
fn read_file_tables(
    sections: &Sections,
    tables: &mut GraphTables,
    strings: u32,
) -> Result<(), String> {
    let node_count = tables.nodes.len() as u32;
    let string_ok = |id: u32| id < strings;
    let node_ok = |id: u32| id < node_count;
    let mut diags = sections.get(b"DIAG", "DIAG")?;
    let count = diags.count()?;
    for _ in 0..count {
        let diag = DiagRec {
            file: diags.var_u32()?,
            line: diags.var_opt()?,
            code: diags.var_u32()?,
            message: diags.var_u32()?,
        };
        if !string_ok(diag.file) || !string_ok(diag.code) || !string_ok(diag.message) {
            return Err("graph diagnostic references are out of range".into());
        }
        tables.diagnostics.push(diag);
    }
    for pair in sections.get(b"FDIG", "FDIG")?.var_vec()?.chunks_exact(2) {
        if !node_ok(pair[0]) || !string_ok(pair[1]) {
            return Err("graph digest references are out of range".into());
        }
        tables.digests.push((pair[0], pair[1]));
    }
    // Optional sections: absent in a snapshot means empty.
    if sections.0.contains_key(b"FCMP") {
        for quad in sections.get(b"FCMP", "FCMP")?.var_vec()?.chunks_exact(4) {
            if !node_ok(quad[0]) || !quad[1..].iter().all(|id| string_ok(*id)) {
                return Err("graph component references are out of range".into());
            }
            tables.components.push((quad[0], quad[1], quad[2], quad[3]));
        }
    }
    if sections.0.contains_key(b"ENTR") {
        for pair in sections.get(b"ENTR", "ENTR")?.var_vec()?.chunks_exact(2) {
            if !node_ok(pair[0]) || !string_ok(pair[1]) {
                return Err("graph entry references are out of range".into());
            }
            tables.entries.push((pair[0], pair[1]));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varints_and_zigzag_round_trip() {
        for value in [0u64, 1, 127, 128, 300, u64::from(u32::MAX), u64::MAX] {
            let mut out = Vec::new();
            put(&mut out, value);
            assert_eq!(Reader::new(&out, "t").var().unwrap(), value);
        }
        for value in [0i64, 1, -1, 1_000_000, -1_000_000] {
            assert_eq!(unzigzag(zigzag(value)), value);
        }
    }
}
