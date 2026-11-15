use super::retain_guidance;
use crate::id::ToolId;
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
    json!({"nextPage": {"kind": "continue", "tool": "x", "input": {"cursor": "c"}}})
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
            "related": every_lead(), "next": next()
        }});
        let empty = json!({"index": 1, "status": "empty", "data": {
            "hints": ["Try an exact spelling."], "related": every_lead()
        }});
        let error = json!({"index": 2, "status": "error", "data": {
            "errorCode": "notFound", "error": "Missing path",
            "hints": ["List its parent."], "related": every_lead()
        }});
        let mut output = json!({"results": [success, empty, error]});
        retain_guidance(&mut output, &Value::Null);
        let data = &output["results"][0]["data"];
        assert!(data.get("hints").is_none(), "{tool}: {data}");
        assert!(data.get("related").is_none(), "{tool}: {data}");
        assert_eq!(data["content"], "1\tα evidence\n2\tβ evidence");
        assert_eq!(data["warnings"], json!(["Selected source was redacted."]));
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
        let mut related = every_lead().as_array().expect("leads").clone();
        related.retain(|call| call["tool"] != target.as_str());
        related.extend(calls.clone());
        let mut output = response(json!({"data": {
            "ids": ["first", "second"], "related": related, "next": next()
        }}));
        retain_guidance(&mut output, &Value::Null);
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
        retain_guidance(&mut output, &Value::Null);
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
            retain_guidance(&mut output, &request);
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
        let optional = json!({"kind": "related", "tool": "localFetch",
            "input": {"queries": [{"path": "known.rs", "view": "symbols"}]}});
        let mut output = response(json!({"index": 0, "data": {
            "path": "known.rs", "related": [read, optional], "next": next()
        }}));
        retain_guidance(&mut output, &Value::Null);
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
                let recovery =
                    format!("Run the related `{target}` request to inspect the alternatives.");
                let mut data = json!({"files": [], "related": every_lead()});
                data[channel] = if channel == "diagnostics" {
                    json!([{"message": "A different spelling exists.", "recovery": recovery}])
                } else {
                    json!([recovery])
                };
                let mut output = response(json!({"index": 0, "data": data}));
                retain_guidance(&mut output, &Value::Null);
                let expected = every_lead()
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
                    "related": every_lead()
                }}));
                retain_guidance(&mut output, &Value::Null);
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
    retain_guidance(&mut output, &Value::Null);
    let data = &output["results"][0]["data"];
    assert_eq!(data["next"], next());
    assert!(data.get("related").is_none(), "{data}");
}

/// A row that did not answer whole keeps every related call (its
/// recovery) and its `next`, for every tool.
#[test]
fn a_row_that_did_not_answer_whole_keeps_every_lead() {
    let unwhole = [
        json!({"status": "error", "data": {"errorCode": "notFound", "error": "x"}}),
        json!({"status": "empty", "data": {}}),
        json!({"status": "outOfRange", "data": {}}),
        json!({"data": {"errorCode": "timeout"}}),
    ];
    for tool in ToolId::ALL {
        for row in &unwhole {
            let mut row = row.clone();
            row["index"] = json!(0);
            row["data"]["related"] = every_lead();
            row["data"]["next"] = next();
            let before = row.clone();
            let mut output = response(row);
            retain_guidance(&mut output, &Value::Null);
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
    retain_guidance(&mut output, &Value::Null);
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
    let sections = json!({"kind": "related", "tool": "ghGetHistoryItem",
        "input": {"queries": [{"operation": "pullRequest", "sections": ["comments"]}]}});
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
    retain_guidance(&mut output, &Value::Null);
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
            Value::Array(items) => items.iter().any(|item| holds(item, identity)),
            Value::Object(map) => map
                .iter()
                .any(|(key, item)| key != "related" && holds(item, identity)),
            _ => false,
        }
    }
    for tool in ToolId::ALL {
        let asked = json!({"path": "src/asked-elsewhere.rs"});
        let call = |query: Value| json!({"kind": "related", "tool": tool.as_str(), "input": {"queries": [query]}});
        let request = json!({"queries": [asked, {"path": "src/own.rs"}]});
        let only_in_lead = call(json!({"path": "src/only-in-lead.rs"}));
        let mut related = every_lead().as_array().expect("leads").clone();
        related.extend([
            call(asked.clone()),
            call(json!({"path": "src/own.rs"})),
            only_in_lead.clone(),
        ]);
        let mut output = json!({"results": [
            {"index": 0, "data": {"files": [{"path": "src/asked-elsewhere.rs"}]}},
            {"index": 1, "data": {"files": [{"path": "src/own.rs"}], "related": related.clone()}},
        ]});
        super::super::answered_leads::drop_answered_leads(tool, &mut output, &request);
        retain_guidance(&mut output, &request);
        let kept = output["results"][1]["data"]["related"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        assert!(kept.contains(&only_in_lead), "{tool}: {output}");
        for dropped in related.iter().filter(|call| !kept.contains(call)) {
            for identity in super::carried_identities(dropped) {
                assert!(
                    holds(&output, identity) || holds(&request, identity),
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
        json!({"isPartial": true, "partialReasons": ["providerIncompleteResults"]}),
        json!({"selection": {"status": "partial", "unknownItems": 2, "relatedItems": 1}}),
    ] {
        let mut data = coverage.clone();
        data["answers"] = json!({"relevant": {"noul": 0.9}});
        data["source"] = json!({"tool": "localFetch", "query": {"path": "α.rs"}});
        data["hints"] = json!(["Explore another tool."]);
        data["warnings"] = json!(["Provider did not cover every candidate."]);
        data["related"] = every_lead();
        data["next"] = json!({
            "read": {"kind": "continue", "tool": "localFetch", "input": {"cursor": "source"}},
            "unfilteredUnknown": {"kind": "continue", "tool": "localSearch", "input": {"cursor": "unknown"}},
        });
        let next = data["next"].clone();
        let mut output = response(json!({"data": data}));
        output["responsePagination"] = json!({"hasMore": true, "next": next["read"]});
        let response_page = output["responsePagination"].clone();
        retain_guidance(&mut output, &Value::Null);
        let data = &output["results"][0]["data"];
        for (key, value) in coverage.as_object().expect("coverage") {
            assert_eq!(&data[key], value);
        }
        assert_eq!(data["answers"], json!({"relevant": {"noul": 0.9}}));
        assert_eq!(
            data["source"],
            json!({"tool": "localFetch", "query": {"path": "α.rs"}})
        );
        assert_eq!(
            data["warnings"],
            json!(["Provider did not cover every candidate."])
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
    let evidence = json!({"hints": ["source fact"], "related": ["source value"],
        "errorCode": "quoted source error", "warnings": ["source warning"]});
    let mut output = response(json!({"data": {"content": evidence,
        "hints": ["Explore another tool."], "related": every_lead()}}));
    retain_guidance(&mut output, &Value::Null);
    assert_eq!(output["results"][0]["data"]["content"], evidence);
    assert!(output["results"][0]["data"].get("hints").is_none());
    assert!(output["results"][0]["data"].get("related").is_none());
}
