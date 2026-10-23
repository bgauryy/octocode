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
//! - `FSTM` (optional) size and change times of the bytes each `FDIG` digest
//!   hashed, so an unchanged-tree check can skip the re-hash.
//!
//! Adjacency (both CSR directions) is rebuilt on load in O(V + E) instead of
//! being stored. Readers reject an unknown major version and ignore unknown
//! section tags, so additive sections never break older binaries.
use sha2::{Digest, Sha256};
use std::collections::HashMap;

mod reader;
mod sections;
mod writer;
use reader::*;
pub(crate) use reader::{decode, decode_files};
use sections::*;
pub(crate) use writer::encode;

pub(crate) const MAGIC: &[u8; 8] = b"OCGRAPH\0";
/// v3: every file `imports` edge carries the line of its own kind's evidence.
/// v4: fact diagnostics are typed, so a Rust module-attribute note stores its
/// line in the record instead of in its message.
/// v5: a diagnostic names the file node it is about (`target`), so
/// `dependents` finds the unlinked path imports of a file.
/// v6: an edge records whether its evidence is test code (`test`).
pub(crate) const FORMAT_VERSION: u32 = 6;
/// Packed edge byte bit: the edge's evidence is test code.
const EDGE_TEST: u8 = 0x80;
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
    /// Every site of this edge is test code: a test file, or an item
    /// compiled only under test (Rust `#[cfg(test)]`). Production
    /// detectors, `cycles` and `hubs` leave it out; traversals keep it.
    pub test: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DiagRec {
    pub file: u32,
    pub line: u32,
    pub code: u32,
    pub message: u32,
    /// File node the diagnostic is about besides `file`, or [`NONE`].
    pub target: u32,
}

/// The decoded snapshot. Edges are sorted by `(src, kind, dst)`, then
/// production evidence before test evidence, then `line`;
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
    /// `(file node, stamp)` of the bytes each digest hashed, for files that
    /// were settled when read: an equal stamp skips the re-hash.
    pub stamps: Vec<(u32, octocode_engine::stamp::SourceStamp)>,
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

    /// Recorded source stamps by file node.
    pub(crate) fn stamps_by_node(&self) -> HashMap<u32, octocode_engine::stamp::SourceStamp> {
        self.stamps.iter().copied().collect()
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

#[cfg(test)]
mod tests;
