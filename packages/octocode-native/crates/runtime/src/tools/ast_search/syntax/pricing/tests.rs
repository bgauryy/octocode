use super::*;
use crate::execution::cancel::NeverCancel;
use serde_json::json;

fn catalog() -> OutputSourceEvidence {
    serde_json::from_value(json!({
        "snapshots":[{"id":"raw","path":"/fixture/a.ts","digest":"a".repeat(64),
            "byteLength":1,"encoding":"utf8","bomBytes":0}],
        "representations":[{"id":"representation","sourceDigest":"a".repeat(64),
            "byteLength":1,"producer":{"id":"fixture","version":"1"},"sourceIds":["raw"]}],
        "units":[{"id":"unit","adapter":{"id":"fixture","version":"1"},
            "operation":"syntaxTree","representationId":"representation",
            "regions":[{"startByte":0,"endByte":1}]}]
    }))
    .unwrap()
}

#[test]
fn descriptor_pricing_rejects_missing_and_duplicate_records_without_panicking() {
    let valid = catalog();
    let mut budget = ProjectionBudget::new(ProjectionLimits::default());
    let costs = DescriptorCosts::new(&valid, &mut budget, &NeverCancel).unwrap();
    assert_eq!(costs.closures["unit"].len(), 3);
    for case in 0..3 {
        let mut broken = catalog();
        match case {
            0 => broken.snapshots.clear(),
            1 => broken.representations.clear(),
            _ => broken.units.push(broken.units[0].clone()),
        }
        let mut budget = ProjectionBudget::new(ProjectionLimits::default());
        assert!(DescriptorCosts::new(&broken, &mut budget, &NeverCancel).is_err());
    }
}
