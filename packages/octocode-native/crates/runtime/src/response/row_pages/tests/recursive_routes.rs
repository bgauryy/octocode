use super::*;
use serde_json::json;

#[test]
fn recursive_routes_follow_schema_edges_without_entering_lookalike_context() {
    let routes = CollectionRoutes::new(
        &[
            "/data/records".into(),
            "/data/records/*/evidence".into(),
            "/data/records/*/children".into(),
        ],
        &[(
            "/data/records/*/children/*".into(),
            "/data/records/*".into(),
        )],
    );
    let mut nested = json!({"evidence":["leaf-a","leaf-b"]});
    for depth in 0..32 {
        nested = json!({"evidence":[depth],"children":[nested],
            "context":{"evidence":["atomic-a","atomic-b"],"children":[1,2]}});
    }
    let row = json!({"data":{"records":{"0":nested,"a/b~c":{"evidence":[3,4]}}}});
    let found = collections(&row, &routes, false);
    assert_eq!(found.len(), 67);
    assert!(
        found
            .iter()
            .all(|(pointer, _)| !pointer.contains("/context/"))
    );
    assert!(
        found
            .iter()
            .any(|(pointer, _)| pointer == "/data/records/0/evidence")
    );
    assert!(
        found
            .iter()
            .any(|(pointer, _)| pointer == "/data/records/a~1b~0c/evidence")
    );
    assert_eq!(
        collection_pointer(&row, &routes).as_deref(),
        Some("/data/records")
    );
    let one = json!({"data":{"records":{"0":row["data"]["records"]["0"]}}});
    assert_eq!(
        collection_pointer(&one, &routes),
        Some(format!(
            "/data/records/0{}/evidence",
            "/children/0".repeat(32)
        ))
    );
}

#[test]
fn reference_cycles_at_the_same_value_terminate_and_deduplicate_collections() {
    let routes = CollectionRoutes::new(
        &["/data/left/items".into()],
        &[
            ("/data/left".into(), "/data/right".into()),
            ("/data/right".into(), "/data/left".into()),
            ("/data/left".into(), "/data/left".into()),
        ],
    );
    let row = json!({"data":{"left":{"items":[1,2]},"right":{"items":[3,4]}}});
    let found = collections(&row, &routes, false);
    assert_eq!(found.len(), 2);
    assert_eq!(
        found[0],
        ("/data/left/items".into(), &row["data"]["left"]["items"])
    );
    assert_eq!(
        found[1],
        ("/data/right/items".into(), &row["data"]["right"]["items"])
    );
}

#[test]
fn only_optional_declared_empty_siblings_are_omitted() {
    let routes = CollectionRoutes::new(
        &[
            "/data/owners/*/items".into(),
            "/data/owners/*/optional".into(),
            "/data/owners/*/required".into(),
            "/data/nested/entries".into(),
        ],
        &[],
    );
    let mut row = json!({"data":{"owners":[{"items":[1,2],"optional":[],"required":[],
        "context":{"optional":[],"required":["keep"]},"coordinates":[0,1]}],"nested":{"entries":[]}}});
    let mut source = row.clone();
    source["data"]["owners"][0]["optional"] = json!(["already delivered"]);
    source["data"]["nested"]["entries"] = json!(["already delivered"]);
    drop_empty_declared_siblings(
        &mut row,
        &source,
        "/data/owners/0/items",
        &routes,
        &BTreeSet::from(["required".into()]),
    );
    let owner = &row["data"]["owners"][0];
    assert!(owner.get("optional").is_none());
    assert_eq!(owner["required"], json!([]));
    assert_eq!(owner["items"], json!([1, 2]));
    assert_eq!(owner["coordinates"], json!([0, 1]));
    assert_eq!(row["data"]["nested"]["entries"], json!([]));
    assert_eq!(owner["context"], json!({"optional":[],"required":["keep"]}));
    let mut original_empty = source.clone();
    original_empty["data"]["owners"][0]["optional"] = json!([]);
    let mut later = original_empty.clone();
    drop_empty_declared_siblings(
        &mut later,
        &original_empty,
        "/data/owners/0/items",
        &routes,
        &BTreeSet::new(),
    );
    assert_eq!(
        later["data"]["owners"][0]["optional"],
        json!([]),
        "known empty stays distinct from absent"
    );
}
