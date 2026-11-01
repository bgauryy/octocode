use super::*;
use crate::tools::ast_graph::store::format::NodeRec;
use crate::tools::ast_graph::test_support::next_offset;

fn wide_graph(leaves: u32) -> Graph {
    let mut tables = GraphTables::default();
    for id in 0..=leaves {
        tables.strings.push(format!("file-{id}.ts"));
        tables.nodes.push(NodeRec {
            kind: NodeKind::File,
            flags: 0,
            key: id,
            name: id,
            detail: NONE,
            file: id,
            parent: NONE,
            line: NONE,
            end_line: NONE,
        });
        if id > 0 {
            tables.edges.push(EdgeRec {
                src: 0,
                dst: id,
                kind: EdgeKind::Imports,
                confidence: Confidence::High,
                detail: NONE,
                line: NONE,
                test: false,
            });
        }
    }
    tables.index();
    Graph {
        id: "wide-snapshot".into(),
        manifest: json!({"root":"/fixture", "scan":{}}),
        t: tables,
        ..Default::default()
    }
}

#[test]
fn traversal_pages_reach_every_node_beyond_the_former_visit_cap() {
    let graph = wide_graph(200_002);
    let mut options = QueryOptions {
        op: "deps".into(),
        target: Some("file-0.ts".into()),
        offset: Some(200_000),
        limit: Some(1),
        ..Default::default()
    };
    for (offset, expected) in [
        (199_999, "file-200000.ts"),
        (200_000, "file-200001.ts"),
        (200_001, "file-200002.ts"),
    ] {
        options.offset = Some(offset);
        let answer = Query {
            cancel: &crate::execution::cancel::NeverCancel,
            flags: Flags::parse(&options).unwrap(),
            options: &options,
            g: &graph,
        }
        .traversal()
        .unwrap();
        assert_eq!(answer.class, ExitClass::Success, "{}", answer.value);
        assert_eq!(answer.value["pagination"]["totalItems"], 200_002);
        assert_eq!(answer.value["rows"].as_array().unwrap().len(), 1);
        assert_eq!(answer.value["rows"][0]["id"], expected);
        if offset < 200_001 {
            assert_eq!(next_offset(&answer), Some(offset + 1));
        } else {
            assert!(answer.next.is_empty());
            assert_eq!(answer.value["pagination"]["hasMore"], false);
        }
    }
}

#[test]
fn traversal_windows_preserve_order_evidence_cycles_and_absolute_pagination() {
    let mut graph = wide_graph(5);
    for (src, dst) in [(1, 0), (2, 2), (3, 4)] {
        graph.t.edges.push(EdgeRec {
            src,
            dst,
            kind: EdgeKind::Imports,
            confidence: Confidence::High,
            detail: NONE,
            line: 42,
            test: false,
        });
    }
    graph.t.index();
    let filter = EdgeFilter {
        kinds: vec![EdgeKind::Imports],
        weakest: Confidence::High,
        tests: true,
    };
    let mut how = Walk {
        direction: Direction::Out,
        filter: &filter,
        depth: 2,
        kind: None,
        include_seeds: true,
        reexports: false,
        window: 0..usize::MAX,
    };
    let expected = walk(&graph, &[0], &how, &crate::execution::cancel::NeverCancel).unwrap();
    assert_eq!(expected.total, 6);
    assert_eq!(expected.rows[5]["id"], "file-0.ts");
    assert_eq!(expected.rows[5]["depth"], 2);
    assert_eq!(expected.rows[5]["edgeLine"], 42);
    let mut actual = Vec::new();
    for offset in [0, 2, 4, 6, 20] {
        how.window = offset..offset + 2;
        let page = walk(&graph, &[0], &how, &crate::execution::cancel::NeverCancel).unwrap();
        assert_eq!(page.total, 6);
        assert!(page.rows.len() <= 2);
        assert_eq!(page.start, offset);
        let options = QueryOptions {
            op: "walk".into(),
            target: Some("file-0.ts".into()),
            offset: Some(offset),
            limit: Some(2),
            ..Default::default()
        };
        let answer = run_page(&options, &graph, Map::new(), page);
        actual.extend(answer.value["rows"].as_array().unwrap().iter().cloned());
        if offset < 4 {
            assert_eq!(next_offset(&answer), Some(offset + 2));
        } else {
            assert!(answer.next.is_empty());
        }
    }
    assert_eq!(actual, expected.rows);
}

#[test]
fn filtered_traversal_does_not_hide_matches_after_many_nonmatching_nodes() {
    let mut graph = wide_graph(200_002);
    graph.t.nodes[200_001].kind = NodeKind::Package;
    graph.t.nodes[200_002].kind = NodeKind::Package;
    let page = walk(
        &graph,
        &[0],
        &Walk {
            direction: Direction::Out,
            filter: &EdgeFilter {
                kinds: vec![EdgeKind::Imports],
                weakest: Confidence::High,
                tests: true,
            },
            depth: 1,
            kind: Some(NodeKind::Package),
            include_seeds: false,
            reexports: false,
            window: 0..50,
        },
        &crate::execution::cancel::NeverCancel,
    )
    .unwrap();
    assert_eq!(page.total, 2);
    assert_eq!(page.rows[0]["id"], "file-200001.ts");
    assert_eq!(page.rows[1]["id"], "file-200002.ts");
}

#[test]
fn reingest_is_a_related_build_that_keeps_the_scan_options() {
    let input = |root: &str, scan: Value| {
        let call = reingest_call(&json!({"root": root, "workspace": "/w", "scan": scan}));
        assert_eq!((&call["kind"], &call["tool"]), (&json!("related"), &json!("graphBuild")));
        call["input"]["queries"][0].clone()
    };
    assert_eq!(input("/w/app", json!({})), json!({"path": "app"}));
    assert_eq!(
        input(
            "/w/app",
            json!({"excludeDir": ["gen"], "languageGlobs": {"cpp": ["include/**/*.h"]}})
        ),
        json!({"path": "app", "exclude": ["gen"], "parserGlobs": {"cpp": ["include/**/*.h"]}})
    );
    assert_eq!(input("/w", json!({})), json!({"path": "."}));
}
