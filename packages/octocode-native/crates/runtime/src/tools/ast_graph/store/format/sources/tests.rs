use super::*;
mod admission;
mod lazy;
mod profile;
use crate::execution::cancel::NeverCancel;
use crate::response::source_evidence::{ProjectionBudget, ProjectionLimits};
use crate::tools::ast_graph::limits::read_charge;
use octocode_engine::text::analysis as domain;

/// The snapshot bounds with the source table charged at most `bytes` and
/// `items`.
fn bounded(bytes: usize, items: usize) -> Limits {
    Limits {
        source_bytes: bytes,
        table_items: items,
        ..LIMITS
    }
}

fn fixture() -> (SourceDescriptors, Vec<OriginProjection>) {
    use domain::*;
    let snapshots = [
        ("a", Encoding::Utf8, 0),
        ("b", Encoding::Utf16Le, 2),
        ("c", Encoding::Utf16Be, 2),
    ]
    .into_iter()
    .map(|(id, encoding, bom_bytes)| SourceSnapshot {
        id: id.into(),
        path: format!("{id}.rs"),
        digest: "a".repeat(64),
        byte_length: 32,
        encoding,
        bom_bytes,
    })
    .collect();
    let descriptors = SourceDescriptors {
        snapshots,
        representations: vec![TextRepresentation {
            id: "representation".into(),
            source_digest: "b".repeat(64),
            byte_length: 16,
            producer: Producer::new("producer", "1"),
            source_ids: vec!["a".into(), "b".into(), "c".into()],
        }],
        units: vec![AnalysisUnit {
            id: "unit".into(),
            adapter: Producer::new("adapter", "1"),
            operation: AnalysisOperation::GraphFacts,
            representation_id: "representation".into(),
            regions: vec![ByteRange::new(0, 4), ByteRange::new(8, 16)],
        }],
    };
    let raw = |id: &str, a, b| OriginalRange {
        source_id: id.into(),
        raw_range: ByteRange::new(a, b),
    };
    let branches = vec![
        OriginBranch::Copied(raw("a", 0, 4)),
        OriginBranch::Transcoded(raw("b", 2, 8)),
        OriginBranch::Derived(vec![
            Contribution::Source(raw("a", 1, 3)),
            Contribution::Unmapped(UnmappedReason::Synthetic),
            Contribution::Source(raw("c", 2, 10)),
            Contribution::Source(raw("a", 1, 3)),
            Contribution::Unmapped(UnmappedReason::RawSourceUnavailable),
            Contribution::Unmapped(UnmappedReason::MappingUnavailable),
        ]),
        OriginBranch::Unmapped(UnmappedReason::Synthetic),
        OriginBranch::Unmapped(UnmappedReason::RawSourceUnavailable),
        OriginBranch::Unmapped(UnmappedReason::MappingUnavailable),
    ];
    let origins = vec![
        OriginProjection {
            analysis_unit_id: "unit".into(),
            segments: vec![
                OriginSegment {
                    source_range: ByteRange::new(0, 4),
                    analysis: AnalysisMembership::Included,
                    branches,
                },
                OriginSegment {
                    source_range: ByteRange::new(4, 8),
                    analysis: AnalysisMembership::Excluded,
                    branches: vec![OriginBranch::Unmapped(UnmappedReason::MappingUnavailable)],
                },
            ],
        },
        OriginProjection {
            analysis_unit_id: "unit".into(),
            segments: vec![OriginSegment {
                source_range: ByteRange::new(16, 16),
                analysis: AnalysisMembership::Included,
                branches: vec![
                    OriginBranch::Copied(raw("a", 16, 16)),
                    OriginBranch::Transcoded(raw("c", 18, 18)),
                ],
            }],
        },
    ];
    (descriptors, origins)
}
fn table() -> SourceTable {
    let (descriptors, origins) = fixture();
    SourceTable::from_origins(descriptors, &origins, &NeverCancel).unwrap()
}
#[test]
fn all_variants_and_order_roundtrip_with_full_validation() {
    let (descriptors, origins) = fixture();
    let value = SourceTable::from_origins(descriptors.clone(), &origins, &NeverCancel).unwrap();
    let encoded = encode(&value, &NeverCancel).unwrap();
    let decoded = decode(&encoded, &NeverCancel).unwrap();
    assert_eq!(decoded, value);
    assert_eq!(decoded.descriptors, descriptors);
    for (i, expected) in origins.iter().enumerate() {
        let origin = decoded
            .origin(i as u32, &mut ProjectionBudget::default(), &NeverCancel)
            .unwrap();
        assert_eq!(&origin, expected);
    }
    assert_eq!(encoded, encode(&decoded, &NeverCancel).unwrap());
}
#[test]
fn truncation_trailing_unknown_tags_and_unselected_corruption_fail() {
    let value = table();
    let encoded = encode(&value, &NeverCancel).unwrap();
    for length in 0..encoded.len() {
        assert!(
            decode(&encoded[..length], &NeverCancel).is_err(),
            "accepted prefix {length}"
        );
    }
    let mut trailing = encoded.clone();
    trailing.push(0);
    assert!(decode(&trailing, &NeverCancel).is_err());
    // The encoded records end the section, after the persisted index.
    let begin = encoded.len() - value.origins.storage_bytes().0;
    let last = begin + value.origins.get(1).unwrap().offset as usize;
    // Last origin: unit, count, start, end, membership, branch-count, tag,
    // source-ref, raw-start, raw-end. Each fixture field occupies one byte.
    for relative in [0, 4, 6, 7] {
        let mut malformed = encoded.clone();
        malformed[last + relative] = 127;
        assert!(
            decode(&malformed, &NeverCancel).is_err(),
            "accepted tag/index {relative}"
        );
    }
    let mut malformed = encoded.clone();
    malformed[last + 9] = 17;
    assert!(
        decode(&malformed, &NeverCancel).is_err(),
        "unselected copied range length mismatch accepted"
    );
    // Recompute the outer graph checksum: rejection must come from semantic
    // validation, even with no declaration/node selecting this origin.
    let mut t = GraphTables {
        sources: value,
        ..Default::default()
    };
    t.index();
    let (graph, _) = writer::encode(&t);
    let count = u32::from_le_bytes(graph[12..16].try_into().unwrap()) as usize;
    let sections = reader::read_section_table(&graph[HEADER_LEN..], count)
        .unwrap()
        .iter()
        .map(|(tag, bytes)| writer::Section {
            tag: *tag,
            bytes: if *tag == *b"SRCE" {
                malformed.clone()
            } else {
                bytes.to_vec()
            },
        })
        .collect();
    let (graph, _) = writer::assemble(sections);
    assert!(reader::decode(&graph).unwrap_err().contains("source"));
}
#[test]
fn expanded_identity_cost_is_admitted_before_selected_allocation() {
    let (mut descriptors, mut origins) = fixture();
    descriptors.snapshots[0].id = "a".repeat(4096);
    descriptors.representations[0].source_ids[0] = descriptors.snapshots[0].id.clone();
    for origin in &mut origins {
        for segment in &mut origin.segments {
            for branch in &mut segment.branches {
                match branch {
                    domain::OriginBranch::Copied(raw) => {
                        raw.source_id.clone_from(&descriptors.snapshots[0].id);
                    }
                    domain::OriginBranch::Derived(values) => {
                        for value in values {
                            if let domain::Contribution::Source(raw) = value
                                && raw.source_id == "a"
                            {
                                raw.source_id.clone_from(&descriptors.snapshots[0].id);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    let value = SourceTable::from_origins(descriptors, &origins, &NeverCancel).unwrap();
    let entry = value.origins.get(1).unwrap();
    assert!(entry.bytes > 4096);
    let mut budget = ProjectionBudget::new(ProjectionLimits {
        bytes: entry.bytes as usize - 1,
        records: usize::MAX,
    });
    assert_eq!(
        value.origin(1, &mut budget, &NeverCancel).unwrap_err().code,
        octocode_contract_types::ErrorCode::ResponseTooLarge
    );
    let mut budget = ProjectionBudget::new(ProjectionLimits {
        bytes: entry.bytes as usize,
        records: entry.items as usize,
    });
    assert_eq!(
        value.origin(1, &mut budget, &NeverCancel).unwrap(),
        origins[1]
    );
}
#[test]
fn encoded_counts_cannot_bypass_allocation_or_cancellation_limits() {
    let value = table();
    let encoded = encode(&value, &NeverCancel).unwrap();
    let mut budget = budget::Budget::new(&NeverCancel, &bounded(1, LIMITS.table_items));
    let mut input = budget::Input::new(&encoded, &mut budget);
    assert!(matches!(
        catalog::read(&mut input),
        Err(SourceError::LimitExceeded(_))
    ));
    assert_eq!(budget.bytes, 0);
    struct Stop(std::sync::atomic::AtomicUsize);
    impl CancellationCheck for Stop {
        fn check(&self) -> Result<(), String> {
            if self.0.fetch_sub(1, std::sync::atomic::Ordering::Relaxed) <= 1 {
                Err("test stop".into())
            } else {
                Ok(())
            }
        }
    }
    for polls in [1, 20, 100] {
        let stop = Stop(std::sync::atomic::AtomicUsize::new(polls));
        assert!(matches!(
            decode(&encoded, &stop),
            Err(SourceError::Cancelled)
        ));
    }
    let stop = Stop(std::sync::atomic::AtomicUsize::new(10));
    assert!(
        value
            .origin(0, &mut ProjectionBudget::default(), &stop)
            .is_err()
    );
}

/// The charge a scan sums to bound a table before building it is what
/// admitting the built table's origins back spends: every item, and every
/// byte but the encoded records themselves (known only once encoded).
#[test]
fn read_charge_is_the_origin_admission_cost() {
    let (descriptors, origins) = fixture();
    let value = SourceTable::from_origins(descriptors, &origins, &NeverCancel).unwrap();
    let mut spent = budget::Budget::new(&NeverCancel, &LIMITS);
    value.origins.admit_read(&mut spent).unwrap();
    let summed = origins
        .iter()
        .map(read_charge)
        .fold((0, 0), |(bytes, items), (b, i)| (bytes + b, items + i));
    assert_eq!(summed.1, spent.items);
    assert!(
        summed.0 > 0 && summed.0 <= spent.bytes,
        "{summed:?} vs {}",
        spent.bytes
    );
}
