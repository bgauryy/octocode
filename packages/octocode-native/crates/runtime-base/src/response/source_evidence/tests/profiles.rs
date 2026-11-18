use super::*;
use std::time::Instant;

fn report(name: &str, budget: &ProjectionBudget, wire: &serde_json::Value, start: Instant) {
    let (charged_bytes, charged_records) = budget.accounting();
    eprintln!(
        "projection-profile {}",
        json!({
            "case": name,
            "chargedBytes": charged_bytes,
            "chargedRecords": charged_records,
            "jsonBytes": serde_json::to_vec(wire).unwrap().len(),
            "elapsedMicros": start.elapsed().as_micros(),
            "byteLimit": ProjectionLimits::default().bytes,
            "recordLimit": ProjectionLimits::default().records,
        })
    );
}

#[test]
fn projection_accounting_profiles_large_closure_and_many_small_origins() {
    let mut descriptors = SourceDescriptors::default();
    let mut origins = Vec::new();
    for n in 0..1_000 {
        let source_id = format!("{n:064x}");
        let representation_id = format!("{:064x}", n + 1_000);
        let unit_id = format!("{:064x}", n + 2_000);
        descriptors.snapshots.push(d::SourceSnapshot {
            id: source_id.clone(),
            path: format!("src/module_{n}/component.rs"),
            digest: "a".repeat(64),
            byte_length: 8,
            encoding: d::Encoding::Utf8,
            bom_bytes: 0,
        });
        descriptors.representations.push(d::TextRepresentation {
            id: representation_id.clone(),
            source_digest: "b".repeat(64),
            byte_length: 8,
            producer: d::Producer::new("strict-source-decoder", "1"),
            source_ids: vec![source_id.clone()],
        });
        descriptors.units.push(d::AnalysisUnit {
            id: unit_id.clone(),
            adapter: d::Producer::new("rust-graph-facts", "1"),
            operation: d::AnalysisOperation::GraphFacts,
            representation_id,
            regions: vec![d::ByteRange::new(0, 8)],
        });
        origins.push(OriginProjection {
            analysis_unit_id: unit_id,
            segments: vec![d::OriginSegment {
                source_range: d::ByteRange::new(0, 8),
                analysis: d::AnalysisMembership::Included,
                branches: vec![d::OriginBranch::Copied(d::OriginalRange {
                    source_id,
                    raw_range: d::ByteRange::new(0, 8),
                })],
            }],
        });
    }
    let start = Instant::now();
    let mut budget = ProjectionBudget::default();
    let closure = json!(
        project_descriptors(
            &descriptors,
            descriptors.units.iter().map(|unit| unit.id.as_str()),
            &mut budget,
            &NeverCancel,
        )
        .unwrap()
    );
    assert_eq!(closure["units"].as_array().unwrap().len(), 1_000);
    assert_eq!(closure["snapshots"].as_array().unwrap().len(), 1_000);
    report("1000-file-closure", &budget, &closure, start);

    let start = Instant::now();
    let mut budget = ProjectionBudget::default();
    let rows = origins
        .iter()
        .map(|origin| json!(project_origin(origin, &mut budget, &NeverCancel).unwrap()))
        .collect::<Vec<_>>();
    let closure = project_descriptors(
        &descriptors,
        descriptors.units.iter().map(|unit| unit.id.as_str()),
        &mut budget,
        &NeverCancel,
    )
    .unwrap();
    let wire = json!({"origins": rows, "sourceEvidence": closure});
    report("1000-origins-and-file-closure", &budget, &wire, start);

    let start = Instant::now();
    let mut page_budget = ProjectionBudget::default();
    let units = page_budget
        .admit_json_evidence(&wire, &NeverCancel)
        .unwrap();
    assert_eq!(units.len(), 1_000);
    project_descriptors(
        &descriptors,
        units.iter().map(String::as_str),
        &mut page_budget,
        &NeverCancel,
    )
    .unwrap();
    report(
        "1000-preprojected-page-origins-and-closure",
        &page_budget,
        &wire,
        start,
    );

    let catalogs = (0..1_000)
        .map(|n| SourceDescriptors {
            snapshots: vec![descriptors.snapshots[n].clone()],
            representations: vec![descriptors.representations[n].clone()],
            units: vec![descriptors.units[n].clone()],
        })
        .collect::<Vec<_>>();
    let mut costs = Vec::new();
    for count in [500, 1_000] {
        let start = Instant::now();
        let mut budget = ProjectionBudget::default();
        let merged =
            merge_descriptors(catalogs[..count].iter(), &mut budget, &NeverCancel).unwrap();
        assert_eq!(merged.units.len(), count);
        let wire = json!(
            project_descriptors(
                &merged,
                merged.units.iter().map(|unit| unit.id.as_str()),
                &mut budget,
                &NeverCancel,
            )
            .unwrap()
        );
        costs.push(budget.accounting().0);
        report(
            &format!("{count}-file-bulk-merge-and-closure"),
            &budget,
            &wire,
            start,
        );
    }
    assert!(
        costs[1] < costs[0] * 3,
        "bulk merge accounting must grow linearly"
    );
}

#[test]
fn projection_accounting_profiles_fanout_and_terminal_overflow() {
    let (descriptors, mut origin) = fixture();
    let d::OriginBranch::Derived(contributors) = &origin.segments[0].branches[2] else {
        panic!("derived fixture")
    };
    let joint = contributors
        .iter()
        .cycle()
        .take(8)
        .cloned()
        .collect::<Vec<_>>();
    origin.segments[0].branches = vec![d::OriginBranch::Derived(joint); 1_000];
    validate_origin(&origin, d::ByteRange::new(0, 8), &descriptors, &NeverCancel).unwrap();
    let start = Instant::now();
    let mut budget = ProjectionBudget::default();
    let projected = project_origin(&origin, &mut budget, &NeverCancel).unwrap();
    let closure = project_descriptors(&descriptors, ["unit"], &mut budget, &NeverCancel).unwrap();
    let wire = json!({"origin": projected, "sourceEvidence": closure});
    let branches = wire["origin"]["segments"][0]["branches"]
        .as_array()
        .unwrap();
    assert_eq!(branches.len(), 1_000);
    assert!(
        branches
            .iter()
            .all(|branch| branch["contributors"].as_array().unwrap().len() == 8)
    );
    report(
        "1000-alternatives-8000-joint-contributors",
        &budget,
        &wire,
        start,
    );

    let mut tiny = ProjectionBudget::new(ProjectionLimits {
        bytes: 128 << 10,
        records: 1_000,
    });
    let error = project_origin(&origin, &mut tiny, &NeverCancel).unwrap_err();
    assert_eq!(error.code, ErrorCode::ResponseTooLarge);
    eprintln!(
        "projection-profile {}",
        json!({
            "case": "fanout-under-128KiB-terminal-limit",
            "errorCode": "responseTooLarge",
            "partialOriginReturned": false,
            "chargedBytes": tiny.accounting().0,
            "chargedRecords": tiny.accounting().1,
        })
    );
}
