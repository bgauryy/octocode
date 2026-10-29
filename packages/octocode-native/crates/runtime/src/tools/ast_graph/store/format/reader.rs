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
            if shift == 63 && byte > 1 {
                return Err(format!("graph section {} varint overflows", self.what));
            }
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
        if count > LIMITS.table_items || count > self.bytes.len() - self.at {
            return Err(self.truncated());
        }
        Ok(count)
    }
    pub(super) fn var_vec(&mut self) -> Result<Vec<u32>, String> {
        let count = self.count()?;
        (0..count).map(|_| self.var_u32()).collect()
    }
}

/// Decodes and fully validates a snapshot: magic, version, directory and block digests, and
/// every cross-reference, so queries can index without bounds panics.
#[cfg(test)]
pub(crate) fn decode(bytes: &[u8]) -> Result<(GraphTables, String), String> {
    decode_with_cancel(bytes, &crate::execution::cancel::NeverCancel)
}

pub(crate) fn decode_with_cancel(
    bytes: &[u8],
    cancel: &dyn crate::execution::cancel::CancellationCheck,
) -> Result<(GraphTables, String), String> {
    decode_with(bytes, Scope::Full, cancel)
}

/// Decodes what an unchanged-tree check reads: record strings, nodes and the
/// per-file tables (digests, stamps). Edges, adjacency and the lookup
/// indexes are not decoded; all stored blocks are still verified, so a corrupt
/// file is still rejected.
pub(crate) fn decode_files(bytes: &[u8]) -> Result<(GraphTables, String), String> {
    decode_with(bytes, Scope::Files, &crate::execution::cancel::NeverCancel)
}

/// The tables a decode builds from the sections it is given.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Scope {
    /// Record strings, nodes and the per-file tables.
    Files,
    /// Every table but occurrence witnesses, unresolved sites and the strings
    /// only they reference (`STRE`).
    Topology,
    /// Every table.
    Full,
}

pub(super) fn decode_with(
    bytes: &[u8],
    scope: Scope,
    cancel: &dyn crate::execution::cancel::CancellationCheck,
) -> Result<(GraphTables, String), String> {
    cancel.check()?;
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
    let directory_len = section_count
        .checked_mul(TABLE_ENTRY_LEN)
        .ok_or("graph directory overflows")?;
    let directory = body
        .get(..directory_len)
        .ok_or("graph directory is truncated")?;
    let digest = Sha256::digest(directory);
    if digest.as_slice() != stored {
        return Err("graph checksum mismatch (file is corrupt or truncated); re-run `octocode graph ingest`".into());
    }
    let sections = Sections(read_section_table(body, section_count)?);
    let tables = decode_tables(&sections, scope, cancel)?;
    Ok((tables, hex::encode(digest)))
}

pub(super) fn decode_tables(
    sections: &Sections,
    scope: Scope,
    cancel: &dyn crate::execution::cancel::CancellationCheck,
) -> Result<GraphTables, String> {
    let mut tables = GraphTables::default();
    read_strings(&mut sections.get(b"STRS", "STRS")?, &mut tables, &LIMITS)?;
    // Records and lookup indexes reference only these strings; occurrence
    // evidence may also reference the `STRE` strings that follow them.
    let records = tables.strings.len() as u32;
    read_nodes(&mut sections.get(b"NODE", "NODE")?, &mut tables)?;
    if scope == Scope::Files {
        read_file_tables(sections, &mut tables, records)?;
        return Ok(tables);
    }
    read_edges(&mut sections.get(b"EDGE", "EDGE")?, &mut tables, records)?;
    // Topology validates an origin when it renders one; a full decode
    // admits every origin.
    let admission = if scope == Scope::Full {
        super::sources::Admission::Every
    } else {
        super::sources::Admission::Accessed
    };
    super::sources::read(sections, &mut tables, admission, cancel)?;
    tables.declarations = super::evidence::read_declarations(
        &mut sections.get(b"DECL", "DECL")?,
        &tables,
        &tables.sources,
        records,
    )?;
    if scope == Scope::Full {
        read_strings(&mut sections.get(b"STRE", "STRE")?, &mut tables, &LIMITS)?;
        let strings = tables.strings.len() as u32;
        super::evidence::read_occurrences(sections, &mut tables, strings)?;
    }
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
    read_file_tables(sections, &mut tables, records)?;
    selective::validate_lookup(sections, &tables)?;
    Ok(tables)
}

/// Section tag → its bytes in the body.
pub(super) struct Sections<'a>(
    pub(super) std::collections::BTreeMap<[u8; 4], std::borrow::Cow<'a, [u8]>>,
);

impl Sections<'_> {
    pub(super) fn get(&self, tag: &[u8; 4], what: &'static str) -> Result<Reader<'_>, String> {
        self.0
            .get(tag)
            .map(|bytes| Reader::new(bytes, what))
            .ok_or_else(|| format!("graph section {what} is missing"))
    }
}

pub(super) fn read_section_table(
    body: &[u8],
    section_count: usize,
) -> Result<std::collections::BTreeMap<[u8; 4], std::borrow::Cow<'_, [u8]>>, String> {
    let mut table = Reader::new(body, "table");
    let mut sections = std::collections::BTreeMap::new();
    let mut previous_end = (HEADER_LEN + section_count * TABLE_ENTRY_LEN).next_multiple_of(8);
    if body
        .get(section_count * TABLE_ENTRY_LEN..previous_end - HEADER_LEN)
        .is_none_or(|padding| padding.iter().any(|byte| *byte != 0))
    {
        return Err("graph checksum mismatch (directory padding)".into());
    }
    for _ in 0..section_count {
        let mut tag = [0; 4];
        tag.copy_from_slice(table.take(4)?);
        let offset =
            usize::try_from(table.fixed_u64()?).map_err(|_| "graph section offset overflows")?;
        let len =
            usize::try_from(table.fixed_u64()?).map_err(|_| "graph section length overflows")?;
        let digest = table.take(32)?;
        if offset != previous_end {
            return Err("graph sections overlap or contain unindexed bytes".into());
        }
        let start = offset
            .checked_sub(HEADER_LEN)
            .ok_or("graph section offset is invalid")?;
        let end = start
            .checked_add(len)
            .filter(|end| *end <= body.len())
            .ok_or("graph section exceeds the file")?;
        let bytes = &body[start..end];
        if blocks::digest(&tag, bytes)?.as_slice() != digest {
            return Err("graph section checksum mismatch".into());
        }
        let decoded = if blocks::blocked(&tag) {
            std::borrow::Cow::Owned(blocks::unpack(&tag, bytes)?)
        } else {
            std::borrow::Cow::Borrowed(bytes)
        };
        if sections.insert(tag, decoded).is_some() {
            return Err("graph section tag is duplicated".into());
        }
        previous_end = offset
            .checked_add(len.next_multiple_of(8))
            .ok_or("graph section size overflows")?;
        let padding_end = previous_end - HEADER_LEN;
        if body
            .get(end..padding_end)
            .is_none_or(|padding| padding.iter().any(|byte| *byte != 0))
        {
            return Err("graph checksum mismatch (section padding)".into());
        }
    }
    if previous_end != body.len() + HEADER_LEN {
        return Err("graph has trailing unindexed bytes".into());
    }
    Ok(sections)
}
