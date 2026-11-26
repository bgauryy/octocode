//! A classified walk of a tool's rows pages on with the tool's private
//! progress beside its public query, which only an issued cursor carries.
use super::*;
use octocode_runtime_base::security::{ContentSecurity, InputPolicy};

/// The `listed` state of a ghSearchCode walk that listed `files` files: one
/// 16-hex digest each, as the walk writes it.
fn listed(files: usize) -> String {
    (0..files).map(|file| format!("{file:016x}")).collect()
}

/// A classified ghSearchCode row, as a caller sends it.
fn walk_row() -> Row {
    let rows = super::super::super::rows::parse_rows(&[json_fixture(
        r#"{
            "operation": "evaluate",
            "source": {
                "tool": "ghSearchCode",
                "query": {"keywords": ["useState"], "owner": "vercel", "repo": "next.js"}
            },
            "questions": {"hook": {"type": "noul", "instructions": "Calls a React hook."}}
        }"#,
    )]);
    rows.into_iter().next().expect("one row")
}

/// The tool's own next page past 625 listed files (a mocked page-1 lead).
fn page_two() -> Map<String, Value> {
    let query = json!({"keywords":["useState"],"owner":"vercel","repo":"next.js",
        "page":2,"listed":listed(626)});
    query.as_object().expect("a row").clone()
}

/// The restored row's gate: the one a cursor-restored clasify row passes.
fn restored_gate(row: Value) -> Result<(), String> {
    ContentSecurity::new()
        .check_saved_input(
            row,
            InputPolicy::ClasifyEvidence {
                content_limit: octocode_runtime_base::id::clasify_policy::REQUEST_BYTES,
            },
            ToolId::Clasify,
        )
        .map(|_| ())
        .map_err(|issues| format!("{issues:?}"))
}

/// Past 625 files the walk's `listed` outgrows any caller string. The next
/// page keeps it beside the public query, where the restored row's gate
/// leaves it as written, so the walk pages on; the row the cursor restores
/// is one the contract admits.
#[test]
fn a_classified_walk_past_625_files_pages_on() {
    let Some(NextPage::Call(call)) =
        next_page(&walk_row(), ToolId::GhSearchCode, &page_two(), 0, true)
    else {
        panic!("a resumable runtime continues the walk");
    };
    let call = call.build();
    let row = call["input"]["queries"][0].clone();
    let query = &row["source"]["query"];
    assert!(
        query.get("listed").is_none() && query.get("page").is_none(),
        "{query}"
    );
    assert_eq!(row["sourceProgress"]["listed"], json!(listed(626)));
    assert_eq!(row["sourceProgress"]["page"], json!(2));
    restored_gate(row.clone()).expect("a restored walk row passes the gate");
    octocode_runtime_base::contracts::prepare_saved_rows(
        ToolId::Clasify.as_str(),
        json!({"queries": [row]}),
    )
    .expect("the contract admits the restored row");
}

/// A runtime that keeps no saved work cannot carry private progress in a
/// fresh call: the walk's next page is not offered.
#[test]
fn a_runtime_without_saved_work_offers_no_page_it_cannot_resume() {
    assert!(matches!(
        next_page(&walk_row(), ToolId::GhSearchCode, &page_two(), 0, false),
        Some(NextPage::Unsaved)
    ));
}

/// A caller's `source.query` is the tool's public row: a private field in
/// it is named, and nothing is read.
#[test]
fn private_progress_in_a_callers_source_query_is_named() {
    let query = json!({"keywords":["x"],"owner":"o","repo":"r","page":2});
    assert_eq!(private_field(ToolId::GhSearchCode, &query), Some("page"));
    let public = json!({"keywords":["x"],"owner":"o","repo":"r"});
    assert_eq!(private_field(ToolId::GhSearchCode, &public), None);
}

#[test]
fn a_classified_walk_preserves_predicates_and_their_question_keys() {
    use super::super::super::rows::parse_rows;

    let mut query = walk_row().query;
    query["questions"] = json!({"p1":{"type":"noul","instructions":"Is a hook called?"}});
    query["predicates"] = json!(["Is the hook conditional?", "Is the hook in test code?"]);
    let row = parse_rows(&[query.clone()]).pop().expect("predicate row");
    let Some(NextPage::Call(call)) = next_page(&row, ToolId::GhSearchCode, &page_two(), 5, true)
    else {
        panic!("the predicate walk continues");
    };
    let next = call.build()["input"]["queries"][0].clone();
    assert_eq!(next["questions"], query["questions"]);
    assert_eq!(next["predicates"], query["predicates"]);
    restored_gate(next.clone()).expect("the saved predicate request passes the input gate");
    let resumed = parse_rows(&[next]).pop().expect("continued row");
    assert_eq!(resumed.keys, row.keys);
    assert_eq!(resumed.questions, row.questions);
}

#[test]
fn an_all_quiet_search_page_offers_the_saved_file_scores() {
    let full = json!({
        "files":{"src/hooks.ts":{"4":{"hook":{"noul":0.125}}}},
        "coverage":{"totalUnits":1,"answered":1,"quiet":1}, "usage":{"calls":1},
    });
    let mut summary = full.clone();
    assert!(super::super::offers_saved_details(&walk_row(), &full));
    super::super::super::view::summarize(&mut summary, false, true);
    assert!(summary.get("files").is_none());
    // The warning naming the left-out unit points at the unbilled replay.
    let warning = summary["warnings"][0].as_str().expect("a left-out warning");
    assert!(warning.contains("next.details"), "{warning}");
    assert!(!warning.contains("billed again"), "{warning}");
    offer_saved_details(&walk_row(), &mut summary, full.clone());
    let details = &summary["next"]["details"]["input"];
    let replay = &details["queries"][0];
    assert_eq!(replay["view"], "details");
    assert_eq!(replay["savedPage"], full);
    assert_eq!(summary["usage"], full["usage"]);
    restored_gate(replay.clone()).expect("a saved details row passes the input gate");
    octocode_runtime_base::contracts::prepare_saved_rows(ToolId::Clasify.as_str(), details.clone())
        .expect("the contract admits the saved details row");
}

#[test]
fn a_saved_source_page_admits_only_what_the_walk_has_left() {
    use super::super::super::{admission, rows::parse_rows};

    let (max, spent, needed) = (5, 4, 2);
    let mut query = walk_row().query;
    query["maxRequests"] = json!(max);
    let row = parse_rows(&[query]).pop().expect("bounded source row");
    let Some(NextPage::Call(call)) =
        next_page(&row, ToolId::GhSearchCode, &page_two(), spent, true)
    else {
        panic!("the bounded walk continues");
    };
    let next = call.build()["input"]["queries"][0].clone();
    restored_gate(next.clone()).expect("the saved budget passes the input gate");
    let resumed = parse_rows(&[next]).pop().expect("resumed row");
    assert_eq!(resumed.budget(), Some(max - spent));
    let first_round = admission::Plan {
        first: needed,
        at_most: needed,
        bounded: true,
        units: 2,
        tokens: 0,
        window: None,
    };
    let Err(over) = admission::admit(resumed.budget(), first_round, 1, 0) else {
        panic!("the page cannot fit the remaining request allowance");
    };
    let refusal = reply::over_budget(&resumed, &over, true);
    assert_eq!(
        refusal["errorCode"],
        json!(ErrorCode::ClassificationOverBudget)
    );
    assert_eq!(refusal["usage"]["calls"], 0);
    assert_eq!(refusal["usage"]["walkCalls"], spent);
    let confirmed = refusal["next"]["confirm"]["input"]["queries"][0].clone();
    restored_gate(confirmed.clone()).expect("the confirmation preserves the cursor's state");
    let row = parse_rows(&[confirmed]).pop().expect("confirmed row");
    assert_eq!(row.max_requests, Some(spent + needed));
    assert_eq!(row.budget(), Some(needed));
    assert!(admission::admit(row.budget(), first_round, 1, 0).is_ok());
}

#[test]
fn a_walk_reports_the_same_requests_its_cursor_carries_after_a_twin_transfer() {
    use super::super::super::settle_shared_spend;

    let before: u64 = 1;
    let row = walk_row();
    let mut carried = Vec::new();
    let mut outcomes = [0, 2].map(|calls| {
        let mut judged = shape(
            &row,
            Vec::new(),
            UnitAnswers {
                answers: Vec::new(),
                spends: Vec::new(),
                refused: 0,
                model: None,
            },
        );
        judged.spends.push(UnitSpend {
            identity: Some([1; 32]),
            input_tokens: 0,
            output_tokens: 0,
            calls,
        });
        judged.price.ceiling = Some(calls.max(1));
        let spend = walk_spend(&judged, before);
        judged.walk_spend = Some(spend);
        let Some(NextPage::Call(next)) =
            next_page(&row, ToolId::GhSearchCode, &page_two(), spend.total, true)
        else {
            panic!("the twin walk continues");
        };
        carried.push(next.build()["input"]["queries"][0]["walkRequests"].clone());
        RowOutcome::Units(Ok(judged))
    });

    // The second input sent the request. Billing still belongs to the first input.
    settle_shared_spend(&mut outcomes);
    for ((outcome, carried), billed) in outcomes.into_iter().zip(carried).zip([2, 0]) {
        let RowOutcome::Units(Ok(judged)) = outcome else {
            panic!("an assessed page");
        };
        let output = judged.finish();
        assert_eq!(output["usage"]["calls"], billed);
        assert_eq!(output["usage"]["walkCalls"], carried);
        let warnings = output["warnings"]
            .as_array()
            .expect("the transfer is disclosed");
        let warnings = || warnings.iter().filter_map(Value::as_str);
        assert!(
            warnings().any(|warning| warning.contains("usage.calls")),
            "{output}"
        );
        assert!(
            !warnings().any(|warning| warning.contains("past requestsCeiling")),
            "{output}"
        );
    }
}

/// A page of a tool's rows that has a next page says what the reply's `next`
/// costs: the next page is judged, so it bills as this page did. The count
/// of rows left is stated only from an exact total, offset and length.
#[test]
fn a_page_with_a_next_page_says_its_next_bills() {
    let counted = json!({"pagination": {"total": 179, "offset": 20, "length": 20}});
    let note = billed_next_note("artifactSearch", 1, 20, &counted, true);
    assert!(note.contains("bills again"), "{note}");
    assert!(note.contains("1 judge request for 20 rows"), "{note}");
    assert!(
        note.contains("139 of artifactSearch's rows remain"),
        "{note}"
    );
    assert!(note.starts_with("The reply's next "), "{note}");
    for uncounted in [
        json!({}),
        json!({"pagination": {"offset": 20}}),
        json!({"pagination": {"total": 179, "offset": 20, "length": 20, "countScope": "capped"}}),
        // A total below what was already read is no count of what is left.
        json!({"pagination": {"total": 30, "offset": 20, "length": 20}}),
    ] {
        let note = billed_next_note("ghSearchCode", 3, 12, &uncounted, false);
        assert!(note.contains("3 judge requests for 12 rows"), "{note}");
        assert!(
            note.contains("more of ghSearchCode's rows remain"),
            "{uncounted}: {note}"
        );
        // Without saved work the page's call is the row's related call.
        assert!(note.starts_with("The related[] call "), "{note}");
    }
}
