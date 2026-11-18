use super::*;

#[test]
fn compact_pool_storage_profile_retains_bytes_and_index_only() {
    const COUNT: usize = 200_000;
    let fixture = super::super::super::evidence_tests::source_fixture();
    let origin = fixture
        .origin(0, &mut ProjectionBudget::default(), &NeverCancel)
        .unwrap();
    let started = std::time::Instant::now();
    let table = SourceTable::from_origins(
        fixture.descriptors,
        std::iter::repeat_n(&origin, COUNT),
        &NeverCancel,
    )
    .unwrap();
    let encoded = encode(&table, &NeverCancel).unwrap();
    let decoded = decode(&encoded, &NeverCancel).unwrap();
    assert_eq!(decoded, table);
    assert_eq!(
        decoded
            .origin(
                (COUNT - 1) as u32,
                &mut ProjectionBudget::default(),
                &NeverCancel
            )
            .unwrap(),
        origin
    );
    let (pool, index) = table.origins.storage_bytes();
    let json_origin_bytes = serde_json::to_vec(&origin).unwrap().len();
    assert!(encoded.len() < COUNT * json_origin_bytes / 8);
    assert_eq!(index, COUNT * 24);
    println!(
        "COMPACT_SOURCE_PROFILE origins={COUNT} encoded_section_bytes={} pool_bytes={pool} index_payload_bytes={index} repeated_origin_json_payload_bytes={} elapsed_ms={} (synthetic repeated origin; not RSS or query throughput)",
        encoded.len(),
        COUNT * json_origin_bytes,
        started.elapsed().as_millis()
    );
}

#[test]
fn descriptor_enum_inventory_roundtrips_without_an_origin() {
    let (mut descriptors, _) = fixture();
    let unit = descriptors.units.remove(0);
    descriptors.units = [
        domain::AnalysisOperation::StructuralSearch,
        domain::AnalysisOperation::SyntaxTree,
        domain::AnalysisOperation::SignatureOutline,
        domain::AnalysisOperation::Declarations,
        domain::AnalysisOperation::GraphFacts,
    ]
    .into_iter()
    .enumerate()
    .map(|(i, operation)| domain::AnalysisUnit {
        id: i.to_string(),
        operation,
        ..unit.clone()
    })
    .collect();
    let table = SourceTable::from_origins(descriptors, [], &NeverCancel).unwrap();
    assert_eq!(
        decode(&encode(&table, &NeverCancel).unwrap(), &NeverCancel).unwrap(),
        table
    );
}

#[test]
fn publication_and_selected_large_contributor_decode_observe_cancellation() {
    let (descriptors, mut origins) = fixture();
    origins[0].segments[0].branches = vec![domain::OriginBranch::Derived(
        vec![domain::Contribution::Unmapped(domain::UnmappedReason::Synthetic); 10_000],
    )];
    let table = SourceTable::from_origins(descriptors, &origins, &NeverCancel).unwrap();
    struct Stop(std::sync::atomic::AtomicUsize);
    impl CancellationCheck for Stop {
        fn check(&self) -> Result<(), String> {
            if self.0.fetch_sub(1, std::sync::atomic::Ordering::Relaxed) <= 1 {
                Err("stopped".into())
            } else {
                Ok(())
            }
        }
    }
    let stop = Stop(std::sync::atomic::AtomicUsize::new(1000));
    assert_eq!(
        table
            .origin(0, &mut ProjectionBudget::default(), &stop)
            .unwrap_err()
            .code,
        octocode_contract_types::ErrorCode::Cancelled
    );
    let graph = GraphTables {
        sources: table,
        ..Default::default()
    };
    let stop = Stop(std::sync::atomic::AtomicUsize::new(1000));
    assert_eq!(
        writer::encode_for_publish(&graph, &stop).unwrap_err().code,
        octocode_contract_types::ErrorCode::Cancelled
    );
}
