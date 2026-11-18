//! Hierarchy walks over a fake graph: order, levels, cycles, failures
//! and cancellation.

use super::*;
use octocode_test_support::json_fixture;

/// A policy whose workspace is `/repo`, the root the fake rows live in.
fn repo_paths() -> octocode_runtime_base::policy::path::PathPolicy {
    use octocode_runtime_base::policy::path::{PathPolicy, PathPolicyConfig};
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

    let q = query(json_fixture(
        r#"{
            "operation": "callers",
            "path": "file:///repo/a.ts",
            "symbolName": "foo",
            "line": 1,
            "depth": 3
        }"#,
    ));
    let (items, failures) = expansion_outcome(
        vec![json_fixture(r#"{"from": {"symbolName": "caller"}}"#)],
        vec![expansion_error("LSP error: deeper level")],
    )
    .expect("partial expansion keeps what was found");
    let mut row = items_payload(&q, "callers", serde_json::json!(items), TEST_BUDGET);
    mark_partial_expansion(&mut row, &q, &failures, &repo_paths());
    assert_eq!(row["isPartial"], true);
    assert_eq!(
        row["partialReasons"],
        json_fixture(r#"["callHierarchyExpansionFailed"]"#)
    );
    assert!(
        row["expansionFailures"][0]
            .as_str()
            .is_some_and(|w| w.contains("deeper level")),
        "{row}"
    );
    let retry =
        octocode_runtime_base::test_support::checks::related_call(&row, "lspSearch", |_| true);
    assert_eq!(
        retry["input"]["queries"][0],
        crate::lsp_search::failure::same_query_rerun(&q)
    );
    octocode_runtime_base::contracts::validate_output("lspSearch", &public_row(&row))
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
        json_fixture(r#"["callHierarchyExpansionFailed"]"#)
    );
    octocode_runtime_base::test_support::checks::assert_no_reason_meaning(&row);
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
    let q = query(json_fixture(
        r#"{
            "operation": "subtypes",
            "path": "file:///repo/a.ts",
            "symbolName": "Base",
            "line": 1
        }"#,
    ));
    let failure = LspFailure::unestablished(
        "No definition establishes the heritage relation at /repo/src/a.ts:4:10",
    );
    let mut row = items_payload(
        &q,
        "subtypes",
        json_fixture(r#"[{"name": "Child"}]"#),
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
    let q = query(json_fixture(
        r#"{
            "operation": "subtypes",
            "path": "file:///repo/a.ts",
            "symbolName": "Base",
            "line": 1
        }"#,
    ));
    let items = json_fixture(r#"[{"name": "Child"}]"#);
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
        json_fixture(r#"["typeHierarchyExpansionFailed"]"#)
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
    let retry =
        octocode_runtime_base::test_support::checks::related_call(&row, "lspSearch", |_| true);
    assert_eq!(
        retry["input"]["queries"][0],
        crate::lsp_search::failure::same_query_rerun(&q)
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
        json_fixture(r#"[{"line": 3, "column": 5}]"#)
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
        json_fixture(r#"[{"line": 3, "column": 5}, {"line": 4, "column": 5}]"#)
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
    impl octocode_runtime_base::execution::cancel::CancellationCheck for Cancelled {
        fn check(&self) -> Result<(), String> {
            Err("cancelled".into())
        }
    }
    use octocode_runtime_base::policy::path::{PathPolicy, PathPolicyConfig};
    let paths = PathPolicy::new(PathPolicyConfig::default()).expect("path policy");
    let graph = graph(&[("root", &["a"])]);
    let error = walk_from(&graph, node("root"), 2, &paths, &Cancelled)
        .await
        .expect_err("cancelled");
    assert_eq!(error.code, ErrorCode::Cancelled);
    assert_eq!(graph.requests.get(), 0);
}

/// A current-thread runtime with no timer driver: a future that arms a
/// timer (a polling interval, a sleep) panics on it.
fn timerless_runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("a runtime without timers")
}

/// An await is woken by the request's signal, never by a poll: it arms no
/// timer, and how often it checks the cancellation does not grow with how
/// long the work runs.
#[test]
fn an_await_checks_cancellation_without_polling() {
    struct Counting(std::sync::atomic::AtomicUsize);
    impl octocode_runtime_base::execution::cancel::CancellationCheck for Counting {
        fn check(&self) -> Result<(), String> {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }
    }
    /// Work that runs a while: pending until it has been woken `wakes` times.
    struct Working {
        wakes: usize,
    }
    impl std::future::Future for Working {
        type Output = ();
        fn poll(
            mut self: std::pin::Pin<&mut Self>,
            context: &mut std::task::Context<'_>,
        ) -> std::task::Poll<()> {
            if self.wakes == 0 {
                return std::task::Poll::Ready(());
            }
            self.wakes -= 1;
            context.waker().wake_by_ref();
            std::task::Poll::Pending
        }
    }
    async fn checks(wakes: usize) -> usize {
        let cancel = Counting(std::sync::atomic::AtomicUsize::new(0));
        cancellable(&cancel, Working { wakes })
            .await
            .expect("never cancelled");
        cancel.0.load(std::sync::atomic::Ordering::SeqCst)
    }
    let (short, long) =
        timerless_runtime().block_on(async { (checks(1).await, checks(100).await) });
    assert_eq!(short, long, "checks per await, 1 wake against 100");
}

/// A signal raised while the work is pending ends the await at once.
#[test]
fn a_raised_signal_ends_a_pending_await() {
    struct Signalled(octocode_io::cancellation::Cancellation);
    impl octocode_runtime_base::execution::cancel::CancellationCheck for Signalled {
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
    timerless_runtime().block_on(async {
        let cancel = Signalled(octocode_io::cancellation::Cancellation::new());
        let raise = cancel.0.clone();
        tokio::spawn(async move {
            tokio::task::yield_now().await;
            raise.cancel();
        });
        // Work that never ends, and a bound on how many turns the signal may
        // take to end it.
        let gave_up = async {
            for _ in 0..64 {
                tokio::task::yield_now().await;
            }
        };
        let result = tokio::select! {
            biased;
            result = cancellable(&cancel, std::future::pending::<()>()) => result,
            () = gave_up => panic!("the raised signal did not end the pending await"),
        };
        assert_eq!(result.expect_err("cancelled").code, ErrorCode::Cancelled);
    });
}

#[tokio::test(flavor = "current_thread")]
async fn ready_awaits_observe_cancellation_at_completion() {
    struct CancelAfterFirst(std::sync::atomic::AtomicUsize);
    impl octocode_runtime_base::execution::cancel::CancellationCheck for CancelAfterFirst {
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
            &octocode_runtime_base::execution::cancel::NeverCancel,
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
        &octocode_runtime_base::execution::cancel::NeverCancel,
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

/// A failure names a file as the rows do: from the workspace inside it, and
/// outside it from the root of its tree, which the row states once; never
/// by an absolute path the rows do not use.
#[test]
fn expansion_failures_name_files_outside_the_workspace_as_rows_do() {
    use octocode_runtime_base::policy::path::{PathPolicy, PathPolicyConfig};
    let workspace = tempfile::tempdir().expect("workspace");
    let outside = tempfile::tempdir().expect("outside");
    let (workspace, outside) = (
        workspace.path().canonicalize().expect("canonical"),
        outside.path().canonicalize().expect("canonical"),
    );
    let paths = PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(workspace.clone()),
        additional_roots: vec![outside.clone()],
        ..PathPolicyConfig::default()
    })
    .expect("path policy");
    let q = query(json_fixture(
        r#"{"operation": "subtypes", "path": "file:///repo/a.ts", "symbolName": "Base", "line": 1}"#,
    ));
    let failures = [
        expansion_error(&format!(
            "No parsed type declaration at {}:3:5",
            outside.join("crate/lib.rs").display()
        )),
        expansion_error(&format!(
            "Implementation candidate at '{}:1:10' resolves to 2 distinct type declarations",
            workspace.join("src/a.ts").display()
        )),
    ];
    let mut row = items_payload(&q, "subtypes", serde_json::json!([]), TEST_BUDGET);
    mark_partial_expansion(&mut row, &q, &failures, &paths);
    let text = row["expansionFailures"].to_string();
    assert!(
        text.contains("No parsed type declaration at crate/lib.rs:3:5"),
        "{text}"
    );
    assert!(text.contains("at 'src/a.ts:1:10' resolves"), "{text}");
    assert!(!text.contains(&*outside.to_string_lossy()), "{text}");
}
