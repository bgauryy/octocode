//! The string dictionary splits into record strings (`STRS`) and strings only
//! occurrence evidence references (`STRE`), so readers that skip evidence skip
//! its text too.
use super::evidence_tests::evidence_fixture;
use super::*;

fn sorted(run: &[String]) -> bool {
    run.windows(2).all(|pair| pair[0] < pair[1])
}

#[test]
fn record_strings_precede_evidence_only_strings() {
    let mut t = evidence_fixture();
    // Only an unresolved site names this reason; only occurrences name the
    // derivation version `local`.
    let reason = t.strings.len() as u32;
    t.strings.push("only-evidence-reason".into());
    t.unresolved[0].reason = reason;
    let (bytes, _) = encode(&t);
    let (full, _) = decode(&bytes).unwrap();
    let (files, _) = decode_files(&bytes).unwrap();
    let records = files.strings.len();
    assert!(records < full.strings.len(), "{:?}", full.strings);
    assert_eq!(full.strings[..records], files.strings[..]);
    assert!(sorted(&full.strings[..records]) && sorted(&full.strings[records..]));
    for evidence_only in ["only-evidence-reason", "local"] {
        assert!(full.strings[records..].iter().any(|s| s == evidence_only));
        assert!(!files.strings.iter().any(|s| s == evidence_only));
    }
    // A string both a declaration and a witness reference is a record string, once.
    assert_eq!(full.strings.iter().filter(|s| *s == "call:1").count(), 1);
    assert!(files.strings.iter().any(|s| s == "call:1"));
    let record = |id: u32| (id as usize) < records;
    assert!(
        full.nodes
            .iter()
            .all(|n| record(n.key) && record(n.name) && record(n.detail))
    );
    assert!(full.edges.iter().all(|e| record(e.detail)));
    assert!(full.declarations.iter().all(|d| {
        record(d.occurrence)
            && record(d.source_digest)
            && record(d.test_identity)
            && record(d.assessment)
    }));
    assert_eq!(full.str(full.unresolved[0].reason), "only-evidence-reason");
    assert_eq!(full.str(full.witnesses[0].occurrence), "call:1");
}

#[test]
fn empty_graph_round_trips_with_both_dictionaries() {
    let (bytes, _) = encode(&GraphTables::default());
    let (t, _) = decode(&bytes).unwrap();
    assert!(t.strings.is_empty() && t.nodes.is_empty());
    assert!(decode_files(&bytes).unwrap().0.strings.is_empty());
}

#[test]
fn evidence_strings_share_the_record_dictionary_budget() {
    // Two strings per section: each fits alone, together they exceed the limits.
    let section = [2, 0, 1, b'a', 0, 1, b'b'];
    let limits = |string_bytes, table_items| Limits {
        string_bytes,
        table_items,
        ..LIMITS
    };
    let mut t = GraphTables::default();
    read_strings(&mut Reader::new(&section, "STRS"), &mut t, &limits(100, 3)).unwrap();
    let error = read_strings(&mut Reader::new(&section, "STRE"), &mut t, &limits(100, 3));
    assert!(error.is_err(), "item limit spans both dictionaries");
    let mut t = GraphTables::default();
    read_strings(&mut Reader::new(&section, "STRS"), &mut t, &limits(3, 10)).unwrap();
    let error = read_strings(&mut Reader::new(&section, "STRE"), &mut t, &limits(3, 10));
    assert!(error.is_err(), "byte limit spans both dictionaries");
}

#[test]
fn record_reference_into_evidence_dictionary_is_rejected() {
    let mut t = evidence_fixture();
    // The symbol's detail sorts last among record strings.
    let detail = t.strings.len() as u32;
    t.strings.push("~last-record".into());
    t.nodes[1].detail = detail;
    let mut encoder = writer::Encoder::new(&t);
    assert_eq!(encoder.texts[encoder.records - 1], "~last-record");
    assert!(writer::encode_with(&encoder, &crate::execution::cancel::NeverCancel).is_ok());
    // Moving the boundary turns that record string into evidence text.
    encoder.records -= 1;
    let (bytes, _) = writer::encode_with(&encoder, &crate::execution::cancel::NeverCancel).unwrap();
    assert!(decode(&bytes).is_err(), "a node may not name evidence text");
    assert!(decode_files(&bytes).is_err(), "nor in a files-only read");
}
