use super::*;

#[test]
fn mixed_branches_compose_without_collapsing_multiplicity() {
    let text = "abcd";
    let source = AdmittedSource::from_bytes(text.as_bytes(), "source", &mut budget()).unwrap();
    let mut templates = mapped(
        text,
        vec![
            MapSegment {
                range: ByteRange::new(0, 2),
                branches: vec![
                    MapBranch::Exact {
                        input: RepresentationHandle(0),
                        range: ByteRange::new(0, 2),
                    },
                    MapBranch::Unmapped(UnmappedReason::MappingUnavailable),
                ],
            },
            MapSegment {
                range: ByteRange::new(2, 4),
                branches: vec![MapBranch::Exact {
                    input: RepresentationHandle(0),
                    range: ByteRange::new(2, 4),
                }],
            },
        ],
    );
    let rep = templates
        .add_representation(
            Producer::new("copy", "1"),
            RepresentationContent::Mapped {
                text: text.into(),
                segments: vec![MapSegment {
                    range: ByteRange::new(0, 4),
                    branches: vec![
                        MapBranch::Exact {
                            input: RepresentationHandle(1),
                            range: ByteRange::new(0, 4)
                        };
                        2
                    ],
                }],
            },
        )
        .unwrap();
    let unit = templates
        .add_unit(
            Producer::new("copy", "1"),
            AnalysisOperation::Declarations,
            rep,
            vec![ByteRange::new(1, 3)],
        )
        .unwrap();
    let bound = templates.bind(&source, &mut budget()).unwrap();
    let result = bound
        .project(
            ObservedSpan {
                unit,
                range: ByteRange::new(0, 4),
            },
            &mut budget(),
        )
        .unwrap();
    assert_eq!(
        result
            .segments
            .iter()
            .map(|s| (s.source_range, s.analysis, s.branches.len()))
            .collect::<Vec<_>>(),
        vec![
            (ByteRange::new(0, 1), AnalysisMembership::Excluded, 4),
            (ByteRange::new(1, 2), AnalysisMembership::Included, 4),
            (ByteRange::new(2, 3), AnalysisMembership::Included, 2),
            (ByteRange::new(3, 4), AnalysisMembership::Excluded, 2)
        ]
    );
    assert!(matches!(
        &result.segments[0].branches[1],
        OriginBranch::Unmapped(UnmappedReason::MappingUnavailable)
    ));
    assert_eq!(
        result.segments[0].branches[0],
        result.segments[0].branches[2]
    );
}

#[test]
fn derived_contributors_are_joint_and_alternatives_cartesian() {
    let source = AdmittedSource::from_bytes(b"ab", "source", &mut budget()).unwrap();
    let mut templates = mapped(
        "ab",
        vec![MapSegment {
            range: ByteRange::new(0, 2),
            branches: vec![
                MapBranch::Exact {
                    input: RepresentationHandle(0),
                    range: ByteRange::new(0, 2),
                },
                MapBranch::Unmapped(UnmappedReason::MappingUnavailable),
            ],
        }],
    );
    let rep = templates
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
                                range: ByteRange::new(0, 2),
                            },
                            MapContribution::Unmapped(UnmappedReason::Synthetic),
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
            rep,
            vec![ByteRange::new(0, 1)],
        )
        .unwrap();
    let projection = templates
        .bind(&source, &mut budget())
        .unwrap()
        .project(
            ObservedSpan {
                unit,
                range: ByteRange::new(0, 1),
            },
            &mut budget(),
        )
        .unwrap();
    assert_eq!(projection.segments[0].branches.len(), 2);
    assert!(
        matches!(&projection.segments[0].branches[0], OriginBranch::Derived(v) if v.len()==2 && matches!(v[0],Contribution::Source(_)) && v[1]==Contribution::Unmapped(UnmappedReason::Synthetic))
    );
    assert!(
        matches!(&projection.segments[0].branches[1], OriginBranch::Derived(v) if v==&vec![Contribution::Unmapped(UnmappedReason::MappingUnavailable),Contribution::Unmapped(UnmappedReason::Synthetic)])
    );
}

#[test]
fn zero_width_bias_preserves_gaps_and_eof_without_crossing_branches() {
    let source = AdmittedSource::from_bytes(b"abcd", "source", &mut budget()).unwrap();
    let mut templates = mapped(
        "abcd",
        vec![
            MapSegment {
                range: ByteRange::new(0, 2),
                branches: vec![MapBranch::Exact {
                    input: RepresentationHandle(0),
                    range: ByteRange::new(0, 2),
                }],
            },
            MapSegment {
                range: ByteRange::new(2, 4),
                branches: vec![MapBranch::Unmapped(UnmappedReason::Synthetic)],
            },
        ],
    );
    templates.units[1].regions = vec![ByteRange::new(0, 1), ByteRange::new(3, 4)];
    let bound = templates.bind(&source, &mut budget()).unwrap();
    for (at, included, copied) in [
        (0, true, true),
        (1, false, true),
        (2, false, false),
        (3, true, false),
        (4, true, false),
    ] {
        let projection = bound
            .project(
                ObservedSpan {
                    unit: UnitHandle(1),
                    range: ByteRange::new(at, at),
                },
                &mut budget(),
            )
            .unwrap();
        assert_eq!(projection.segments.len(), 1);
        assert_eq!(
            projection.segments[0].analysis == AnalysisMembership::Included,
            included
        );
        assert_eq!(
            matches!(projection.segments[0].branches[0], OriginBranch::Copied(_)),
            copied
        );
    }
}

#[test]
fn equivalent_raw_origins_preserve_copied_and_transcoded_branches() {
    let text = "é😀";
    let wide: Vec<_> = [0xff, 0xfe]
        .into_iter()
        .chain(text.encode_utf16().flat_map(u16::to_le_bytes))
        .collect();
    let a = AdmittedSource::from_bytes(text.as_bytes(), "a", &mut budget()).unwrap();
    let b = AdmittedSource::from_bytes(&wide, "b", &mut budget()).unwrap();
    let mut templates = SourceTemplates::new(
        text,
        Producer::new("fixture", "1"),
        AnalysisOperation::Declarations,
    )
    .unwrap();
    let second = templates.add_root(text, 1).unwrap();
    let range = ByteRange::new(0, text.len() as u32);
    let copied = templates
        .add_representation(
            Producer::new("equivalent", "1"),
            RepresentationContent::Mapped {
                text: text.into(),
                segments: vec![MapSegment {
                    range,
                    branches: vec![
                        MapBranch::Exact {
                            input: RepresentationHandle(0),
                            range,
                        },
                        MapBranch::Exact {
                            input: second,
                            range,
                        },
                    ],
                }],
            },
        )
        .unwrap();
    let unit = templates
        .add_unit(
            Producer::new("equivalent", "1"),
            AnalysisOperation::Declarations,
            copied,
            vec![range],
        )
        .unwrap();
    let bound = templates.bind_sources(&[&a, &b], &mut budget()).unwrap();
    let projection = bound
        .project(ObservedSpan { unit, range }, &mut budget())
        .unwrap();
    assert_eq!(projection.segments.len(), 1);
    assert!(matches!(
        projection.segments[0].branches.as_slice(),
        [OriginBranch::Copied(_), OriginBranch::Transcoded(_)]
    ));
    let catalog = bound
        .descriptors(std::slice::from_ref(&projection), &mut budget())
        .unwrap();
    assert_eq!(catalog.snapshots.len(), 2);
    assert_eq!(catalog.representations.len(), 1);
    SourceCatalog::new(&catalog, &mut budget())
        .unwrap()
        .validate_projection(&projection, range, &mut budget())
        .unwrap();
}

#[test]
fn copied_endpoint_anchors_do_not_cross_an_upstream_partition_gap() {
    let source = AdmittedSource::from_bytes(b"ab", "source", &mut budget()).unwrap();
    let mut templates = mapped(
        "ab",
        vec![
            MapSegment {
                range: ByteRange::new(0, 1),
                branches: vec![MapBranch::Exact {
                    input: RepresentationHandle(0),
                    range: ByteRange::new(0, 1),
                }],
            },
            MapSegment {
                range: ByteRange::new(1, 2),
                branches: vec![MapBranch::Unmapped(UnmappedReason::MappingUnavailable)],
            },
        ],
    );
    for content in [
        RepresentationContent::Prefix {
            input: RepresentationHandle(1),
            end_byte: 1,
            tail: String::new(),
        },
        RepresentationContent::Mapped {
            text: "a".into(),
            segments: vec![MapSegment {
                range: ByteRange::new(0, 1),
                branches: vec![MapBranch::Exact {
                    input: RepresentationHandle(1),
                    range: ByteRange::new(0, 1),
                }],
            }],
        },
    ] {
        let representation = templates
            .add_representation(Producer::new("copy", "1"), content)
            .unwrap();
        let unit = templates
            .add_unit(
                Producer::new("copy", "1"),
                AnalysisOperation::StructuralSearch,
                representation,
                vec![ByteRange::new(0, 1)],
            )
            .unwrap();
        let bound = templates.bind(&source, &mut budget()).unwrap();
        let projection = bound
            .project(
                ObservedSpan {
                    unit,
                    range: ByteRange::new(1, 1),
                },
                &mut budget(),
            )
            .unwrap();
        assert!(
            matches!(&projection.segments[0].branches[0],OriginBranch::Copied(origin) if origin.raw_range==ByteRange::new(1,1)),
            "{projection:?}"
        );
    }
    let bound = templates.bind(&source, &mut budget()).unwrap();
    let parent = bound
        .project(
            ObservedSpan {
                unit: UnitHandle(1),
                range: ByteRange::new(1, 1),
            },
            &mut budget(),
        )
        .unwrap();
    assert!(matches!(
        parent.segments[0].branches[0],
        OriginBranch::Unmapped(UnmappedReason::MappingUnavailable)
    ));
}
