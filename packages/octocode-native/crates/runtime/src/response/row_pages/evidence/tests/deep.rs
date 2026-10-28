use super::*;

fn nested_outline(origin: Value, depth: usize) -> Value {
    let mut node = json!({"name":"leaf", "kind":"function", "sourceOrigin":origin});
    for level in 0..depth {
        let mut parent = Map::new();
        parent.insert("name".into(), Value::String(format!("Container{level}")));
        parent.insert("kind".into(), Value::String("class".into()));
        parent.insert("members".into(), Value::Array(vec![node]));
        node = Value::Object(parent);
    }
    let mut data = Map::new();
    data.insert("sourceEvidence".into(), descriptors(1, true));
    data.insert("symbols".into(), Value::Array(vec![node]));
    Value::Object(Map::from_iter([("data".into(), Value::Object(data))]))
}

#[test]
fn catalog_present_origin_beyond_sixty_four_outline_levels_is_supported() {
    let token = CancellationToken::new();
    let expected_origin = origin(0, true);
    let mut row = nested_outline(expected_origin.clone(), 256);
    let catalog = Catalog::take(&mut row, &token).unwrap().unwrap();
    let set = catalog.dependencies(&row, &token).unwrap();
    assert_eq!(set.indices[UNITS].len(), 1);
    assert_eq!(set.indices[REPRESENTATIONS].len(), 1);
    assert_eq!(set.indices[SNAPSHOTS].len(), 1);
    catalog.attach(&mut row, &set, &token).unwrap();
    let mut leaf = &row["data"]["symbols"][0];
    for _ in 0..256 {
        leaf = &leaf["members"][0];
    }
    assert_eq!(leaf["sourceOrigin"], expected_origin);
    assert_eq!(row["data"]["sourceEvidence"]["units"][0]["id"], "u0");
}

#[test]
fn dangling_ids_beyond_sixty_four_outline_levels_are_still_errors() {
    let token = CancellationToken::new();
    let mut bad_source = origin(0, true);
    bad_source["segments"][0]["branches"][0]["origin"]["sourceId"] = json!("missing-source");
    for (bad, expected_collection, expected_id) in [
        (origin(1, true), "units", "u1"),
        (bad_source, "snapshots", "missing-source"),
    ] {
        let mut row = nested_outline(bad, 256);
        match Catalog::take(&mut row, &token) {
            Err(DependencyError::UnknownId { collection, id }) => {
                assert_eq!(collection, expected_collection);
                assert_eq!(id, expected_id);
            }
            _ => panic!("deep dangling reference must fail with its exact identity"),
        }
    }
}
