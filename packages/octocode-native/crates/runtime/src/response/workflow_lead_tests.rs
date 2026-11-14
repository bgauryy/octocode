use super::keep_workflow_lead;
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

/// Separate batches of the selected workflow are all needed to reach the
/// evidence promised by the row: each names items (here ids) the row does
/// not state, so each stays; optional tools still fall away.
#[test]
fn every_batch_of_the_selected_workflow_remains_reachable() {
    for tool in ToolId::ALL {
        let Some(target) = tool.workflow_leads().first() else {
            continue;
        };
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
        keep_workflow_lead(tool, &mut output, &Value::Null);
        assert_eq!(
            output["results"][0]["data"]["related"],
            json!(calls),
            "{tool}"
        );
        assert_eq!(output["results"][0]["data"]["next"], next(), "{tool}");
    }
}

/// A preferred workflow may offer several operations over identities the
/// answer already states. Every batch stays, including when the preferred
/// available target is a fallback; carrying a new identity is not required.
#[test]
fn preferred_batches_with_already_stated_identities_all_survive() {
    for tool in ToolId::ALL {
        for target in tool.workflow_leads() {
            let calls = ["src/first.rs", "src/second.rs", "src/third.rs"].map(|path| {
                json!({"kind": "related", "tool": target.as_str(),
                    "input": {"queries": [{"path": path}]}})
            });
            let mut output = response(json!({"index": 0, "data": {
                "files": [{"path": "src/first.rs"}, {"path": "src/second.rs"}, {"path": "src/third.rs"}],
                "related": calls, "next": next()
            }}));
            keep_workflow_lead(tool, &mut output, &Value::Null);
            assert_eq!(
                output["results"][0]["data"]["related"],
                json!(calls),
                "{tool} -> {target}: stated identities do not make later batches redundant"
            );
            assert_eq!(output["results"][0]["data"]["next"], next());
        }
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
            keep_workflow_lead(ToolId::LocalFetch, &mut output, &request);
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
        keep_workflow_lead(ToolId::LocalFetch, &mut output, &Value::Null);
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

/// A complete row keeps one related call, the first that runs the most
/// preferred of its tool's declared workflow targets, or none when its
/// tool declares none; `next` is the same call before and after.
#[test]
fn a_complete_row_keeps_only_its_declared_workflow_lead() {
    for tool in ToolId::ALL {
        let mut output = response(json!({"index": 0, "data": {
            "files": [], "related": every_lead(), "next": next()}}));
        keep_workflow_lead(tool, &mut output, &Value::Null);
        let data = &output["results"][0]["data"];
        assert_eq!(data["next"], next(), "{tool}: next changed");
        let kept: Vec<&str> = data["related"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|call| call["tool"].as_str())
            .collect();
        let declared: Vec<&str> = tool
            .workflow_leads()
            .first()
            .map(|target| target.as_str())
            .into_iter()
            .collect();
        assert_eq!(kept, declared, "{tool}: {data}");
        assert!(
            !declared.is_empty() || data.get("related").is_none(),
            "{tool}: an emptied related[] is absent, not []"
        );
    }
}

/// When the preferred target offers no call, the next declared one is kept;
/// a row offering none of them keeps none.
#[test]
fn the_kept_lead_falls_back_in_declared_order() {
    for tool in ToolId::ALL {
        let targets = tool.workflow_leads();
        let Some((_, rest)) = targets.split_first() else {
            continue;
        };
        let offered: Vec<Value> = every_lead()
            .as_array()
            .into_iter()
            .flatten()
            .filter(|call| call["tool"] != targets[0].as_str())
            .cloned()
            .collect();
        let mut output = response(json!({"index": 0, "data": {"related": offered}}));
        keep_workflow_lead(tool, &mut output, &Value::Null);
        let kept = &output["results"][0]["data"]["related"];
        match rest.first() {
            Some(second) => assert_eq!(kept[0]["tool"], second.as_str(), "{tool}"),
            None => assert!(kept.is_null(), "{tool}: {kept}"),
        }
    }
}

/// A row that did not answer whole keeps every related call (its
/// recovery) and its `next`, for every tool.
#[test]
fn a_row_that_did_not_answer_whole_keeps_every_lead() {
    let unwhole = [
        json!({"status": "error", "data": {"errorCode": "notFound", "error": "x"}}),
        json!({"status": "empty", "data": {}}),
        json!({"status": "outOfRange", "data": {}}),
        json!({"data": {"isPartial": true, "partialReasons": ["x"]}}),
        json!({"data": {"errorCode": "timeout"}}),
        json!({"data": {"selection": {"status": "partial", "unknownItems": 0}}}),
        json!({"data": {"selection": {"status": "complete", "unknownItems": 2}}}),
        json!({"data": {"selection": {"status": "complete", "unknownItems": 0, "relatedItems": 1}}}),
    ];
    for tool in ToolId::ALL {
        for row in &unwhole {
            let mut row = row.clone();
            row["index"] = json!(0);
            row["data"]["related"] = every_lead();
            row["data"]["next"] = next();
            let before = row.clone();
            let mut output = response(row);
            keep_workflow_lead(tool, &mut output, &Value::Null);
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
    keep_workflow_lead(ToolId::LocalFetch, &mut output, &Value::Null);
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
    keep_workflow_lead(ToolId::GhGetHistoryItem, &mut output, &Value::Null);
    let data = &output["results"][0]["data"];
    assert_eq!(
        data["related"],
        json!([
            read("src/a.rs", "head1"),
            read("src/a.rs", "mergebase1"),
            advisory("GHSA-gone")
        ]),
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
        keep_workflow_lead(tool, &mut output, &request);
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
