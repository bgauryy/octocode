use super::capture::read_nul_records;
use std::io::Cursor;

#[test]
fn record_boundaries_preserve_tabs_newlines_and_chunks() {
    let first = b"one\tline\n".to_vec();
    let second = vec![b'x'; 16 * 1024 + 7];
    let input = [first.as_slice(), &[0], second.as_slice(), &[0]].concat();
    let mut records = Vec::new();
    read_nul_records(Some(Cursor::new(input)), second.len(), &mut |record| {
        records.push(record.to_vec());
        Ok(())
    })
    .expect("complete records across reader chunks");
    assert_eq!(records, [first, second]);
}

#[test]
fn incomplete_and_oversized_records_fail_instead_of_becoming_partial_evidence() {
    let mut record = |_: &[u8]| Ok(());
    let incomplete = read_nul_records(Some(Cursor::new(b"no delimiter")), 100, &mut record)
        .expect_err("unterminated record");
    assert!(incomplete.contains("unterminated"));
    let oversized =
        read_nul_records(Some(Cursor::new(b"long\0")), 3, &mut record).expect_err("per-record cap");
    assert!(oversized.contains("3 byte"), "{oversized}");
}
