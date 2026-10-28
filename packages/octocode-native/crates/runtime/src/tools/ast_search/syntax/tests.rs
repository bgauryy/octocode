use super::*;
use crate::execution::cancel::NeverCancel;
use crate::tools::test_support::Fixture;

fn inspected(text: &str, limit: u32) -> octocode_engine::structural::SyntaxTreeInspectResult {
    octocode_engine::structural::inspect_with_extension(
        text,
        "fixture.ts",
        None,
        Some(SyntaxTreeInspectOptions {
            named_only: Some(true),
            node_limit: Some(limit),
            ..Default::default()
        }),
    )
}

#[test]
fn replays_preserve_the_first_census_and_reject_a_missing_numbered_window() {
    let full = inspected("let value = 1;\nlet other = 2;\n", 100);
    assert!(full.nodes.len() > 4);
    let mut first = full.clone();
    first.nodes.truncate(2);
    first.total_nodes = 4;
    first.next_offset = Some(2);
    first.status = "partial".into();
    let observed = view_snapshot("same-raw-source", &first);
    assert_ne!(
        observed,
        view_snapshot("same-raw-source", &full),
        "a cold changed census cannot silently continue"
    );
    let mut replay = full.clone();
    replay.nodes = full.nodes[2..4].to_vec();
    let mut missing = replay.clone();
    missing.nodes.pop();
    let error = append_observed_window(&mut first, missing, 2, 2).expect_err("missing evidence");
    assert_eq!(error.code, ErrorCode::ScopeTooBroad);
    assert_eq!(first.nodes.len(), 2);
    append_observed_window(&mut first, replay, 2, 2).expect("same observed window");
    assert_eq!(first.total_nodes, 4);
    assert_eq!(
        first.nodes.iter().map(|node| node.id).collect::<Vec<_>>(),
        vec![0, 1, 2, 3]
    );
    assert_eq!(first.next_offset, None);
    assert_eq!(first.status, "partial");
    assert_eq!(
        observed,
        view_snapshot("same-raw-source", &first),
        "materializing known rows preserves identity"
    );
}

#[test]
fn tiny_windows_price_a_large_chunk_without_allocating_each_closure_again() {
    let text = "let item = 1;\n".repeat(25_100);
    let result = inspected(&text, NODE_CHUNK);
    assert_eq!(result.nodes.len(), NODE_CHUNK as usize);
    let stop = || false;
    let mut maps = super::super::source::mapping_budget(&stop);
    let admitted = octocode_engine::text::analysis::AdmittedSource::from_bytes(
        text.as_bytes(),
        "fixture.ts",
        &mut maps,
    )
    .expect("admitted");
    let mut prices = pricing::Prices::default();
    let windows = pricing::windows(&result, &mut prices, &admitted, &mut maps, 1, &NeverCancel)
        .expect("small atomic pages fit retained-state limits");
    assert_eq!(windows.len(), NODE_CHUNK as usize);
    assert!(
        windows
            .iter()
            .enumerate()
            .all(|(at, window)| *window == (at..at + 1))
    );
}

#[test]
fn recovered_trees_continue_beyond_the_materialized_chunk_without_repricing_old_nodes() {
    let root = Fixture::new();
    let text = format!("{}function (", "let item = 1;\n".repeat(75_100));
    root.write("recovered.ts", &text);
    let path = root.0.join("recovered.ts");
    let stop = || false;
    let mut maps = super::super::source::mapping_budget(&stop);
    let admitted =
        super::super::source::admitted(&path, text.as_bytes(), &mut maps).expect("admitted");
    let source = Source {
        admitted: &admitted,
        content: admitted.text(),
        path: &path,
        extension: None,
    };
    let budget = 131_072;
    let snapshot = path.to_string_lossy().into_owned();
    let mut page = 1usize;
    let mut previous = 0;
    let before = pricing::priced_nodes();
    loop {
        let query: AstSearchQuerySyntaxTree = serde_json::from_value(json!({
            "operation":"syntaxTree", "path":path, "page":page,
        }))
        .expect("query");
        let mut maps = super::super::source::mapping_budget(&stop);
        read_page(&query, &source, budget, &snapshot, &mut maps, &NeverCancel)
            .expect("continued tree");
        let walked = WALKS
            .get(&snapshot, &budget.to_string(), std::sync::Arc::clone)
            .expect("bounded walk memo");
        assert_eq!(walked.r.status, "partial");
        assert!(walked.r.nodes.len() > previous);
        assert_eq!(
            pricing::priced_nodes() - before,
            walked.r.nodes.len(),
            "each newly read node priced once"
        );
        if walked.r.next_offset.is_none() {
            assert!(walked.r.total_nodes > 300_000);
            assert_eq!(walked.r.nodes.len(), walked.r.total_nodes as usize);
            assert!(
                walked
                    .r
                    .nodes
                    .iter()
                    .enumerate()
                    .all(|(at, node)| node.id == at as u32)
            );
            let indices = walked
                .windows
                .iter()
                .flat_map(Clone::clone)
                .collect::<Vec<_>>();
            assert_eq!(indices, (0..walked.r.nodes.len()).collect::<Vec<_>>());
            let expected = walked.r.nodes[walked.windows.last().expect("last window").clone()]
                .iter()
                .map(render::node_page_row)
                .collect::<Vec<_>>();
            let expected_windows = walked.windows.clone();
            let total = walked.r.total_nodes;
            let late: AstSearchQuerySyntaxTree = serde_json::from_value(json!({
                "operation":"syntaxTree", "path":path, "page":expected_windows.len(),
            }))
            .expect("late query");
            WALKS.evict(&snapshot);
            let priced_before = pricing::priced_nodes();
            let mut maps = super::super::source::mapping_budget(&stop);
            let cold = read_page(&late, &source, budget, &snapshot, &mut maps, &NeverCancel)
                .expect("cold late page");
            assert_eq!(
                cold.r
                    .nodes
                    .iter()
                    .map(render::node_page_row)
                    .collect::<Vec<_>>(),
                expected
            );
            assert_eq!(pricing::priced_nodes() - priced_before, total as usize);
            assert_eq!(
                WALKS
                    .get(&snapshot, &budget.to_string(), |walk| walk.windows.clone())
                    .expect("cold memo"),
                expected_windows
            );
            break;
        }
        previous = walked.r.nodes.len();
        page = walked.windows.len() + 1;
    }
}
