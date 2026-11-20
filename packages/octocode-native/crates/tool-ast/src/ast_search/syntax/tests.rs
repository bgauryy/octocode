use super::*;
use octocode_runtime_base::execution::cancel::NeverCancel;
use octocode_runtime_base::test_support::Fixture;
use octocode_test_support::json_fixture;

/// The chunk the chunked walks below read in: small, so a walk past several
/// chunks is a small tree. Production reads [`NODE_CHUNK`] nodes at a time.
const TEST_CHUNK: u32 = 2_000;

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
fn plain_windows_cover_every_node_once_and_price_each_only_once() {
    let result = inspected(&"let item = 1;\n".repeat(50), NODE_CHUNK);
    let total = result.nodes.len();
    let mut prices = pricing::Prices::default();
    let before = pricing::priced_nodes();
    let windows = pricing::plain_windows(&result, &mut prices, 400, &NeverCancel).expect("windows");
    assert!(windows.len() > 1, "{windows:?}");
    assert_eq!(windows[0].start, 0);
    assert_eq!(windows[windows.len() - 1].end, total);
    assert!(windows.windows(2).all(|pair| pair[0].end == pair[1].start));
    assert_eq!(pricing::priced_nodes() - before, total);
    assert_eq!(
        pricing::plain_windows(&result, &mut prices, 400, &NeverCancel).expect("repeat"),
        windows
    );
    assert_eq!(
        pricing::priced_nodes() - before,
        total,
        "a later chunk reprices nothing"
    );
}

#[test]
fn tiny_windows_price_a_large_chunk_without_allocating_each_closure_again() {
    let text = "let item = 1;\n".repeat(TEST_CHUNK as usize / 4 + 100);
    let result = inspected(&text, TEST_CHUNK);
    assert_eq!(result.nodes.len(), TEST_CHUNK as usize);
    let stop = || false;
    let mut maps = super::super::source::mapping_budget(
        &octocode_runtime_base::execution::cancel::NeverCancel,
        &stop,
    );
    let admitted = octocode_engine::text::analysis::AdmittedSource::from_bytes(
        text.as_bytes(),
        "fixture.ts",
        &mut maps,
    )
    .expect("admitted");
    let mut prices = pricing::Prices::default();
    let windows = pricing::windows(&result, &mut prices, &admitted, &mut maps, 1, &NeverCancel)
        .expect("small atomic pages fit retained-state limits");
    assert_eq!(windows.len(), TEST_CHUNK as usize);
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
    let lines = 3 * TEST_CHUNK as usize / 4 + 100;
    let text = format!("{}function (", "let item = 1;\n".repeat(lines));
    root.write("recovered.ts", &text);
    let path = root.0.join("recovered.ts");
    let stop = || false;
    let mut maps = super::super::source::mapping_budget(
        &octocode_runtime_base::execution::cancel::NeverCancel,
        &stop,
    );
    let admitted =
        super::super::source::admitted(&path, text.as_bytes(), &mut maps).expect("admitted");
    let source = Source {
        admitted: &admitted,
        content: admitted.text(),
        path: &path,
        extension: None,
        node_chunk: TEST_CHUNK,
    };
    let budget = 131_072;
    let snapshot = path.to_string_lossy().into_owned();
    let mut page = 1usize;
    let mut previous = 0;
    let before = pricing::priced_nodes();
    loop {
        let query: AstSearchQuerySyntaxTree = serde_json::from_value(json!({
            "operation":"syntaxTree", "path":path, "page":page, "includeProvenance":true,
        }))
        .expect("query");
        let mut maps = super::super::source::mapping_budget(
            &octocode_runtime_base::execution::cancel::NeverCancel,
            &stop,
        );
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
            assert!(walked.r.total_nodes > 3 * TEST_CHUNK);
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
                "operation":"syntaxTree", "path":path, "page":expected_windows.len(), "includeProvenance":true,
            }))
            .expect("late query");
            WALKS.evict(&snapshot);
            let priced_before = pricing::priced_nodes();
            let mut maps = super::super::source::mapping_budget(
                &octocode_runtime_base::execution::cancel::NeverCancel,
                &stop,
            );
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

/// Every rendered page of `text`'s syntax tree, read at `budget` characters.
fn rendered_pages(text: &str, budget: usize) -> Vec<Value> {
    let root = Fixture::new();
    root.write("tree.py", text);
    let path = root.0.join("tree.py");
    let stop = || false;
    let mut maps = super::super::source::mapping_budget(&NeverCancel, &stop);
    let admitted =
        super::super::source::admitted(&path, text.as_bytes(), &mut maps).expect("admitted");
    let source = Source {
        admitted: &admitted,
        content: admitted.text(),
        path: &path,
        extension: None,
        node_chunk: TEST_CHUNK,
    };
    let snapshot = path.to_string_lossy().into_owned();
    let mut pages = Vec::new();
    loop {
        let query: AstSearchQuerySyntaxTree = serde_json::from_value(json!({
            "operation":"syntaxTree", "path":path, "page":pages.len() + 1,
        }))
        .expect("query");
        let mut maps = super::super::source::mapping_budget(&NeverCancel, &stop);
        let page =
            read_page(&query, &source, budget, &snapshot, &mut maps, &NeverCancel).expect("page");
        let more = page.more;
        pages.push(render_tree_page(&query, "tree.py", &page, &snapshot));
        if !more {
            return pages;
        }
    }
}

/// A recovered parse walks the whole tree: the row is partial for
/// `parseRecovered`, stated as that reason alone. No limit ended the
/// listing, and no diagnostic row repeats the reason.
#[test]
fn a_recovered_parse_is_partial_once_and_no_terminal_limit() {
    let pages = rendered_pages(
        "def f(x):\n    return x + (\n\ndef g(y):\n    return y(1)\n",
        131_072,
    );
    let [row] = pages.as_slice() else {
        panic!("one page: {pages:?}");
    };
    assert_eq!(row["isPartial"], true, "{row}");
    assert_eq!(
        row["partialReasons"],
        json_fixture(r#"["parseRecovered"]"#),
        "{row}"
    );
    assert_eq!(row.get("terminalLimit"), None, "{row}");
    assert_eq!(row.get("diagnostics"), None, "{row}");
    assert_eq!(row.get("warnings"), None, "{row}");
    assert_eq!(row.get("calls"), None, "{row}");
}

/// Across pages every page names the recovery: a page read alone says why
/// it is partial, and no page says `isPartial` without a reason.
#[test]
fn a_recovered_parse_over_several_pages_states_its_reason_on_every_page() {
    let text = format!("{}def broken(:\n", "value = 1\n".repeat(200));
    let pages = rendered_pages(&text, 2_000);
    assert!(pages.len() > 2, "{}", pages.len());
    let last = pages.len() - 1;
    for (at, row) in pages.iter().enumerate() {
        assert_eq!(row["isPartial"], true, "page {at}: {row}");
        assert_eq!(row.get("terminalLimit"), None, "page {at}: {row}");
        assert_eq!(row.get("diagnostics"), None, "page {at}: {row}");
        assert_eq!(
            row["partialReasons"],
            json_fixture(r#"["parseRecovered"]"#),
            "page {at}: {row}"
        );
        assert_eq!(row["calls"]["nextPage"].is_object(), at != last, "{row}");
    }
}

/// A whole tree over several pages is pagination, not partial coverage:
/// no page says `isPartial` beside `hasMore`; page 1 alone names the tree's
/// size and how to read part of it (`line`), and every node comes once.
#[test]
fn a_whole_tree_over_pages_is_never_partial_and_page_one_names_line() {
    let text = "value = 1\n".repeat(200);
    let pages = rendered_pages(&text, 2_000);
    assert!(pages.len() > 2, "{}", pages.len());
    let mut ids = std::collections::BTreeSet::new();
    let total = pages[0]["pagination"]["total"].as_u64().expect("a total") as usize;
    for (at, row) in pages.iter().enumerate() {
        assert!(
            row.get("isPartial") != Some(&json!(true)),
            "page {at}: {row}"
        );
        assert!(row.get("partialReasons").is_none(), "page {at}: {row}");
        let notices = row["notices"].as_array().map_or(0, Vec::len);
        let names_line = row["notices"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .any(|notice| notice.contains(&total.to_string()) && notice.contains("line"));
        assert_eq!(names_line, at == 0, "page {at}: {row}");
        assert_eq!(notices, usize::from(at == 0), "page {at}: {row}");
        for node in row["nodes"].as_array().expect("nodes") {
            let id = node["node"].as_str().expect("node row");
            assert!(ids.insert(id.to_owned()), "a node repeats: {id}");
        }
    }
    assert_eq!(ids.len(), total);
    // A tree that fits one page names nothing.
    let [one] = rendered_pages("value = 1\n", 131_072)
        .try_into()
        .expect("one page");
    assert!(one.get("notices").is_none(), "{one}");
}

/// A walk the deadline stopped is a terminal limit: nodes past it are on no
/// page, and its diagnostic keeps the recovery that reaches them.
#[test]
fn a_walk_cut_by_its_deadline_stays_a_terminal_limit_with_its_recovery() {
    let root = Fixture::new();
    let text = "def f(x):\n    return x + (\n";
    root.write("tree.py", text);
    let path = root.0.join("tree.py");
    let stop = || false;
    let mut maps = super::super::source::mapping_budget(&NeverCancel, &stop);
    let admitted =
        super::super::source::admitted(&path, text.as_bytes(), &mut maps).expect("admitted");
    let source = Source {
        admitted: &admitted,
        content: admitted.text(),
        path: &path,
        extension: None,
        node_chunk: TEST_CHUNK,
    };
    let snapshot = format!("{}#deadline", path.to_string_lossy());
    let query: AstSearchQuerySyntaxTree =
        serde_json::from_value(json!({"operation":"syntaxTree", "path":path})).expect("query");
    let mut page =
        read_page(&query, &source, 131_072, &snapshot, &mut maps, &NeverCancel).expect("page");
    for diagnostic in &mut page.r.diagnostics {
        diagnostic.code = "syntaxTree.budget.deadline".into();
    }
    let row = render_tree_page(&query, "tree.py", &page, &snapshot);
    assert_eq!(
        row["partialReasons"],
        json_fixture(r#"["deadline"]"#),
        "{row}"
    );
    assert_eq!(row["terminalLimit"], true, "{row}");
    let [diagnostic] = row["diagnostics"]
        .as_array()
        .expect("diagnostics")
        .as_slice()
    else {
        panic!("one diagnostic: {row}");
    };
    assert!(diagnostic["recovery"].is_string(), "{row}");
}
