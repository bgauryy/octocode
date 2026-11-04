//! Independently authenticated record blocks. The directory commits to every body.
use super::*;
use crate::response::result::ToolError;

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
    if count > LIMITS.table_items || blocks > count.div_ceil(ROWS) {
        return Err("graph block directory count exceeds bounds".into());
    }
    PREFIX
        .checked_add(blocks * ENTRY)
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

/// One block of a section being written, in id order.
pub(super) struct Body<'b> {
    pub first: u32,
    pub last: u32,
    pub count: u32,
    pub bytes: &'b [u8],
}

/// Writes one block; the writer owns where it goes.
pub(super) type Emit<'e> = &'e mut dyn FnMut(Body<'_>) -> Result<(), ToolError>;

fn corrupt(message: &str) -> ToolError {
    ToolError::new(
        octocode_contract_types::ErrorCode::Decode,
        message.to_owned(),
    )
}

/// The first `count` strings of `texts` as `STRE` blocks. Each block restarts
/// its front coding (its first string shares no prefix), so one block decodes
/// alone and the concatenated stream still decodes in order.
pub(super) fn string_blocks<T: AsRef<[u8]>>(texts: &[T], emit: Emit<'_>) -> Result<(), ToolError> {
    let mut body = Vec::new();
    for (index, group) in texts.chunks(ROWS).enumerate() {
        body.clear();
        let mut previous: &[u8] = &[];
        for text in group {
            let bytes = text.as_ref();
            let shared = writer::shared_prefix(previous, bytes);
            writer::put(&mut body, shared as u64);
            writer::put(&mut body, (bytes.len() - shared) as u64);
            body.extend_from_slice(&bytes[shared..]);
            previous = bytes;
        }
        let (first, count) = first_and_count(index, group.len())?;
        emit(Body {
            first,
            last: first + count - 1,
            count,
            bytes: &body,
        })?;
    }
    Ok(())
}

fn first_and_count(index: usize, count: usize) -> Result<(u32, u32), ToolError> {
    let first = index
        .checked_mul(ROWS)
        .and_then(|first| u32::try_from(first).ok())
        .ok_or_else(|| corrupt("encoder block id overflows"))?;
    let count = u32::try_from(count).map_err(|_| corrupt("encoder block overflows"))?;
    Ok((first, count))
}

/// The blocks of a flat `NODE`, `DECL` or `SRCO` section.
pub(super) fn record_blocks(tag: &[u8; 4], flat: &[u8], emit: Emit<'_>) -> Result<(), ToolError> {
    let mut r = Reader::new(flat, "encoder records");
    let count = r.count().map_err(|error| corrupt(&error))?;
    if tag == b"SRCO" {
        return origin_blocks(flat, r, count, emit);
    }
    let mut body = Vec::new();
    let mut range = (0, 0);
    for index in 0..count {
        let start = r.at;
        let key = record_key(tag, &mut r, index).map_err(|error| corrupt(&error))?;
        if index % ROWS == 0 {
            range = (key, key);
        }
        range.1 = key;
        body.extend_from_slice(&flat[start..r.at]);
        if (index + 1) % ROWS == 0 || index + 1 == count {
            emit(Body {
                first: range.0,
                last: range.1,
                count: u32::try_from(index % ROWS + 1)
                    .map_err(|_| corrupt("encoder block overflows"))?,
                bytes: &body,
            })?;
            body.clear();
        }
    }
    if r.at != flat.len() {
        return Err(corrupt("encoder record count does not match its body"));
    }
    Ok(())
}

/// Reads past one record and returns the id its block directory keys it by.
fn record_key(tag: &[u8; 4], r: &mut Reader<'_>, index: usize) -> Result<u32, String> {
    if tag == b"NODE" {
        r.take(2)?;
        for _ in 0..7 {
            r.var()?;
        }
        return u32::try_from(index).map_err(|_| "encoder node id overflows".into());
    }
    r.var()?;
    let node = r.var_u32()?;
    for _ in 0..9 {
        r.var()?;
    }
    Ok(node)
}

/// The origin pool's index entries, then its records: each block takes 256
/// consecutive entries followed by exactly their records.
fn origin_blocks(
    flat: &[u8],
    mut r: Reader<'_>,
    count: usize,
    emit: Emit<'_>,
) -> Result<(), ToolError> {
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        let start = r.at;
        let length = r
            .var()
            .ok()
            .and_then(|length| usize::try_from(length).ok())
            .ok_or_else(|| corrupt("encoder origin overflows"))?;
        for _ in 0..5 {
            r.var().map_err(|error| corrupt(&error))?;
        }
        entries.push((start..r.at, length));
    }
    let mut record = r.at;
    let mut body = Vec::new();
    for (index, group) in entries.chunks(ROWS).enumerate() {
        let (first, group_len) = first_and_count(index, group.len())?;
        body.clear();
        for (range, _) in group {
            body.extend_from_slice(&flat[range.clone()]);
        }
        let length = group.iter().map(|(_, length)| length).sum::<usize>();
        let end = record
            .checked_add(length)
            .filter(|end| *end <= flat.len())
            .ok_or_else(|| corrupt("encoder origin records are truncated"))?;
        body.extend_from_slice(&flat[record..end]);
        record = end;
        emit(Body {
            first,
            last: first + group_len - 1,
            count: group_len,
            bytes: &body,
        })?;
    }
    if record != flat.len() {
        return Err(corrupt("encoder origin count does not match its body"));
    }
    Ok(())
}

/// The directory bytes one block adds, its body at `offset` in the section.
pub(super) fn directory_entry(directory: &mut Vec<u8>, body: &Body<'_>, offset: u64) {
    directory.extend(body.first.to_le_bytes());
    directory.extend(body.last.to_le_bytes());
    directory.extend(offset.to_le_bytes());
    directory.extend((body.bytes.len() as u64).to_le_bytes());
    directory.extend(body.count.to_le_bytes());
    directory.extend(octocode_engine::digest::sha256_digest(body.bytes));
}

/// A blocked section's directory for `rows` rows, before its block entries.
pub(super) fn directory_start(rows: usize) -> Result<Vec<u8>, ToolError> {
    let blocks = rows.div_ceil(ROWS);
    let mut directory = Vec::with_capacity(PREFIX + blocks * ENTRY);
    directory.extend(
        u32::try_from(rows)
            .map_err(|_| corrupt("encoder record count overflows"))?
            .to_le_bytes(),
    );
    directory.extend(
        u32::try_from(blocks)
            .map_err(|_| corrupt("encoder block count overflows"))?
            .to_le_bytes(),
    );
    Ok(directory)
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
