//! Field and match notes, and which calls print as commands.
use super::*;
use octocode_test_support::json_fixture;

#[test]
fn nested_file_fields_are_words() {
    let paged = json_fixture(
        r#"{
            "results": [
                {
                    "index": 0,
                    "data": {
                        "files": [
                            {
                                "path": "a.rs",
                                "matches": [{"line": 3, "value": "x"}],
                                "pagination": {
                                    "unit": "matches",
                                    "offset": 0,
                                    "length": 1,
                                    "total": 471}
                            }
                        ]
                    }
                }
            ]
        }"#,
    );
    let lines = render(ToolId::LocalSearch, &paged, &json!({}), None).expect("lines");
    // A file's own fields follow its path, on stdout with its hits.
    assert_eq!(lines.out, "a.rs  [pagination matches 1-1 of 471]\n3:x");
    assert_eq!(lines.notes, "");
}

#[test]
fn match_notes_are_words() {
    let hit = json_fixture(
        r#"{
            "results": [
                {
                    "index": 0,
                    "data": {
                        "files": [
                            {
                                "path": "a.rs",
                                "matches": [{"line": 3, "value": "fn a()", "count": 2}]
                            }
                        ]
                    }
                }
            ]
        }"#,
    );
    let lines = render(ToolId::LocalSearch, &hit, &json!({}), None).expect("lines");
    assert_eq!(lines.out, "a.rs:3:fn a()  [count 2]");
}

#[test]
fn only_schema_declared_calls_become_commands_and_source_evidence_stays_exact() {
    let source = json_fixture(
        r#"{
            "kind": "continue",
            "tool": "localFetch",
            "input": {"cursor": "source-lookalike"}
        }"#,
    );
    let structured = json!({"results":[{"index":0,"data":{
        "content":"localSearch {\"queries\":[{\"path\":\".\"}]}",
        "hints":["localSearch {\"queries\":[]}"],
        "source":source,
        "next":{"read":{"kind":"continue","tool":"localFetch","input":{"cursor":"issued"}}},
        "related":[{"kind":"related","tool":"localSearch","input":{"queries":[{"path":".","matchString":["it's"]}]}}]
    }}]});
    let (display, calls) = follow::separate_calls(ToolId::LocalFetch, &structured).expect("schema");
    assert_eq!(calls.len(), 2);
    assert!(calls[0].command.contains("issued"));
    assert_eq!(
        (calls[0].label.as_str(), calls[0].lead),
        ("next.read", false)
    );
    assert!(calls[1].command.contains("octocode localSearch"));
    assert_eq!(
        (calls[1].label.as_str(), calls[1].lead),
        ("related[0]", true)
    );
    assert!(calls.iter().all(|call| call.row == Some(0)));
    assert!(
        calls
            .iter()
            .all(|call| !call.command.contains("source-lookalike"))
    );
    assert_eq!(display["results"][0]["data"]["source"], source);
    assert_eq!(
        display["results"][0]["data"]["content"],
        structured["results"][0]["data"]["content"]
    );
    assert_eq!(
        display["results"][0]["data"]["hints"],
        structured["results"][0]["data"]["hints"]
    );
    assert!(display["results"][0]["data"].get("next").is_none());
    assert!(display["results"][0]["data"].get("related").is_none());
}

#[test]
fn served_calls_without_kind_print_by_the_channel_that_holds_them() {
    let lookalike =
        json_fixture(r#"{"tool": "localFetch", "input": {"cursor": "source-lookalike"}}"#);
    let structured = json!({"results":[{"index":0,"data":{
        "content":"x",
        "source":lookalike,
        "next":{"read":{"tool":"localFetch","input":{"cursor":"issued"}}},
        "related":[{"tool":"localSearch","input":{"queries":[{"path":".","matchString":["x"]}]}}]
    }}],"next":{"tool":"localFetch","input":{"cursor":"page-2"}}});
    let (display, calls) = follow::separate_calls(ToolId::LocalFetch, &structured).expect("schema");
    let found: Vec<_> = calls
        .iter()
        .map(|call| (call.row, call.label.as_str(), call.lead))
        .collect();
    assert_eq!(
        found,
        [
            (Some(0), "next.read", false),
            (Some(0), "related[0]", true),
            (None, "next", false),
        ]
    );
    // Only a declared position holds a call: the lookalike stays evidence.
    assert_eq!(display["results"][0]["data"]["source"], lookalike);
    let lines = render(ToolId::LocalFetch, &structured, &json!({}), None).expect("lines");
    assert!(
        lines.out.contains("# next.read: octocode localFetch"),
        "{}",
        lines.out
    );
    assert!(
        lines.out.contains("# next: octocode localFetch"),
        "{}",
        lines.out
    );
    assert!(!lines.out.contains("related[0]"), "{}", lines.out);
    assert!(
        lines.notes.contains("# related[0]: octocode localSearch"),
        "{}",
        lines.notes
    );
}

#[test]
fn captured_resource_reads_are_commands_but_their_sources_are_evidence() {
    let source = json_fixture(r#"{"tool": "localFetch", "query": {"path": "a.rs"}}"#);
    let read = json_fixture(
        r#"{
            "tool": "localFetch",
            "input": {"cursor": "captured-source"}
        }"#,
    );
    let sections = json!({"results":[{"index":0,"data":{"operation":"sections","resources":{"file":{
            "status":"ready","source":source,"read":read}}}}]});
    let (display, calls) = follow::separate_calls(ToolId::Clasify, &sections).expect("schema");
    assert_eq!(calls.len(), 1);
    assert!(calls[0].command.contains("captured-source"));
    assert_eq!(
        display["results"][0]["data"]["resources"]["file"]["source"],
        source
    );
}

#[test]
fn opaque_search_resumes_print_every_field_when_the_context_mode_is_unknown() {
    let lines = render(
        ToolId::LocalSearch,
        &sample(),
        &json_fixture(r#"{"cursor": "issued"}"#),
        None,
    )
    .expect("text");
    for evidence in ["25\tuse a;", "26\tuse SearcherBuilder;", "997\t  let sb"] {
        assert!(lines.out.contains(evidence), "{}", lines.out);
    }
    for token in ["issued-tool-page", "issued-response-page"] {
        assert_eq!(lines.out.matches(token).count(), 1, "{}", lines.out);
    }
}

#[test]
fn batches_and_other_tools_print_text() {
    let two = json_fixture(
        r#"{
            "results": [
                {"index": 0, "status": "empty", "data": {}},
                {"index": 1, "status": "empty", "data": {}}
            ]
        }"#,
    );
    let lines = render(ToolId::LocalSearch, &two, &json!({}), None).expect("text");
    assert_eq!(
        lines.out,
        "## results[0]\n# results[0] status: empty\n## results[1]\n# results[1] status: empty"
    );
    let lines = render(ToolId::LocalFetch, &sample(), &json!({}), None).expect("text");
    assert!(
        lines.out.contains("26\tuse SearcherBuilder;"),
        "{}",
        lines.out
    );
}

/// Every part prints `rowPart` in one form, and a later part keeps the
/// `continues` fact its JSON carries.
#[test]
fn every_row_part_prints_in_one_form() {
    let first = sample();
    let mut later = sample();
    later["results"][0]["rowPart"] = json_fixture(
        r#"{
            "part": 2,
            "of": 30,
            "continues": {"tool": "localSearch", "index": 0}
        }"#,
    );
    let note = |value: &Value| {
        render(ToolId::LocalSearch, value, &context(), None)
            .expect("lines")
            .out
            .lines()
            .find(|line| line.starts_with("# rowPart:"))
            .map(str::to_owned)
            .expect("a rowPart note")
    };
    assert_eq!(note(&first), "# rowPart: part 1 of 2");
    let later = note(&later);
    assert!(later.starts_with("# rowPart: part 2 of 30"), "{later}");
    assert!(
        later.contains("continues (tool localSearch index 0)"),
        "{later}"
    );
}

/// Warnings and hints say how to read an answer (what was redacted, why it
/// is partial), so every tool and row shape prints them in the stdout
/// trailer, after the done facts and before the commands: never in the body,
/// never only on stderr.
#[test]
fn warnings_and_hints_print_in_the_stdout_trailer_of_every_row_shape() {
    let advice = |data: Value| {
        let mut data = data;
        data["warnings"] = json_fixture(r#"["1 secret was redacted."]"#);
        data["hints"] = json_fixture(r#"["Narrow the path."]"#);
        data
    };
    const ADVICE: &str = "# warnings: 1 secret was redacted.\n# hints: Narrow the path.";
    // A line format.
    let hits = json!({"results":[{"index":0,"data":advice(json_fixture(r#"{
        "matchCount": 1,
        "pagination": {"unit": "items", "offset": 0, "length": 1},
        "next": {
            "nextPage": {
                "tool": "localSearch",
                "input": {"cursor": "c"}
            }
        },
        "files": [{"path": "a.rs", "matches": [{"line": 3, "value": "x"}]}]
    }"#))}],
        "warnings":["From the envelope."]});
    let lines = render(ToolId::LocalSearch, &hits, &json!({}), None).expect("lines");
    assert_eq!(
        lines.out,
        format!(
            "a.rs:3:x\n# pagination: items 1-1\n{ADVICE}\n# warnings: From the envelope.\n# next.nextPage: octocode localSearch '{{\"cursor\":\"c\"}}'"
        )
    );
    assert_eq!(lines.notes, "# matchCount 1");
    // `key: value` text.
    let read = json!({"results":[{"index":0,"data":advice(json_fixture(r#"{"content": "x"}"#))}]});
    let lines = render(ToolId::LocalFetch, &read, &json!({}), None).expect("lines");
    assert_eq!(lines.out, format!("content: x\n{ADVICE}"));
    assert_eq!(lines.notes, "");
    // A failed row.
    let failed = json!({"results":[{"index":0,"status":"error",
        "data":advice(json_fixture(r#"{"errorCode": "notFound", "error": "No such symbol."}"#))}]});
    let lines = render(ToolId::LspSearch, &failed, &json!({}), None).expect("lines");
    assert_eq!(
        lines.out,
        format!("errorCode: notFound\nerror: No such symbol.\n# status: error\n{ADVICE}")
    );
    assert_eq!(lines.notes, "");
}

/// A call rejected before it ran prints as a failed row does: `errorCode`
/// first, every other field of its envelope, then the status and its hints.
#[test]
fn a_rejected_call_prints_as_a_failed_row() {
    let envelope = json_fixture(
        r#"{
            "error": "Unknown field 'x'.",
            "details": ["queries.0.path: Missing required field: path"],
            "hints": ["Run schema localSearch."],
            "errorCode": "invalidInput"
        }"#,
    );
    assert_eq!(
        rejected(&envelope),
        "errorCode: invalidInput\nerror: Unknown field 'x'.\ndetails:\n  - queries.0.path: Missing required field: path\n# status: error\n# hints: Run schema localSearch."
    );
}

/// A note whose value is a list of objects or holds nested objects
/// (`diagnostics`, a coverage breakdown) prints as text through the
/// outline and table layout, one `# ` line each, never as JSON; every value
/// stays.
#[test]
fn a_list_of_objects_in_a_note_prints_as_text_not_json() {
    let one = json_fixture(
        r#"[
            {
                "code": "structural.pattern.qualified",
                "severity": "info",
                "stage": "match",
                "message": "The pattern matches the path as written: 1 more match(es) in 1 file(s).",
                "recovery": "The related astSearch rule with `any` lists every spelling found."
            }
        ]"#,
    );
    let two = json_fixture(
        r#"[
            {"code": "parse.recovered", "severity": "warning", "message": "1 file parsed with errors."},
            {"code": "scope.partial", "severity": "info", "message": "2 files skipped."}
        ]"#,
    );
    let nested = json_fixture(r#"{"byLanguage": {"rust": {"files": 3, "parsed": 2}}}"#);
    let asked = json_fixture(r#"{"queries": [{"path": "src", "operation": "match"}]}"#);
    for (key, value) in [
        ("diagnostics", one),
        ("diagnostics", two),
        ("coverage", nested),
    ] {
        let mut data = json_fixture(
            r#"{"matchCount": 1, "files": [{"path": "a.ts", "matches": [{"line": 5, "value": "x"}]}]}"#,
        );
        data[key] = value.clone();
        for batch in [false, true] {
            let mut result = json!({"results": [{"index": 0, "data": data}]});
            let mut asked = asked.clone();
            if batch {
                let mut second = result["results"][0].clone();
                second["index"] = 1.into();
                result["results"].as_array_mut().expect("rows").push(second);
                let first = asked["queries"][0].clone();
                asked["queries"]
                    .as_array_mut()
                    .expect("queries")
                    .push(first);
            }
            let notes = render(ToolId::AstSearch, &result, &asked, None)
                .expect("text")
                .notes;
            assert!(
                !notes.contains('{') && !notes.contains("\":"),
                "{key}:\n{notes}"
            );
            assert!(notes.lines().all(|line| line.starts_with("# ")), "{notes}");
            let mut leaves = Vec::new();
            scalars(&value, &mut leaves);
            for leaf in leaves {
                assert!(notes.contains(&leaf), "{key} lost {leaf}:\n{notes}");
            }
        }
    }
}

/// Every string and number of `value`.
fn scalars(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Object(fields) => fields.values().for_each(|value| scalars(value, out)),
        Value::Array(items) => items.iter().for_each(|item| scalars(item, out)),
        Value::String(text) => out.push(text.clone()),
        other => out.push(other.to_string()),
    }
}
