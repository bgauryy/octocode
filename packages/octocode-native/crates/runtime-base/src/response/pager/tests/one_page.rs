//! A complete one-page response is no page of a split walk: it states no
//! `responsePagination` block, even when an indivisible value makes it wider
//! than `responseChars` (SHR-1: a `hasMore:false` page 1 of 1 read as the end
//! of a walk that never began). Its size is stated by the envelope's own
//! top-level `oversized`, as on every page of a walk.
use super::*;
use octocode_test_support::json_fixture;

/// An envelope whose only row is one indivisible value wider than `budget`:
/// a ready call, which runs only whole (free text would be windowed).
fn indivisible(budget: usize) -> Value {
    let atomic = format!("{}\"\\界😀", "x".repeat(budget));
    json!({"results":[{"index":0,"data":{"rows":[],"related":[
        {"kind":"related","tool":"localFetch","input":{"queries":[{"path":atomic}]}}
    ]}}]})
}

fn page_of(input: &Value, budget: usize, replayed: Option<bool>) -> PreparedPage {
    prepare(
        ResponseInput {
            tool: crate::id::ToolId::GraphQuery,
            structured: input.clone(),
            options: ResponsePageOptions {
                response_chars: Some(budget),
                paging: true,
                saved: replayed.map(|replayed| crate::response::page_model::SavedResponse {
                    key: [7; 32],
                    expires_at: "2026-11-07T00:00:00Z".into(),
                    replayed,
                }),
                ..Default::default()
            },
        },
        &Cancellation::new(),
    )
    .expect("page")
}

#[test]
fn a_complete_page_wider_than_the_budget_states_no_page_block() {
    let budget = 300;
    let input = indivisible(budget);
    for saved in [None, Some(false)] {
        let page = page_of(&input, budget, saved);
        assert!(
            page.value.get("responsePagination").is_none(),
            "saved {saved:?}: {}",
            page.value
        );
        assert!(page.next.is_none());
        assert_eq!(
            page.value["results"], input["results"],
            "the value stays whole"
        );
        assert!(page.value.to_string().encode_utf16().count() > budget);
        assert_eq!(
            page.value["oversized"], true,
            "saved {saved:?}: a complete answer states its size: {}",
            page.value
        );
    }
}

#[test]
fn a_page_within_the_budget_states_no_size() {
    let input = json_fixture(
        r#"{
            "results": [
                {"index": 0, "data": {"node": {"id": "small"}, "rows": []}}
            ]
        }"#,
    );
    for saved in [None, Some(false), Some(true)] {
        let page = page_of(&input, 10_000, saved);
        assert!(
            page.value.get("oversized").is_none(),
            "saved {saved:?}: {}",
            page.value
        );
    }
}

#[test]
fn a_saved_read_still_states_where_its_walk_ends() {
    let budget = 300;
    let page = page_of(&indivisible(budget), budget, Some(true));
    let block = &page.value["responsePagination"];
    assert!(
        block.get("hasMore").is_none() && block.get("next").is_none(),
        "{}",
        page.value
    );
    assert_eq!(block["replayed"], true);
    assert!(block.get("oversized").is_none(), "{}", page.value);
    assert_eq!(
        page.value["oversized"], true,
        "a page of a walk discloses its size at the top level"
    );
}

fn assert_unpaged(input: &Value, budget: Option<usize>, oversized: bool) {
    let prepared = prepare(
        ResponseInput {
            tool: crate::id::ToolId::GhCloneRepo,
            structured: input.clone(),
            options: ResponsePageOptions {
                response_chars: budget,
                ..Default::default()
            },
        },
        &Cancellation::new(),
    )
    .expect("unpaged receipt");
    let mut expected = input.clone();
    if oversized {
        expected["oversized"] = json!(true);
    }
    assert_eq!(prepared.value, expected, "only the size disclosure changes");
    assert!(prepared.next.is_none(), "no replay is introduced");
    assert!(prepared.value.get("responsePagination").is_none());
}

#[test]
fn unpaged_receipts_disclose_indivisible_values_without_replay() {
    let path = format!("/{}\"\\界😀", "nested/".repeat(40));
    let path_units = serde_json::to_string(&path)
        .expect("serialized path")
        .encode_utf16()
        .count()
        - 2;
    let receipt = json!({"results":[{"index":0,"data":{
        "location":{"kind":"tree","path":path}
    }}]});
    let call = json_fixture(
        r#"{
            "kind": "related",
            "tool": "structureSearch",
            "input": {"queries": [{"operation": "tree", "path": "p"}]}
        }"#,
    );
    let call_units = call.to_string().encode_utf16().count();
    let with_call = json!({"results":[{"index":0,"data":{
        "location":{"kind":"tree","path":"p"},"related":[call]
    }}]});
    for (input, budget) in [(receipt, path_units - 1), (with_call, call_units - 1)] {
        assert_unpaged(&input, Some(budget), true);
        assert_unpaged(&input, Some(1), true);
    }
}

#[test]
fn unpaged_fitting_or_unbounded_receipts_stay_unchanged() {
    let path = "a complete location";
    let input = json!({"results":[{"index":0,"data":{
        "location":{"kind":"tree","path":path}
    }}]});
    for budget in [None, Some(path.len()), Some(input.to_string().len())] {
        assert_unpaged(&input, budget, false);
    }
}

#[test]
fn unpaged_text_disclosure_survives_rejudge_and_clears_when_it_fits() {
    let budget = 20;
    let input = json!({"results":[{"index":0,"data":{
        "location":{"kind":"tree","path":"p".repeat(budget + 1)}
    }}]});
    let mut output = prepare(
        ResponseInput {
            tool: crate::id::ToolId::GhCloneRepo,
            structured: input,
            options: ResponsePageOptions {
                response_chars: Some(budget),
                ..Default::default()
            },
        },
        &Cancellation::new(),
    )
    .expect("unpaged receipt")
    .value;
    unflag_fitting(&mut output, crate::id::ToolId::GhCloneRepo, budget, false);
    assert_eq!(output["oversized"], true);

    output["results"][0]["data"]["location"]["path"] = json!("p");
    unflag_fitting(&mut output, crate::id::ToolId::GhCloneRepo, budget, false);
    assert!(output.get("oversized").is_none());
}

#[test]
fn unpaged_exempts_only_volatile_row_data() {
    let budget = "tree".len();
    for (field, facts) in [
        (
            "rateLimit",
            json_fixture(r#"{"resource": "search", "remaining": 0}"#),
        ),
        (
            "clientWaits",
            json_fixture(r#"[{"kind": "quotaPacing", "ms": 1}]"#),
        ),
        ("observedAt", json!("1970-01-01T00:00:00Z")),
    ] {
        let mut input = json_fixture(
            r#"{
                "results": [
                    {
                        "index": 0,
                        "data": {"location": {"kind": "tree", "path": "p"}}
                    }
                ]
            }"#,
        );
        input["results"][0]["data"][field] = facts.clone();
        assert_unpaged(&input, Some(budget), false);

        let mut nested = json_fixture(
            r#"{
                "results": [
                    {
                        "index": 0,
                        "data": {"location": {"kind": "tree", "path": "p"}, "details": {}}
                    }
                ]
            }"#,
        );
        nested["results"][0]["data"]["details"][field] = facts;
        assert_unpaged(&nested, Some(budget), true);
    }
}
