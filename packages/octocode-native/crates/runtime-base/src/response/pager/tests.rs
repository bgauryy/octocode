use super::*;
use octocode_test_support::json_fixture;
use serde_json::json;

mod one_page;
mod terminal;
mod volatile;

fn walk_tool(tool: &str, input: &Value, budget: usize) -> Vec<Value> {
    let mut options = ResponsePageOptions {
        response_chars: Some(budget),
        paging: true,
        ..Default::default()
    };
    let mut pages = Vec::new();
    loop {
        let prepared = prepare(
            ResponseInput {
                tool: tool.parse().expect("tool name"),
                structured: input.clone(),
                options: options.clone(),
            },
            &Cancellation::new(),
        )
        .expect("page");
        let page = prepared.value;
        let chars = page.to_string().encode_utf16().count();
        // Exactly the pages holding a value longer than the budget alone
        // say so, at the top level, whether or not the page carries a
        // block; any other page over the budget is over by its fixed parts.
        let flagged = page.get("oversized") == Some(&json!(true));
        assert_eq!(
            flagged,
            oversized::holds_value_over(
                page.as_object().expect("envelope"),
                tool.parse().expect("tool name"),
                budget
            ),
            "{chars} chars at {budget}: {page}"
        );
        assert!(
            !flagged || chars > budget,
            "{chars} chars at {budget}: {page}"
        );
        assert_ne!(page.get("oversized"), Some(&json!(false)), "{page}");
        if let Some(pagination) = page.get("responsePagination") {
            for removed in [
                "oversized",
                "returnedChars",
                "totalChars",
                "scope",
                "snapshot",
                "offset",
                "length",
                "currentPage",
                "expectedSnapshot",
                "storage",
                "restart",
            ] {
                assert!(pagination.get(removed).is_none(), "{removed}: {pagination}");
            }
        }
        let next = prepared.next;
        pages.push(page);
        let Some(next) = next else {
            break;
        };
        options.open(&next);
        assert!(pages.len() < 1000, "finite page walk");
    }
    pages
}

#[test]
fn every_nested_evidence_item_is_reachable_exactly_once_at_each_budget() {
    let values=(0..80).map(|index| json!({"id":index,"evidence":format!("token-{index} {}", "界😀\n\"".repeat(index%9+1))})).collect::<Vec<_>>();
    let input = json!({"results":[{"index":0,"data":{"files":values}}]});
    for budget in [350, 700, 1700, 4000, 20000] {
        let pages = walk_tool("localSearch", &input, budget);
        let collected = pages
            .iter()
            .flat_map(|page| page["results"].as_array().expect("rows"))
            .flat_map(|row| row["data"]["files"].as_array().into_iter().flatten())
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(collected, values, "budget {budget}");
    }
}

#[test]
fn split_rows_publish_required_continuation_after_all_evidence_and_related_once() {
    let call = json_fixture(
        r#"{
            "kind": "continue",
            "tool": "localSearch",
            "input": {"cursor": "issued-evidence"}
        }"#,
    );
    let related = json_fixture(
        r#"{
            "kind": "related",
            "tool": "localFetch",
            "input": {"queries": [{"path": "a"}]}
        }"#,
    );
    let input = json!({"results":[{"index":0,"data":{"files":(0..25).map(|i|json!({"id":i,"content":"x".repeat(70)})).collect::<Vec<_>>(),"next":{"anything":call},"related":[related],"hints":["full prose guidance"]}}]});
    let pages = walk_tool("localSearch", &input, 700);
    let rows = pages
        .iter()
        .flat_map(|page| page["results"].as_array().expect("rows"))
        .collect::<Vec<_>>();
    assert!(rows.len() > 1);
    assert!(
        rows[..rows.len() - 1]
            .iter()
            .all(|row| row["data"].get("next").is_none())
    );
    assert_eq!(rows.last().expect("last")["data"]["next"]["anything"], call);
    assert_eq!(
        rows.iter()
            .filter(|row| row["data"].get("related").is_some())
            .count(),
        1
    );
    assert_eq!(
        rows.iter()
            .filter(|row| row["data"].get("hints").is_some())
            .count(),
        1
    );
}

#[test]
fn unpaged_output_is_unchanged_and_cancellation_never_returns_a_partial_page() {
    let input = json_fixture(
        r#"{
            "results": [{"index": 0, "data": {"content": "😀\nexact"}}]
        }"#,
    );
    let response = ResponseInput {
        tool: crate::id::ToolId::LocalFetch,
        structured: input.clone(),
        options: ResponsePageOptions::default(),
    };
    assert_eq!(
        prepare(response.clone(), &Cancellation::new())
            .expect("unpaged")
            .value,
        input
    );
    let cancelled = Cancellation::new();
    cancelled.cancel();
    assert_eq!(prepare(response, &cancelled), Err(ResponseError::Cancelled));
}

#[test]
fn immutable_replay_uses_one_plan_and_preserves_provider_facts_once() {
    let input = json!({"results":[{"index":0,"data":{"rateLimit":{"remaining":1},"files":(0..30).map(|i|json!({"id":i,"text":"y".repeat(50)})).collect::<Vec<_>>()}}]});
    let pages = SavedPages::default();
    let mut options = ResponsePageOptions {
        response_chars: Some(700),
        paging: true,
        ..Default::default()
    };
    let mut ids = Vec::new();
    let mut requests = 0;
    loop {
        let prepared = prepare_saved(
            &ResponseInput {
                tool: crate::id::ToolId::LocalSearch,
                structured: Value::Null,
                options: options.clone(),
            },
            &input,
            &pages,
            &Cancellation::new(),
        )
        .expect("saved page");
        let page = prepared.value;
        for row in page["results"].as_array().expect("rows") {
            requests += usize::from(row["data"].get("rateLimit").is_some());
            ids.extend(
                row["data"]["files"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|item| item["id"].as_u64().expect("id")),
            );
        }
        let Some(next) = prepared.next else {
            break;
        };
        options.open(&next);
    }
    assert_eq!(ids, (0..30).collect::<Vec<_>>());
    assert_eq!(requests, 1);
}

/// B91: `clientWaits` differs on every execution of one request, like
/// `rateLimit`: a re-executed page whose waits differ (or are absent) keeps
/// the content snapshot and the page edges of the first run.
#[test]
fn client_waits_move_neither_the_snapshot_nor_the_page_edges() {
    let run = |waits: Option<Value>| {
        let mut data = json!({"files":(0..30).map(|i|json!({"id":i,"text":"w".repeat(50)})).collect::<Vec<_>>()});
        if let Some(waits) = waits {
            data["clientWaits"] = waits;
        }
        json!({"results":[{"index":0,"data":data}]})
    };
    let runs = [
        run(Some(json_fixture(
            r#"[
                {"kind": "quotaPacing", "ms": 1307},
                {"kind": "spacing", "ms": 4}
            ]"#,
        ))),
        run(Some(json_fixture(r#"[{"kind": "quotaPacing", "ms": 5}]"#))),
        run(None),
    ];
    let first_page = |input: &Value| {
        let prepared = prepare_saved(
            &ResponseInput {
                tool: crate::id::ToolId::GhSearchCode,
                structured: Value::Null,
                options: ResponsePageOptions {
                    response_chars: Some(700),
                    paging: true,
                    ..Default::default()
                },
            },
            input,
            &SavedPages::default(),
            &Cancellation::new(),
        )
        .expect("saved page");
        prepared.value["results"][0]["data"]["files"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|item| item["id"].as_u64().expect("id"))
            .collect::<Vec<_>>()
    };
    let snapshot = crate::response::pager::content_snapshot(&runs[0], 700, "build");
    let edges = first_page(&runs[0]);
    for other in &runs[1..] {
        assert_eq!(
            crate::response::pager::content_snapshot(other, 700, "build"),
            snapshot
        );
    }
    assert_eq!(
        first_page(&runs[1]),
        edges,
        "the first page ends where it did"
    );
}

#[test]
fn mismatched_or_out_of_range_internal_page_locators_are_rejected_without_restarting() {
    let input = json!({"results":[{"index":0,"data":{"files":(0..50).map(|i|json!({"id":i,"text":"e".repeat(50)})).collect::<Vec<_>>()}}]});
    let base = ResponsePageOptions {
        response_chars: Some(700),
        paging: true,
        ..Default::default()
    };
    let first = prepare(
        ResponseInput {
            tool: crate::id::ToolId::LocalSearch,
            structured: input.clone(),
            options: base.clone(),
        },
        &Cancellation::new(),
    )
    .expect("first page");
    let mut valid = base;
    valid.open(&first.next.expect("next"));
    for options in [
        ResponsePageOptions {
            response_offset: Some(usize::MAX),
            ..valid.clone()
        },
        ResponsePageOptions {
            response_snapshot: Some("wrong-snapshot".into()),
            ..valid.clone()
        },
        ResponsePageOptions {
            response_chars: Some(900),
            ..valid
        },
    ] {
        assert_eq!(
            prepare(
                ResponseInput {
                    tool: crate::id::ToolId::LocalSearch,
                    structured: input.clone(),
                    options,
                },
                &Cancellation::new()
            ),
            Err(ResponseError::InvalidPage)
        );
    }
}

#[test]
fn outline_fragments_keep_each_members_kind_and_export_state() {
    let members = (0..80).map(|index| json!({"symbolName":format!("method{index}"),"kind":"method","line":index+2,"exported":index%2==0})).collect::<Vec<_>>();
    let input = json!({"results":[{"index":0,"data":{"symbols":[{"symbolName":"Container","kind":"class","line":1,"members":members}]}}]});
    for budget in [700, 1400, 4000] {
        let pages = walk_tool("astSearch", &input, budget);
        assert!(pages.len() > 1);
        let seen = pages
            .iter()
            .flat_map(|page| page["results"].as_array().into_iter().flatten())
            .flat_map(|row| row["data"]["symbols"].as_array().into_iter().flatten())
            .flat_map(|symbol| {
                assert_eq!(symbol["symbolName"], "Container");
                assert!(symbol.get("shared").is_none());
                symbol["members"].as_array().into_iter().flatten().cloned()
            })
            .collect::<Vec<_>>();
        assert_eq!(seen, members);
    }
}
