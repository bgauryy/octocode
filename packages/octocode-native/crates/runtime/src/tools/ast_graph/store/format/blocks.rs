//! Independently authenticated record blocks. The directory commits to every body.
use super::*;
use crate::response::result::ToolError;

mod encode;
pub(super) use encode::{Body, directory_entry, directory_start, record_blocks, string_blocks};

pub(super) const ROWS: usize = 256;
pub(super) const ENTRY: usize = 4 + 4 + 8 + 8 + 4 + 32;
pub(super) const PREFIX: usize = 8;

#[derive(Clone, Debug)]
pub(super) struct Block {
    pub first: u32,
    pub last: u32,
    pub offset: u64,
    pub len: u64,
    pub count: u32,
    pub digest: [u8; 32],
}

pub(super) fn blocked(tag: &[u8; 4]) -> bool {
    matches!(tag, b"NODE" | b"DECL" | b"STRE" | b"SRCO")
}

/// Sections stored deflated: the occurrence evidence (witnesses, unresolved
/// sites and the strings only they name) and the origin pool, most of a
/// snapshot's bytes, which only evidence and source pages read. Blocked
/// sections deflate each block body, the others their whole body. Every
/// byte survives: a stored body is the raw body's length, then the raw body
/// deflated, and it reads back to exactly that body. Topology sections stay
/// raw, so traversals pay no inflation.
pub(super) fn sealed(tag: &[u8; 4]) -> bool {
    matches!(tag, b"STRE" | b"SRCO" | b"WITN" | b"UNRS")
}

/// The stored form of one raw body of `tag`: a block's, or a whole
/// unblocked section's.
pub(super) fn seal(tag: &[u8; 4], raw: &[u8], limit: u64) -> Result<Vec<u8>, ToolError> {
    use std::io::Write;
    if !sealed(tag) {
        return Ok(raw.to_vec());
    }
    // Compression must not admit a body that `open` cannot inflate. The
    // writer's limit can be lower than the process limit in focused tests.
    if u64::try_from(raw.len())
        .ok()
        .is_none_or(|length| length > limit)
    {
        return Err(ToolError::new(
            octocode_contract_types::ErrorCode::ScopeTooBroad,
            format!(
                "expanded graph block exceeds the {limit}-byte snapshot limit; ingest a narrower scope"
            ),
        ));
    }
    let mut stored = Vec::with_capacity(raw.len() / 4 + 16);
    writer::put(&mut stored, raw.len() as u64);
    let mut deflate = flate2::write::DeflateEncoder::new(stored, flate2::Compression::fast());
    deflate
        .write_all(raw)
        .and_then(|()| deflate.finish())
        .map_err(|error| corrupt(&format!("cannot deflate a graph block: {error}")))
}

/// The raw body of one authenticated stored body of a [`sealed`] section.
/// It inflates to exactly its recorded length, which no body may make
/// larger than a whole snapshot.
pub(super) fn open(stored: &[u8]) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let mut r = Reader::new(stored, "sealed block");
    let length = usize::try_from(r.var()?)
        .ok()
        .filter(|length| *length <= LIMITS.snapshot_bytes)
        .ok_or("graph block length exceeds the snapshot bound")?;
    let mut raw = Vec::with_capacity(length);
    flate2::read::DeflateDecoder::new(&stored[r.at..])
        .take(length as u64 + 1)
        .read_to_end(&mut raw)
        .map_err(|_| "graph block does not inflate")?;
    if raw.len() != length {
        return Err("graph block inflates to a length other than its own".into());
    }
    Ok(raw)
}

/// One `SRCO` block body: its `count` origin index entries, then their
/// records. Returns the two parts and the records' total length.
pub(super) fn split_origin_block(body: &[u8], count: u32) -> Result<(&[u8], &[u8]), String> {
    let mut r = Reader::new(body, "SRCO");
    let mut length = 0usize;
    for _ in 0..count {
        length = length
            .checked_add(usize::try_from(r.var()?).map_err(|_| "graph origin length overflows")?)
            .ok_or("graph origin length overflows")?;
        for _ in 0..5 {
            r.var()?;
        }
    }
    let (entries, records) = body.split_at(r.at);
    if records.len() != length {
        return Err("graph origin block records disagree with its index".into());
    }
    Ok((entries, records))
}

pub(super) fn directory_len(bytes: &[u8]) -> Result<usize, String> {
    let mut r = Reader::new(bytes, "block directory");
    let count = r.fixed_u32()? as usize;
    let blocks = r.fixed_u32()? as usize;
    // Rows are bounded where they are read: a whole table by its reader's
    // row bound, an evidence block by its own 256 rows. The directory is
    // bounded by its section, which every caller checks before reading it.
    if blocks > count.div_ceil(ROWS) {
        return Err("graph block directory count exceeds bounds".into());
    }
    blocks
        .checked_mul(ENTRY)
        .and_then(|entries| entries.checked_add(PREFIX))
        .ok_or_else(|| "graph block directory overflows".into())
}

pub(super) fn parse(bytes: &[u8], section_len: u64) -> Result<(usize, Vec<Block>), String> {
    let length = directory_len(bytes)?;
    if length > bytes.len() {
        return Err("graph block directory is truncated".into());
    }
    let mut r = Reader::new(&bytes[..length], "block directory");
    let count = r.fixed_u32()? as usize;
    let block_count = r.fixed_u32()? as usize;
    let mut out = Vec::with_capacity(block_count);
    let mut total = 0usize;
    let mut next = length as u64;
    for _ in 0..block_count {
        let first = r.fixed_u32()?;
        let last = r.fixed_u32()?;
        let offset = r.fixed_u64()?;
        let len = r.fixed_u64()?;
        let count = r.fixed_u32()?;
        let mut digest = [0; 32];
        digest.copy_from_slice(r.take(32)?);
        if first > last
            || count == 0
            || count as usize > ROWS
            || offset != next
            || out.last().is_some_and(|b: &Block| b.last >= first)
        {
            return Err("graph block directory has inconsistent ranges".into());
        }
        next = offset
            .checked_add(len)
            .filter(|end| *end <= section_len)
            .ok_or("graph block exceeds section")?;
        total += count as usize;
        out.push(Block {
            first,
            last,
            offset,
            len,
            count,
            digest,
        });
    }
    if total != count || next != section_len {
        return Err("graph block directory does not cover its section".into());
    }
    Ok((count, out))
}

fn corrupt(message: &str) -> ToolError {
    ToolError::new(
        octocode_contract_types::ErrorCode::Decode,
        message.to_owned(),
    )
}

pub(super) fn digest(tag: &[u8; 4], bytes: &[u8]) -> Result<[u8; 32], String> {
    let length = if blocked(tag) {
        directory_len(bytes)?
    } else {
        bytes.len()
    };
    let bytes = bytes
        .get(..length)
        .ok_or("graph section directory is truncated")?;
    Ok(octocode_engine::digest::sha256_digest(bytes))
}

pub(super) fn unpack(tag: &[u8; 4], bytes: &[u8]) -> Result<Vec<u8>, String> {
    let (count, blocks) = parse(bytes, bytes.len() as u64)?;
    validate_ranges(tag, &blocks, count)?;
    let mut out = Vec::new();
    writer::put(&mut out, count as u64);
    let mut records = Vec::new();
    for block in blocks {
        let body = usize::try_from(block.offset)
            .ok()
            .zip(usize::try_from(block.offset + block.len).ok())
            .and_then(|(start, end)| bytes.get(start..end))
            .ok_or("graph record block is outside its section")?;
        if octocode_engine::digest::sha256_digest(body).as_slice() != block.digest {
            return Err("graph record block checksum mismatch".into());
        }
        let opened;
        let body = if sealed(tag) {
            opened = open(body)?;
            opened.as_slice()
        } else {
            body
        };
        if tag == b"SRCO" {
            // Back to the pool layout: every index entry, then every record.
            let (entries, body) = split_origin_block(body, block.count)?;
            out.extend(entries);
            records.extend_from_slice(body);
        } else {
            out.extend(body);
        }
    }
    out.extend(records);
    Ok(out)
}

pub(super) fn validate_ranges(tag: &[u8; 4], blocks: &[Block], count: usize) -> Result<(), String> {
    // Node and string blocks hold consecutive ids, so an id names its block.
    if matches!(tag, b"NODE" | b"STRE" | b"SRCO") {
        let mut next = 0u32;
        for block in blocks {
            if block.first != next
                || u64::from(block.last) - u64::from(block.first) + 1 != u64::from(block.count)
            {
                return Err("graph node block directory has gaps".into());
            }
            next += block.count;
        }
        if next as usize != count {
            return Err("graph node block directory population differs".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
#[cfg(test)]
pub(super) use tests::pack;
