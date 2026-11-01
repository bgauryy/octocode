//! Independent held-out cases for response::row_pages::tests.
//! Integrate as a sibling of evidence_closure. Expose its fixture,
//! exact_closure and chars helpers as pub(super). This artifact was not compiled.
use super::evidence_closure::{chars, exact_closure, fixture};
use super::*;
use crate::id::ToolId;
use crate::tools::test_support::assert_row_contract;
use tokio_util::sync::CancellationToken;

#[test]
fn exact_tail_boundary_includes_row_part_without_losing_closure_or_rows() {
    let data = fixture(100, 100);
    let catalog = data["sourceEvidence"].clone();
    let expected = data["rows"].as_array().unwrap().clone();
    assert_row_contract(ToolId::GraphQuery, &data);
    let token = CancellationToken::new();
    // Two adjacent results plus their exact closure serialize to 8,472 chars.
    // The old tail packing admitted them at 8,473, then added rowPart afterward.
    let budget = 8_473;
    let plan = PreparedRows::new(
        vec![json!({"index":0,"data":data})],
        &[None],
        ToolId::GraphQuery,
    )
    .plan(budget, &token)
    .expect("valid plan");
    assert!(plan.total_pages > 1);
    let mut seen = Vec::new();
    for page in 0..plan.total_pages {
        let rows = plan.build_page(page, &[None], &[], &token).unwrap();
        assert!(!rows.is_empty());
        assert!(
            chars(&json!(rows)) <= budget,
            "page {page} exceeds its row budget after rowPart is attached"
        );
        for row in rows {
            assert_row_contract(ToolId::GraphQuery, &row["data"]);
            exact_closure(&row["data"], &catalog);
            seen.extend(row["data"]["rows"].as_array().unwrap().iter().cloned());
        }
    }
    assert_eq!(
        seen, expected,
        "all occurrences and every nested origin survive in order"
    );
}

#[test]
fn first_only_oversized_context_does_not_enlarge_later_nested_pages() {
    let description = "x".repeat(10_000);
    let expected = (0..1_000)
        .map(|index| json!({"id":index.to_string(),"value":"x".repeat(62)}))
        .collect::<Vec<_>>();
    let data = json!({
        "operation":"diagnostics",
        "graph":{"id":"g","root":"/fixture","createdAt":"now"},
        "rows":[{"id":"finding","description":description,
            "evidence":{"members":expected}}]
    });
    assert_row_contract(ToolId::GraphQuery, &data);
    let token = CancellationToken::new();
    let budget = 2_000;
    let plan = PreparedRows::new(
        vec![json!({"index":0,"data":data})],
        &[None],
        ToolId::GraphQuery,
    )
    .plan(budget, &token)
    .expect("valid plan");
    assert!(plan.total_pages > 2);
    let mut seen = Vec::new();
    let mut descriptions = 0;
    for page in 0..plan.total_pages {
        let rows = plan.build_page(page, &[None], &[], &token).unwrap();
        assert!(!rows.is_empty());
        if page == 0 {
            assert!(
                chars(&json!(rows)) > budget,
                "first context is indivisibly oversized"
            );
        } else {
            assert!(
                chars(&json!(rows)) <= budget,
                "later page {page} must fit including rowPart.continues"
            );
        }
        for row in rows {
            assert_row_contract(ToolId::GraphQuery, &row["data"]);
            for finding in row["data"]["rows"].as_array().unwrap() {
                assert_eq!(finding["id"], "finding");
                if let Some(actual) = finding.get("description") {
                    assert_eq!(actual, &json!(description));
                    assert_eq!(page, 0, "first-only context repeated on a later page");
                    descriptions += 1;
                }
                seen.extend(
                    finding["evidence"]["members"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .cloned(),
                );
            }
        }
    }
    assert_eq!(descriptions, 1);
    assert_eq!(
        seen, expected,
        "nested evidence is delivered exactly once in order"
    );
}
