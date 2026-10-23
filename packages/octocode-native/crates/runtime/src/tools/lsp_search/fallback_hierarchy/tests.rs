use super::super::{LspSearchQuery, locations};
use super::*;

fn candidates() -> Vec<LspCodeSnippet> {
    (0..MAX_HIERARCHY_NODES + 3)
        .map(|line| {
            serde_json::from_value(json!({
                "uri": "file:///workspace/types.ts",
                "range": {"start":{"line":line,"character":13},"end":{"line":line,"character":21}},
                "content": format!("export class Child{line} extends Base {{}}"),
            }))
            .expect("snippet")
        })
        .collect()
}

fn query() -> LspSearchQuery {
    serde_json::from_value(json!({"operation":"subtypes","path":"/workspace/types.ts","line":1,"symbolName":"Base","pageSize":1})).expect("query")
}

#[test]
fn candidate_windows_reject_mutation_and_preserve_stable_order() {
    let query = query();
    let first = CandidateWindow::cut(&query, candidates()).expect("first window");
    assert_eq!(first.items.len(), MAX_HIERARCHY_NODES);
    assert!(first.has_more);
    let mut next = lsp_search_query::to_row(&query);
    next["candidatePage"] = json!(2);
    next["snapshot"] = json!(first.digest);
    let next: LspSearchQuery = serde_json::from_value(next).expect("next window");
    let mut reordered = candidates();
    reordered.reverse();
    // Duplicate provider locations must not shift the candidate boundary.
    reordered.push(reordered[0].clone());
    let second = CandidateWindow::cut(&next, reordered).expect("same candidates");
    assert_eq!(second.items.len(), 3);
    assert_eq!(
        second.items[0].range.start.line as usize,
        MAX_HIERARCHY_NODES
    );
    assert!(!second.has_more);
    let mut removed = candidates();
    removed.remove(0);
    assert!(CandidateWindow::cut(&next, removed).is_none());
    let mut changed = candidates();
    changed[0].content.push_str(" // changed");
    assert!(CandidateWindow::cut(&next, changed).is_none());
    let mut missing = lsp_search_query::to_row(&next);
    missing
        .as_object_mut()
        .expect("query object")
        .remove("snapshot");
    assert!(
        CandidateWindow::cut(
            &serde_json::from_value(missing).expect("query"),
            candidates()
        )
        .is_none()
    );
}

#[test]
fn candidate_mutation_stales_later_result_pages_even_when_rows_disappear() {
    let query = query();
    let window = CandidateWindow::cut(&query, candidates()).expect("window");
    let items = json!([{"name":"first"},{"name":"second"}]);
    let first = super::super::failure::with_next(
        &query,
        locations::salted_items_payload(
            &query,
            "subtypes",
            items.clone(),
            Some(&window.digest),
            16000,
            super::super::render::plain_sizes,
        ),
    );
    let next: LspSearchQuery =
        serde_json::from_value(first["next"]["nextPage"]["query"]["queries"][0].clone())
            .expect("next result page");
    let stable = locations::salted_items_payload(
        &next,
        "subtypes",
        items.clone(),
        Some(&window.digest),
        16000,
        super::super::render::plain_sizes,
    );
    assert!(stable.get("errorCode").is_none(), "{stable}");
    let mut changed = candidates();
    changed.remove(0);
    let changed =
        CandidateWindow::cut(&next, changed).expect("digest checked by result pagination");
    for rows in [items, json!([])] {
        let stale = locations::salted_items_payload(
            &next,
            "subtypes",
            rows,
            Some(&changed.digest),
            16000,
            super::super::render::plain_sizes,
        );
        assert_eq!(stale["errorCode"], "staleSnapshot", "{stale}");
        assert!(stale.pointer("/next/restart").is_some(), "{stale}");
    }
}
