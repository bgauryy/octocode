use super::*;

fn fixture() -> Fixture {
    let dir = Fixture::new();
    dir.write("app/plain.ts", "export function handler(){} export function run(){ handler(); sink(handler, handler); } function shadow(handler:unknown){ sink(handler); }\n");
    let text = "export function encoded(){ missing(); }\n";
    let utf16 = [0xff, 0xfe]
        .into_iter()
        .chain(text.encode_utf16().flat_map(u16::to_le_bytes))
        .collect::<Vec<_>>();
    dir.write("app/encoded.ts", utf16);
    dir.write("app/bom.ts", format!("\u{feff}{text}"));
    call(&dir.0, ToolId::GraphBuild, json!({"path":"app"}));
    dir
}

/// The one rule of a lean row: a row whose origins are all the file's own text
/// loses its provenance and nothing else; any other row keeps all of it.
fn assert_same_evidence(compact: &Value, full: &Value, symbol_file: Option<&Value>) {
    let mut expected = full.clone();
    let row = expected.as_object_mut().expect("row");
    // Lean rows omit derivation; includeProvenance returns it.
    row.remove("derivation");
    let plain = compact
        .get("sourceOrigin")
        .is_none_or(|origin| !origin.is_object())
        && compact
            .get("selectionOrigin")
            .is_none_or(|origin| !origin.is_object());
    if plain {
        for field in [
            "occurrenceId",
            "sourceDigest",
            "sourceRange",
            "sourceOrigin",
            "selectionByteRange",
            "selectionOrigin",
            "bindingSourceRange",
            "bindingOrigin",
            "recipient",
        ] {
            row.remove(field);
        }
        if row
            .get("parseAssessment")
            .is_some_and(|assessment| assessment["status"] == "parsed")
        {
            row.remove("parseAssessment");
        }
    }
    let mut actual = compact.clone();
    if let Some(file) = symbol_file {
        assert_eq!(&actual["file"], file);
        assert!(full.get("file").is_none());
        actual.as_object_mut().expect("row").remove("file");
    }
    if !plain {
        for field in ["sourceOrigin", "selectionOrigin", "bindingOrigin"] {
            if actual[field] == "identity" {
                actual[field] = full[field].clone();
            }
        }
        if let (Some(recipient), Some(original)) =
            (actual.get_mut("recipient"), full.get("recipient"))
        {
            if recipient["sourceOrigin"] == "identity" {
                recipient["sourceOrigin"] = original["sourceOrigin"].clone();
            }
        }
    }
    assert_eq!(
        &actual, &expected,
        "a lean row differs only by its provenance"
    );
}

#[test]
fn compact_graph_origins_keep_transformed_sources_and_complete_evidence() {
    let dir = fixture();
    for operation in ["symbols", "witnesses", "unresolved"] {
        let mut row = json!({"operation":operation,"path":"app"});
        if operation == "symbols" {
            row["target"] = json!("plain.ts");
        } else if operation == "witnesses" {
            row["edges"] = json!(["calls", "passes"]);
        }
        let default = call(&dir.0, ToolId::GraphQuery, row.clone());
        let mut full_query = row.clone();
        full_query["includeProvenance"] = json!(true);
        let full = call(&dir.0, ToolId::GraphQuery, full_query);
        let mut compact_query = row;
        compact_query["includeProvenance"] = json!(false);
        let compact = call(&dir.0, ToolId::GraphQuery, compact_query);
        assert_eq!(default["rows"], compact["rows"], "lean is the default");
        assert_eq!(default["sourceEvidence"], compact["sourceEvidence"]);
        assert_eq!(
            compact["pagination"]["totalItems"],
            full["pagination"]["totalItems"]
        );
        assert_eq!(compact["coverage"], full["coverage"]);
        assert_eq!(compact["argumentCoverage"], full["argumentCoverage"]);
        let compact_rows = compact["rows"].as_array().expect("compact results");
        let full_rows = full["rows"].as_array().expect("full results");
        assert!(!compact_rows.is_empty(), "{operation}: {compact}");
        assert_eq!(compact_rows.len(), full_rows.len());
        let mut plain = 0;
        let mut encoded = 0;
        let mut bom = 0;
        for (projected, original) in compact_rows.iter().zip(full_rows) {
            assert_same_evidence(
                projected,
                original,
                (operation == "symbols").then_some(&full["target"]["id"]),
            );
            match projected["file"].as_str() {
                Some("plain.ts") => {
                    plain += 1;
                    assert!(projected.get("sourceOrigin").is_none(), "{projected}");
                }
                Some("encoded.ts") => {
                    encoded += 1;
                    assert!(projected["sourceOrigin"].is_object());
                }
                Some("bom.ts") => {
                    bom += 1;
                    assert!(projected["sourceOrigin"].is_object());
                }
                other => panic!("{operation}: unexpected file {other:?}: {projected}"),
            }
        }
        assert!(plain > 0, "{operation}");
        let snapshots = compact["sourceEvidence"]["snapshots"].as_array();
        if operation == "unresolved" {
            assert!(encoded > 0 && bom > 0, "{operation}");
            let sources = snapshots.expect("complex source closure");
            assert_eq!(sources.len(), 2);
            let names = sources
                .iter()
                .map(|source| {
                    Path::new(source["path"].as_str().expect("source path"))
                        .file_name()
                        .expect("file name")
                        .to_str()
                        .expect("UTF-8")
                })
                .collect::<BTreeSet<_>>();
            assert_eq!(names, BTreeSet::from(["bom.ts", "encoded.ts"]));
        } else {
            assert!(snapshots.is_none(), "only plain.ts rows are returned");
        }
    }
}

#[test]
fn compact_graph_pages_preserve_all_occurrence_ids_and_projection_choice() {
    let dir = fixture();
    for operation in ["symbols", "witnesses", "unresolved"] {
        let mut views = Vec::new();
        for include in [true, false] {
            let mut query =
                json!({"operation":operation,"path":"app","includeProvenance":include,"limit":1});
            if operation == "symbols" {
                query["target"] = json!("plain.ts");
            } else if operation == "witnesses" {
                query["edges"] = json!(["calls", "passes"]);
            }
            let mut rows = Vec::new();
            let mut ids = BTreeSet::new();
            loop {
                let page = call(&dir.0, ToolId::GraphQuery, query);
                for row in page["rows"].as_array().expect("page results") {
                    assert!(ids.insert(row["id"].as_str().expect("id").to_owned()));
                    rows.push(row.clone());
                }
                match page.pointer("/next/nextPage/input/queries/0") {
                    Some(next) => {
                        let opened = crate::tools::test_support::opened(ToolId::GraphQuery, next);
                        assert_eq!(opened["includeProvenance"], include);
                        query = next.clone();
                    }
                    None => {
                        assert_eq!(ids.len() as u64, page["pagination"]["totalItems"]);
                        break;
                    }
                }
            }
            views.push(rows);
        }
        assert_eq!(views[0].len(), views[1].len());
        assert!(!views[0].is_empty());
        for (full, compact) in views[0].iter().zip(&views[1]) {
            assert_same_evidence(
                compact,
                full,
                (operation == "symbols").then_some(&json!("plain.ts")),
            );
        }
    }
}

#[test]
fn compact_ambiguity_continuation_keeps_the_projection_choice() {
    let dir = Fixture::new();
    for number in 0..21 {
        dir.write(&format!("app/{number:02}.ts"), "export function shared(){}");
    }
    call(&dir.0, ToolId::GraphBuild, json!({"path":"app"}));
    let first = call(
        &dir.0,
        ToolId::GraphQuery,
        json!({"operation":"node","path":"app","target":"shared","includeProvenance":false}),
    );
    assert_eq!(first["pagination"]["totalItems"], 21);
    let candidates = first["candidates"].as_array().expect("candidates");
    assert_eq!(candidates.len(), 20);
    assert!(
        candidates
            .iter()
            .all(|row| row.get("sourceOrigin").is_none()),
        "{first}"
    );
    let next = first
        .pointer("/next/nextPage/input/queries/0")
        .expect("next page");
    assert_eq!(
        crate::tools::test_support::opened(ToolId::GraphQuery, next)["includeProvenance"],
        false
    );
    let remaining = call(&dir.0, ToolId::GraphQuery, next.clone());
    let rows = remaining["rows"].as_array().expect("remaining candidate");
    assert_eq!(rows.len(), 1);
    assert!(rows[0].get("sourceOrigin").is_none());
    assert!(candidates.iter().all(|row| row["id"] != rows[0]["id"]));
}
