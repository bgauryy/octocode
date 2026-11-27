use super::*;
use octocode_runtime_base::execution::cancel::NeverCancel;

/// Each block reaches the file as it is built, so a section is never held
/// whole; the directory fills its reserved place last.
#[test]
fn a_blocked_section_writes_each_body_as_it_is_built() {
    let mut output = std::io::Cursor::new(Vec::new());
    let body = |first, count, bytes| blocks::Body {
        first,
        last: first + count - 1,
        count,
        bytes,
    };
    let mut section =
        BlockedSection::begin(&mut output, 8, 257, 1 << 20, &NeverCancel).expect("two blocks fit");
    let start = 8 + blocks::PREFIX + 2 * blocks::ENTRY;
    section.body(&body(0, 256, b"first")).expect("first block");
    assert_eq!(&section.output.get_ref()[start..], b"first");
    section
        .body(&body(256, 1, b"second"))
        .expect("second block");
    let mut table = Vec::new();
    let next = section.finish(b"NODE", &mut table).expect("finish");
    let bytes = output.into_inner();
    assert_eq!(&bytes[start..start + 11], b"firstsecond");
    assert_eq!(next as usize % 8, 0, "padded");
    assert_eq!(bytes.len() as u64, next);
    let section = &bytes[8..start + 11];
    let (rows, parsed) = blocks::parse(section, section.len() as u64).expect("directory");
    assert_eq!(rows, 257);
    assert_eq!(parsed.len(), 2);
    assert_eq!(
        parsed[1].offset,
        (blocks::PREFIX + 2 * blocks::ENTRY + 5) as u64
    );
    assert_eq!(
        table[20..],
        blocks::digest(b"NODE", section).expect("digest"),
        "the outer table commits to the directory"
    );
}

#[test]
fn a_block_past_the_snapshot_limit_never_reaches_the_file() {
    let limit = (blocks::PREFIX + blocks::ENTRY + 8) as u64;
    let mut output = std::io::Cursor::new(Vec::new());
    let mut section =
        BlockedSection::begin(&mut output, 0, 1, limit, &NeverCancel).expect("directory fits");
    let error = section
        .body(&blocks::Body {
            first: 0,
            last: 0,
            count: 1,
            bytes: &[1; 9],
        })
        .expect_err("over limit");
    assert_eq!(
        error.code,
        octocode_contract_types::ErrorCode::ScopeTooBroad
    );
    assert!(output.into_inner().is_empty());
}

#[test]
fn compressed_blocks_cannot_hide_oversized_encoded_bodies() {
    let limit = (blocks::PREFIX + blocks::ENTRY) as u64 * 16;
    let text = "x".repeat(limit as usize);
    let mut output = std::io::Cursor::new(Vec::new());
    let mut section =
        BlockedSection::begin(&mut output, 0, 1, limit, &NeverCancel).expect("directory fits");
    // The string itself fits; its front-coding varints push the raw
    // encoded body beyond the bound that the reader can inflate.
    let error = blocks::string_blocks(&[text], &mut |body| section.sealed(b"STRE", body))
        .expect_err("compression cannot make an unreadable raw body publishable");
    assert_eq!(
        error.code,
        octocode_contract_types::ErrorCode::ScopeTooBroad
    );
    assert!(output.into_inner().is_empty());
}

#[test]
fn snapshot_write_limit_includes_padding_before_writing() {
    let limit = LIMITS.snapshot_bytes as u64;
    let mut output = Vec::new();
    assert_eq!(
        write_section(&mut output, limit - 8, &[1], limit, &NeverCancel).expect("last block"),
        limit
    );
    assert_eq!(output, [1, 0, 0, 0, 0, 0, 0, 0]);
    for (offset, bytes) in [(limit, &[1][..]), (limit - 8, &[1; 9][..])] {
        let error = write_section(&mut output, offset, bytes, limit, &NeverCancel)
            .expect_err("over-limit block");
        assert_eq!(
            error.code,
            octocode_contract_types::ErrorCode::ScopeTooBroad
        );
        assert_eq!(output.len(), 8, "rejected bytes never reach the writer");
    }
    assert!(
        checked_end(limit - 1, 1, limit).is_err(),
        "padding is bounded"
    );
    assert!(
        checked_end(u64::MAX, 1, limit).is_err(),
        "offset cannot wrap"
    );
    assert!(
        checked_end(0, usize::MAX, limit).is_err(),
        "length cannot wrap"
    );
    assert_eq!(
        checked_end(limit, 0, limit).expect("empty section"),
        (limit, 0)
    );
}
