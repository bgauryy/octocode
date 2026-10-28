use super::*;
use serde_json::json;

mod cancellation;
mod deep;

fn descriptors(count: usize, shared_source: bool) -> Value {
    let sources = if shared_source { 1 } else { count };
    json!({
        "snapshots": (0..sources).map(|index| json!({
            "id": format!("s{index}"), "path": format!("é/😀-{index}.rs"),
            "digest": "a".repeat(64), "byteLength": 1, "encoding": "utf8", "bomBytes": 0
        })).collect::<Vec<_>>(),
        "representations": (0..count).map(|index| json!({
            "id": format!("r{index}"), "sourceDigest": "b".repeat(64), "byteLength": 1,
            "producer": {"id":"fixture", "version":"1"},
            "sourceIds": [format!("s{}", if shared_source { 0 } else { index })]
        })).collect::<Vec<_>>(),
        // Order intentionally differs from lexical and numeric identity order.
        "units": (0..count).rev().map(|index| json!({
            "id": format!("u{index}"), "adapter":{"id":"fixture", "version":"1"},
            "operation":"graphFacts", "representationId":format!("r{index}"),
            "regions":[{"startByte":0,"endByte":1}]
        })).collect::<Vec<_>>()
    })
}

fn origin(index: usize, shared_source: bool) -> Value {
    json!({"analysisUnitId":format!("u{index}"), "segments":[{
        "sourceRange":{"startByte":0,"endByte":1}, "analysis":"included", "branches":[{
            "kind":"copied", "origin":{
                "sourceId":format!("s{}", if shared_source { 0 } else { index }),
                "rawRange":{"startByte":0,"endByte":1}
            }
        }]
    }]})
}

fn indexed(count: usize, shared_source: bool) -> Catalog {
    let mut row = json!({"data":{"sourceEvidence": descriptors(count, shared_source)}});
    Catalog::take(&mut row, &CancellationToken::new())
        .unwrap()
        .unwrap()
}

#[test]
fn one_hundred_independent_units_select_only_their_closures() {
    let catalog = indexed(100, false);
    for index in 0..100 {
        let set = catalog
            .dependencies(
                &json!({"sourceOrigin":origin(index, false)}),
                &CancellationToken::new(),
            )
            .unwrap();
        let selected = catalog.selected(&set, &CancellationToken::new()).unwrap();
        for name in NAMES {
            assert_eq!(selected[name].as_array().unwrap().len(), 1);
        }
        assert_eq!(selected["units"][0]["id"], format!("u{index}"));
        assert_eq!(selected["representations"][0]["id"], format!("r{index}"));
        assert_eq!(selected["snapshots"][0]["id"], format!("s{index}"));
        assert_eq!(
            catalog.chars(&set).unwrap(),
            size(&selected, &CancellationToken::new()).unwrap()
        );
    }
}

#[test]
fn shared_sources_are_counted_once_and_merges_price_exactly() {
    let catalog = indexed(100, true);
    let mut union = catalog.empty();
    let mut charged = 0;
    for index in 0..100 {
        let set = catalog
            .dependencies(&origin(index, true), &CancellationToken::new())
            .unwrap();
        let candidate_delta = catalog
            .delta(&union, &set, &CancellationToken::new())
            .unwrap();
        assert_eq!(
            catalog
                .merge_delta(&mut union, &set, &CancellationToken::new())
                .unwrap(),
            candidate_delta
        );
        charged += candidate_delta;
        assert_eq!(
            catalog
                .merge_delta(&mut union, &set, &CancellationToken::new())
                .unwrap(),
            0
        );
        let selected = catalog.selected(&union, &CancellationToken::new()).unwrap();
        assert_eq!(selected["snapshots"].as_array().unwrap().len(), 1);
        assert_eq!(selected["units"].as_array().unwrap().len(), index + 1);
        assert_eq!(charged, size(&selected, &CancellationToken::new()).unwrap());
        assert_eq!(catalog.chars(&union).unwrap(), charged);
    }
    let selected = catalog.selected(&union, &CancellationToken::new()).unwrap();
    assert_eq!(selected["units"][0]["id"], "u99");
    assert_eq!(selected["units"][99]["id"], "u0");
    assert_eq!(selected["representations"][0]["id"], "r0");
    assert_eq!(selected["representations"][99]["id"], "r99");
}

#[test]
fn every_span_attachment_is_walked_without_changing_origins() {
    let spans = json!({
        "sourceOrigin":origin(0, true), "headerOrigin":origin(1, true),
        "selectionOrigin":origin(2, true), "bindingOrigin":origin(3, true),
        "recipient":{"sourceOrigin":origin(4, true)},
        "captures":{"A":[{"sourceOrigin":origin(5, true)}]},
        "enclosing":{"sourceOrigin":origin(6, true)}
    });
    for nested in [false, true] {
        let body = json!({"sourceEvidence":descriptors(100, true), "items":[spans.clone()]});
        let mut row = if nested { json!({"data":body}) } else { body };
        let catalog = Catalog::take(&mut row, &CancellationToken::new())
            .unwrap()
            .unwrap();
        assert!(row.get("sourceEvidence").is_none());
        assert!(row.pointer("/data/sourceEvidence").is_none());
        let set = catalog
            .dependencies(&row, &CancellationToken::new())
            .unwrap();
        assert_eq!(set.indices[UNITS].len(), 7);
        catalog
            .attach(&mut row, &set, &CancellationToken::new())
            .unwrap();
        let body = if nested { &row["data"] } else { &row };
        assert_eq!(body["items"][0], spans);
        assert_eq!(body["sourceEvidence"]["units"].as_array().unwrap().len(), 7);
    }
}

#[test]
fn all_dangling_reference_levels_fail_explicitly() {
    let mut bad_unit = descriptors(2, false);
    bad_unit["units"][0]["representationId"] = json!("absent");
    let mut bad_representation = descriptors(2, false);
    bad_representation["representations"][0]["sourceIds"][0] = json!("absent");
    let mut duplicate = descriptors(2, false);
    duplicate["units"][0]["id"] = json!("u0");
    for value in [bad_unit, bad_representation, duplicate] {
        let mut row = json!({"sourceEvidence":value});
        assert!(Catalog::take(&mut row, &CancellationToken::new()).is_err());
    }
    let catalog = indexed(2, false);
    assert!(matches!(
        catalog.dependencies(&origin(2, false), &CancellationToken::new()),
        Err(DependencyError::UnknownId {
            collection: "units",
            ..
        })
    ));
    let mut unknown_source = origin(0, false);
    unknown_source["segments"][0]["branches"][0]["origin"]["sourceId"] = json!("missing");
    assert!(matches!(
        catalog.dependencies(&unknown_source, &CancellationToken::new()),
        Err(DependencyError::UnknownId {
            collection: "snapshots",
            ..
        })
    ));
    unknown_source["segments"][0]["branches"][0]["origin"]["sourceId"] = json!("s1");
    assert!(matches!(
        catalog.dependencies(&unknown_source, &CancellationToken::new()),
        Err(DependencyError::Invalid(
            "origin source is outside representation closure"
        ))
    ));
    let mut row = json!({"sourceEvidence":descriptors(2, false), "sourceOrigin":origin(2, false)});
    assert!(
        Catalog::take(&mut row, &CancellationToken::new()).is_err(),
        "constructor checks emitted origins too"
    );
}

#[test]
fn mixed_branch_contributors_preserve_alternatives_and_validate_sources() {
    let catalog = indexed(1, true);
    let mut mixed = origin(0, true);
    mixed["segments"][0]["branches"].as_array_mut().unwrap().extend([
        json!({"kind":"unmapped", "reason":"synthetic"}),
        json!({"kind":"derived", "contributors":[
            {"kind":"source", "origin":{"sourceId":"s0", "rawRange":{"startByte":0,"endByte":1}}},
            {"kind":"unmapped", "reason":"mappingUnavailable"}
        ]})
    ]);
    let original = mixed.clone();
    let set = catalog
        .dependencies(&mixed, &CancellationToken::new())
        .unwrap();
    assert_eq!(
        set.indices[SNAPSHOTS].iter().copied().collect::<Vec<_>>(),
        [0]
    );
    assert_eq!(mixed, original);
    mixed["segments"][0]["branches"][2]["contributors"][0]["origin"]["sourceId"] = json!("absent");
    assert!(matches!(
        catalog.dependencies(&mixed, &CancellationToken::new()),
        Err(DependencyError::UnknownId {
            collection: "snapshots",
            ..
        })
    ));
}

#[test]
fn empty_and_foreign_sets_never_create_invalid_or_wrong_catalogs() {
    let catalog = indexed(1, true);
    let empty = catalog
        .dependencies(&json!({"literal":1}), &CancellationToken::new())
        .unwrap();
    assert_eq!(catalog.chars(&empty).unwrap(), 0);
    let mut row = json!({"data":{"literal":1}});
    catalog
        .attach(&mut row, &empty, &CancellationToken::new())
        .unwrap();
    assert!(row.pointer("/data/sourceEvidence").is_none());
    assert!(catalog.selected(&empty, &CancellationToken::new()).is_err());
    let mut unrelated = json!({"analysisUnitId":"arbitrary user data"});
    for _ in 0..100 {
        unrelated = json!({"nested":unrelated});
    }
    assert!(
        Catalog::take(&mut unrelated, &CancellationToken::new())
            .unwrap()
            .is_none(),
        "catalog-free rows are not scanned"
    );
    let other = indexed(1, true);
    let foreign = other
        .dependencies(&origin(0, true), &CancellationToken::new())
        .unwrap();
    assert!(matches!(
        catalog.merge_delta(&mut empty.clone(), &foreign, &CancellationToken::new()),
        Err(DependencyError::ForeignSet)
    ));
    assert!(matches!(
        catalog.selected(&foreign, &CancellationToken::new()),
        Err(DependencyError::ForeignSet)
    ));
}
