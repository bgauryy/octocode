use super::retain_guidance;
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

/// [`every_lead`] without the calls `tool`'s description promises
/// (`ToolId::promised_links`), which a row keeps by that promise alone.
fn leads_for(tool: ToolId) -> Value {
    every_lead()
        .as_array()
        .expect("leads")
        .iter()
        .filter(|call| {
            !tool
                .promised_links()
                .iter()
                .any(|to| to.as_str() == call["tool"])
        })
        .cloned()
        .collect()
}

fn next() -> Value {
    json_fixture(
        r#"{
            "nextPage": {"kind": "continue", "tool": "x", "input": {"cursor": "c"}}
        }"#,
    )
}

/// Every tool uses the normalized row outcome, including mixed batches.
/// Success advice is optional; recovery, evidence and continuations are not.
#[test]
fn nonempty_success_has_no_optional_guidance() {
    for tool in ToolId::ALL {
        let success = json!({"index": 0, "data": {
            "content": "1\tα evidence\n2\tβ evidence",
            "hints": ["Read the next workflow tool."],
            "warnings": ["Selected source was redacted."],
            "related": leads_for(tool), "next": next()
        }});
        let empty = json!({"index": 1, "status": "empty", "data": {
            "hints": ["Try an exact spelling."], "related": every_lead()
        }});
        let error = json!({"index": 2, "status": "error", "data": {
            "errorCode": "notFound", "error": "Missing path",
            "hints": ["List its parent."], "related": every_lead()
        }});
        let mut output = json!({"results": [success, empty, error]});
        retain_guidance(tool, &mut output, &Value::Null);
        let data = &output["results"][0]["data"];
        assert!(data.get("hints").is_none(), "{tool}: {data}");
        assert!(data.get("related").is_none(), "{tool}: {data}");
        assert_eq!(data["content"], "1\tα evidence\n2\tβ evidence");
        assert_eq!(
            data["warnings"],
            json_fixture(r#"["Selected source was redacted."]"#)
        );
        assert_eq!(data["next"], next());
        assert_eq!(output["results"][1], empty);
        assert_eq!(output["results"][2], error);
    }
}

/// Separate batches of the selected workflow are all needed to reach the
/// evidence promised by the row: each names items (here ids) the row does
/// not state, so each stays; optional tools still fall away.
#[test]
fn every_batch_of_the_selected_workflow_remains_reachable() {
    for tool in ToolId::ALL {
        let target = ToolId::LocalFetch;
        let calls = ["batch-1/items.rs", "batch-2/items.rs"].map(|id| {
            json!({"kind": "related", "tool": target.as_str(),
                "input": {"queries": [{"id": id}]}})
        });
        let mut related = leads_for(tool).as_array().expect("leads").clone();
        related.retain(|call| call["tool"] != target.as_str());
        related.extend(calls.clone());
        let mut output = response(json!({"data": {
            "ids": ["first", "second"], "related": related, "next": next()
        }}));
        retain_guidance(tool, &mut output, &Value::Null);
        assert_eq!(
            output["results"][0]["data"]["related"],
            json!(calls),
            "{tool}"
        );
        assert_eq!(output["results"][0]["data"]["next"], next(), "{tool}");
    }
}

/// A documented next step can be formed from identities already stated by
/// the answer, so every such optional batch disappears on success.
#[test]
fn batches_with_already_stated_identities_are_optional() {
    for target in ToolId::ALL {
        let calls = ["src/first.rs", "src/second.rs", "src/third.rs"].map(|path| {
            json!({"kind": "related", "tool": target.as_str(),
                "input": {"queries": [{"path": path}]}})
        });
        let mut output = response(json!({"index": 0, "data": {
            "files": [{"path": "src/first.rs"}, {"path": "src/second.rs"}, {"path": "src/third.rs"}],
            "related": calls, "next": next()
        }}));
        retain_guidance(target, &mut output, &Value::Null);
        assert!(
            output["results"][0]["data"].get("related").is_none(),
            "{target}: {output}"
        );
        assert_eq!(output["results"][0]["data"]["next"], next());
    }
}

/// An identity contained in another scalar is not that scalar's identity.
/// A source named only by a non-workflow lead must remain reachable even
/// when the request or an answered row names a path with that prefix.
#[test]
fn substring_collisions_do_not_erase_unstated_lead_identities() {
    for path in ["src/a.rs", "nested/β.ts"] {
        let read = json!({"kind": "related", "tool": "localFetch",
            "input": {"queries": [{"path": path}]}});
        for in_request in [false, true] {
            let longer = format!("{path}.backup");
            let request = if in_request {
                json!({"queries": [{"path": longer}]})
            } else {
                Value::Null
            };
            let mut output = response(json!({"index": 0, "data": {
                "path": if in_request { "other.rs" } else { longer.as_str() },
                "related": [read], "next": next()
            }}));
            retain_guidance(ToolId::LocalSearch, &mut output, &request);
            assert_eq!(
                output["results"][0]["data"]["related"],
                json!([read]),
                "{output}"
            );
            assert_eq!(output["results"][0]["data"]["next"], next());
        }
    }
}

/// A free-form path is an identity even when its spelling is a selector
/// value of another field in the receiving tool's schema. The actual view
/// selector is still not an identity that can retain an optional call.
#[test]
fn enum_spelling_in_a_path_remains_an_identity() {
    for path in ["content", "symbols", "compact"] {
        let read = json!({"kind": "related", "tool": "localFetch",
            "input": {"queries": [{"path": path, "view": "content"}]}});
        let optional = json_fixture(
            r#"{
                "kind": "related",
                "tool": "localFetch",
                "input": {"queries": [{"path": "known.rs", "view": "symbols"}]}
            }"#,
        );
        let mut output = response(json!({"index": 0, "data": {
            "path": "known.rs", "related": [read, optional], "next": next()
        }}));
        retain_guidance(ToolId::LocalSearch, &mut output, &Value::Null);
        assert_eq!(
            output["results"][0]["data"]["related"],
            json!([read]),
            "{output}"
        );
        assert_eq!(output["results"][0]["data"]["next"], next());
    }
}

fn response(row: Value) -> Value {
    json!({"results": [row]})
}

/// An explicit recovery names the tool that runs its promised call. Such a
/// call stays even when its selectors carry no new scalar identity.
#[test]
fn a_note_promised_tool_retains_its_related_calls() {
    for tool in ToolId::ALL {
        for target in ToolId::ALL {
            for channel in ["warnings", "hints", "notices", "diagnostics"] {
                let recovery = format!("Run the `{target}` request to inspect the alternatives.");
                let mut data = json!({"files": [], "related": leads_for(tool)});
                data[channel] = if channel == "diagnostics" {
                    json!([{"message": "A different spelling exists.", "recovery": recovery}])
                } else {
                    json!([recovery])
                };
                let mut output = response(json!({"index": 0, "data": data}));
                retain_guidance(tool, &mut output, &Value::Null);
                let expected = leads_for(tool)
                    .as_array()
                    .expect("leads")
                    .iter()
                    .filter(|call| channel != "hints" && call["tool"] == target.as_str())
                    .cloned()
                    .collect::<Vec<_>>();
                assert_eq!(
                    output["results"][0]["data"]["related"]
                        .as_array()
                        .cloned()
                        .unwrap_or_default(),
                    expected,
                    "{tool} -> {target}, {channel}: the note's promised call stays"
                );
            }
        }
    }
}

/// Source text, an explanatory diagnostic, and an identifier containing a
/// tool name do not promise a recovery call to that tool.
#[test]
fn source_and_tool_name_substrings_do_not_retain_optional_calls() {
    for tool in ToolId::ALL {
        for target in ToolId::ALL {
            let prose = format!("Run the related {target} request.");
            for spelling in [
                format!("prefix_{target}_suffix"),
                format!("x{target}x"),
                format!("α{target}β"),
                format!("${target}"),
                format!("{target}$"),
                format!("{target}\u{301}"),
                format!("src/{target}.rs"),
            ] {
                let mut output = response(json!({"index": 0, "data": {
                    "content": prose,
                    "files": [{"value": prose}],
                    "diagnostics": [{"message": prose, "recovery": format!("Use {spelling}.")}],
                    "warnings": [format!("Use {spelling}.")],
                    "hints": [format!("Use {spelling}.")],
                    "notices": [format!("Use {spelling}.")],
                    "related": leads_for(tool)
                }}));
                retain_guidance(tool, &mut output, &Value::Null);
                let related = output["results"][0]["data"]["related"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                assert!(related.is_empty(), "{tool} -> {target}: {output}");
            }
        }
    }
}

/// No per-tool workflow preference permits advisory related calls on a
/// successful result. Its required next channels are unchanged.
#[test]
fn a_complete_row_has_no_optional_related_calls() {
    let mut output = response(json!({"index": 0, "data": {
        "files": [], "related": every_lead(), "next": next()}}));
    retain_guidance(ToolId::LocalSearch, &mut output, &Value::Null);
    let data = &output["results"][0]["data"];
    assert_eq!(data["next"], next());
    assert!(data.get("related").is_none(), "{data}");
}

/// A row that did not answer whole keeps every related call (its
/// recovery) and its `next`, for every tool.
#[test]
fn a_row_that_did_not_answer_whole_keeps_every_lead() {
    let unwhole = [
        json_fixture(
            r#"{
                "status": "error",
                "data": {"errorCode": "notFound", "error": "x"}
            }"#,
        ),
        json_fixture(r#"{"status": "empty", "data": {}}"#),
        json_fixture(r#"{"status": "outOfRange", "data": {}}"#),
        json_fixture(r#"{"data": {"errorCode": "timeout"}}"#),
    ];
    for tool in ToolId::ALL {
        for row in &unwhole {
            let mut row = row.clone();
            row["index"] = json!(0);
            row["data"]["related"] = every_lead();
            row["data"]["next"] = next();
            let before = row.clone();
            let mut output = response(row);
            retain_guidance(tool, &mut output, &Value::Null);
            assert_eq!(output["results"][0], before, "{tool}");
        }
    }
}

/// A complete classified row (nothing left unknown or unshown) is complete.
#[test]
fn a_complete_selection_is_a_complete_row() {
    let mut output = response(json!({"index": 0, "data": {
        "selection": {"status": "complete", "unknownItems": 0, "excludedItems": 3},
        "related": every_lead()}}));
    retain_guidance(ToolId::LocalSearch, &mut output, &Value::Null);
    assert!(output["results"][0]["data"].get("related").is_none());
}

/// No fact lives only in a dropped lead: beside the workflow lead, a
/// complete row keeps a call carrying an identity it states nowhere (the
/// old side of a diff at its merge base) and one its warning names (the read
/// the warning points to); a call whose identities the row states, or that
/// only selects (lists, enumerated values), is dropped.
#[test]
fn a_call_carrying_an_unstated_fact_is_part_of_the_answer() {
    let read = |path: &str, at: &str| {
        json!({"kind": "related", "tool": "ghGetFileContent",
            "input": {"queries": [{"path": path, "ref": at}]}})
    };
    let advisory = |id: &str| {
        json!({"kind": "related", "tool": "securityCheck",
            "input": {"queries": [{"operation": "advisory", "id": id}]}})
    };
    let sections = json_fixture(
        r#"{
            "kind": "related",
            "tool": "ghGetHistoryItem",
            "input": {
                "queries": [{"operation": "pullRequest", "sections": ["comments"]}]
            }
        }"#,
    );
    let mut data = json!({
        "files": [{"path": "src/a.rs"}], "headCommitSha": "head1",
        "advisories": [{"id": "GHSA-kept"}],
        "warnings": ["GitHub withdrew GHSA-gone at 2021"],
        "next": next()
    });
    for call in [
        read("src/a.rs", "head1"),
        read("src/a.rs", "mergebase1"),
        read("src/a.rs", "head1"),
        advisory("GHSA-gone"),
        advisory("GHSA-kept"),
        sections,
    ] {
        crate::response::result::attach_related(&mut data, call);
    }
    let mut output = response(json!({"index": 0, "data": data}));
    retain_guidance(ToolId::LocalSearch, &mut output, &Value::Null);
    let data = &output["results"][0]["data"];
    assert_eq!(
        data["related"],
        json!([read("src/a.rs", "mergebase1"), advisory("GHSA-gone")]),
        "{data}"
    );
    assert_eq!(data["next"], next());
}

/// The stage's two `related[]` rules together (a call another row answers,
/// then the workflow lead) drop no fact the response holds nowhere else:
/// every identity of a dropped call is stated by a row or the request, and
/// a call naming an item found nowhere else stays, for every tool.
#[test]
fn the_two_lead_rules_together_drop_no_fact_found_nowhere_else() {
    fn holds(value: &Value, identity: &str) -> bool {
        match value {
            Value::String(text) => text.contains(identity),
            Value::Number(number) => number.to_string() == identity,
            Value::Array(items) => items.iter().any(|item| holds(item, identity)),
            Value::Object(map) => map
                .iter()
                .any(|(key, item)| key != "related" && holds(item, identity)),
            _ => false,
        }
    }
    for tool in ToolId::ALL {
        let asked = json_fixture(r#"{"path": "src/asked-elsewhere.rs"}"#);
        let call = |query: Value| json!({"kind": "related", "tool": tool.as_str(), "input": {"queries": [query]}});
        let request = json!({"queries": [asked, {"path": "src/own.rs"}]});
        let only_in_lead = call(json_fixture(r#"{"path": "src/only-in-lead.rs"}"#));
        let mut related = every_lead().as_array().expect("leads").clone();
        related.extend([
            call(asked.clone()),
            call(json_fixture(r#"{"path": "src/own.rs"}"#)),
            only_in_lead.clone(),
        ]);
        let mut output = json!({"results": [
            {"index": 0, "data": {"files": [{"path": "src/asked-elsewhere.rs"}]}},
            {"index": 1, "data": {"files": [{"path": "src/own.rs"}], "related": related.clone()}},
        ]});
        super::super::answered_leads::drop_answered_leads(tool, &mut output, &request);
        retain_guidance(tool, &mut output, &request);
        let kept = output["results"][1]["data"]["related"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        assert!(kept.contains(&only_in_lead), "{tool}: {output}");
        for dropped in related.iter().filter(|call| !kept.contains(call)) {
            for identity in super::carried_identities(dropped) {
                assert!(
                    holds(&output, &identity) || holds(&request, &identity),
                    "{tool}: `{identity}` of {dropped} is found nowhere else"
                );
            }
        }
    }
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
        output["responsePagination"] = json!({"hasMore": true, "next": next["read"]});
        let response_page = output["responsePagination"].clone();
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
        assert_eq!(output["responsePagination"], response_page);
        assert!(data.get("hints").is_none());
        assert!(data.get("related").is_none());
    }
}

/// Nested domain values can themselves contain words such as hints and
/// related. The policy only owns the row's guidance, never source evidence.
#[test]
fn nested_source_values_and_error_recovery_are_not_advice() {
    let evidence = json_fixture(
        r#"{
            "hints": ["source fact"],
            "related": ["source value"],
            "errorCode": "quoted source error",
            "warnings": ["source warning"]
        }"#,
    );
    let mut output = response(json!({"data": {"content": evidence,
        "hints": ["Explore another tool."], "related": every_lead()}}));
    retain_guidance(ToolId::LocalSearch, &mut output, &Value::Null);
    assert_eq!(output["results"][0]["data"]["content"], evidence);
    assert!(output["results"][0]["data"].get("hints").is_none());
    assert!(output["results"][0]["data"].get("related").is_none());
}

/// A call to the row's own tool that asks one asked item for other parts
/// (only its lists differ or are added, such as sections or includes) is
/// the way to the rest of this answer: a complete row keeps it. A call that
/// changes another field (the item, the operation, a view), the asked query
/// itself, or the same row sent to another tool stays optional. A call for
/// another number is another item, so it stays as a fact the row lacks.
#[test]
fn a_call_reading_more_of_an_asked_item_stays() {
    let asked = json_fixture(
        r#"{
            "operation": "pullRequest",
            "owner": "o",
            "repo": "r",
            "number": 7,
            "mainGoal": "why it changed"
        }"#,
    );
    let request = json!({"queries": [asked]});
    let call = |tool: &str, row: Value| json!({"kind": "related", "tool": tool, "input": {"queries": [row]}});
    let pr = |number: u32, sections: Option<Value>| {
        let mut row =
            json!({"operation": "pullRequest", "owner": "o", "repo": "r", "number": number});
        if let Some(sections) = sections {
            row["sections"] = sections;
        }
        row
    };
    let more_files = call(
        "ghGetHistoryItem",
        pr(7, Some(json_fixture(r#"["files", "body"]"#))),
    );
    let more_discussion = call(
        "ghGetHistoryItem",
        pr(7, Some(json_fixture(r#"["comments", "reviews"]"#))),
    );
    // Another number names another item: a fact the row lacks, so it stays.
    let other_item = call(
        "ghGetHistoryItem",
        pr(8, Some(json_fixture(r#"["files"]"#))),
    );
    let dropped = [
        call(
            "ghGetHistoryItem",
            json_fixture(
                r#"{
                    "operation": "issue",
                    "owner": "o",
                    "repo": "r",
                    "number": 7
                }"#,
            ),
        ),
        call("ghGetHistoryItem", pr(7, None)),
        call("ghSearchHistory", pr(7, Some(json_fixture(r#"["files"]"#)))),
    ];
    let mut related = vec![
        more_files.clone(),
        more_discussion.clone(),
        other_item.clone(),
    ];
    related.extend(dropped);
    let mut output = response(json!({"index": 0, "data": {
        "pullRequests": [{"number": 7, "title": "t"}], "related": related
    }}));
    retain_guidance(ToolId::GhGetHistoryItem, &mut output, &request);
    assert_eq!(
        output["results"][0]["data"]["related"],
        json!([more_files, more_discussion, other_item]),
        "{output}"
    );
}

/// A list the contract does not enumerate (search keywords, match strings,
/// globs) is not a choice among the tool's parts: a call that changes only
/// such a list asks another question and is no way to the rest of the
/// answer. Its lists ride along with a changed part selector (`sections`).
#[test]
fn only_a_changed_part_selector_reads_more_of_an_asked_item() {
    let search = |keywords: Value| {
        json!({"kind": "related", "tool": "ghSearchCode",
            "input": {"queries": [{"owner": "o", "repo": "r", "keywords": keywords}]}})
    };
    let request = json_fixture(
        r#"{
            "queries": [{"owner": "o", "repo": "r", "keywords": ["a"]}]
        }"#,
    );
    let mut output = response(json!({"index": 0, "data": {
        "files": [{"path": "p"}],
        "related": [search(json_fixture(r#"["b"]"#)), search(json_fixture(r#"["a", "b"]"#))]
    }}));
    retain_guidance(ToolId::GhSearchCode, &mut output, &request);
    assert!(
        output["results"][0]["data"].get("related").is_none(),
        "{output}"
    );

    let asked = json_fixture(
        r#"{
            "operation": "pullRequest",
            "owner": "o",
            "repo": "r",
            "number": 7
        }"#,
    );
    let request = json!({"queries": [asked]});
    let patches = json_fixture(
        r#"{
            "kind": "related",
            "tool": "ghGetHistoryItem",
            "input": {
                "queries": [
                    {
                        "operation": "pullRequest",
                        "owner": "o",
                        "repo": "r",
                        "number": 7,
                        "sections": ["patches"],
                        "include": ["src/a.ts"]
                    }
                ]
            }
        }"#,
    );
    let include_only = json_fixture(
        r#"{
            "kind": "related",
            "tool": "ghGetHistoryItem",
            "input": {
                "queries": [
                    {
                        "operation": "pullRequest",
                        "owner": "o",
                        "repo": "r",
                        "number": 7,
                        "include": ["src/a.ts"]
                    }
                ]
            }
        }"#,
    );
    let another_item = json_fixture(
        r#"{
            "kind": "related",
            "tool": "ghGetHistoryItem",
            "input": {
                "queries": [
                    {
                        "operation": "pullRequest",
                        "owner": "o",
                        "repo": "r",
                        "number": 7,
                        "sections": ["files"]
                    },
                    {
                        "operation": "pullRequest",
                        "owner": "o",
                        "repo": "r",
                        "number": 8,
                        "sections": ["files"]
                    }
                ]
            }
        }"#,
    );
    let mut output = response(json!({"index": 0, "data": {
        "pullRequests": [{"number": 7}],
        "related": [patches.clone(), include_only, another_item]
    }}));
    retain_guidance(ToolId::GhGetHistoryItem, &mut output, &request);
    assert_eq!(
        output["results"][0]["data"]["related"],
        json!([patches, another_item]),
        "{output}"
    );
}

fn related_call(tool: &str, query: Value) -> Value {
    json!({"kind": "related", "tool": tool, "input": {"queries": [query]}})
}

/// A note that sends the reader to `related` promises that row's own calls,
/// whatever facts those calls repeat: the warning's advisory id is not in
/// the call, which carries only the package the request states.
#[test]
fn a_note_naming_related_keeps_that_rows_calls() {
    let package = json_fixture(
        r#"{
            "operation": "package",
            "ecosystem": "npm",
            "packageName": "axios",
            "version": "1.6.0"
        }"#,
    );
    let details = related_call(
        "securityCheck",
        json_fixture(
            r#"{
                "operation": "package",
                "ecosystem": "npm",
                "packageName": "axios",
                "version": "1.6.0",
                "view": "details"
            }"#,
        ),
    );
    let request = json!({"queries": [package]});
    for (channel, kept) in [
        ("warnings", true),
        ("notices", true),
        ("hints", false),
        ("content", false),
    ] {
        let note = "GitHub withdrew GHSA-xxxx-yyyy-zzzz: not a match; related reads it whole.";
        let mut data = json!({"answer": "none", "related": [details]});
        data[channel] = if channel == "content" {
            json!(note)
        } else {
            json!([note])
        };
        let mut output = response(json!({"index": 0, "data": data}));
        retain_guidance(ToolId::SecurityCheck, &mut output, &request);
        let related = &output["results"][0]["data"]["related"];
        assert_eq!(
            related.as_array().is_some_and(|calls| !calls.is_empty()),
            kept,
            "{channel}: {output}"
        );
    }
    // Only a standalone word names `related`.
    let mut output = response(json!({"index": 0, "data": {
        "answer": "none", "warnings": ["unrelated or relatedness are not it"], "related": [details]}}));
    retain_guidance(ToolId::SecurityCheck, &mut output, &request);
    assert!(output["results"][0]["data"].get("related").is_none());
}

/// The artifact row's securityCheck call is promised by the producing tool's
/// own description; a cross-tool call nothing promised stays optional.
#[test]
fn a_promised_cross_tool_call_stays_and_an_unpromised_one_goes() {
    let artifact = json_fixture(
        r#"{
            "packageName": "axios",
            "version": "1.6.0",
            "ecosystem": "npm"
        }"#,
    );
    let check = related_call(
        "securityCheck",
        json_fixture(
            r#"{
                "operation": "package",
                "ecosystem": "npm",
                "packageName": "axios",
                "version": "1.6.0"
            }"#,
        ),
    );
    let mut output =
        response(json!({"index": 0, "data": {"artifacts": [artifact], "related": [check]}}));
    retain_guidance(ToolId::ArtifactSearch, &mut output, &Value::Null);
    assert_eq!(
        output["results"][0]["data"]["related"],
        json!([check]),
        "{output}"
    );

    let lsp = related_call(
        "lspSearch",
        json_fixture(r#"{"path": "src/a.rs", "symbolName": "a", "line": 1}"#),
    );
    let mut output = response(
        json!({"index": 0, "data": {"files": [{"path": "src/a.rs", "symbolName": "a"}], "related": [lsp]}}),
    );
    retain_guidance(ToolId::LocalSearch, &mut output, &Value::Null);
    assert!(
        output["results"][0]["data"].get("related").is_none(),
        "{output}"
    );
}

/// A pull request number is an identity: the commit that names "(#874)"
/// states it only inside prose, so the read of that pull request stays, and
/// a row that states the number itself drops it.
#[test]
fn a_number_is_an_identity_only_in_its_identity_field() {
    let read = related_call(
        "ghGetHistoryItem",
        json_fixture(
            r#"{
                "operation": "pullRequest",
                "owner": "o",
                "repo": "r",
                "number": 874
            }"#,
        ),
    );
    let request = json_fixture(
        r#"{
            "queries": [
                {
                    "operation": "commit",
                    "owner": "o",
                    "repo": "r",
                    "sha": "abc"
                }
            ]
        }"#,
    );
    let mut output = response(json!({"index": 0, "data": {
        "message": "Fix (#874)", "related": [read]}}));
    retain_guidance(ToolId::GhGetHistoryItem, &mut output, &request);
    assert_eq!(
        output["results"][0]["data"]["related"],
        json!([read]),
        "{output}"
    );

    let mut output = response(json!({"index": 0, "data": {
        "number": 874, "message": "Fix (#874)", "related": [read]}}));
    retain_guidance(ToolId::GhGetHistoryItem, &mut output, &request);
    assert!(
        output["results"][0]["data"].get("related").is_none(),
        "{output}"
    );

    // A selector number (a line) is no identity: a read differing only in it is optional.
    let fetch =
        |line: u32| related_call("localFetch", json!({"path": "src/a.rs", "startLine": line}));
    let mut output = response(json!({"index": 0, "data": {
        "path": "src/a.rs", "related": [fetch(10)]}}));
    retain_guidance(ToolId::LocalSearch, &mut output, &Value::Null);
    assert!(
        output["results"][0]["data"].get("related").is_none(),
        "{output}"
    );
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
        assert_ne!(from, to, "a promised link crosses tools");
    }
}

fn names_word(text: &str, word: &str) -> bool {
    text.split(|c: char| !c.is_alphanumeric())
        .any(|w| w == word)
}

/// A number is an identity only in an `IDENTITY_NUMBERS` field: every
/// numeric property any tool's query schema declares is classified, so a
/// new one cannot silently become (or fail to be) an identity.
#[test]
fn every_numeric_query_property_is_classified() {
    /// Numbers that select a part of an answer (a line, a page, a limit).
    const SELECTOR_NUMBERS: &[&str] = &[
        "after",
        "before",
        "candidatePage",
        "column",
        "depth",
        "endLine",
        "historyDepth",
        "importerPage",
        "keep",
        "limit",
        "line",
        "matched",
        "materializeOffset",
        "max",
        "maxDepth",
        "min",
        "minChanges",
        "minDepth",
        "minScore",
        "nthChild",
        "occurrence",
        "offset",
        "page",
        "pageSize",
        "position",
        "rowOffset",
        "scanOffset",
        "startLine",
        "walkOffset",
    ];
    fn walk(
        value: &Value,
        shared: &Value,
        field: &str,
        out: &mut Vec<String>,
        seen: &mut Vec<String>,
    ) {
        match value {
            Value::Object(map) => {
                if matches!(
                    map.get("type").and_then(Value::as_str),
                    Some("integer" | "number")
                ) {
                    out.push(field.to_owned());
                }
                for (key, item) in map {
                    match (key.as_str(), item) {
                        ("properties", Value::Object(properties)) => {
                            for (name, schema) in properties {
                                walk(schema, shared, name, out, seen);
                            }
                        }
                        ("$ref", Value::String(reference)) => {
                            if let Some(name) = reference.strip_prefix("#/$defs/")
                                && !seen.contains(&format!("{name}@{field}"))
                            {
                                seen.push(format!("{name}@{field}"));
                                walk(&shared[name], shared, field, out, seen);
                            }
                        }
                        _ => walk(item, shared, field, out, seen),
                    }
                }
            }
            Value::Array(items) => items
                .iter()
                .for_each(|item| walk(item, shared, field, out, seen)),
            _ => {}
        }
    }
    let whole = crate::contracts::parsed_contract().expect("contract");
    let mut unclassified = Vec::new();
    for tool in ToolId::ALL {
        let contract = crate::contracts::tool_contract(tool).expect("tool contract");
        let mut numeric = Vec::new();
        walk(
            &contract["querySchema"],
            &whole["$defs"],
            "",
            &mut numeric,
            &mut Vec::new(),
        );
        for field in numeric {
            if !super::IDENTITY_NUMBERS.contains(&field.as_str())
                && !SELECTOR_NUMBERS.contains(&field.as_str())
            {
                unclassified.push(format!("{tool}.{field}"));
            }
        }
    }
    assert!(
        unclassified.is_empty(),
        "classify these numeric fields: {unclassified:?}"
    );
}

/// A short identity is named only as a whole segment: the owner `o` is not
/// in "no body holds it", and a path or a number still is where it stands.
#[test]
fn an_identity_is_mentioned_as_a_whole_segment_only() {
    use super::mentions;
    assert!(!mentions("no body holds it", "o"));
    assert!(!mentions("src/lib.rs.backup", "lib.rs"));
    assert!(!mentions("my-repo", "repo"));
    assert!(mentions("owned by o, see r", "o"));
    assert!(mentions("it is held by o.", "o"));
    assert!(mentions("src/lib.rs", "lib.rs"));
    assert!(mentions("read `a/b` next", "a/b"));
    assert!(mentions("Fix (#874)", "874"));
    assert!(!mentions("Fix (#1874)", "874"));
    assert!(!mentions("anything", ""));
}

/// A row outside the workspace names its paths from its stated `root`, and
/// its calls name the same files absolute: a call that only reads a file the
/// row names states nothing new and goes, as it does inside the workspace.
#[test]
fn a_call_naming_a_file_the_row_names_from_its_root_states_nothing_new() {
    let root = "/clones/o/r/main";
    let same = json!({"kind": "related", "tool": "localFetch",
        "input": {"queries": [{"path": format!("{root}/src/a.ts"), "view": "symbols"}]}});
    let other = json!({"kind": "related", "tool": "localFetch",
        "input": {"queries": [{"path": format!("{root}/src/b.ts"), "view": "symbols"}]}});
    let mut output = response(json!({"index": 0, "data": {
        "root": root, "files": [{"path": "src/a.ts"}], "related": [same, other]
    }}));
    retain_guidance(ToolId::LocalSearch, &mut output, &Value::Null);
    assert_eq!(
        output["results"][0]["data"]["related"],
        json!([other]),
        "{output}"
    );
}

/// A note whose recovery the producing tool's output reference states once,
/// by its code, carries no `recovery` of its own: astSearch's
/// `structural.pattern.qualified` beside matches promises the related rule
/// with `any`, so a complete row keeps that call though no note names its
/// tool. Without such a note, the same call repeats facts the row states
/// and goes.
#[test]
fn a_code_whose_reference_states_its_recovery_keeps_its_lead() {
    let lead = related_call(
        "astSearch",
        json!({"path": "src/calls.ts", "operation": "match", "rule": {"any": [
            {"pattern": "runTask($$$A)"}, {"pattern": "$QUALIFIER.runTask($$$A)"}]}}),
    );
    let request = json!({"queries": [
        {"path": "src/calls.ts", "operation": "match", "rule": {"pattern": "runTask($$$A)"}}]});
    let row = |code: &str| {
        response(json!({"index": 0, "data": {
            "matchCount": 2,
            "files": [{"path": "src/calls.ts", "matches": [
                {"line": 5, "value": "runTask(\"inside\")"},
                {"line": 9, "value": "runTask(\"outside\")"}]}],
            "diagnostics": [{"code": code, "severity": "info", "stage": "match",
                "message": "1 more match(es) in 1 file(s) spell a path or receiver before `runTask` (first: line 7)."}],
            "related": [lead.clone()]
        }}))
    };
    let mut promised = row("structural.pattern.qualified");
    retain_guidance(ToolId::AstSearch, &mut promised, &request);
    assert_eq!(
        promised["results"][0]["data"]["related"],
        json!([lead]),
        "{promised}"
    );
    let mut unpromised = row("structural.prefilter.skipped");
    retain_guidance(ToolId::AstSearch, &mut unpromised, &request);
    assert!(
        unpromised["results"][0]["data"].get("related").is_none(),
        "{unpromised}"
    );
}

/// Each code whose recovery a tool's output reference states once, as the
/// row's related call (`ToolId::codes_promising_related`), is named by that
/// reference's `diagnostics` field, which sends the reader to `related`.
#[test]
fn every_code_promising_a_lead_is_named_by_the_output_reference() {
    let presentation = crate::contracts::cli_presentation().expect("presentation");
    for tool in ToolId::ALL {
        let codes = tool.codes_promising_related();
        if codes.is_empty() {
            continue;
        }
        let reference = presentation["tools"]
            .as_array()
            .and_then(|tools| tools.iter().find(|entry| entry["name"] == tool.as_str()))
            .and_then(|entry| entry["output"]["diagnostics"].as_str())
            .unwrap_or_else(|| panic!("{tool}: a diagnostics reference"));
        assert!(names_word(reference, "related"), "{tool}: {reference}");
        for code in codes {
            assert!(reference.contains(code), "{tool}: {code} in {reference}");
        }
    }
}
