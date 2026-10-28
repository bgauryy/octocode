use super::*;
use crate::execution::cancel::NeverCancel;
use octocode_engine::text::analysis as d;
use serde_json::json;

mod profiles;

fn fixture() -> (SourceDescriptors, OriginProjection) {
    let producer = d::Producer {
        id: "fixture".into(),
        version: "1".into(),
    };
    let mut descriptors = SourceDescriptors {
        snapshots: vec![d::SourceSnapshot {
            id: "raw".into(),
            path: "input.rs".into(),
            digest: "a".repeat(64),
            byte_length: 8,
            encoding: d::Encoding::Utf8,
            bom_bytes: 0,
        }],
        representations: vec![d::TextRepresentation {
            id: "text".into(),
            source_digest: "b".repeat(64),
            byte_length: 8,
            producer: producer.clone(),
            source_ids: vec!["raw".into()],
        }],
        units: vec![d::AnalysisUnit {
            id: "unit".into(),
            adapter: producer,
            operation: d::AnalysisOperation::GraphFacts,
            representation_id: "text".into(),
            regions: vec![d::ByteRange::new(0, 8)],
        }],
    };
    descriptors.snapshots.push(d::SourceSnapshot {
        id: "raw16".into(),
        path: "input16.rs".into(),
        digest: "c".repeat(64),
        byte_length: 18,
        encoding: d::Encoding::Utf16Le,
        bom_bytes: 2,
    });
    descriptors.representations[0]
        .source_ids
        .push("raw16".into());
    let original = d::OriginalRange {
        source_id: "raw".into(),
        raw_range: d::ByteRange::new(0, 8),
    };
    let origin = OriginProjection {
        analysis_unit_id: "unit".into(),
        segments: vec![d::OriginSegment {
            source_range: d::ByteRange::new(0, 8),
            analysis: d::AnalysisMembership::Included,
            branches: vec![
                d::OriginBranch::Copied(original.clone()),
                d::OriginBranch::Transcoded(d::OriginalRange {
                    source_id: "raw16".into(),
                    raw_range: d::ByteRange::new(2, 18),
                }),
                d::OriginBranch::Derived(vec![
                    d::Contribution::Source(original.clone()),
                    d::Contribution::Source(original),
                    d::Contribution::Unmapped(d::UnmappedReason::RawSourceUnavailable),
                ]),
                d::OriginBranch::Unmapped(d::UnmappedReason::Synthetic),
            ],
        }],
    };
    (descriptors, origin)
}

#[test]
fn generated_projection_keeps_all_alternatives_and_joint_contributors() {
    let (descriptors, origin) = fixture();
    validate_origin(&origin, d::ByteRange::new(0, 8), &descriptors, &NeverCancel).unwrap();
    let mut budget = ProjectionBudget::default();
    let wire = json!(project_origin(&origin, &mut budget, &NeverCancel).unwrap());
    let branches = wire["segments"][0]["branches"].as_array().unwrap();
    assert_eq!(branches.len(), 4);
    assert_eq!(branches[2]["contributors"].as_array().unwrap().len(), 3);
    assert_eq!(
        branches[2]["contributors"][0],
        branches[2]["contributors"][1]
    );
    assert_eq!(
        branches[2]["contributors"][2]["reason"],
        "rawSourceUnavailable"
    );
    let closure =
        json!(project_descriptors(&descriptors, ["unit"], &mut budget, &NeverCancel).unwrap());
    assert_eq!(closure["units"][0]["representationId"], "text");
    assert_eq!(
        closure["representations"][0]["sourceIds"],
        json!(["raw", "raw16"])
    );
    assert_eq!(closure["snapshots"][0]["id"], "raw");
}

#[test]
fn closure_contains_only_emitted_units_and_all_raw_contributors() {
    let (mut descriptors, _) = fixture();
    let mut unused = descriptors.units[0].clone();
    unused.id = "unused".into();
    descriptors.units.push(unused);
    let mut raw = descriptors.snapshots[0].clone();
    raw.id = "raw2".into();
    descriptors.snapshots.push(raw);
    descriptors.representations[0]
        .source_ids
        .push("raw2".into());
    let closure = json!(
        project_descriptors(
            &descriptors,
            ["unit", "unit"],
            &mut ProjectionBudget::default(),
            &NeverCancel
        )
        .unwrap()
    );
    assert_eq!(closure["units"].as_array().unwrap().len(), 1);
    assert_eq!(closure["snapshots"].as_array().unwrap().len(), 3);
}

#[test]
fn overflow_is_terminal_without_a_partial_origin_or_merge() {
    let (descriptors, origin) = fixture();
    let mut budget = ProjectionBudget::new(ProjectionLimits {
        bytes: 1,
        records: 1,
    });
    assert_eq!(
        project_origin(&origin, &mut budget, &NeverCancel)
            .unwrap_err()
            .code,
        ErrorCode::ResponseTooLarge
    );
    let target = SourceDescriptors::default();
    let before = target.clone();
    let mut budget = ProjectionBudget::new(ProjectionLimits {
        bytes: 1,
        records: 1,
    });
    assert_eq!(
        merge_descriptors([&target, &descriptors], &mut budget, &NeverCancel)
            .unwrap_err()
            .code,
        ErrorCode::ResponseTooLarge
    );
    assert_eq!(target, before);
}

#[test]
fn conflicting_catalog_merge_is_atomic() {
    let (target, _) = fixture();
    let before = target.clone();
    let mut source = target.clone();
    source.units[0].adapter.version = "2".into();
    assert!(
        merge_descriptors(
            [&target, &source],
            &mut ProjectionBudget::default(),
            &NeverCancel
        )
        .is_err()
    );
    assert_eq!(target, before);
}

#[test]
fn bulk_catalog_merge_deduplicates_equal_records_without_changing_inputs() {
    let (source, _) = fixture();
    let before = source.clone();
    let merged = merge_descriptors(
        [&source, &source],
        &mut ProjectionBudget::default(),
        &NeverCancel,
    )
    .unwrap();
    assert_eq!(merged, source);
    assert_eq!(source, before);
}

#[test]
fn excluded_segment_cannot_hide_an_included_region() {
    let (mut descriptors, mut origin) = fixture();
    descriptors.units[0].regions = vec![d::ByteRange::new(2, 4)];
    origin.segments[0].analysis = d::AnalysisMembership::Excluded;
    assert!(validate_origin(&origin, d::ByteRange::new(0, 8), &descriptors, &NeverCancel).is_err());
}

#[test]
fn zero_width_anchors_use_right_region_except_at_eof() {
    let (mut descriptors, mut origin) = fixture();
    descriptors.units[0].regions = vec![d::ByteRange::new(2, 4), d::ByteRange::new(6, 8)];
    for (at, included) in [(0, false), (2, true), (4, false), (6, true), (8, true)] {
        let range = d::ByteRange::new(at, at);
        origin.segments[0].source_range = range;
        origin.segments[0].analysis = if included {
            d::AnalysisMembership::Included
        } else {
            d::AnalysisMembership::Excluded
        };
        origin.segments[0].branches = vec![d::OriginBranch::Copied(d::OriginalRange {
            source_id: "raw".into(),
            raw_range: range,
        })];
        validate_origin(&origin, range, &descriptors, &NeverCancel).unwrap();
    }
}

#[test]
fn nested_row_origins_are_charged_without_charging_arbitrary_text() {
    let (_, origin) = fixture();
    let wire =
        json!(project_origin(&origin, &mut ProjectionBudget::default(), &NeverCancel).unwrap());
    let rows = json!({"target": {"sourceOrigin": wire}, "results": [{"recipient": {"sourceOrigin": wire}, "text": "x".repeat(1_000_000)}]});
    let mut budget = ProjectionBudget::new(ProjectionLimits {
        bytes: 64 << 10,
        records: 1000,
    });
    assert_eq!(
        budget.admit_json_evidence(&rows, &NeverCancel).unwrap(),
        std::collections::BTreeSet::from(["unit".to_owned()])
    );
    assert_eq!(
        referenced_units(&rows, &mut Default::default(), &NeverCancel)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn tiny_budget_and_cancellation_precede_catalog_indexing() {
    struct Cancel;
    impl CancellationCheck for Cancel {
        fn check(&self) -> Result<(), String> {
            Err("cancelled before projection".into())
        }
    }
    let (mut descriptors, _) = fixture();
    for n in 0..10_000 {
        let mut unit = descriptors.units[0].clone();
        unit.id = format!("unit-{n}");
        descriptors.units.push(unit);
    }
    let mut budget = ProjectionBudget::new(ProjectionLimits {
        bytes: 1,
        records: 1,
    });
    assert_eq!(
        project_descriptors(&descriptors, ["unit"], &mut budget, &NeverCancel)
            .unwrap_err()
            .code,
        ErrorCode::ResponseTooLarge
    );
    assert_eq!(
        project_descriptors(
            &descriptors,
            ["unit"],
            &mut ProjectionBudget::default(),
            &Cancel
        )
        .unwrap_err()
        .code,
        ErrorCode::Cancelled
    );
}
