//! Hierarchy walks over a fake graph: order, levels, cycles, failures
//! and cancellation.

use super::*;

/// A policy whose workspace is `/repo`, the root the fake rows live in.
fn repo_paths() -> crate::policy::path::PathPolicy {
    use crate::policy::path::{PathPolicy, PathPolicyConfig};
    PathPolicy::new(PathPolicyConfig {
        workspace_root: Some("/repo".into()),
        ..PathPolicyConfig::default()
    })
    .expect("path policy")
}

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
    mark_partial_expansion(&mut row, &q, &failures, &repo_paths());
    assert_eq!(row["isPartial"], true);
    assert_eq!(
        row["partialReasons"],
        serde_json::json!(["callHierarchyExpansionFailed"])
    );
    assert!(
        row["expansionFailures"][0]
            .as_str()
            .is_some_and(|w| w.contains("deeper level")),
        "{row}"
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
    mark_partial_expansion(&mut row, &q, &failures, &repo_paths());
    // The failures are stated once, in `expansionFailures`: no warning
    // restates them, and the row names its reason by code alone.
    assert!(row.get("warnings").is_none(), "{row}");
    assert_eq!(
        row["partialReasons"],
        serde_json::json!(["callHierarchyExpansionFailed"])
    );
    crate::tools::test_support::checks::assert_no_reason_meaning(&row);
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

/// A failure naming a file inside the workspace spells it as rows do.
#[test]
fn expansion_failures_name_workspace_files_relative() {
    let q = query(serde_json::json!({
        "operation": "subtypes", "path": "file:///repo/a.ts", "symbolName": "Base", "line": 1
    }));
    let failure = LspFailure::unestablished(
        "No definition establishes the heritage relation at /repo/src/a.ts:4:10",
    );
    let mut row = items_payload(
        &q,
        "subtypes",
        serde_json::json!([{"name": "Child"}]),
        TEST_BUDGET,
    );
    mark_partial_expansion(&mut row, &q, &[failure], &repo_paths());
    let listed = row["expansionFailures"].to_string();
    assert!(
        listed.contains("src/a.ts:4:10") && !listed.contains("/repo/"),
        "{listed}"
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
    mark_partial_expansion(
        &mut row,
        &q,
        std::slice::from_ref(&unestablished),
        &repo_paths(),
    );
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
    mark_partial_expansion(&mut row, &q, &[unestablished, timeout], &repo_paths());
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
        (
            &edges[0]["from"]["line"],
            &edges[0]["from"]["column"],
            &edges[0]["from"]["endLine"]
        ),
        (
            &serde_json::json!(b + 1),
            &serde_json::json!(10),
            &serde_json::json!(b + 4)
        )
    );
    assert_eq!(edges[0]["from"]["kind"], "function");
    assert_eq!(
        edges[0]["fromRanges"],
        serde_json::json!([{"line": 3, "column": 5}])
    );
    assert_eq!(edges[2]["via"]["line"], b + 1);
    assert_eq!(edges[2]["via"]["column"], 10);
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
            {"line": 3, "column": 5},
            {"line": 4, "column": 5}
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

/// An await is woken by the request's signal, never by a poll: how often it
/// checks the cancellation does not grow with how long the work takes.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn an_await_checks_cancellation_without_polling() {
    struct Counting(std::sync::atomic::AtomicUsize);
    impl crate::execution::cancel::CancellationCheck for Counting {
        fn check(&self) -> Result<(), String> {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }
    }
    async fn checks(wait: std::time::Duration) -> usize {
        let cancel = Counting(std::sync::atomic::AtomicUsize::new(0));
        cancellable(&cancel, tokio::time::sleep(wait))
            .await
            .expect("never cancelled");
        cancel.0.load(std::sync::atomic::Ordering::SeqCst)
    }
    let short = checks(std::time::Duration::from_secs(1)).await;
    let long = checks(std::time::Duration::from_secs(100)).await;
    assert_eq!(short, long, "checks per await, 1 s against 100 s");
}

/// A signal raised while the work is pending ends the await at once.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_raised_signal_ends_a_pending_await() {
    struct Signalled(octocode_io::cancellation::Cancellation);
    impl crate::execution::cancel::CancellationCheck for Signalled {
        fn check(&self) -> Result<(), String> {
            if self.0.is_cancelled() {
                Err("cancelled".into())
            } else {
                Ok(())
            }
        }
        fn signal(&self) -> Option<&octocode_io::cancellation::Cancellation> {
            Some(&self.0)
        }
    }
    let cancel = Signalled(octocode_io::cancellation::Cancellation::new());
    let raise = cancel.0.clone();
    let started = tokio::time::Instant::now();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        raise.cancel();
    });
    let result = cancellable(&cancel, std::future::pending::<()>()).await;
    assert_eq!(result.expect_err("cancelled").code, ErrorCode::Cancelled);
    assert_eq!(started.elapsed(), std::time::Duration::from_secs(1));
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

/// Roots of one symbol in two files (clangd prepares the declaration in a
/// header and the definition in the source) that call one callee. An
/// outgoing call's sites lie in its root's file, so each file keeps its own
/// row with its own sites; two roots in one file are one row.
#[tokio::test(flavor = "current_thread")]
async fn one_callee_of_roots_in_two_files_keeps_a_row_per_file() {
    struct Callees {
        callee: serde_json::Value,
    }
    impl HierarchySource for Callees {
        async fn expand(
            &self,
            _: Expansion,
            item: serde_json::Value,
        ) -> Result<serde_json::Value, LspFailure> {
            // One call per root, on the line after the root's own line.
            let line = item["range"]["start"]["line"].as_u64().unwrap_or(0) + 1;
            Ok(serde_json::json!([{
                "to": self.callee,
                "fromRanges": [{"start": {"line": line, "character": 2}, "end": {"line": line, "character": 8}}]
            }]))
        }
    }
    let (_dir, root, paths) = temp_workspace();
    let uri_of = |name: &str| {
        let file = root.join(name);
        std::fs::write(&file, "// c\n").expect("file");
        octocode_engine::lsp::uri::path_to_uri(&file.to_string_lossy()).expect("uri")
    };
    let (header, source, other) = (uri_of("a.h"), uri_of("a.c"), uri_of("b.c"));
    let roots = [
        node_at("helper", Some(&header)),
        node_at("helper", Some(&source)),
        node_at("helper_inline", Some(&source)),
    ];
    let callees = Callees {
        callee: node_at("callee", Some(&other)),
    };
    let walk = walk_hierarchy(
        &callees,
        &roots,
        Expansion::OutgoingCalls,
        1,
        &paths,
        &crate::execution::cancel::NeverCancel,
    )
    .await
    .expect("walk");
    let mut rows = walk
        .edges
        .iter()
        .map(|edge| public_edge(Expansion::OutgoingCalls, edge, None))
        .map(|row| {
            let path = row["callerPath"].as_str().unwrap_or_default().to_owned();
            let file = path.rsplit('/').next().unwrap_or_default().to_owned();
            (file, row["fromRanges"].as_array().map_or(0, Vec::len))
        })
        .collect::<Vec<_>>();
    rows.sort();
    assert_eq!(rows, [("a.c".to_owned(), 2), ("a.h".to_owned(), 1)]);
}
