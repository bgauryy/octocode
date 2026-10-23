//! Decoding: the bounded byte reader, the section table and the decode
//! entry points.
use super::*;

pub(super) fn unzigzag(value: u64) -> i64 {
    ((value >> 1) as i64) ^ -((value & 1) as i64)
}

// ── Decoding ────────────────────────────────────────────────────────────────

pub(super) struct Reader<'a> {
    pub(super) bytes: &'a [u8],
    pub(super) at: usize,
    pub(super) what: &'static str,
}

impl<'a> Reader<'a> {
    pub(super) fn new(bytes: &'a [u8], what: &'static str) -> Self {
        Self { bytes, at: 0, what }
    }
    pub(super) fn truncated(&self) -> String {
        format!("graph section {} is truncated", self.what)
    }
    pub(super) fn take(&mut self, len: usize) -> Result<&'a [u8], String> {
        let end = self
            .at
            .checked_add(len)
            .filter(|end| *end <= self.bytes.len())
            .ok_or_else(|| self.truncated())?;
        let slice = &self.bytes[self.at..end];
        self.at = end;
        Ok(slice)
    }
    pub(super) fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
    pub(super) fn fixed_u32(&mut self) -> Result<u32, String> {
        let raw = self.take(4)?;
        Ok(u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]))
    }
    pub(super) fn fixed_u64(&mut self) -> Result<u64, String> {
        let raw = self.take(8)?;
        let mut buf = [0u8; 8];
        buf.copy_from_slice(raw);
        Ok(u64::from_le_bytes(buf))
    }
    pub(super) fn var(&mut self) -> Result<u64, String> {
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
    pub(super) fn var_u32(&mut self) -> Result<u32, String> {
        u32::try_from(self.var()?)
            .map_err(|_| format!("graph section {} value overflows", self.what))
    }
    pub(super) fn var_opt(&mut self) -> Result<u32, String> {
        match self.var()? {
            0 => Ok(NONE),
            value => u32::try_from(value - 1).map_err(|_| self.truncated()),
        }
    }
    /// A varint count checked against the bytes left (each item needs one).
    pub(super) fn count(&mut self) -> Result<usize, String> {
        let count = usize::try_from(self.var()?).map_err(|_| self.truncated())?;
        if count > MAX_DECODED_ITEMS || count > self.bytes.len() - self.at {
            return Err(self.truncated());
        }
        Ok(count)
    }
    pub(super) fn var_vec(&mut self) -> Result<Vec<u32>, String> {
        let count = self.count()?;
        (0..count).map(|_| self.var_u32()).collect()
    }
}

/// Decodes and fully validates a snapshot: magic, version, body digest, and
/// every cross-reference, so queries can index without bounds panics.
pub(crate) fn decode(bytes: &[u8]) -> Result<(GraphTables, String), String> {
    decode_with(bytes, true)
}

/// [`decode`] of what an unchanged-tree check reads: strings, nodes and the
/// per-file tables (digests, stamps). Edges, adjacency and the lookup
/// indexes are not decoded; the body digest is still verified, so a corrupt
/// file is still rejected.
pub(crate) fn decode_files(bytes: &[u8]) -> Result<(GraphTables, String), String> {
    decode_with(bytes, false)
}

pub(super) fn decode_with(bytes: &[u8], full: bool) -> Result<(GraphTables, String), String> {
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
    if section_count > MAX_SECTIONS {
        return Err("graph section count exceeds the format limit".into());
    }
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
    if !full {
        read_file_tables(&sections, &mut tables, strings)?;
        return Ok((tables, hex::encode(digest)));
    }
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
pub(super) struct Sections<'b>(pub(super) std::collections::BTreeMap<[u8; 4], &'b [u8]>);

impl<'b> Sections<'b> {
    pub(super) fn get(&self, tag: &[u8; 4], what: &'static str) -> Result<Reader<'b>, String> {
        self.0
            .get(tag)
            .map(|bytes| Reader::new(bytes, what))
            .ok_or_else(|| format!("graph section {what} is missing"))
    }
}

pub(super) fn read_section_table(
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
