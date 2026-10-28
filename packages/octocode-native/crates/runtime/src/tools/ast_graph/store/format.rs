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

pub(crate) mod arguments;
mod evidence;
mod indexes;
mod limits;
mod reader;
mod sections;
pub(crate) mod sources;
mod writer;
use reader::*;
pub(crate) use reader::{decode_files, decode_with_cancel};
use sections::*;
pub(crate) use writer::encode_for_publish;
#[cfg(test)]
pub(crate) use {reader::decode, writer::encode};

pub(crate) const MAGIC: &[u8; 8] = b"OCGRAPH\0";
/// v3: every file `imports` edge carries the line of its own kind's evidence.
/// v4: fact diagnostics are typed, so a Rust module-attribute note stores its
/// line in the record instead of in its message.
/// v5: a diagnostic names the file node it is about (`target`), so
/// `dependents` finds the unlinked path imports of a file.
/// v6: an edge records whether its evidence is test code (`test`).
/// v7: occurrence witnesses, unresolved sites and declaration evidence.
/// v8: occurrence byte spans and decoded-source derivation provenance.
/// v9: typed named-argument observations, separate from invocation.
/// v10: captured raw-source correspondence and per-span analysis units.
/// v11: bounded typed source catalogs and compact numeric-reference origin pools.
pub(crate) const FORMAT_VERSION: u32 = 11;
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
    /// Source owner passes a named value; this does not invoke it.
    Passes = 5,
}

impl EdgeKind {
    pub(crate) const ALL: [Self; 6] = [
        Self::Contains,
        Self::Imports,
        Self::Calls,
        Self::Uses,
        Self::Inherits,
        Self::Passes,
    ];
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Contains => "contains",
            Self::Imports => "imports",
            Self::Calls => "calls",
            Self::Uses => "uses",
            Self::Inherits => "inherits",
            Self::Passes => "passes",
        }
    }
    fn from_u8(value: u8) -> Option<Self> {
        Some(match value {
            0 => Self::Contains,
            1 => Self::Imports,
            2 => Self::Calls,
            3 => Self::Uses,
            4 => Self::Inherits,
            5 => Self::Passes,
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
            "passes" => Self::Passes,
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

/// Every occurrence supporting topology, before adjacency deduplication.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct WitnessRec {
    pub edge: EdgeRec,
    pub provenance: ProvenanceRec,
    pub argument: u32,
    /// Occurrence identity within this source snapshot (interned string).
    pub occurrence: u32,
}

/// A source call site whose target could not be established by this model.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct UnresolvedRec {
    pub provenance: ProvenanceRec,
    pub argument: u32,
    pub relation: EdgeKind,
    pub src: u32,
    pub occurrence: u32,
    pub callee: u32,
    pub line: u32,
    pub reason: u32,
}

/// Parser-input identity and half-open byte coordinates; `NONE` means unavailable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ProvenanceRec {
    pub source_digest: u32,
    pub source_start: u32,
    pub source_end: u32,
    pub source_origin: u32,
    pub derivation_version: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DeclarationRec {
    pub test_identity: u32,
    pub node: u32,
    pub occurrence: u32,
    pub source_digest: u32,
    pub source_start: u32,
    pub source_end: u32,
    pub source_origin: u32,
    pub selection_start: u32,
    pub selection_end: u32,
    pub selection_origin: u32,
    /// Serialized typed parse assessment; only enums and source positions.
    pub assessment: u32,
}

/// The decoded snapshot. Edges are sorted by `(src, kind, dst)`, then
/// production evidence before test evidence, then `line`;
/// `out_offsets[n]..out_offsets[n + 1]` are node `n`'s outgoing edges and
/// `in_edges[in_offsets[n]..in_offsets[n + 1]]` index its incoming ones.
#[derive(Debug, Default)]
pub(crate) struct GraphTables {
    pub sources: sources::SourceTable,
    pub strings: Vec<String>,
    pub nodes: Vec<NodeRec>,
    pub edges: Vec<EdgeRec>,
    pub witnesses: Vec<WitnessRec>,
    pub unresolved: Vec<UnresolvedRec>,
    pub declarations: Vec<DeclarationRec>,
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

#[cfg(test)]
pub(crate) mod evidence_tests;
#[cfg(test)]
mod tests;
