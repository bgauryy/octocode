//! Independently authenticated record blocks. The directory commits to every body.
use super::*;

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

pub(super) fn pack(tag: &[u8; 4], flat: &[u8]) -> Result<Vec<u8>, String> {
    let mut r = Reader::new(flat, "encoder records");
    let count = r.count()?;
    let mut chunks = Vec::<(u32, u32, u32, Vec<u8>)>::new();
    if tag == b"STRE" {
        // Each block restarts its front coding (its first string shares no
        // prefix), so one block decodes alone and the concatenated stream
        // still decodes in order.
        let mut previous = Vec::<u8>::new();
        for index in 0..count {
            let shared = usize::try_from(r.var()?).map_err(|_| "encoder string overflows")?;
            let suffix = usize::try_from(r.var()?).map_err(|_| "encoder string overflows")?;
            if shared > previous.len() {
                return Err("encoder string table is corrupt".into());
            }
            previous.truncate(shared);
            previous.extend_from_slice(r.take(suffix)?);
            if index % ROWS == 0 {
                chunks.push((index as u32, index as u32, 0, Vec::new()));
            }
            let chunk = chunks.last_mut().ok_or("encoder string chunk is absent")?;
            let restart = chunk.2 == 0;
            let shared = if restart { 0 } else { shared };
            chunk.1 = index as u32;
            chunk.2 += 1;
            writer::put(&mut chunk.3, shared as u64);
            writer::put(&mut chunk.3, (previous.len() - shared) as u64);
            chunk.3.extend_from_slice(&previous[shared..]);
        }
        if r.at != flat.len() {
            return Err("encoder string count does not match its body".into());
        }
        return Ok(directory_and_bodies(count, chunks));
    }
    if tag == b"SRCO" {
        // The origin pool's index entries, then its records: each block takes
        // 256 consecutive entries followed by exactly their records.
        let mut entries = Vec::with_capacity(count);
        for _ in 0..count {
            let start = r.at;
            let length = usize::try_from(r.var()?).map_err(|_| "encoder origin overflows")?;
            for _ in 0..5 {
                r.var()?;
            }
            entries.push((start..r.at, length));
        }
        let mut record = r.at;
        for (index, group) in entries.chunks(ROWS).enumerate() {
            let first = (index * ROWS) as u32;
            let mut body = Vec::new();
            for (range, _) in group {
                body.extend_from_slice(&flat[range.clone()]);
            }
            let length = group.iter().map(|(_, length)| length).sum::<usize>();
            let end = record
                .checked_add(length)
                .filter(|end| *end <= flat.len())
                .ok_or("encoder origin records are truncated")?;
            body.extend_from_slice(&flat[record..end]);
            record = end;
            chunks.push((
                first,
                first + group.len() as u32 - 1,
                group.len() as u32,
                body,
            ));
        }
        if record != flat.len() {
            return Err("encoder origin count does not match its body".into());
        }
        return Ok(directory_and_bodies(count, chunks));
    }
    for index in 0..count {
        let start = r.at;
        let key = if tag == b"NODE" {
            r.take(2)?;
            for _ in 0..7 {
                r.var()?;
            }
            index as u32
        } else {
            r.var()?;
            let node = r.var_u32()?;
            for _ in 0..9 {
                r.var()?;
            }
            node
        };
        if index % ROWS == 0 {
            chunks.push((key, key, 0, Vec::new()));
        }
        let chunk = chunks.last_mut().ok_or("encoder record chunk is absent")?;
        chunk.1 = key;
        chunk.2 += 1;
        chunk.3.extend_from_slice(&flat[start..r.at]);
    }
    if r.at != flat.len() {
        return Err("encoder record count does not match its body".into());
    }
    Ok(directory_and_bodies(count, chunks))
}

fn directory_and_bodies(count: usize, chunks: Vec<(u32, u32, u32, Vec<u8>)>) -> Vec<u8> {
    let bytes = chunks.iter().map(|chunk| chunk.3.len()).sum::<usize>();
    let mut out = Vec::with_capacity(bytes + PREFIX + chunks.len() * ENTRY);
    out.extend((count as u32).to_le_bytes());
    out.extend((chunks.len() as u32).to_le_bytes());
    let mut offset = (PREFIX + chunks.len() * ENTRY) as u64;
    for (first, last, count, bytes) in &chunks {
        out.extend(first.to_le_bytes());
        out.extend(last.to_le_bytes());
        out.extend(offset.to_le_bytes());
        out.extend((bytes.len() as u64).to_le_bytes());
        out.extend(count.to_le_bytes());
        out.extend(Sha256::digest(bytes));
        offset += bytes.len() as u64;
    }
    for (_, _, _, bytes) in chunks {
        out.extend(bytes);
    }
    out
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
    Ok(Sha256::digest(bytes).into())
}

pub(super) fn unpack(tag: &[u8; 4], bytes: &[u8]) -> Result<Vec<u8>, String> {
    let (count, blocks) = parse(bytes, bytes.len() as u64)?;
    validate_ranges(tag, &blocks, count)?;
    let mut out = Vec::new();
    writer::put(&mut out, count as u64);
    let mut records = Vec::new();
    for block in blocks {
        let body = &bytes[block.offset as usize..(block.offset + block.len) as usize];
        if Sha256::digest(body).as_slice() != block.digest {
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
mod tests {
    use super::*;

    #[test]
    fn malformed_encoding_and_extreme_block_ranges_return_errors() {
        assert!(pack(b"NODE", &[1, 0]).is_err());
        let block = Block {
            first: 0,
            last: u32::MAX,
            offset: 0,
            len: 1,
            count: 1,
            digest: [0; 32],
        };
        assert!(validate_ranges(b"NODE", &[block], 1).is_err());
    }
}
