//! Writing record blocks: a section's bodies in id order and the directory
//! entries that commit to them.
use super::*;

/// One block of a section being written, in id order.
pub(in super::super) struct Body<'b> {
    pub first: u32,
    pub last: u32,
    pub count: u32,
    pub bytes: &'b [u8],
}

/// Writes one block; the writer owns where it goes.
pub(in super::super) type Emit<'e> = &'e mut dyn FnMut(Body<'_>) -> Result<(), ToolError>;

/// The first `count` strings of `texts` as `STRE` blocks. Each block restarts
/// its front coding (its first string shares no prefix), so one block decodes
/// alone and the concatenated stream still decodes in order.
pub(in super::super) fn string_blocks<T: AsRef<[u8]>>(
    texts: &[T],
    emit: Emit<'_>,
) -> Result<(), ToolError> {
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
pub(in super::super) fn record_blocks(
    tag: &[u8; 4],
    flat: &[u8],
    emit: Emit<'_>,
) -> Result<(), ToolError> {
    let mut r = Reader::new(flat, "encoder records");
    let count = r.rows().map_err(|error| corrupt(&error))?;
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
pub(in super::super) fn directory_entry(directory: &mut Vec<u8>, body: &Body<'_>, offset: u64) {
    directory.extend(body.first.to_le_bytes());
    directory.extend(body.last.to_le_bytes());
    directory.extend(offset.to_le_bytes());
    directory.extend((body.bytes.len() as u64).to_le_bytes());
    directory.extend(body.count.to_le_bytes());
    directory.extend(octocode_engine::digest::sha256_digest(body.bytes));
}

/// A blocked section's directory for `rows` rows, before its block entries.
pub(in super::super) fn directory_start(rows: usize) -> Result<Vec<u8>, ToolError> {
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
