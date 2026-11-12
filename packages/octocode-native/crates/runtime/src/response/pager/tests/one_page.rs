//! A complete one-page response is no page of a split walk: it states no
//! `responsePagination` block, even when an indivisible value makes it wider
//! than `responseChars` (SHR-1: a `hasMore:false` page 1 of 1 read as the end
//! of a walk that never began). Its size is stated by the envelope's own
//! top-level `oversized`, as on every page of a walk.
use super::*;

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
    let input = json!({"results":[{"index":0,"data":{"node":{"id":"small"},"rows":[]}}]});
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
    assert_eq!(block["hasMore"], false, "{}", page.value);
    assert_eq!(block["replayed"], true);
    assert!(block.get("oversized").is_none(), "{}", page.value);
    assert_eq!(
        page.value["oversized"], true,
        "a page of a walk discloses its size at the top level"
    );
}
