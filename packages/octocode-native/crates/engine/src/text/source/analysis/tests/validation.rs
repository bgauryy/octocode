use super::*;

#[test]
fn empty_partition_has_one_anchor_and_preserves_all_branches() {
    let segment = |reason| MapSegment {
        range: ByteRange::new(0, 0),
        branches: vec![MapBranch::Unmapped(reason)],
    };
    let source = AdmittedSource::from_text("");
    let invalid = mapped(
        "",
        vec![
            segment(UnmappedReason::Synthetic),
            segment(UnmappedReason::MappingUnavailable),
        ],
    );
    assert!(matches!(
        invalid.bind(&source, &mut budget()),
        Err(SourceError::InvalidMap("partition"))
    ));
    let valid = mapped(
        "",
        vec![MapSegment {
            range: ByteRange::new(0, 0),
            branches: vec![
                MapBranch::Unmapped(UnmappedReason::Synthetic),
                MapBranch::Unmapped(UnmappedReason::MappingUnavailable),
            ],
        }],
    );
    let projection = valid
        .bind(&source, &mut budget())
        .unwrap()
        .project(
            ObservedSpan {
                unit: UnitHandle(1),
                range: ByteRange::new(0, 0),
            },
            &mut budget(),
        )
        .unwrap();
    assert_eq!(
        projection.segments[0].branches,
        vec![
            OriginBranch::Unmapped(UnmappedReason::Synthetic),
            OriginBranch::Unmapped(UnmappedReason::MappingUnavailable)
        ]
    );
}

#[test]
fn invalid_maps_and_resource_interruptions_are_errors() {
    let source = AdmittedSource::from_text("ab");
    for segments in [
        vec![MapSegment {
            range: ByteRange::new(1, 2),
            branches: vec![MapBranch::Unmapped(UnmappedReason::Synthetic)],
        }],
        vec![MapSegment {
            range: ByteRange::new(0, 2),
            branches: vec![MapBranch::Exact {
                input: RepresentationHandle(0),
                range: ByteRange::new(1, 2),
            }],
        }],
        vec![MapSegment {
            range: ByteRange::new(0, 2),
            branches: vec![MapBranch::Exact {
                input: RepresentationHandle(1),
                range: ByteRange::new(0, 2),
            }],
        }],
    ] {
        assert!(matches!(
            mapped("ab", segments).bind(&source, &mut budget()),
            Err(SourceError::InvalidMap(_))
        ));
    }
    let templates = SourceTemplates::new(
        "ab",
        Producer::new("plain", "1"),
        AnalysisOperation::Declarations,
    )
    .unwrap();
    let mut cancelled = MapBudget::new(
        MapLimits::default(),
        std::time::Instant::now() + std::time::Duration::from_secs(1),
        &|| true,
    );
    assert!(matches!(
        templates.bind(&source, &mut cancelled),
        Err(SourceError::Cancelled)
    ));
    let mut expired = MapBudget::new(MapLimits::default(), std::time::Instant::now(), &|| false);
    assert!(matches!(
        templates.bind(&source, &mut expired),
        Err(SourceError::Deadline)
    ));
    let mut tiny = MapBudget::new(
        MapLimits {
            bytes: 1,
            ..MapLimits::default()
        },
        std::time::Instant::now() + std::time::Duration::from_secs(1),
        &|| false,
    );
    assert!(matches!(
        templates.bind(&source, &mut tiny),
        Err(SourceError::LimitExceeded("bytes"))
    ));
    assert!(matches!(
        templates.bind(&AdmittedSource::from_text("ac"), &mut budget()),
        Err(SourceError::InvalidMap("decoded source identity"))
    ));
}

#[test]
fn catalog_rejects_dangling_contributors_and_partition_membership_lies() {
    let source = AdmittedSource::from_bytes(b"abcd", "source", &mut budget()).unwrap();
    let mut templates = SourceTemplates::new(
        "abcd",
        Producer::new("fixture", "1"),
        AnalysisOperation::Declarations,
    )
    .unwrap();
    templates.units[0].regions = vec![ByteRange::new(1, 3)];
    let bound = templates.bind(&source, &mut budget()).unwrap();
    let valid = bound
        .project(ObservedSpan::root(0, 4), &mut budget())
        .unwrap();
    let descriptors = bound
        .descriptors(std::slice::from_ref(&valid), &mut budget())
        .unwrap();
    let catalog = SourceCatalog::new(&descriptors, &mut budget()).unwrap();
    catalog
        .validate_projection(&valid, ByteRange::new(0, 4), &mut budget())
        .unwrap();
    let mut invalid = valid.clone();
    invalid.segments[0].branches = vec![OriginBranch::Derived(vec![Contribution::Source(
        OriginalRange {
            source_id: "missing".into(),
            raw_range: ByteRange::new(0, 1),
        },
    )])];
    assert!(
        catalog
            .validate_projection(&invalid, ByteRange::new(0, 4), &mut budget())
            .is_err()
    );
    let invalid = OriginProjection {
        analysis_unit_id: valid.analysis_unit_id.clone(),
        segments: vec![OriginSegment {
            source_range: ByteRange::new(0, 4),
            analysis: AnalysisMembership::Excluded,
            branches: vec![OriginBranch::Unmapped(UnmappedReason::MappingUnavailable)],
        }],
    };
    assert!(
        catalog
            .validate_projection(&invalid, ByteRange::new(0, 4), &mut budget())
            .is_err()
    );
    let mut duplicate = descriptors.clone();
    duplicate.units.push(duplicate.units[0].clone());
    assert!(SourceCatalog::new(&duplicate, &mut budget()).is_err());
}

#[test]
fn cartesian_expansion_rejects_before_returning_a_partial_projection() {
    let source = AdmittedSource::from_bytes(b"ab", "source", &mut budget()).unwrap();
    let mut templates = mapped(
        "ab",
        vec![MapSegment {
            range: ByteRange::new(0, 2),
            branches: vec![
                MapBranch::Exact {
                    input: RepresentationHandle(0),
                    range: ByteRange::new(0, 2)
                };
                2
            ],
        }],
    );
    let representation = templates
        .add_representation(
            Producer::new("derived", "1"),
            RepresentationContent::Mapped {
                text: "!".into(),
                segments: vec![MapSegment {
                    range: ByteRange::new(0, 1),
                    branches: vec![MapBranch::Derived {
                        contributors: vec![
                            MapContribution::Source {
                                input: RepresentationHandle(1),
                                range: ByteRange::new(0, 2)
                            };
                            8
                        ],
                    }],
                }],
            },
        )
        .unwrap();
    let unit = templates
        .add_unit(
            Producer::new("derived", "1"),
            AnalysisOperation::StructuralSearch,
            representation,
            vec![ByteRange::new(0, 1)],
        )
        .unwrap();
    let bound = templates.bind(&source, &mut budget()).unwrap();
    let mut constrained = MapBudget::new(
        MapLimits {
            branches: 32,
            ..MapLimits::default()
        },
        std::time::Instant::now() + std::time::Duration::from_secs(10),
        &|| false,
    );
    assert!(matches!(
        bound.project(
            ObservedSpan {
                unit,
                range: ByteRange::new(0, 1)
            },
            &mut constrained
        ),
        Err(SourceError::LimitExceeded(_))
    ));
}

#[test]
fn catalog_nested_membership_scans_respect_the_work_budget() {
    let text = "x".repeat(200);
    let source = AdmittedSource::from_bytes(text.as_bytes(), "regions", &mut budget()).unwrap();
    let mut templates = SourceTemplates::new(
        &text,
        Producer::new("fixture", "1"),
        AnalysisOperation::Declarations,
    )
    .unwrap();
    templates.units[0].regions = (0..200)
        .step_by(2)
        .map(|at| ByteRange::new(at, at + 1))
        .collect();
    let bound = templates.bind(&source, &mut budget()).unwrap();
    let span = ObservedSpan::root(198, 199);
    let origin = bound.project(span, &mut budget()).unwrap();
    let descriptors = bound
        .descriptors(std::slice::from_ref(&origin), &mut budget())
        .unwrap();
    let catalog = SourceCatalog::new(&descriptors, &mut budget()).unwrap();
    let mut limited = MapBudget::new(
        MapLimits {
            work: 20,
            ..MapLimits::default()
        },
        std::time::Instant::now() + std::time::Duration::from_secs(10),
        &|| false,
    );
    assert_eq!(
        catalog.validate_projection(&origin, span.range, &mut limited),
        Err(SourceError::LimitExceeded("work"))
    );

    let sources: Vec<_> = (0..100)
        .map(|index| {
            AdmittedSource::from_bytes(b"a", &format!("source{index}"), &mut budget()).unwrap()
        })
        .collect();
    let inputs: Vec<_> = sources.iter().collect();
    let mut templates = SourceTemplates::new(
        "a",
        Producer::new("fixture", "1"),
        AnalysisOperation::Declarations,
    )
    .unwrap();
    let mut branches = vec![MapBranch::Exact {
        input: RepresentationHandle(0),
        range: ByteRange::new(0, 1),
    }];
    for index in 1..100 {
        let input = templates.add_root("a", index).unwrap();
        branches.push(MapBranch::Exact {
            input,
            range: ByteRange::new(0, 1),
        });
    }
    let representation = templates
        .add_representation(
            Producer::new("equivalent", "1"),
            RepresentationContent::Mapped {
                text: "a".into(),
                segments: vec![MapSegment {
                    range: ByteRange::new(0, 1),
                    branches,
                }],
            },
        )
        .unwrap();
    let unit = templates
        .add_unit(
            Producer::new("fixture", "1"),
            AnalysisOperation::Declarations,
            representation,
            vec![ByteRange::new(0, 1)],
        )
        .unwrap();
    let bound = templates.bind_sources(&inputs, &mut budget()).unwrap();
    let span = ObservedSpan {
        unit,
        range: ByteRange::new(0, 1),
    };
    let origin = bound.project(span, &mut budget()).unwrap();
    let descriptors = bound
        .descriptors(std::slice::from_ref(&origin), &mut budget())
        .unwrap();
    let catalog = SourceCatalog::new(&descriptors, &mut budget()).unwrap();
    catalog
        .validate_projection(&origin, span.range, &mut budget())
        .unwrap();
    let mut limited = MapBudget::new(
        MapLimits {
            work: 500,
            ..MapLimits::default()
        },
        std::time::Instant::now() + std::time::Duration::from_secs(10),
        &|| false,
    );
    assert_eq!(
        catalog.validate_projection(&origin, span.range, &mut limited),
        Err(SourceError::LimitExceeded("work"))
    );
}
