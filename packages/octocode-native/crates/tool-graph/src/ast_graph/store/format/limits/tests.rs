use super::*;

/// The pre-encoding checks, at `items` rows, `edges` EDGE rows and
/// `string_bytes` expanded string bytes.
fn validate_with(
    encoder: &writer::Encoder<'_>,
    items: usize,
    edges: usize,
    string_bytes: usize,
) -> Result<(), String> {
    let limits = Limits {
        table_items: items,
        edges,
        string_bytes,
        ..*LIMITS
    };
    validate_counts(encoder.tables, &limits)?;
    validate_strings(encoder, &limits)
}

#[test]
fn publication_preflight_counts_flattened_file_tables() {
    for table in ["FDIG", "FCMP", "ENTR"] {
        let mut t = GraphTables {
            strings: ["source"].into_iter().collect(),
            ..Default::default()
        };
        match table {
            "FDIG" => t.digests = vec![(0, 0); 2],
            "FCMP" => t.components = vec![(0, 0, 0, 0)],
            _ => t.entries = vec![(0, 0); 2],
        }
        let encoder = writer::Encoder::new(&t);
        let error = validate_with(&encoder, 3, 3, 100).unwrap_err();
        assert!(error.contains(table), "{error}");
        assert!(validate_with(&encoder, 4, 4, 100).is_ok());
    }
}

#[test]
fn front_coding_does_not_hide_expanded_string_budget() {
    let prefix = "shared-prefix-".repeat(30);
    let t = GraphTables {
        strings: [format!("{prefix}a"), format!("{prefix}b")]
            .into_iter()
            .collect(),
        digests: vec![(0, 0), (0, 1)],
        ..Default::default()
    };
    let encoder = writer::Encoder::new(&t);
    assert!(encoder.record_strings().len() < 500);
    let error = validate_with(&encoder, 10, 10, 500).unwrap_err();
    assert!(error.contains("expanded string limit"));
    assert!(validate_with(&encoder, 10, 10, 1000).is_ok());
}

#[test]
fn occurrence_counts_are_checked_without_truncation() {
    let mut t = GraphTables {
        strings: ["source"].into_iter().collect(),
        ..Default::default()
    };
    let edge = EdgeRec {
        src: 0,
        dst: 0,
        kind: EdgeKind::Calls,
        confidence: Confidence::High,
        detail: 0,
        line: 1,
        test: false,
    };
    let provenance = ProvenanceRec {
        source_digest: 0,
        source_start: 0,
        source_end: 1,
        source_origin: NONE,
        derivation_version: 0,
    };
    t.witnesses = vec![
        WitnessRec {
            argument: NONE,
            edge,
            occurrence: 0,
            provenance
        };
        3
    ];
    let error = validate_with(&writer::Encoder::new(&t), 2, 2, 100).unwrap_err();
    assert!(error.contains("WITN"));
    assert_eq!(t.witnesses.len(), 3);
    t.witnesses.clear();
    t.unresolved = vec![
        UnresolvedRec {
            argument: NONE,
            provenance,
            relation: EdgeKind::Calls,
            src: 0,
            occurrence: 0,
            callee: 0,
            line: 1,
            reason: 0,
        };
        3
    ];
    assert!(
        validate_with(&writer::Encoder::new(&t), 2, 2, 100)
            .unwrap_err()
            .contains("UNRS")
    );
    assert_eq!(t.unresolved.len(), 3);
    t.unresolved.clear();
    t.edges = vec![edge; 3];
    assert!(
        validate_with(&writer::Encoder::new(&t), 4, 2, 100)
            .unwrap_err()
            .contains("EDGE")
    );
}
