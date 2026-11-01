//! Hierarchy walks over a fake graph: order, levels, cycles, failures
//! and cancellation.

use super::*;

#[test]
fn failed_hierarchy_expansion_is_an_error_or_a_marked_partial_row() {
    // Nothing expanded and the provider failed: surface the error.
    assert!(expansion_outcome(Vec::new(), vec![expansion_error("boom")]).is_err());
    // Nothing expanded and nothing failed: a genuine empty result.
    let (items, failures) = expansion_outcome(Vec::new(), Vec::new()).expect("empty");
    assert!(items.is_empty() && failures.is_empty());

    let q = query(serde_json::json!({
        "operation": "callers", "path": "file:///repo/a.ts",
        "symbolName": "foo", "line": 1,
        "depth": 3
    }));
    let (items, failures) = expansion_outcome(
        vec![serde_json::json!({"from": {"symbolName": "caller"}})],
        vec![expansion_error("LSP error: deeper level")],
    )
    .expect("partial expansion keeps what was found");
    let mut row = items_payload(&q, "callers", serde_json::json!(items), TEST_BUDGET);
    mark_partial_expansion(&mut row, &q, &failures);
    assert_eq!(row["isPartial"], true);
    assert_eq!(
        row["partialReasons"],
        serde_json::json!(["callHierarchyExpansionFailed"])
    );
    assert!(
        row["warnings"][0]
            .as_str()
            .is_some_and(|w| w.contains("deeper level"))
    );
    let retry = crate::tools::test_support::checks::related_call(&row, "lspSearch", |_| true);
    assert_eq!(
        retry["input"]["queries"][0],
        crate::tools::lsp_search::failure::same_query_rerun(&q)
    );
    crate::contracts::validate_output("lspSearch", &public_row(&row))
        .expect("partial hierarchy row satisfies the output contract");

    // The disclosure is one count plus an optional reattempt; every distinct failure
    // is listed once, with its count, in the verbose `expansionFailures`.
    let mut failures = (0..6)
        .map(|n| expansion_error(&format!("LSP error: node {n}")))
        .collect::<Vec<_>>();
    failures.extend((0..4).map(|_| expansion_error("LSP error: timeout")));
    let mut row = items_payload(&q, "callers", serde_json::json!(items), TEST_BUDGET);
    mark_partial_expansion(&mut row, &q, &failures);
    let warnings = row["warnings"].as_array().cloned().unwrap_or_default();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(
        warnings[0].as_str().is_some_and(|w| w.contains("10 items")
            && w.contains("7 distinct")
            && w.contains("4×")
            && w.contains("timeout")),
        "{warnings:?}"
    );
    let listed = row["expansionFailures"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert_eq!(listed.len(), 7, "{listed:?}");
    assert!(
        listed
            .iter()
            .any(|w| w.as_str().is_some_and(|w| w.contains("node 5"))),
        "{listed:?}"
    );
    assert!(
        listed.iter().any(|w| w
            .as_str()
            .is_some_and(|w| w.starts_with("4×") && w.contains("timeout"))),
        "{listed:?}"
    );
    let public = public_row(&row);
    assert_eq!(
        public["results"][0]["data"]["expansionFailures"],
        row["expansionFailures"]
    );
}

/// An optional reattempt only when asking again can change the answer: a relation
/// the walk could not establish is partial without it; a timed-out request
/// beside it keeps it.
#[test]
fn a_partial_walk_retries_only_a_transient_failure() {
    let q = query(serde_json::json!({
        "operation": "subtypes", "path": "file:///repo/a.ts", "symbolName": "Base", "line": 1
    }));
    let items = serde_json::json!([{"name": "Child"}]);
    let unestablished = LspFailure::unestablished("No resolved immediate heritage");
    let mut row = items_payload(&q, "subtypes", items.clone(), TEST_BUDGET);
    mark_partial_expansion(&mut row, &q, std::slice::from_ref(&unestablished));
    assert_eq!(row["isPartial"], true, "{row}");
    assert_eq!(
        row["partialReasons"],
        serde_json::json!(["typeHierarchyExpansionFailed"])
    );
    assert!(row.pointer("/next/retry").is_none(), "{row}");
    assert!(
        !row["related"].as_array().is_some_and(|calls| calls
            .iter()
            .any(|call| { call["kind"] == "related" && call["tool"] == "lspSearch" })),
        "{row}"
    );

    let timeout = LspFailure::overdue("no answer in time".into());
    let mut row = items_payload(&q, "subtypes", items, TEST_BUDGET);
    mark_partial_expansion(&mut row, &q, &[unestablished, timeout]);
    let retry = crate::tools::test_support::checks::related_call(&row, "lspSearch", |_| true);
    assert_eq!(
        retry["input"]["queries"][0],
        crate::tools::lsp_search::failure::same_query_rerun(&q)
    );
}

#[tokio::test(flavor = "current_thread")]
async fn hierarchy_walk_is_breadth_first_and_labels_every_edge_with_level_and_via() {
    // `a` calls root directly AND through `b`; breadth-first expansion must
    // expand `a` once, at level 1, so its caller `c` is reached at level 2.
    let graph = graph(&[("root", &["b", "a"]), ("b", &["a"]), ("a", &["c"])]);
    let walk = walk(&graph, 2).await;
    assert_eq!(
        edge_summary(&walk),
        vec![
            ("b".into(), 1, None),
            ("a".into(), 1, None),
            ("a".into(), 2, Some("b".into())),
            ("c".into(), 2, Some("a".into())),
        ]
    );
    // root, b, a expanded once each; level-2 nodes are not expanded at depth 2.
    assert_eq!(graph.requests.get(), 3);
    let edges = walk
        .edges
        .iter()
        .map(|edge| public_edge(Expansion::IncomingCalls, edge, None))
        .collect::<Vec<_>>();
    // Public coordinates are one-based lines and UTF-16 columns, like
    // `public_location`.
    let b = name_line("b");
    assert_eq!(
        edges[0]["from"]["displayRange"],
        serde_json::json!({"startLine": b + 1, "startCharacter": 10, "endLine": b + 4})
    );
    assert_eq!(edges[0]["from"]["kind"], "function");
    assert_eq!(
        edges[0]["fromRanges"],
        serde_json::json!([{"startLine": 3, "startCharacter": 5, "endLine": 3}])
    );
    assert_eq!(edges[2]["via"]["line"], b + 1);
    assert_eq!(edges[2]["via"]["character"], 10);
    assert!(edges[0].get("via").is_none());
}

#[tokio::test(flavor = "current_thread")]
async fn hierarchy_walk_keeps_cycle_edges_without_re_expanding() {
    let graph = graph(&[("root", &["a"]), ("a", &["root"])]);
    let walk = walk(&graph, 5).await;
    assert_eq!(
        edge_summary(&walk),
        vec![("a".into(), 1, None), ("root".into(), 2, Some("a".into()))]
    );
    assert_eq!(graph.requests.get(), 2, "root and a expand once each");
}

#[tokio::test(flavor = "current_thread")]
async fn hierarchy_walk_expands_a_diamond_node_once() {
    let graph = graph(&[
        ("root", &["a", "b"]),
        ("a", &["c"]),
        ("b", &["c"]),
        ("c", &["d"]),
    ]);
    let walk = walk(&graph, 3).await;
    // The walk orders each level by position, not by name.
    let mut edges = edge_summary(&walk);
    edges.sort();
    assert_eq!(
        edges,
        vec![
            ("a".into(), 1, None),
            ("b".into(), 1, None),
            ("c".into(), 2, Some("a".into())),
            ("c".into(), 2, Some("b".into())),
            ("d".into(), 3, Some("c".into())),
        ]
    );
    assert_eq!(graph.requests.get(), 4, "c is expanded once");
}

#[tokio::test(flavor = "current_thread")]
async fn repeated_parent_node_pairs_merge_into_one_edge_with_all_sites() {
    let graph = graph(&[("root", &["a", "a"])]);
    let walk = walk(&graph, 1).await;
    assert_eq!(walk.edges.len(), 1);
    let edge = public_edge(Expansion::IncomingCalls, &walk.edges[0], None);
    assert_eq!(
        edge["fromRanges"],
        serde_json::json!([
            {"startLine": 3, "startCharacter": 5, "endLine": 3},
            {"startLine": 4, "startCharacter": 5, "endLine": 4}
        ])
    );
}

#[tokio::test(flavor = "current_thread")]
async fn hierarchy_depth_follows_the_request_past_any_fixed_level() {
    // The contract sets no depth maximum: a 30-long chain walked with depth
    // 1,000 ends where the chain does, every level listed.
    let names = (0..30).map(|index| format!("n{index}")).collect::<Vec<_>>();
    let mut graph = graph(&[]);
    graph.callers.insert("root".into(), vec![names[0].clone()]);
    for pair in names.windows(2) {
        graph.callers.insert(pair[0].clone(), vec![pair[1].clone()]);
    }
    let walk = walk(&graph, 1_000).await;
    assert_eq!(walk.edges.len(), names.len());
    assert_eq!(
        walk.edges.last().map(|edge| edge.level),
        Some(u32::try_from(names.len()).expect("levels"))
    );
}

#[tokio::test(flavor = "current_thread")]
async fn hierarchy_walk_collects_per_node_failures_and_skips_out_of_policy_nodes() {
    let mut graph = graph(&[("root", &["broken", "fine"]), ("fine", &["deep"])]);
    let walk_all = walk(&graph, 2).await;
    assert_eq!(walk_all.failures.len(), 1, "broken failed at level 2");
    assert_eq!(edge_summary(&walk_all).len(), 3);

    // With a uri outside the (empty) policy, every result is skipped.
    graph.uri = Some("file:///etc/hosts".into());
    let walk = walk(&graph, 2).await;
    assert!(walk.edges.is_empty());
    assert_eq!(walk.out_of_policy, 2);
}

#[tokio::test(flavor = "current_thread")]
async fn hierarchy_walk_checks_cancellation_between_requests() {
    struct Cancelled;
    impl crate::execution::cancel::CancellationCheck for Cancelled {
        fn check(&self) -> Result<(), String> {
            Err("cancelled".into())
        }
    }
    use crate::policy::path::{PathPolicy, PathPolicyConfig};
    let paths = PathPolicy::new(PathPolicyConfig::default()).expect("path policy");
    let graph = graph(&[("root", &["a"])]);
    let error = walk_from(&graph, node("root"), 2, &paths, &Cancelled)
        .await
        .expect_err("cancelled");
    assert_eq!(error.code, ErrorCode::Cancelled);
    assert_eq!(graph.requests.get(), 0);
}

#[tokio::test(flavor = "current_thread")]
async fn long_awaits_observe_cancellation() {
    struct CancelAfter(std::sync::atomic::AtomicUsize);
    impl crate::execution::cancel::CancellationCheck for CancelAfter {
        fn check(&self) -> Result<(), String> {
            if self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst) >= 2 {
                Err("cancelled".into())
            } else {
                Ok(())
            }
        }
    }
    let cancel = CancelAfter(std::sync::atomic::AtomicUsize::new(0));
    let started = std::time::Instant::now();
    let result = cancellable(&cancel, std::future::pending::<()>()).await;
    assert_eq!(result.expect_err("cancelled").code, ErrorCode::Cancelled);
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
}

#[tokio::test(flavor = "current_thread")]
async fn ready_awaits_observe_cancellation_at_completion() {
    struct CancelAfterFirst(std::sync::atomic::AtomicUsize);
    impl crate::execution::cancel::CancellationCheck for CancelAfterFirst {
        fn check(&self) -> Result<(), String> {
            if self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                Ok(())
            } else {
                Err("cancelled after launch".into())
            }
        }
    }
    let cancel = CancelAfterFirst(std::sync::atomic::AtomicUsize::new(0));
    let error = cancellable(&cancel, std::future::ready(7))
        .await
        .expect_err("a ready result cannot bypass cancellation");
    assert_eq!(error.code, ErrorCode::Cancelled);
    assert_eq!(error.message, "cancelled after launch");
    assert_eq!(
        cancellable(
            &crate::execution::cancel::NeverCancel,
            std::future::ready(7)
        )
        .await
        .expect("completes"),
        7
    );
}
