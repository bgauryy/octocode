//! File-backed snapshot access. Only authenticated, requested bodies are decoded.
use super::*;
use crate::execution::cancel::CancellationCheck;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

mod evidence_pages;
mod lookup;
mod records;
use lookup::Degrees;
pub(super) use lookup::{encode_lookup, validate_lookup};

pub(crate) fn full_validation(
    bytes: &[u8],
    tables: &GraphTables,
) -> Result<serde_json::Value, String> {
    let mut header = Reader::new(&bytes[8..HEADER_LEN], "header");
    header.fixed_u32()?;
    let count = header.fixed_u32()? as usize;
    let mut total = 1 + count;
    let mut directory = Reader::new(&bytes[HEADER_LEN..], "directory");
    for _ in 0..count {
        let tag = directory.take(4)?;
        let offset = directory.fixed_u64()? as usize;
        let len = directory.fixed_u64()? as usize;
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

#[derive(Debug)]
struct SectionRef {
    offset: u64,
    len: u64,
    digest: [u8; 32],
}

#[derive(Debug)]
pub(crate) struct SnapshotReader {
    file: File,
    sections: BTreeMap<[u8; 4], SectionRef>,
    blocks: BTreeMap<[u8; 4], (usize, Vec<blocks::Block>)>,
    checked: BTreeSet<([u8; 4], Option<usize>)>,
    degrees: Vec<Degrees>,
    node_cache: Option<(usize, Vec<NodeRec>)>,
    declaration_cache: Option<(usize, Vec<DeclarationRec>)>,
    sources: Option<sources::SourceTable>,
    bytes_read: u64,
    snapshot_bytes: u64,
    decoded_nodes: usize,
    decoded_edges: usize,
    decoded_occurrences: usize,
    point: bool,
    /// Set by [`Self::evidence_tables`]: evidence strings are read per block.
    evidence: Option<evidence_pages::EvidenceStrings>,
}

impl SnapshotReader {
    pub(crate) fn open(
        path: &std::path::Path,
        expected: &str,
        cancel: &dyn CancellationCheck,
    ) -> Result<Self, String> {
        let mut file = File::open(path).map_err(|e| e.to_string())?;
        let size = file.metadata().map_err(|e| e.to_string())?.len();
        if size > LIMITS.snapshot_bytes as u64 {
            return Err("graph exceeds snapshot byte limit; narrow the ingest scope".into());
        }
        let mut header = [0; HEADER_LEN];
        file.read_exact(&mut header).map_err(|e| e.to_string())?;
        if &header[..8] != MAGIC {
            return Err("not an octocode graph (bad magic)".into());
        }
        let mut r = Reader::new(&header[8..], "header");
        let version = r.fixed_u32()?;
        if version != FORMAT_VERSION {
            return Err(format!(
                "graph format v{version} is not supported (v{FORMAT_VERSION}); re-run `octocode graph ingest`"
            ));
        }
        let count = r.fixed_u32()? as usize;
        if count > MAX_SECTIONS {
            return Err("graph section count exceeds format limit".into());
        }
        let expected_header = r.take(32)?;
        let mut directory = vec![0; count * TABLE_ENTRY_LEN];
        file.read_exact(&mut directory).map_err(|e| e.to_string())?;
        let digest = Sha256::digest(&directory);
        if digest.as_slice() != expected_header || hex::encode(digest) != expected {
            return Err("graph directory checksum does not match its manifest".into());
        }
        let mut r = Reader::new(&directory, "directory");
        let mut sections = BTreeMap::new();
        let mut next = (HEADER_LEN + directory.len()).next_multiple_of(8) as u64;
        for _ in 0..count {
            let mut tag = [0; 4];
            tag.copy_from_slice(r.take(4)?);
            let offset = r.fixed_u64()?;
            let len = r.fixed_u64()?;
            let mut digest = [0; 32];
            digest.copy_from_slice(r.take(32)?);
            if offset != next {
                return Err("graph sections overlap or contain unindexed bytes".into());
            }
            next = offset
                .checked_add(len.checked_add(7).ok_or("section length overflow")? & !7)
                .filter(|end| *end <= size)
                .ok_or("graph section exceeds file")?;
            if sections
                .insert(
                    tag,
                    SectionRef {
                        offset,
                        len,
                        digest,
                    },
                )
                .is_some()
            {
                return Err("graph section tag is duplicated".into());
            }
        }
        if next != size {
            return Err("graph has trailing unindexed bytes".into());
        }
        let mut reader = Self {
            file,
            sections,
            blocks: BTreeMap::new(),
            checked: BTreeSet::new(),
            degrees: Vec::new(),
            node_cache: None,
            declaration_cache: None,
            sources: None,
            bytes_read: (HEADER_LEN + directory.len()) as u64,
            snapshot_bytes: size,
            decoded_nodes: 0,
            decoded_edges: 0,
            decoded_occurrences: 0,
            point: false,
            evidence: None,
        };
        for tag in [*b"NODE", *b"DECL", *b"STRE", *b"SRCO"] {
            let section = reader
                .sections
                .get(&tag)
                .ok_or("graph record section is missing")?;
            let (offset, len, digest) = (section.offset, section.len, section.digest);
            let prefix = reader.read_at(offset, blocks::PREFIX as u64, cancel)?;
            let length = blocks::directory_len(&prefix)?;
            if length as u64 > len {
                return Err("graph block directory exceeds section".into());
            }
            let directory = reader.read_at(offset, length as u64, cancel)?;
            if Sha256::digest(&directory).as_slice() != digest {
                return Err("graph block directory checksum mismatch".into());
            }
            let parsed = blocks::parse(&directory, len)?;
            blocks::validate_ranges(&tag, &parsed.1, parsed.0)?;
            reader.blocks.insert(tag, parsed);
            reader.checked.insert((tag, None));
        }
        Ok(reader)
    }

    fn read_at(
        &mut self,
        offset: u64,
        length: u64,
        cancel: &dyn CancellationCheck,
    ) -> Result<Vec<u8>, String> {
        if offset
            .checked_add(length)
            .is_none_or(|end| end > self.snapshot_bytes)
        {
            return Err("graph read exceeds snapshot".into());
        }
        let mut bytes =
            vec![0; usize::try_from(length).map_err(|_| "graph read length overflows")?];
        self.file
            .seek(SeekFrom::Start(offset))
            .map_err(|e| e.to_string())?;
        for chunk in bytes.chunks_mut(64 * 1024) {
            cancel.check()?;
            self.file.read_exact(chunk).map_err(|e| e.to_string())?;
            self.bytes_read += chunk.len() as u64;
        }
        Ok(bytes)
    }

    fn section(&mut self, tag: [u8; 4], cancel: &dyn CancellationCheck) -> Result<Vec<u8>, String> {
        let section = self
            .sections
            .get(&tag)
            .ok_or_else(|| format!("graph section {} is missing", String::from_utf8_lossy(&tag)))?;
        let (offset, len, digest) = (section.offset, section.len, section.digest);
        let bytes = self.read_at(offset, len, cancel)?;
        if blocks::digest(&tag, &bytes)? != digest {
            return Err("graph section checksum mismatch".into());
        }
        self.checked.insert((tag, None));
        Ok(bytes)
    }

    fn record_block(
        &mut self,
        tag: [u8; 4],
        index: usize,
        cancel: &dyn CancellationCheck,
    ) -> Result<Vec<u8>, String> {
        let block = &self.blocks[&tag].1[index];
        let (offset, len, count, digest) = (
            self.sections[&tag].offset + block.offset,
            block.len,
            block.count,
            block.digest,
        );
        let bytes = self.read_at(offset, len, cancel)?;
        if Sha256::digest(&bytes).as_slice() != digest {
            return Err("graph record block checksum mismatch".into());
        }
        self.checked.insert((tag, Some(index)));
        let mut out = Vec::with_capacity(bytes.len() + 5);
        writer::put(&mut out, u64::from(count));
        out.extend(bytes);
        Ok(out)
    }

    pub(crate) fn is_point(&self) -> bool {
        self.point
    }

    /// Whether origins and source records are read through this reader
    /// rather than from decoded tables.
    pub(crate) fn reads_sources(&self) -> bool {
        self.point || self.evidence.is_some()
    }

    pub(crate) fn topology_tables(
        &mut self,
        cancel: &dyn CancellationCheck,
    ) -> Result<GraphTables, String> {
        let mut sections = BTreeMap::new();
        let tags = self.sections.keys().copied().collect::<Vec<_>>();
        for tag in tags {
            // Occurrence evidence and the strings only it references.
            if matches!(&tag, b"STRE" | b"WITN" | b"UNRS") {
                continue;
            }
            let bytes = self.section(tag, cancel)?;
            let bytes = if blocks::blocked(&tag) {
                let flat = blocks::unpack(&tag, &bytes)?;
                for index in 0..self.blocks[&tag].1.len() {
                    self.checked.insert((tag, Some(index)));
                }
                flat
            } else {
                bytes
            };
            sections.insert(tag, std::borrow::Cow::Owned(bytes));
        }
        let tables = reader::decode_tables(&Sections(sections), reader::Scope::Topology, cancel)?;
        self.decoded_nodes = tables.nodes.len();
        self.decoded_edges = tables.edges.len();
        Ok(tables)
    }

    pub(crate) fn point_tables(
        &mut self,
        keep_degrees: bool,
        cancel: &dyn CancellationCheck,
    ) -> Result<GraphTables, String> {
        self.point = true;
        let mut t = GraphTables::default();
        read_strings(
            &mut Reader::new(&self.section(*b"STRS", cancel)?, "STRS"),
            &mut t,
            &LIMITS,
        )?;
        let (nodes, degrees) =
            lookup::read_lookup(&self.section(*b"LOOK", cancel)?, &t, keep_degrees)?;
        if nodes.len() != self.blocks[b"NODE"].0 {
            return Err("graph lookup node count differs from records".into());
        }
        if self.blocks[b"DECL"]
            .1
            .last()
            .is_some_and(|block| block.last as usize >= nodes.len())
        {
            return Err("graph declaration directory references an absent node".into());
        }
        t.nodes = nodes;
        self.degrees = degrees;
        t.key_index = Reader::new(&self.section(*b"KEYX", cancel)?, "KEYX").var_vec()?;
        t.name_index = Reader::new(&self.section(*b"NAMX", cancel)?, "NAMX").var_vec()?;
        lookup::validate_indexes(&t)?;
        let mut files = BTreeMap::new();
        for tag in [*b"DIAG", *b"FDIG", *b"FSTM"] {
            if tag != *b"FSTM" || self.sections.contains_key(&tag) {
                files.insert(tag, std::borrow::Cow::Owned(self.section(tag, cancel)?));
            }
        }
        let string_count = t.strings.len() as u32;
        read_file_tables(&Sections(files), &mut t, string_count)?;

        Ok(t)
    }

    /// The source table, authenticated and decoded on first use: a point
    /// lookup that renders no declaration or origin never reads it.
    pub(crate) fn sources(
        &mut self,
        cancel: &dyn CancellationCheck,
    ) -> Result<&sources::SourceTable, String> {
        if self.sources.is_none() {
            let bytes = self.section(*b"SRCE", cancel)?;
            self.sources = Some(sources::read_catalog(
                &bytes,
                self.blocks[b"SRCO"].0,
                cancel,
            )?);
        }
        self.sources
            .as_ref()
            .ok_or_else(|| "graph source table is absent".into())
    }

    /// Reads and authenticates the `SRCO` block holding origin `reference`
    /// unless it is already loaded.
    pub(crate) fn load_origin(
        &mut self,
        reference: u32,
        cancel: &dyn CancellationCheck,
    ) -> Result<(), String> {
        if self.sources(cancel)?.has_origin(reference) {
            return Ok(());
        }
        let index = self.blocks[b"SRCO"]
            .1
            .iter()
            .position(|block| block.first <= reference && reference <= block.last)
            .ok_or("graph source origin reference is out of bounds")?;
        let (first, count) = {
            let block = &self.blocks[b"SRCO"].1[index];
            (block.first, block.count)
        };
        let bytes = self.record_block(*b"SRCO", index, cancel)?;
        // `record_block` prefixes the block's record count.
        let mut prefix = Reader::new(&bytes, "SRCO");
        prefix.var()?;
        let body = &bytes[prefix.at..];
        self.sources
            .as_mut()
            .ok_or("graph source table is absent")?
            .insert_origin_block(first, count, body, cancel)
    }

    pub(crate) fn degrees(&self, node: u32, incoming: bool) -> BTreeMap<&'static str, u64> {
        let degrees = &self.degrees[node as usize];
        let counts = if incoming {
            &degrees.incoming
        } else {
            &degrees.outgoing
        };
        EdgeKind::ALL
            .into_iter()
            .zip(counts)
            .filter(|(_, count)| **count != 0)
            .map(|(kind, count)| (kind.as_str(), u64::from(*count)))
            .collect()
    }

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
            "decodedNodes":self.decoded_nodes, "decodedEdges":self.decoded_edges, "decodedWitnesses":self.decoded_occurrences})
    }
}
