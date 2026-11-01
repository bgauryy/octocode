use super::*;

#[test]
fn symbols_preserve_occurrence_evidence_and_change_identity_with_source() {
    let root = Fixture::new();
    let source = "// 😀\r\nfn café() {}\r\n";
    root.write("evidence.rs", source);
    let query =
        json!({"operation":"symbols","path":root.0.join("evidence.rs"),"includeProvenance":true});
    let first = run(&root.0, query.clone()).expect("symbols");
    let row = &first["symbols"][0];
    assert_eq!(row["symbolName"], "café");
    assert_eq!(row["parseAssessment"]["status"], "parsed");
    let start = row["selectionByteRange"]["startByte"].as_u64().unwrap() as usize;
    let end = row["selectionByteRange"]["endByte"].as_u64().unwrap() as usize;
    assert_eq!(&source[start..end], "café");
    assert_eq!(row["sourceDigest"].as_str().unwrap().len(), 64);
    assert_eq!(row["occurrenceId"].as_str().unwrap().len(), 64);
    let repeated = run(&root.0, query.clone()).expect("same source");
    assert_eq!(row["occurrenceId"], repeated["symbols"][0]["occurrenceId"]);
    root.write("evidence.rs", format!("{source}// changed\n"));
    let changed = run(&root.0, query).expect("changed source");
    assert_ne!(row["sourceDigest"], changed["symbols"][0]["sourceDigest"]);
    assert_ne!(row["occurrenceId"], changed["symbols"][0]["occurrenceId"]);
}

/// A default outline lists each declaration once without its per-symbol
/// provenance (ids, byte spans, origins and the descriptors they cite);
/// `includeProvenance` returns all of it for the same declarations.
#[test]
fn symbols_return_provenance_only_on_request() {
    let root = Fixture::new();
    root.write("lib.rs", "/// Doc.\nfn alpha() {}\nstruct Beta { x: u8 }\n");
    let query = json!({"operation":"symbols","path":root.0.join("lib.rs")});
    let plain = run(&root.0, query.clone()).expect("symbols");
    let mut full_query = query;
    full_query["includeProvenance"] = json!(true);
    let full = run(&root.0, full_query).expect("symbols with provenance");
    let names = |out: &Value| {
        crate::response::symbol_outline::flatten_members(out["symbols"].as_array().unwrap())
            .iter()
            .map(|row| (row["symbolName"].clone(), row["line"].clone()))
            .collect::<Vec<_>>()
    };
    assert_eq!(names(&plain), names(&full), "same declarations either way");
    for row in
        crate::response::symbol_outline::flatten_members(plain["symbols"].as_array().unwrap())
    {
        for key in [
            "occurrenceId",
            "sourceDigest",
            "sourceRange",
            "sourceOrigin",
            "selectionByteRange",
            "selectionOrigin",
            "parseAssessment",
        ] {
            assert!(row.get(key).is_none(), "{key} only on request: {plain}");
        }
    }
    assert!(plain.get("sourceEvidence").is_none(), "{plain}");
    let full_rows =
        crate::response::symbol_outline::flatten_members(full["symbols"].as_array().unwrap());
    assert!(
        full_rows.iter().all(|row| row["sourceOrigin"].is_object()),
        "{full}"
    );
    assert!(full["sourceEvidence"].is_object(), "{full}");
}

#[test]
fn symbols_keep_recovery_on_the_observed_declaration() {
    let root = Fixture::new();
    root.write(
        "evidence.cpp",
        "class Tag Actual : public Base { public: void run(); };\nclass Healthy {};\n",
    );
    let out = run(
        &root.0,
        json!({"operation":"symbols","path":root.0.join("evidence.cpp"),"parser":"cpp"}),
    )
    .expect("symbols");
    let rows = crate::response::symbol_outline::flatten_members(out["symbols"].as_array().unwrap());
    let recovered = rows
        .iter()
        .find(|row| row["symbolName"] == "Tag")
        .expect("observed parser name");
    assert_eq!(recovered["parseAssessment"]["status"], "recovered");
    assert_eq!(
        recovered["parseAssessment"]["scope"],
        "enclosingDeclaration"
    );
    let healthy = rows
        .iter()
        .find(|row| row["symbolName"] == "Healthy")
        .expect("healthy");
    assert!(
        healthy.get("parseAssessment").is_none(),
        "a clean parse says nothing"
    );
}

#[test]
fn test_declarations_are_discoverable_by_test_name() {
    let root = Fixture::new();
    root.write(
        "tests.cpp",
        "#include <gtest/gtest.h>\nTEST_F(Suite, HandlesValue) {}\nTEST_F(Suite, Other) {}\n",
    );
    let out = run(&root.0, json!({"operation":"symbols","path":root.0.join("tests.cpp"),"parser":"cpp","symbolFilter":{"terms":["HandlesValue"]}})).expect("test discovery");
    let rows = out["symbols"].as_array().expect("symbols");
    assert_eq!(rows.len(), 1, "{out}");
    assert_eq!(rows[0]["symbolName"], "Suite.HandlesValue");
    assert_eq!(rows[0]["testIdentity"]["kind"], "declaration");
    assert_eq!(rows[0]["testIdentity"]["name"], "HandlesValue");
}
