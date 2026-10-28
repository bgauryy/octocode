use super::*;

#[test]
#[expect(
    clippy::print_stderr,
    reason = "explicit resource-profile test emits its measured accounting receipt"
)]
fn admission_boundary_and_large_projection_profiles() {
    for raw_limit in [1 << 20, 8 << 20] {
        for scalar in ['x', '中', '😀'] {
            let count = (raw_limit - 2) / (scalar.len_utf16() * 2);
            let text: String = std::iter::repeat_n(scalar, count).collect();
            let raw: Vec<_> = [0xff, 0xfe]
                .into_iter()
                .chain(text.encode_utf16().flat_map(u16::to_le_bytes))
                .collect();
            let mut budget = budget();
            let source = AdmittedSource::from_bytes(&raw, "profile", &mut budget).unwrap();
            let templates = SourceTemplates::new(
                source.text(),
                Producer::new("profile", "1"),
                AnalysisOperation::GraphFacts,
            )
            .unwrap();
            let bound = templates.bind(&source, &mut budget).unwrap();
            let projection = bound
                .project(
                    ObservedSpan::root(0, source.text().len() as u32),
                    &mut budget,
                )
                .unwrap();
            let descriptors = bound
                .descriptors(std::slice::from_ref(&projection), &mut budget)
                .unwrap();
            SourceCatalog::new(&descriptors, &mut budget)
                .unwrap()
                .validate_projection(
                    &projection,
                    projection.segments[0].source_range,
                    &mut budget,
                )
                .unwrap();
            assert!(raw.len() <= raw_limit);
            assert_eq!(projection.segments.len(), 1);
            assert!(
                matches!(&projection.segments[0].branches[0], OriginBranch::Transcoded(origin) if origin.raw_range==ByteRange::new(2,raw.len() as u32))
            );
            eprintln!(
                "rawLimit={raw_limit} scalar={scalar:?} raw={} decoded={} admittedOwned={} descriptorOwned={} usage={:?}",
                raw.len(),
                source.text().len(),
                source.owned_bytes(),
                descriptors.owned_bytes(),
                budget.usage
            );
        }
    }
    let text = "x".repeat(100_000);
    let mut budget = budget();
    let source = AdmittedSource::from_bytes(text.as_bytes(), "nodes", &mut budget).unwrap();
    let templates = SourceTemplates::new(
        &text,
        Producer::new("nodes", "1"),
        AnalysisOperation::Declarations,
    )
    .unwrap();
    let bound = templates.bind(&source, &mut budget).unwrap();
    let mut retained_bytes = 0;
    for at in (0..100_000).step_by(10) {
        let projection = bound
            .project(ObservedSpan::root(at, at + 1), &mut budget)
            .unwrap();
        retained_bytes += projection.owned_bytes();
    }
    let descriptors = bound
        .descriptors_for_units(&[UnitHandle(0)], &mut budget)
        .unwrap();
    eprintln!(
        "10000 selected node origins: originOwned={retained_bytes} descriptorOwned={} usage={:?}",
        descriptors.owned_bytes(),
        budget.usage
    );
    assert_eq!(budget.usage.segments, 10_000);
    assert!(budget.usage.bytes < MapLimits::default().bytes);
}

#[test]
#[expect(
    clippy::print_stderr,
    reason = "explicit resource-profile test emits its measured accounting receipt"
)]
fn temporary_pricing_bounds_one_origin_and_preserves_cumulative_work() {
    let text = "x".repeat(100_000);
    let mut retained = budget();
    let source = AdmittedSource::from_bytes(text.as_bytes(), "pricing", &mut retained).unwrap();
    let templates = SourceTemplates::new(
        &text,
        Producer::new("pricing", "1"),
        AnalysisOperation::SyntaxTree,
    )
    .unwrap();
    let bound = templates.bind(&source, &mut retained).unwrap();
    let retained_bytes = retained.usage.bytes;
    let expected = serde_json::to_vec(
        &bound
            .project(ObservedSpan::root(0, 1), &mut budget())
            .unwrap(),
    )
    .unwrap()
    .len();
    for at in 0..100_000 {
        let price = bound
            .project_size(ObservedSpan::root(at, at + 1), &mut retained, |origin| {
                serde_json::to_vec(origin).unwrap().len()
            })
            .unwrap();
        assert!(price >= expected);
    }
    assert_eq!(retained.usage.bytes, retained_bytes);
    assert_eq!(retained.usage.segments, 0);
    assert!(retained.usage.peak_temporary_bytes > 0);
    assert!(retained.usage.peak_temporary_bytes + retained_bytes <= retained.limits.bytes);
    eprintln!("100000 transient syntax prices: usage={:?}", retained.usage);
    let before = retained.usage.bytes;
    let _selected = bound
        .project(ObservedSpan::root(0, 1), &mut retained)
        .unwrap();
    assert!(retained.usage.bytes > before);

    let mut tiny = MapBudget::new(
        MapLimits {
            bytes: 1,
            ..MapLimits::default()
        },
        std::time::Instant::now() + std::time::Duration::from_secs(10),
        &|| false,
    );
    assert!(matches!(
        bound.project_size(ObservedSpan::root(0, 1), &mut tiny, |_| panic!(
            "must reject before pricing"
        )),
        Err(SourceError::LimitExceeded("bytes"))
    ));
    let mut work = MapBudget::new(
        MapLimits {
            work: 50,
            ..MapLimits::default()
        },
        std::time::Instant::now() + std::time::Duration::from_secs(10),
        &|| false,
    );
    let mut limited = false;
    for _ in 0..20 {
        if matches!(
            bound.project_size(ObservedSpan::root(0, 1), &mut work, |_| 1),
            Err(SourceError::LimitExceeded("work"))
        ) {
            limited = true;
            break;
        }
    }
    assert!(limited);
    for (stop, deadline, expected) in [
        (
            true,
            std::time::Instant::now() + std::time::Duration::from_secs(10),
            SourceError::Cancelled,
        ),
        (false, std::time::Instant::now(), SourceError::Deadline),
    ] {
        let cancelled = || stop;
        let mut budget = MapBudget::new(MapLimits::default(), deadline, &cancelled);
        assert_eq!(
            bound.project_size(ObservedSpan::root(0, 1), &mut budget, |_| panic!(
                "interrupted"
            )),
            Err(expected)
        );
    }
}
