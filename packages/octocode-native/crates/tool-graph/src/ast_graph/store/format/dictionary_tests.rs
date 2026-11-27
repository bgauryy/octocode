//! The string dictionary splits into record strings (`STRS`) and strings only
//! occurrence evidence references (`STRE`), so readers that skip evidence skip
//! its text too.
use super::evidence_tests::evidence_fixture;
use super::*;

fn sorted(run: &[&str]) -> bool {
    run.windows(2).all(|pair| pair[0] < pair[1])
}

/// A string section is its entries and nothing more, like `LOOK` and `NODE`.
#[test]
fn a_string_section_with_trailing_bytes_is_refused() {
    // One entry, "a", then a byte no entry accounts for.
    let bytes = [1, 0, 1, b'a', 0];
    let mut t = GraphTables::default();
    assert_eq!(
        read_strings(&mut Reader::new(&bytes, "STRS"), &mut t, &LIMITS),
        Err("graph section STRS has trailing data".into())
    );
    let mut t = GraphTables::default();
    read_strings(&mut Reader::new(&bytes[..4], "STRS"), &mut t, &LIMITS).unwrap();
    assert_eq!(t.str(0), "a");
}

#[test]
fn record_strings_precede_evidence_only_strings() {
    let mut t = evidence_fixture();
    // Only an unresolved site names this reason; only occurrences name the
    // derivation version `local`.
    let reason = t.strings.len() as u32;
    t.strings.push("only-evidence-reason");
    t.unresolved[0].reason = reason;
    let (bytes, _) = encode(&t);
    let (full, _) = decode(&bytes).unwrap();
    let (files, _) = decode_files(&bytes).unwrap();
    let records = files.strings.len();
    assert!(records < full.strings.len(), "{:?}", full.strings);
    let all = full.strings.iter().collect::<Vec<_>>();
    assert_eq!(all[..records], files.strings.iter().collect::<Vec<_>>());
    assert!(sorted(&all[..records]) && sorted(&all[records..]));
    for evidence_only in ["only-evidence-reason", "local"] {
        assert!(all[records..].contains(&evidence_only));
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
        ..*LIMITS
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
    t.strings.push("~last-record");
    t.nodes[1].detail = detail;
    let mut encoder = writer::Encoder::new(&t);
    assert_eq!(encoder.texts[encoder.records - 1], "~last-record");
    assert!(
        writer::encode_with(
            &encoder,
            &octocode_runtime_base::execution::cancel::NeverCancel
        )
        .is_ok()
    );
    // Moving the boundary turns that record string into evidence text.
    encoder.records -= 1;
    let (bytes, _) = writer::encode_with(
        &encoder,
        &octocode_runtime_base::execution::cancel::NeverCancel,
    )
    .unwrap();
    assert!(decode(&bytes).is_err(), "a node may not name evidence text");
    assert!(decode_files(&bytes).is_err(), "nor in a files-only read");
}

/// Every blocked section (`STRE`, `NODE`, `DECL`, `SRCO`) spans more than one
/// block here.
#[test]
fn snapshot_bytes_are_pinned_across_block_boundaries() {
    let mut t = evidence_fixture();
    t.sources = super::evidence_tests::origins_fixture(&[[(0, 8), (0, 12), (2, 5)]; 200].concat());
    let (symbol, declaration, site) = (t.nodes[1], t.declarations[0], t.unresolved[0]);
    for id in 0..600_u32 {
        let (key, reason) = (t.strings.len() as u32, t.strings.len() as u32 + 1);
        t.strings.push(format!("a.py#s{id:04}"));
        t.strings
            .push(format!("reason-{id:04}-{}", "x".repeat(id as usize % 40)));
        let node = t.nodes.len() as u32;
        t.nodes.push(NodeRec { key, ..symbol });
        t.declarations.push(DeclarationRec {
            node,
            ..declaration
        });
        t.unresolved.push(UnresolvedRec { reason, ..site });
    }
    t.index();
    let (bytes, _) = encode(&t);
    decode(&bytes).expect("the blocks decode");
    // Readers of this format version hold exactly these bytes.
    assert_eq!(
        octocode_engine::digest::sha256(&bytes),
        "8f313e1c930e6e0752f5e1637ef8695d8aae4ef5b44eb84932f7931a4c81c2f6"
    );
}
