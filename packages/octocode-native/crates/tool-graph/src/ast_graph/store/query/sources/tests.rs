use super::*;
use octocode_engine::text::analysis::{ByteRange, Contribution, UnmappedReason};
use octocode_runtime_base::execution::cancel::NeverCancel;

#[test]
fn a_source_page_requires_the_current_read_policy_even_for_stored_descriptors() {
    use octocode_runtime_base::test_support::{Fixture, workspace_policy};
    let allowed = Fixture::new();
    let other = Fixture::new();
    let graph = Graph {
        manifest: json!({"workspace": allowed.0, "root": allowed.0}),
        ..Graph::default()
    };
    let evidence = json!({"sourceEvidence":{"snapshots":[{"path":"module.ts"}]}});
    let absolute = json!({"sourceEvidence":{"snapshots":[{"path":allowed.0.join("module.ts")}]}});
    let witnesses = json!({"rows":[{"path":"module.ts","line":1}]});
    for (page, occurrences) in [(&evidence, false), (&absolute, false), (&witnesses, true)] {
        assert!(
            graph
                .authorize_source_page(page, &workspace_policy(&allowed.0), occurrences)
                .is_ok()
        );
        let refused = graph
            .authorize_source_page(page, &workspace_policy(&other.0), occurrences)
            .expect_err("a stored read must honor the current scope");
        assert_eq!(refused.value["errorCode"], "outsideAllowedRoots");
    }
}

#[test]
fn compact_identity_requires_the_span_copied_from_its_single_raw_utf8_source() {
    let table = super::super::super::format::evidence_tests::source_fixture();
    let origin = table
        .origin(
            0,
            &mut source_evidence::ProjectionBudget::default(),
            &NeverCancel,
        )
        .unwrap();
    let descriptors = table.descriptors;
    let digest = "a".repeat(64);
    assert!(is_identity(&origin, &descriptors, "a.py", &digest));
    assert!(!is_identity(&origin, &descriptors, "other.py", &digest));
    assert!(!is_identity(&origin, &descriptors, "a.py", &"b".repeat(64)));

    let mut alternatives = Vec::new();
    let mut bom = descriptors.clone();
    bom.snapshots[0].bom_bytes = 3;
    alternatives.push(("UTF-8 BOM", origin.clone(), bom));
    let mut utf16 = descriptors.clone();
    utf16.snapshots[0].encoding = Encoding::Utf16Le;
    utf16.snapshots[0].bom_bytes = 2;
    alternatives.push(("UTF-16", origin.clone(), utf16));
    // A unit that parsed part of the file (a macro body) still reads the
    // row's bytes where the file has them: what it read holds the span.
    let mut partial = descriptors.clone();
    partial.units[0].regions = vec![ByteRange::new(0, 8)];
    assert!(is_identity(&origin, &partial, "a.py", &digest));
    let mut elsewhere = descriptors.clone();
    elsewhere.units[0].regions = vec![ByteRange::new(8, 12)];
    alternatives.push((
        "span outside the analyzed regions",
        origin.clone(),
        elsewhere,
    ));
    let mut multiple = descriptors.clone();
    multiple.representations[0]
        .source_ids
        .push("another-source".into());
    alternatives.push(("multiple sources", origin.clone(), multiple));
    let mut changed = descriptors.clone();
    changed.representations[0].source_digest = "b".repeat(64);
    alternatives.push(("different representation digest", origin.clone(), changed));
    let mut resized = descriptors.clone();
    resized.representations[0].byte_length += 1;
    alternatives.push(("different representation length", origin.clone(), resized));
    let mut excluded = origin.clone();
    excluded.segments[0].analysis = AnalysisMembership::Excluded;
    alternatives.push(("excluded range", excluded, descriptors.clone()));
    let mut partitioned = origin.clone();
    partitioned.segments.push(partitioned.segments[0].clone());
    alternatives.push(("multiple segments", partitioned, descriptors.clone()));
    let OriginBranch::Copied(raw) = &origin.segments[0].branches[0] else {
        panic!("identity fixture");
    };
    for (name, branch) in [
        ("transcoded", OriginBranch::Transcoded(raw.clone())),
        (
            "derived",
            OriginBranch::Derived(vec![Contribution::Source(raw.clone())]),
        ),
        (
            "unmapped",
            OriginBranch::Unmapped(UnmappedReason::Synthetic),
        ),
    ] {
        let mut altered = origin.clone();
        altered.segments[0].branches = vec![branch];
        alternatives.push((name, altered, descriptors.clone()));
    }
    let mut shifted = origin.clone();
    shifted.segments[0].source_range = ByteRange::new(1, 9);
    alternatives.push(("different raw offsets", shifted, descriptors.clone()));
    let mut branched = origin.clone();
    branched.segments[0]
        .branches
        .push(OriginBranch::Copied(raw.clone()));
    alternatives.push(("multiple contributors", branched, descriptors.clone()));
    for (name, candidate, sources) in alternatives {
        assert!(
            !is_identity(&candidate, &sources, "a.py", &digest),
            "{name}"
        );
    }
}

#[test]
fn graph_projection_overflow_returns_only_a_terminal_error() {
    let mut graph = Graph::default();
    graph.t.sources = super::super::super::format::evidence_tests::source_fixture();
    graph.source_budget = std::cell::RefCell::new(source_evidence::ProjectionBudget::new(
        source_evidence::ProjectionLimits {
            bytes: 1,
            records: 1,
        },
    ));
    let error = graph.origin(0, &NeverCancel).unwrap_err();
    assert_eq!(error.value["errorCode"], "responseTooLarge");
    assert!(error.value.get("sourceOrigin").is_none());
    assert!(error.value.get("sourceEvidence").is_none());
    assert!(error.value.get("results").is_none());
}
