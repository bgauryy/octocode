use super::*;
use octocode_engine::text::analysis as source;

pub(crate) fn source_fixture() -> sources::SourceTable {
    let descriptor = source::SourceDescriptors {
        snapshots: vec![source::SourceSnapshot {
            id: "source".into(),
            path: "a.py".into(),
            digest: "a".repeat(64),
            byte_length: 12,
            encoding: source::Encoding::Utf8,
            bom_bytes: 0,
        }],
        representations: vec![source::TextRepresentation {
            id: "representation".into(),
            source_digest: "a".repeat(64),
            byte_length: 12,
            producer: source::Producer::new("fixture", "1"),
            source_ids: vec!["source".into()],
        }],
        units: vec![source::AnalysisUnit {
            id: "unit".into(),
            adapter: source::Producer::new("fixture", "1"),
            operation: source::AnalysisOperation::GraphFacts,
            representation_id: "representation".into(),
            regions: vec![source::ByteRange::new(0, 12)],
        }],
    };
    let origins = [(0, 8), (0, 12), (2, 5)]
        .into_iter()
        .map(|(start, end)| source::OriginProjection {
            analysis_unit_id: "unit".into(),
            segments: vec![source::OriginSegment {
                source_range: source::ByteRange::new(start, end),
                analysis: source::AnalysisMembership::Included,
                branches: vec![source::OriginBranch::Copied(source::OriginalRange {
                    source_id: "source".into(),
                    raw_range: source::ByteRange::new(start, end),
                })],
            }],
        })
        .collect::<Vec<_>>();
    sources::SourceTable::from_origins(descriptor, &origins, &crate::execution::cancel::NeverCancel)
        .unwrap()
}

pub(super) fn evidence_fixture() -> GraphTables {
    let mut t = GraphTables {
        sources: source_fixture(),
        strings: vec![
            "a.py".into(),
            "run".into(),
            "function".into(),
            "python".into(),
            "pkg:x".into(),
            "x".into(),
            "local".into(),
            "call:1".into(),
            "not-declared-in-repo".into(),
            "null".into(),
            r#"{"status":"notAssessed","reason":"assessmentUnavailable"}"#.into(),
            "a".repeat(64),
        ],
        nodes: vec![
            NodeRec {
                kind: NodeKind::File,
                flags: 0,
                key: 0,
                name: 0,
                detail: 3,
                file: 0,
                parent: NONE,
                line: NONE,
                end_line: NONE,
            },
            NodeRec {
                kind: NodeKind::Symbol,
                flags: 0,
                key: 1,
                name: 1,
                detail: 2,
                file: 0,
                parent: NONE,
                line: 1,
                end_line: 1,
            },
            NodeRec {
                kind: NodeKind::Package,
                flags: 0,
                key: 4,
                name: 5,
                detail: 5,
                file: NONE,
                parent: NONE,
                line: NONE,
                end_line: NONE,
            },
        ],
        ..Default::default()
    };
    t.witnesses.push(WitnessRec {
        argument: NONE,
        provenance: ProvenanceRec {
            source_digest: 11,
            source_start: 0,
            source_end: 8,
            source_origin: 0,
            derivation_version: 6,
        },
        occurrence: 7,
        edge: EdgeRec {
            src: 1,
            dst: 1,
            kind: EdgeKind::Calls,
            confidence: Confidence::High,
            detail: 6,
            line: 1,
            test: false,
        },
    });
    t.unresolved.push(UnresolvedRec {
        argument: NONE,
        provenance: t.witnesses[0].provenance,
        relation: EdgeKind::Calls,
        src: 1,
        occurrence: 7,
        callee: 5,
        line: 1,
        reason: 8,
    });
    t.declarations.push(DeclarationRec {
        node: 1,
        occurrence: 7,
        source_digest: 11,
        source_start: 0,
        source_end: 8,
        source_origin: 0,
        selection_start: 2,
        selection_end: 5,
        selection_origin: 2,
        assessment: 10,
        test_identity: 9,
    });
    t.index();
    t
}

#[test]
fn evidence_round_trips_and_rejects_non_source_nodes() {
    let t = evidence_fixture();
    let (bytes, _) = encode(&t);
    assert!(decode(&bytes).is_ok());
    for corrupt in [0, 1] {
        let mut t = evidence_fixture();
        if corrupt == 0 {
            t.witnesses[0].edge.src = 2;
        } else {
            t.unresolved[0].src = 2;
        }
        let (bytes, _) = encode(&t);
        assert!(
            decode(&bytes).is_err(),
            "package cannot own a source occurrence"
        );
    }
}

#[test]
fn declaration_metadata_rejects_bad_nodes_ranges_identity_and_order() {
    for corrupt in 0..5 {
        let mut t = evidence_fixture();
        match corrupt {
            0 => t.declarations[0].node = 0,
            1 => t.declarations[0].source_end = NONE,
            2 => t.declarations[0].test_identity = 6,
            3 => t.declarations.push(t.declarations[0]),
            _ => t.declarations[0].assessment = 6,
        }
        let (bytes, _) = encode(&t);
        assert!(decode(&bytes).is_err(), "corruption {corrupt} accepted");
    }
}

#[test]
fn evidence_locations_reject_zero_and_unresolved_absence() {
    for corrupt in 0..3 {
        let mut t = evidence_fixture();
        match corrupt {
            0 => t.witnesses[0].edge.line = 0,
            1 => t.unresolved[0].line = 0,
            _ => t.unresolved[0].line = NONE,
        }
        let (bytes, _) = encode(&t);
        assert!(decode(&bytes).is_err());
    }
}

#[test]
fn occurrence_provenance_rejects_partial_and_reversed_ranges() {
    for corrupt in 0..4 {
        let mut t = evidence_fixture();
        match corrupt {
            0 => t.witnesses[0].provenance.source_end = NONE,
            1 => t.unresolved[0].provenance.source_start = 9,
            2 => t.witnesses[0].provenance.source_start = NONE,
            _ => t.unresolved[0].provenance.source_end = NONE,
        }
        let (bytes, _) = encode(&t);
        assert!(
            decode(&bytes).is_err(),
            "malformed provenance {corrupt} accepted"
        );
    }
}

#[test]
fn overflowing_varint_is_not_a_valid_smaller_value() {
    let mut bytes = vec![0xff; 9];
    bytes.push(2);
    assert!(Reader::new(&bytes, "test").var().is_err());
}

#[test]
fn argument_metadata_round_trips_and_rejects_mismatched_premises() {
    use arguments::{ArgumentEvidence, Observation, StoredSpan};
    use octocode_engine::graph::{GraphArgumentForm, GraphParseAssessment};
    let evidence = ArgumentEvidence {
        recipient: "recipient:1".into(),
        recipient_range: StoredSpan {
            range: source::ByteRange::new(0, 12),
            origin: 1,
        },
        group: 0,
        assessment: GraphParseAssessment::default(),
        observation: Observation::Reference {
            index: 0,
            label: None,
            target: "run".into(),
            form: GraphArgumentForm::Identifier,
            selection: StoredSpan {
                range: source::ByteRange::new(2, 5),
                origin: 2,
            },
            blocker: None,
        },
    };
    for corruption in 0..9 {
        let mut t = evidence_fixture();
        let mut metadata = serde_json::json!(evidence);
        t.witnesses[0].edge.kind = EdgeKind::Passes;
        t.witnesses[0].argument = t.strings.len() as u32;
        match corruption {
            0 => {}
            1 => metadata["recipient_range"]["range"]["end_byte"] = serde_json::json!(4),
            2 => metadata["observation"]["selection"]["range"]["end_byte"] = serde_json::json!(9),
            3 => {
                metadata["observation"]["blocker"] =
                    serde_json::json!({"range":{"start_byte":0,"end_byte":1},"origin":0});
            }
            4 => metadata["observation"]["kind"] = serde_json::json!("UnsupportedGroup"),
            5 => t.witnesses[0].edge.kind = EdgeKind::Calls,
            6 => t.witnesses[0].argument = NONE,
            7 => metadata["recipient"] = serde_json::json!(""),
            _ => t.witnesses[0].edge.line = NONE,
        }
        t.strings.push(metadata.to_string());
        assert_eq!(
            encode_for_publish(&t, &crate::execution::cancel::NeverCancel).is_ok(),
            corruption == 0,
            "publication admitted corruption {corruption}"
        );
        let (bytes, _) = encode(&t);
        let decoded = decode(&bytes);
        assert_eq!(
            decoded.is_ok(),
            corruption == 0,
            "corruption {corruption}: {decoded:?}"
        );
        if let Ok((decoded, _)) = decoded {
            let parsed: ArgumentEvidence =
                serde_json::from_str(decoded.str(decoded.witnesses[0].argument)).unwrap();
            assert_eq!(serde_json::json!(parsed), serde_json::json!(evidence));
        }
    }
}

#[test]
fn partial_file_metadata_records_are_rejected_by_full_and_freshness_reads() {
    let (bytes, _) = encode(&evidence_fixture());
    let count = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    let original = read_section_table(&bytes[HEADER_LEN..], count).unwrap();
    for bad_tag in [*b"FDIG", *b"FCMP", *b"ENTR"] {
        let sections = original
            .iter()
            .map(|(tag, data)| writer::Section {
                tag: *tag,
                bytes: if *tag == bad_tag {
                    writer::varints(&[0])
                } else {
                    data.to_vec()
                },
            })
            .collect();
        let (malformed, _) = writer::assemble(sections);
        for read in [decode, decode_files] {
            assert!(read(&malformed).unwrap_err().contains("incomplete record"));
        }
    }
}

#[test]
fn saved_source_correspondence_rejects_dangling_and_mismatched_evidence() {
    for corruption in 0..6 {
        let mut t = evidence_fixture();
        match corruption {
            0 => t.declarations[0].source_origin = NONE,
            1 => t.declarations[0].selection_origin = 900,
            2 => t.declarations[0].source_end = 7,
            3 => t.sources.descriptors.representations[0].source_digest = "b".repeat(64),
            4 => t.sources.descriptors.units[0].regions = vec![source::ByteRange::new(2, 4)],
            _ => t.declarations[0].selection_origin = 0,
        }
        assert!(
            encode_for_publish(&t, &crate::execution::cancel::NeverCancel).is_err(),
            "publication accepted corruption {corruption}"
        );
        let (bytes, _) = encode(&t);
        assert!(
            decode(&bytes).is_err(),
            "load accepted corruption {corruption}"
        );
    }
}

#[test]
fn source_correspondence_section_is_mandatory() {
    let (bytes, _) = encode(&evidence_fixture());
    let count = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    let original = read_section_table(&bytes[HEADER_LEN..], count).unwrap();
    let sections = original
        .iter()
        .filter(|(tag, _)| *tag != b"SRCE")
        .map(|(tag, data)| writer::Section {
            tag: *tag,
            bytes: data.to_vec(),
        })
        .collect();
    let (missing, _) = writer::assemble(sections);
    assert!(decode(&missing).unwrap_err().contains("SRCE"));
}

#[test]
fn file_metadata_cannot_redirect_freshness_reads_through_symbol_keys() {
    for table in ["digest", "stamp", "component", "entry"] {
        let mut t = evidence_fixture();
        match table {
            "digest" => t.digests = vec![(1, 0)],
            "stamp" => {
                t.stamps = vec![(
                    1,
                    octocode_engine::stamp::SourceStamp {
                        size: 0,
                        modified_ns: 0,
                        changed_ns: 0,
                        inode: 0,
                    },
                )];
            }
            "component" => t.components = vec![(1, 0, 0, 0)],
            _ => t.entries = vec![(1, 0)],
        }
        let (bytes, _) = encode(&t);
        assert!(decode(&bytes).is_err(), "{table}");
        assert!(decode_files(&bytes).is_err(), "{table}");
    }
}
