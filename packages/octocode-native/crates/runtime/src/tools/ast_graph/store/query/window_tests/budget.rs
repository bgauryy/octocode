use super::*;
use crate::response::source_evidence::{ProjectionBudget, ProjectionLimits};

#[test]
fn unique_reference_resolution_does_not_project_discarded_candidate_evidence() {
    let mut g = graph(&[("a.ts#only", "only", NodeKind::Symbol, 1)]);
    g.t.declarations
        .push(super::super::super::format::DeclarationRec {
            node: 0,
            test_identity: NONE,
            occurrence: NONE,
            source_digest: NONE,
            source_start: 0,
            source_end: 1,
            source_origin: 0,
            selection_start: NONE,
            selection_end: NONE,
            selection_origin: NONE,
            assessment: NONE,
        });
    g.t.sources = super::super::super::format::evidence_tests::source_fixture();
    g.source_budget = std::cell::RefCell::new(ProjectionBudget::new(ProjectionLimits {
        bytes: 1,
        records: 1,
    }));
    let options = QueryOptions {
        op: "node".into(),
        ..Default::default()
    };
    let query = Query {
        g: &g,
        options: &options,
        flags: Flags::parse(&options).unwrap(),
        cancel: &NeverCancel,
    };
    assert_eq!(query.resolve("a.ts#only", None).unwrap(), 0);
    assert_eq!(query.resolve("only", None).unwrap(), 0);
    let candidates = g
        .reference_candidates("only", None, false, &NeverCancel)
        .unwrap();
    let empty = candidates.page(1..2, &NeverCancel).unwrap();
    assert_eq!(empty.total, 1);
    assert!(empty.rows.is_empty());
    assert_eq!(
        candidates.page(0..1, &NeverCancel).err().unwrap().value["errorCode"],
        "responseTooLarge"
    );
}
