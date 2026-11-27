use super::*;

/// A flat section as its stored bytes, for fixtures that assemble a snapshot
/// by hand.
pub(in super::super) fn pack(tag: &[u8; 4], flat: &[u8]) -> Result<Vec<u8>, ToolError> {
    let mut r = Reader::new(flat, "encoder records");
    let rows = r.rows().map_err(|error| corrupt(&error))?;
    let mut directory = directory_start(rows)?;
    let mut bodies = Vec::new();
    let start = (PREFIX + rows.div_ceil(ROWS) * ENTRY) as u64;
    let mut emit = |body: Body<'_>| {
        let stored = seal(tag, body.bytes, LIMITS.snapshot_bytes as u64)?;
        let body = Body {
            bytes: &stored,
            ..body
        };
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

/// Evidence strings and origin blocks are stored deflated and read back
/// byte for byte; other blocks are stored as they are. A stored body that
/// inflates to any length but its recorded one is refused.
#[test]
fn sealed_blocks_round_trip_and_shrink() {
    let raw = br#"{"recipient":"a.go:1:9:recipient","group":0,"assessment":{"status":"parsed"}}"#
        .repeat(64);
    for tag in [b"STRE", b"SRCO"] {
        let stored = seal(tag, &raw, LIMITS.snapshot_bytes as u64).expect("seal");
        assert!(
            stored.len() * 4 < raw.len(),
            "{} of {}",
            stored.len(),
            raw.len()
        );
        assert_eq!(open(&stored).expect("open"), raw);
    }
    for tag in [b"NODE", b"DECL"] {
        assert_eq!(
            seal(tag, &raw, LIMITS.snapshot_bytes as u64).expect("seal"),
            raw
        );
    }
    let mut stored = seal(b"STRE", &raw, LIMITS.snapshot_bytes as u64).expect("seal");
    let mut longer = Vec::new();
    writer::put(&mut longer, raw.len() as u64 + 1);
    let header = {
        let mut header = Vec::new();
        writer::put(&mut header, raw.len() as u64);
        header.len()
    };
    longer.extend_from_slice(&stored[header..]);
    assert!(open(&longer).is_err(), "a body shorter than its length");
    stored.truncate(stored.len() - 4);
    assert!(open(&stored).is_err(), "a truncated body");
    let mut huge = Vec::new();
    writer::put(&mut huge, LIMITS.snapshot_bytes as u64 + 1);
    assert!(open(&huge).is_err(), "a length past the snapshot bound");
}

#[test]
fn every_sealed_section_admits_exactly_its_expanded_byte_bound() {
    let raw = b"compressible evidence\n".repeat(64);
    let limit = raw.len() as u64;
    for tag in [b"STRE", b"SRCO", b"WITN", b"UNRS"] {
        let stored = seal(tag, &raw, limit).expect("raw body at the bound fits");
        assert_eq!(open(&stored).expect("open"), raw);
        let error = seal(tag, &raw, limit - 1).expect_err("raw body past the bound is refused");
        assert_eq!(
            error.code,
            octocode_contract_types::ErrorCode::ScopeTooBroad
        );
    }
}
