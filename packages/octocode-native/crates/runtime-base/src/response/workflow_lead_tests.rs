use super::{is_evidence, retain_guidance};
use crate::id::ToolId;
use octocode_test_support::json_fixture;
use serde_json::{Value, json};

/// One related call to every tool, in contract order: whatever a row
/// offers, the rule alone decides what stays.
fn every_lead() -> Value {
    ToolId::ALL
        .iter()
        .map(|tool| json!({"kind": "related", "tool": tool.as_str(), "input": {"queries": [{}]}}))
        .collect()
}

fn next() -> Value {
    json_fixture(
        r#"{
            "nextPage": {"kind": "continue", "tool": "x", "input": {"cursor": "c"}}
        }"#,
    )
}

fn response(row: Value) -> Value {
    json!({"results": [row]})
}

fn related_call(tool: &str, query: Value) -> Value {
    json!({"kind": "related", "tool": tool, "input": {"queries": [query]}})
}

/// The calls an answer of `tool` keeps: the ones its contract promises.
fn promised_by(tool: ToolId) -> Vec<Value> {
    every_lead()
        .as_array()
        .expect("leads")
        .iter()
        .filter(|call| {
            tool.promised_links()
                .iter()
                .any(|to| to.as_str() == call["tool"])
        })
        .cloned()
        .collect()
}

/// The rule, for every tool: an evidence row keeps its data, `next` and
/// `warnings` and carries no `hints` and no `related` beyond a promised
/// link; an empty, failed or out-of-range row keeps the recovery its
/// producer stated, untouched. Rows of one batch are judged one by one.
#[test]
fn guidance_follows_the_row_type_on_every_tool() {
    for tool in ToolId::ALL {
        let guided = |status: Option<&str>| {
            let mut row = json!({"index": 0, "data": {
                "files": [{"path": "src/lib.rs"}],
                "warnings": ["2 files were not read."],
                "hints": ["Try another scope."],
                "related": every_lead(),
                "next": next(),
            }});
            if let Some(status) = status {
                row["status"] = json!(status);
            }
            row
        };
        let stated = [None, Some("empty"), Some("error"), Some("outOfRange")];
        let mut output = json!({"results": stated.map(guided)});
        let before = output.clone();
        retain_guidance(tool, &mut output, &Value::Null);
        for (row, built) in output["results"]
            .as_array()
            .expect("rows")
            .iter()
            .zip(before["results"].as_array().expect("rows"))
        {
            if !is_evidence(row) {
                assert_eq!(row, built, "{tool}: recovery is the producer's");
                continue;
            }
            let data = &row["data"];
            assert!(data.get("hints").is_none(), "{tool}: {row}");
            assert_eq!(
                data["related"].as_array().cloned().unwrap_or_default(),
                promised_by(tool),
                "{tool}: {row}"
            );
            for kept in ["files", "warnings", "next"] {
                assert_eq!(data[kept], built["data"][kept], "{tool}: {kept}");
            }
        }
    }
}

/// No sentence keeps a call: an evidence row whose warning names `related`
/// still carries only what its producer marked or its contract promises,
/// on every tool.
#[test]
fn a_warning_naming_related_keeps_no_unmarked_call() {
    for tool in ToolId::ALL {
        for warning in [
            "1 advisory is not counted; the related read shows it.",
            "Not counted: see related[].",
        ] {
            let mut output = json!({"results": [{"index": 0, "data": {
                "files": [{"path": "a", "value": "related"}],
                "warnings": [warning],
                "related": [related_call("localFetch", json!({"path": "a"}))],
            }}]});
            retain_guidance(tool, &mut output, &Value::Null);
            assert!(
                output["results"][0]["data"].get("related").is_none(),
                "{tool}: {warning}"
            );
        }
    }
}

/// The producer decides: a related call it marked as one the row needs
/// stays on an evidence row of every tool, whatever the row's warnings say,
/// an unmarked one goes, and the mark itself is never served, on an
/// evidence row or a recovery row.
#[test]
fn a_call_its_producer_marked_required_stays_and_the_mark_is_not_served() {
    use crate::response::result::{Continuation, REQUIRED_KEY};
    let lead = |path: &str| Continuation::related(ToolId::LocalFetch, json!({"path": path}));
    let needed = lead("needed.rs").required().build();
    let optional = lead("optional.rs").build();
    for tool in ToolId::ALL {
        for status in [None, Some("empty")] {
            let mut row = json!({"index": 0, "data": {
                "files": [{"path": "a"}],
                "warnings": ["2 files were not read."],
                "related": [needed.clone(), optional.clone()],
            }});
            if let Some(status) = status {
                row["status"] = json!(status);
            }
            let mut output = response(row);
            retain_guidance(tool, &mut output, &Value::Null);
            let kept = output["results"][0]["data"]["related"]
                .as_array()
                .expect("related");
            assert!(
                kept.iter().all(|call| call.get(REQUIRED_KEY).is_none()),
                "{tool}: {kept:?}"
            );
            let paths: Vec<&Value> = kept
                .iter()
                .map(|call| &call["input"]["queries"][0]["path"])
                .collect();
            let promised = tool.promised_links().contains(&ToolId::LocalFetch);
            if status.is_some() || promised {
                assert_eq!(paths, ["needed.rs", "optional.rs"], "{tool}");
            } else {
                assert_eq!(paths, ["needed.rs"], "{tool}");
            }
        }
    }
}

/// A failed row a producer built without `status` (its `errorCode` says it)
/// is no evidence row.
#[test]
fn an_error_code_marks_a_row_as_no_evidence() {
    let mut output = json!({"results": [{"index": 0, "data": {
        "errorCode": "notFound", "error": "x", "hints": ["h"], "related": every_lead()}}]});
    let before = output.clone();
    retain_guidance(ToolId::LocalFetch, &mut output, &Value::Null);
    assert_eq!(output, before);
}

/// Text inside evidence that spells `hints` or `related` is evidence: the
/// rule owns the row's own two keys only.
#[test]
fn nested_evidence_is_never_guidance() {
    let nested = json!({"hints": ["source text"], "related": [{"tool": "x"}]});
    let mut output = json!({"results": [{"index": 0, "data": {
        "files": [{"path": "a.json", "value": nested}], "hints": ["h"]}}]});
    retain_guidance(ToolId::LocalSearch, &mut output, &Value::Null);
    let data = &output["results"][0]["data"];
    assert!(data.get("hints").is_none(), "{output}");
    assert_eq!(data["files"][0]["value"], nested);
}

/// A recovery call another row of the same response answers is dropped
/// before the rule: the batch already holds that answer.
#[test]
fn a_recovery_call_another_row_answers_is_dropped() {
    let asked = json_fixture(r#"{"path": "src/asked-elsewhere.rs"}"#);
    let call = related_call("localFetch", asked.clone());
    let own = related_call("localFetch", json!({"path": "src/other.rs"}));
    let request = json!({"queries": [asked, {"path": "src/missing.rs"}]});
    let mut output = json!({"results": [
        {"index": 0, "data": {"content": "1\tx"}},
        {"index": 1, "status": "error", "data": {
            "errorCode": "notFound", "error": "x", "related": [call, own.clone()]}},
    ]});
    retain_guidance(ToolId::LocalFetch, &mut output, &request);
    assert_eq!(output["results"][1]["data"]["related"], json!([own]));
}

/// Successful partial/selected results keep every coverage fact and required
/// read, while optional advice follows the same success policy.
#[test]
fn partial_success_preserves_coverage_and_required_reads() {
    for coverage in [
        json_fixture(
            r#"{
                "isPartial": true,
                "partialReasons": ["providerIncompleteResults"]
            }"#,
        ),
        json_fixture(
            r#"{
                "selection": {"status": "partial", "unknownItems": 2, "relatedItems": 1}
            }"#,
        ),
    ] {
        let mut data = coverage.clone();
        data["answers"] = json_fixture(r#"{"relevant": {"noul": 0.9}}"#);
        data["source"] = json_fixture(r#"{"tool": "localFetch", "query": {"path": "α.rs"}}"#);
        data["hints"] = json_fixture(r#"["Explore another tool."]"#);
        data["warnings"] = json_fixture(r#"["Provider did not cover every candidate."]"#);
        data["related"] = every_lead();
        data["next"] = json_fixture(
            r#"{
                "read": {
                    "kind": "continue",
                    "tool": "localFetch",
                    "input": {"cursor": "source"}
                },
                "unfilteredUnknown": {
                    "kind": "continue",
                    "tool": "localSearch",
                    "input": {"cursor": "unknown"}
                }
            }"#,
        );
        let next = data["next"].clone();
        let mut output = response(json!({"data": data}));
        output["next"] = next["read"].clone();
        let response_page = output["next"].clone();
        retain_guidance(ToolId::LocalSearch, &mut output, &Value::Null);
        let data = &output["results"][0]["data"];
        for (key, value) in coverage.as_object().expect("coverage") {
            assert_eq!(&data[key], value);
        }
        assert_eq!(
            data["answers"],
            json_fixture(r#"{"relevant": {"noul": 0.9}}"#)
        );
        assert_eq!(
            data["source"],
            json_fixture(r#"{"tool": "localFetch", "query": {"path": "α.rs"}}"#)
        );
        assert_eq!(
            data["warnings"],
            json_fixture(r#"["Provider did not cover every candidate."]"#)
        );
        assert_eq!(data["next"], next);
        assert_eq!(output["next"], response_page);
        assert!(data.get("hints").is_none());
        assert!(data.get("related").is_none());
    }
}

/// Each promised cross-tool link is promised by the producing tool's served
/// description: it names the target tool and says `related`.
#[test]
fn every_promised_link_is_named_by_the_producing_description() {
    let presentation = crate::contracts::cli_presentation().expect("presentation");
    for (from, to) in ToolId::ALL
        .iter()
        .flat_map(|from| from.promised_links().iter().map(move |to| (*from, *to)))
    {
        let description = presentation["tools"]
            .as_array()
            .and_then(|tools| tools.iter().find(|tool| tool["name"] == from.as_str()))
            .and_then(|tool| tool["mcp"]["description"].as_str())
            .expect("served description");
        assert!(
            names_word(description, to.as_str()) && names_word(description, "related"),
            "{from}'s description must name {to} and `related`"
        );
        if from == to {
            // A self-link is the tool's own operation leads (a node's
            // callers and callees): its description must name that operation
            // and those leads, so no other tool gains one silently.
            assert_eq!(from, ToolId::GraphQuery, "only graphQuery promises itself");
            for word in ["node", "callers", "callees"] {
                assert!(
                    names_word(description, word),
                    "{from}'s description must name its own lead `{word}`"
                );
            }
        }
    }
}

fn names_word(text: &str, word: &str) -> bool {
    text.split(|c: char| !c.is_alphanumeric())
        .any(|w| w == word)
}

/// No diagnostic code keeps a call: astSearch's
/// `structural.pattern.qualified` beside matches says spellings were left
/// out, and the related rule with `any` stays only because its producer
/// marked it as one the row needs. The same call unmarked repeats facts the
/// row states and goes, whatever note sits beside it.
#[test]
fn a_diagnostic_keeps_its_lead_only_by_the_producers_mark() {
    let lead = related_call(
        "astSearch",
        json!({"path": "src/calls.ts", "operation": "match", "rule": {"any": [
            {"pattern": "runTask($$$A)"}, {"pattern": "$QUALIFIER.runTask($$$A)"}]}}),
    );
    let request = json!({"queries": [
        {"path": "src/calls.ts", "operation": "match", "rule": {"pattern": "runTask($$$A)"}}]});
    let row = |call: Value| {
        response(json!({"index": 0, "data": {
            "matchCount": 2,
            "files": [{"path": "src/calls.ts", "matches": [
                {"line": 5, "value": "runTask(\"inside\")"},
                {"line": 9, "value": "runTask(\"outside\")"}]}],
            "diagnostics": [{"code": "structural.pattern.qualified", "severity": "info", "stage": "match",
                "message": "1 more match(es) in 1 file(s) spell a path or receiver before `runTask` (first: line 7)."}],
            "related": [call]
        }}))
    };
    let mut marked = row(crate::response::result::required_call(lead.clone()));
    retain_guidance(ToolId::AstSearch, &mut marked, &request);
    assert_eq!(
        marked["results"][0]["data"]["related"],
        json!([lead]),
        "{marked}"
    );
    let mut unmarked = row(lead.clone());
    retain_guidance(ToolId::AstSearch, &mut unmarked, &request);
    assert!(
        unmarked["results"][0]["data"].get("related").is_none(),
        "{unmarked}"
    );
}

/// A node row's own-operation leads (callers, callees) answer it by the
/// tool's promise, though they repeat the row's graph, path and target.
#[test]
fn a_graph_node_row_keeps_its_own_operation_leads() {
    let lead = |operation: &str| {
        json!({"kind": "related", "tool": "graphQuery", "input": {"queries": [
            {"operation": operation, "graph": "g1", "path": "app", "target": "app/lib.ts#run"}
        ]}})
    };
    let request = json!({"queries": [{"operation": "node", "path": "app", "target": "run"}]});
    let mut output = json!({"results": [{"index": 0, "data": {
        "graph": {"id": "g1"},
        "node": {"id": "app/lib.ts#run", "symbolName": "run", "path": "app/lib.ts"},
        "related": [lead("callers"), lead("callees"), json!({
            "kind": "related", "tool": "lspSearch", "input": {"queries": [{"path": "app/lib.ts"}]}
        })]
    }}]});
    retain_guidance(ToolId::GraphQuery, &mut output, &request);
    let related = output["results"][0]["data"]["related"]
        .as_array()
        .expect("related");
    assert_eq!(related.len(), 3, "{output}");
}
