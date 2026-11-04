use super::*;

/// A flat section as its stored bytes, for fixtures that assemble a snapshot
/// by hand.
pub(in super::super) fn pack(tag: &[u8; 4], flat: &[u8]) -> Result<Vec<u8>, ToolError> {
    let mut r = Reader::new(flat, "encoder records");
    let rows = r.count().map_err(|error| corrupt(&error))?;
    let mut directory = directory_start(rows)?;
    let mut bodies = Vec::new();
    let start = (PREFIX + rows.div_ceil(ROWS) * ENTRY) as u64;
    let mut emit = |body: Body<'_>| {
        directory_entry(&mut directory, &body, start + bodies.len() as u64);
        bodies.extend_from_slice(body.bytes);
        Ok(())
    };
    if tag == b"STRE" {
        string_blocks(
            &flat_strings(&mut r, rows).map_err(|e| corrupt(&e))?,
            &mut emit,
        )?;
        if r.at != flat.len() {
            return Err(corrupt("encoder string count does not match its body"));
        }
    } else {
        record_blocks(tag, flat, &mut emit)?;
    }
    directory.extend(bodies);
    Ok(directory)
}

fn flat_strings(r: &mut Reader<'_>, rows: usize) -> Result<Vec<Vec<u8>>, String> {
    let mut strings = Vec::<Vec<u8>>::with_capacity(rows);
    let mut previous = Vec::new();
    for _ in 0..rows {
        let shared = usize::try_from(r.var()?).map_err(|_| "encoder string overflows")?;
        let suffix = usize::try_from(r.var()?).map_err(|_| "encoder string overflows")?;
        if shared > previous.len() {
            return Err("encoder string table is corrupt".into());
        }
        previous.truncate(shared);
        previous.extend_from_slice(r.take(suffix)?);
        strings.push(previous.clone());
    }
    Ok(strings)
}

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
