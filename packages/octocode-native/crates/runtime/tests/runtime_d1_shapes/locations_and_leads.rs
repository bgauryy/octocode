//! Location rows named like tool inputs, runnable recovery leads, and one camelCase error-code set.

use crate::support::{self, Run as _, Workspace};
use octocode_test_support::json_fixture;
use serde_json::{Value, json};

const RUST_SOURCE: &str = "pub fn run() {\n    let a = 1;\n    let b = a + 1;\n}\n";
const SOURCE_DIGEST: &str = "163bc96b3c3d498aede0ea31ad60d45171fbe37b026abcf6556a061374c88871";

fn identity(value: &Value) -> &str {
    let id = value.as_str().expect("source identity");
    assert_eq!(id.len(), 64);
    assert!(id.bytes().all(|byte| byte.is_ascii_hexdigit()));
    id
}

fn source_ids<'a>(data: &'a Value, operation: &str, adapter: (&str, &str)) -> (&'a str, &'a str) {
    let evidence = &data["sourceEvidence"];
    let snapshot = identity(&evidence["snapshots"][0]["id"]);
    let representation = identity(&evidence["representations"][0]["id"]);
    let unit = identity(&evidence["units"][0]["id"]);
    assert_eq!(
        evidence,
        &json!({
            "snapshots":[{
                "id":snapshot,"path":"pkg/src/lib.rs","digest":SOURCE_DIGEST,
                "byteLength":RUST_SOURCE.len(),"encoding":"utf8","bomBytes":0.0
            }],
            "representations":[{
                "id":representation,"sourceDigest":SOURCE_DIGEST,"byteLength":RUST_SOURCE.len(),
                "producer":{"id":"unicode-source","version":"1"},"sourceIds":[snapshot]
            }],
            "units":[{
                "id":unit,"adapter":{"id":adapter.0,"version":adapter.1},"operation":operation,
                "representationId":representation,"regions":[{"startByte":0,"endByte":RUST_SOURCE.len()}]
            }]
        })
    );
    (unit, snapshot)
}

fn copied_origin(unit: &str, snapshot: &str, start: usize, end: usize) -> Value {
    json!({
        "analysisUnitId":unit,"segments":[{
            "sourceRange":{"startByte":start,"endByte":end},"analysis":"included",
            "branches":[{"kind":"copied","origin":{
                "sourceId":snapshot,"rawRange":{"startByte":start,"endByte":end}
            }}]
        }]
    })
}

// ---------- location rows are objects named like tool inputs ----------

/// A declaration as the response states it: a line-first outline string
/// (Q5, formed by the hoist stage), read back as its object.
fn declared(value: &serde_json::Value) -> serde_json::Value {
    value
        .as_str()
        .and_then(octocode_runtime_base::response::symbol_outline::outline_row)
        .map_or_else(|| value.clone(), |(_, row)| row)
}

#[tokio::test]
async fn location_rows_are_objects_named_like_tool_inputs() {
    let workspace = Workspace::new();
    workspace.write("pkg/src/lib.rs", RUST_SOURCE);
    workspace.write("pkg/a.txt", "x\n");
    let runtime = workspace.runtime(&[]);
    // The enclosing declaration is listed once on the file and named by
    // index, as a line-first outline string the hoist stage forms from the
    // sealed object (Q5); it reads back as the object an lspSearch/localFetch
    // call takes.
    let hits = support::run(
        &runtime,
        "localSearch",
        json_fixture(r#"{"path": "pkg", "matchString": ["let"]}"#),
    )
    .await;
    let file = &hits.data()["files"][0];
    let first = &file["matches"][0];
    let owner = first["enclosing"].as_u64().expect("enclosing index");
    let declaration = file["declarations"][usize::try_from(owner).expect("index")]
        .as_str()
        .and_then(octocode_runtime_base::response::symbol_outline::outline_row)
        .map(|(_, row)| row);
    assert_eq!(
        declaration,
        Some(json_fixture(
            r#"{
                "symbolName": "run",
                "kind": "function",
                "line": 1,
                "endLine": 4
            }"#
        )),
        "{}",
        hits.structured_content
    );
    assert!(first.get("in").is_none(), "{first}");
    // A declaration without members is an outline string too, read back
    // as its object; per-symbol provenance only on request.
    let compact = support::run(
        &runtime,
        "astSearch",
        json_fixture(r#"{"operation": "symbols", "path": "pkg"}"#),
    )
    .await;
    assert_eq!(
        declared(&compact.data()["files"][0]["symbols"][0]),
        json_fixture(
            r#"{
                "symbolName": "run",
                "kind": "function",
                "line": 1,
                "endLine": 4,
                "exported": true
            }"#
        ),
        "{}",
        compact.structured_content
    );
    assert!(compact.data().get("sourceEvidence").is_none());
    let outline = support::run(
        &runtime,
        "astSearch",
        json_fixture(
            r#"{
                "operation": "symbols",
                "path": "pkg",
                "includeProvenance": true
            }"#,
        ),
    )
    .await;
    let first_symbol = declared(&outline.data()["files"][0]["symbols"][0]);
    let occurrence_id = identity(&first_symbol["occurrenceId"]);
    let (unit, snapshot) = source_ids(
        outline.data(),
        "declarations",
        ("tree-sitter-facts:rs", "6"),
    );
    assert_eq!(
        declared(&outline.data()["files"][0]["symbols"][0]),
        json!({
            "symbolName":"run","kind":"function","line":1,"endLine":4,"exported":true,
            "occurrenceId":occurrence_id,
            "sourceDigest":SOURCE_DIGEST,
            "sourceRange":{"startByte":0,"endByte":50},
            "sourceOrigin":copied_origin(unit,snapshot,0,50),
            "selectionByteRange":{"startByte":7,"endByte":10},
            "selectionOrigin":copied_origin(unit,snapshot,7,10),
            "parseAssessment":{
                "status":"parsed","scope":"declaration",
                "range":{"start":{"line":0,"character":0},"end":{"line":3,"character":1}}
            }
        }),
        "{}",
        outline.structured_content
    );
    let repeated = support::run(
        &runtime,
        "astSearch",
        json_fixture(
            r#"{
                "operation": "symbols",
                "path": "pkg",
                "includeProvenance": true
            }"#,
        ),
    )
    .await;
    assert_eq!(
        repeated.data()["files"][0]["symbols"][0],
        outline.data()["files"][0]["symbols"][0]
    );
    assert_eq!(
        repeated.data()["sourceEvidence"],
        outline.data()["sourceEvidence"]
    );
    // Match rows are objects with their 1-based column: two matches can
    // start on one line. Provenance is on request.
    let matched = support::run(
        &runtime,
        "astSearch",
        json_fixture(
            r#"{
                "operation": "match",
                "path": "pkg",
                "parser": "rust",
                "rule": {"pattern": "let $A = $B;"},
                "includeProvenance": true
            }"#,
        ),
    )
    .await;
    let (match_unit, match_snapshot) = source_ids(
        matched.data(),
        "structuralSearch",
        ("tree-sitter-structural:Rust", "1"),
    );
    assert_eq!(match_snapshot, snapshot);
    assert_ne!(
        match_unit, unit,
        "analysis operation owns its unit identity"
    );
    assert_eq!(&RUST_SOURCE[7..10], "run");
    assert_eq!(&RUST_SOURCE[19..29], "let a = 1;");
    assert_eq!(
        matched.data()["files"][0]["matches"][0],
        json!({
            "line":2,"value":"let a = 1;","sourceDigest":SOURCE_DIGEST,
            "sourceRange":{"startByte":19,"endByte":29},
            "sourceOrigin":copied_origin(match_unit,match_snapshot,19,29)
        }),
        "{}",
        matched.structured_content
    );
    // Without it, rows and captures keep only their display span and text,
    // inside the output contract.
    let lean = support::run(
        &runtime,
        "astSearch",
        json_fixture(
            r#"{
                "operation": "match",
                "path": "pkg",
                "parser": "rust",
                "rule": {"pattern": "let $A = $B;"},
                "includeCaptures": true
            }"#,
        ),
    )
    .await;
    // A capture on its match's line omits that line (hoist C-8): it reads
    // the row's `line`.
    let row = &lean.data()["files"][0]["matches"][0];
    assert_eq!(
        row["metavarRanges"]["A"][0],
        json_fixture(r#"{"column": 9, "endColumn": 10}"#),
        "{}",
        lean.structured_content
    );
    assert_eq!(row["line"], 2, "{}", lean.structured_content);
    for key in ["sourceDigest", "sourceRange", "sourceOrigin"] {
        assert!(row.get(key).is_none(), "{row}");
    }
    assert!(lean.data().get("sourceEvidence").is_none());
    runtime.close().await;
}

// ---------- lead names and runnable recoveries ----------

#[tokio::test]
async fn a_case_sensitive_miss_leads_to_a_runnable_ignore_case_read() {
    let workspace = Workspace::new();
    workspace.write("pkg/a.txt", "alpha\nHello World\n");
    let runtime = workspace.runtime(&[]);
    let miss = support::run(
        &runtime,
        "localFetch",
        json_fixture(r#"{"path": "pkg/a.txt", "matchString": ["hello World"]}"#),
    )
    .await;
    let ignore = miss.data()["related"]
        .as_array()
        .expect("related calls")
        .iter()
        .find(|call| {
            call["tool"] == "localFetch" && call["input"]["queries"][0]["caseMode"] == "insensitive"
        })
        .expect("explicit insensitive alternative");
    assert_eq!(ignore.get("kind"), None);
    let hit = runtime
        .run(
            "case-recovery".into(),
            "localFetch".into(),
            ignore["input"].clone(),
        )
        .await
        .expect("runnable related read");
    assert!(
        support::row_data(&hit)["content"]
            .as_str()
            .is_some_and(|text| text.contains("Hello World")),
        "{}",
        hit.structured_content
    );
    // A miss that already ignored case offers no ignoreCase lead.
    let insensitive = support::run(
        &runtime,
        "localFetch",
        json_fixture(
            r#"{
                "path": "pkg/a.txt",
                "matchString": ["absent"],
                "caseMode": "insensitive"
            }"#,
        ),
    )
    .await;
    assert!(
        insensitive.data()["related"]
            .as_array()
            .into_iter()
            .flatten()
            .all(|call| call["tool"] != "localFetch"
                || call["input"]["queries"][0]["caseMode"] != "insensitive")
    );
    runtime.close().await;
}

#[tokio::test]
async fn a_missing_file_leads_to_find_file() {
    let workspace = Workspace::new();
    workspace.write("pkg/server.rs", "fn main() {}\n");
    let runtime = workspace.runtime(&[]);
    let missing = support::run(
        &runtime,
        "localFetch",
        json_fixture(r#"{"path": "pkg/server.ts"}"#),
    )
    .await;
    let find = missing.data()["related"]
        .as_array()
        .expect("related calls")
        .iter()
        .find(|call| call["tool"] == "structureSearch")
        .expect("file discovery");
    assert_eq!(find.get("kind"), None);
    let listing = runtime
        .run(
            "find-file".into(),
            "structureSearch".into(),
            find["input"].clone(),
        )
        .await
        .expect("runnable recovery");
    assert!(listing.structured_content.to_string().contains("server.rs"));
    runtime.close().await;
}

// ---------- one camelCase error-code set ----------

#[tokio::test]
async fn error_codes_are_declared_camel_case_names() {
    let workspace = Workspace::new();
    workspace.write("pkg/a.rs", "fn a() {}\n");
    let runtime = workspace.runtime(&[]);
    let unsupported = runtime
        .run(
            "invalid-parser".into(),
            "astSearch".into(),
            json_fixture(
                r#"{
                    "queries": [
                        {
                            "operation": "match",
                            "path": "pkg/a.rs",
                            "parser": "cobol",
                            "rule": {"pattern": "fn $A() {}"}
                        }
                    ]
                }"#,
            ),
        )
        .await
        .expect_err("unknown parser rejected at admission");
    assert_eq!(
        unsupported.code,
        octocode_contract_types::ErrorCode::InvalidInput
    );
    let missing = support::run(
        &runtime,
        "localFetch",
        json_fixture(r#"{"path": "pkg/none.rs"}"#),
    )
    .await;
    assert_eq!(missing.data()["errorCode"], "notFound");
    {
        let code = support::row_data(&missing)["errorCode"]
            .as_str()
            .expect("code");
        assert!(
            code.chars().all(|c| c.is_ascii_alphanumeric()),
            "{code} is not camelCase"
        );
        assert!(
            octocode_runtime_base::id::error_codes::parse(code).is_some(),
            "{code} is not declared"
        );
    }
    runtime.close().await;
}
