use super::super::format::NodeRec;
use super::*;
use crate::execution::cancel::{CancellationCheck, NeverCancel};
use std::sync::atomic::{AtomicUsize, Ordering};

fn graph(names: &[(&str, &str, NodeKind, u32)]) -> Graph {
    let mut t = GraphTables::default();
    t.strings.push("function");
    for (key, name, kind, line) in names {
        let key_id = t.strings.len() as u32;
        t.strings.extend([key.to_string(), name.to_string()]);
        let id = t.nodes.len() as u32;
        t.nodes.push(NodeRec {
            kind: *kind,
            flags: 0,
            key: key_id,
            name: key_id + 1,
            detail: 0,
            file: if *kind == NodeKind::File { id } else { 0 },
            parent: NONE,
            line: *line,
            end_line: *line,
        });
    }
    t.index();
    Graph {
        id: "window-fixture".into(),
        manifest: json!({"root":"/fixture","scan":{}}),
        t,
        ..Default::default()
    }
}

fn ids(page: &Page) -> Vec<&str> {
    page.rows
        .iter()
        .map(|row| row["id"].as_str().unwrap())
        .collect()
}

#[test]
fn find_windows_keep_rank_unicode_kind_and_exact_totals() {
    let g = graph(&[
        ("path-é.ts", "unrelated", NodeKind::File, 1),
        ("a.ts#é", "é", NodeKind::Symbol, 1),
        ("b.ts#É", "É", NodeKind::Symbol, 1),
        ("c.ts#éz", "éz", NodeKind::Symbol, 1),
        ("d.ts#éa", "éa", NodeKind::Symbol, 1),
        ("e.ts#xé", "xé", NodeKind::Symbol, 1),
    ]);
    // Word hits (name words, then the path word `é`) rank above the inner
    // substring `xé`; equal strengths order by key.
    let expected = [
        "a.ts#é",
        "b.ts#É",
        "c.ts#éz",
        "d.ts#éa",
        "path-é.ts",
        "e.ts#xé",
    ];
    let full = find(&g, "É", None, 0..100, &NeverCancel).unwrap();
    assert_eq!(ids(&full), expected);
    assert_eq!(
        full.rows
            .iter()
            .map(|r| r["match"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["exact", "exact", "prefix", "prefix", "words", "contains"]
    );
    let mut union = Vec::new();
    for start in (0..8).step_by(2) {
        let page = find(&g, "É", None, start..start + 2, &NeverCancel).unwrap();
        assert_eq!(page.start, start);
        assert_eq!(page.total, 6);
        assert!(page.rows.len() <= 2);
        union.extend(page.rows);
    }
    assert_eq!(union, full.rows);
    let files = find(&g, "é", Some(NodeKind::File), 0..1, &NeverCancel).unwrap();
    assert_eq!(files.total, 1);
    assert_eq!(ids(&files), ["path-é.ts"]);
}

fn edge(src: u32, dst: u32, kind: EdgeKind) -> EdgeRec {
    EdgeRec {
        src,
        dst,
        kind,
        confidence: Confidence::High,
        detail: NONE,
        line: 1,
        test: false,
    }
}

#[test]
fn symbol_windows_keep_preorder_depth_and_same_line_ties() {
    let mut g = graph(&[
        ("a.ts", "a.ts", NodeKind::File, 1),
        ("a.ts#late", "late", NodeKind::Symbol, 10),
        ("a.ts#early", "early", NodeKind::Symbol, 2),
        ("a.ts#child", "child", NodeKind::Symbol, 12),
        ("a.ts#tie", "tie", NodeKind::Symbol, 2),
    ]);
    g.t.edges = vec![
        edge(0, 1, EdgeKind::Contains),
        edge(0, 2, EdgeKind::Contains),
        edge(0, 4, EdgeKind::Contains),
        edge(1, 3, EdgeKind::Contains),
    ];
    g.t.index();
    let (_, full) = symbols(&g, &[0], 0..100, &NeverCancel).unwrap();
    assert_eq!(
        ids(&full),
        ["a.ts#early", "a.ts#tie", "a.ts#late", "a.ts#child"]
    );
    assert_eq!(
        full.rows
            .iter()
            .map(|r| r["depth"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        [1, 1, 1, 2]
    );
    let mut union = Vec::new();
    for start in 0..6 {
        let (_, page) = symbols(&g, &[0], start..start + 1, &NeverCancel).unwrap();
        assert_eq!(page.total, 4);
        assert!(page.rows.len() <= 1);
        union.extend(page.rows);
    }
    assert_eq!(union, full.rows);
}

#[test]
fn hub_windows_preserve_degree_order_and_key_ties() {
    let names = (0..12)
        .map(|i| (format!("file-{i:02}.ts"), format!("file-{i:02}.ts")))
        .collect::<Vec<_>>();
    let specs = names
        .iter()
        .map(|(k, n)| (k.as_str(), n.as_str(), NodeKind::File, 1))
        .collect::<Vec<_>>();
    let mut g = graph(&specs);
    g.t.edges = (1..12)
        .map(|id| edge(id, 0, EdgeKind::Imports))
        .chain((2..12).map(|id| edge(id, 1, EdgeKind::Imports)))
        .collect();
    g.t.edges.sort_by_key(|edge| (edge.src, edge.dst));
    g.t.index();
    let filter = EdgeFilter {
        kinds: vec![EdgeKind::Imports],
        weakest: Confidence::High,
        tests: false,
    };
    let incoming = hubs(
        &g,
        &filter,
        None,
        Direction::In,
        RankBy::Degree,
        0..100,
        &NeverCancel,
    )
    .unwrap();
    assert_eq!(ids(&incoming), ["file-00.ts", "file-01.ts"]);
    let full = hubs(
        &g,
        &filter,
        None,
        Direction::Out,
        RankBy::Degree,
        0..100,
        &NeverCancel,
    )
    .unwrap();
    assert_eq!(full.total, 11);
    assert_eq!(full.rows[0]["id"], "file-02.ts");
    assert_eq!(full.rows[10]["id"], "file-01.ts");
    let mut union = Vec::new();
    for start in (0..14).step_by(2) {
        let page = hubs(
            &g,
            &filter,
            None,
            Direction::Out,
            RankBy::Degree,
            start..start + 2,
            &NeverCancel,
        )
        .unwrap();
        assert_eq!(page.total, 11);
        assert!(page.rows.len() <= 2);
        union.extend(page.rows);
    }
    assert_eq!(union, full.rows);
    g.t.edges = (1..12).map(|id| edge(0, id, EdgeKind::Imports)).collect();
    g.t.index();
    let top = hubs(
        &g,
        &filter,
        Some(NodeKind::File),
        Direction::In,
        RankBy::Degree,
        0..5,
        &NeverCancel,
    )
    .unwrap();
    assert_eq!(top.rows.len(), 5);
    assert_eq!(top.rows[4]["id"], "file-05.ts");
}

/// `stats` states the snapshot's counts; its rankings are `hubs` rows, which
/// its `related` call lists against the same snapshot and path.
#[test]
fn stats_leads_to_hubs_instead_of_ranking() {
    let g = graph(&[("a.ts", "a.ts", NodeKind::File, 1)]);
    let options = QueryOptions {
        op: "stats".into(),
        path: Some("src".into()),
        ..Default::default()
    };
    let stats = stats(&g, &options).unwrap().value;
    for ranking in [
        "mostImportedFiles",
        "mostImportedPackages",
        "mostCalledSymbols",
    ] {
        assert!(stats.get(ranking).is_none(), "{ranking}: {stats}");
    }
    // Both rankings stay one ready call away: the most imported files and
    // packages (`hubs`, imports by default) and the most called symbols.
    let hubs = stats["related"]
        .as_array()
        .unwrap()
        .iter()
        .inspect(|lead| assert_eq!(lead["tool"], "graphQuery", "{stats}"))
        .flat_map(|lead| {
            lead["input"]["queries"]
                .as_array()
                .cloned()
                .unwrap_or_default()
        })
        .filter(|row| row["operation"] == "hubs")
        .collect::<Vec<_>>();
    assert_eq!(
        stats["related"][0]["input"]["queries"]
            .as_array()
            .map(Vec::len),
        Some(2),
        "one call: {stats}"
    );
    assert_eq!(
        hubs,
        vec![
            json!({"operation": "hubs", "path": "src", "graph": g.id}),
            json!({"operation": "hubs", "path": "src", "graph": g.id, "edges": ["calls"]}),
        ],
        "{stats}"
    );
}

/// A manifest with every section `stats` reads.
fn measured(mut g: Graph) -> Graph {
    g.manifest = json!({
        "octocodeVersion": "1.2.3",
        "scan": {"filesScanned": 1},
        "counts": {"nodes": 1, "edges": 0, "nodesByKind": {"file": 1}, "edgesByKind": {}},
        "imports": {"resolved": 0, "external": 0},
        "calls": {
            "sites": 4,
            "unresolved": 3,
            "byResolution": {"local": 1},
            "unresolvedByReason": {"external-package": 3},
        },
        "argumentCoverage": {"scope": "snapshot"},
        "linkage": {"linked": 1, "eligible": 1},
        "linkageByLanguage": {"typescript": {"linked": 1, "eligible": 1}},
        "extraction": {"parsedFiles": 1},
        "languages": [{"language": "typescript", "files": 1}],
        "diagnostics": {"total": 2, "byCode": {"unresolved-internal": 2}},
    });
    g
}

/// What the rankings behind the `hubs` leads read: stated by details only.
const STATS_RANKING_FACTS: [(&str, &str); 2] =
    [("/callConfidence", "high"), ("/testEdges", "excluded")];
const STATS_DETAILS_ONLY: [&str; 7] = [
    "/octocodeVersion",
    "/argumentCoverage",
    "/linkageByLanguage",
    "/extraction",
    "/calls/byResolution",
    "/calls/unresolvedByReason",
    "/diagnostics/byCode",
];
const STATS_SUMMARY: [&str; 9] = [
    "/graph",
    "/scan",
    "/counts/nodesByKind",
    "/imports",
    "/calls/sites",
    "/calls/unresolved",
    "/linkage",
    "/languages",
    "/diagnostics/total",
];

/// The default `stats` row holds the totals and leads once to the same call
/// with `view:"details"`, which states every breakdown the summary left out.
#[test]
fn stats_summary_leads_once_to_the_details_it_leaves_out() {
    let g = measured(graph(&[("a.ts", "a.ts", NodeKind::File, 1)]));
    let ask = |view: Option<&str>| {
        let options = QueryOptions {
            op: "stats".into(),
            path: Some("src".into()),
            view: view.map(str::to_owned),
            ..Default::default()
        };
        stats(&g, &options).unwrap().value
    };
    let summary = ask(None);
    assert_eq!(summary, ask(Some("summary")));
    for pointer in STATS_SUMMARY {
        assert!(summary.pointer(pointer).is_some(), "{pointer}: {summary}");
    }
    for pointer in STATS_DETAILS_ONLY
        .into_iter()
        .chain(STATS_RANKING_FACTS.map(|(pointer, _)| pointer))
    {
        assert!(summary.pointer(pointer).is_none(), "{pointer}: {summary}");
    }
    let leads = summary["related"].as_array().unwrap();
    let details_leads = leads
        .iter()
        .map(|lead| &lead["input"]["queries"][0])
        .filter(|row| row["operation"] == "stats")
        .collect::<Vec<_>>();
    assert_eq!(details_leads.len(), 1, "{summary}");
    assert_eq!(
        details_leads[0],
        &json!({"operation": "stats", "path": "src", "graph": g.id, "view": "details"})
    );

    let details = ask(Some("details"));
    for pointer in STATS_SUMMARY {
        assert_eq!(
            details.pointer(pointer),
            summary.pointer(pointer),
            "{pointer}"
        );
    }
    for pointer in STATS_DETAILS_ONLY {
        assert!(details.pointer(pointer).is_some(), "{pointer}: {details}");
        assert_eq!(
            details.pointer(pointer),
            g.manifest.pointer(pointer),
            "{pointer}"
        );
    }
    for (pointer, value) in STATS_RANKING_FACTS {
        assert_eq!(details.pointer(pointer), Some(&json!(value)), "{details}");
    }
    // The details row is the whole answer: it offers no further stats call.
    assert!(
        details["related"]
            .as_array()
            .unwrap()
            .iter()
            .all(|lead| lead["input"]["queries"][0]["operation"] == "hubs"),
        "{details}"
    );
}

struct CancelAfter(AtomicUsize);
impl CancellationCheck for CancelAfter {
    fn check(&self) -> Result<(), String> {
        if self.0.fetch_sub(1, Ordering::Relaxed) == 0 {
            Err("test cancelled".into())
        } else {
            Ok(())
        }
    }
}

#[test]
fn scans_cancel_inside_nonmatching_and_ranked_populations() {
    let names = (0..100).map(|i| format!("file-{i}.ts")).collect::<Vec<_>>();
    let g = graph(
        &names
            .iter()
            .map(|n| (n.as_str(), n.as_str(), NodeKind::File, 1))
            .collect::<Vec<_>>(),
    );
    let cancel = || CancelAfter(AtomicUsize::new(5));
    assert_eq!(
        find(&g, "missing", None, 0..1, &cancel())
            .err()
            .unwrap()
            .value["errorCode"],
        "cancelled"
    );
    let filter = EdgeFilter {
        kinds: vec![EdgeKind::Imports],
        weakest: Confidence::High,
        tests: false,
    };
    assert_eq!(
        hubs(
            &g,
            &filter,
            None,
            Direction::In,
            RankBy::Degree,
            0..1,
            &cancel()
        )
        .err()
        .unwrap()
        .value["errorCode"],
        "cancelled"
    );
    let options = QueryOptions {
        op: "candidates".into(),
        target: Some("missing".into()),
        ..Default::default()
    };
    let c = cancel();
    let query = Query {
        g: &g,
        options: &options,
        flags: Flags::parse(&options).unwrap(),
        cancel: &c,
        pending: Default::default(),
    };
    assert_eq!(
        query.candidates().err().unwrap().value["errorCode"],
        "cancelled"
    );
}

#[test]
fn window_never_projects_outside_requested_positions() {
    let mut window = Window::new(9_997..10_000);
    let mut projected = Vec::new();
    for id in 0..20_000 {
        window
            .try_push(|| {
                projected.push(id);
                Ok(json!(id))
            })
            .unwrap();
    }
    let page = window.finish();
    assert_eq!(page.total, 20_000);
    assert_eq!(page.start, 9_997);
    assert_eq!(projected, [9_997, 9_998, 9_999]);
    assert_eq!(
        page.rows,
        json!([9_997, 9_998, 9_999]).as_array().unwrap().clone()
    );
}

#[test]
fn exact_reference_tiers_and_large_windows_preserve_the_complete_population() {
    let names = (0..1203)
        .map(|id| format!("dir-{id:04}/same.ts#Owner.é"))
        .collect::<Vec<_>>();
    let specs = names
        .iter()
        .map(|key| (key.as_str(), "é", NodeKind::Symbol, 1))
        .collect::<Vec<_>>();
    let g = graph(&specs);
    for reference in ["é", " ././é ", "Owner.é", "#Owner.é"] {
        let matches = g
            .reference_candidates(reference, Some(NodeKind::Symbol), false, &NeverCancel)
            .unwrap();
        let mut union = Vec::new();
        for start in (0..1300).step_by(127) {
            let page = matches.page(start..start + 127, &NeverCancel).unwrap();
            assert_eq!(page.total, 1203);
            assert!(page.rows.len() <= 127);
            union.extend(
                page.rows
                    .into_iter()
                    .map(|row| row["id"].as_str().unwrap().to_owned()),
            );
        }
        assert_eq!(union, names);
    }
    let g = graph(&[
        ("a/entry.ts", "entry.ts", NodeKind::File, 1),
        ("b/entry.ts", "entry.ts", NodeKind::File, 1),
        ("a.ts#Owner.é", "é", NodeKind::Symbol, 1),
        ("a.ts#explicit", "Owner.é", NodeKind::Symbol, 1),
    ]);
    for (reference, expected) in [
        ("Owner.é", vec!["a.ts#explicit"]),
        ("#Owner.é", vec!["a.ts#Owner.é"]),
        ("entry.ts", vec!["a/entry.ts", "b/entry.ts"]),
        ("a/entry.ts", vec!["a/entry.ts"]),
        ("ENTRY.TS", vec![]),
    ] {
        let matches = g
            .reference_candidates(reference, None, false, &NeverCancel)
            .unwrap();
        let page = matches.page(0..100, &NeverCancel).unwrap();
        assert_eq!(ids(&page), expected, "{reference}");
    }
}

#[test]
fn symbol_scan_cancellation_never_returns_a_partial_total() {
    let names = (0..100)
        .map(|i| format!("a.ts#item{i}"))
        .collect::<Vec<_>>();
    let mut specs = vec![("a.ts", "a.ts", NodeKind::File, 1)];
    specs.extend(
        names
            .iter()
            .map(|n| (n.as_str(), n.as_str(), NodeKind::Symbol, 2)),
    );
    let mut g = graph(&specs);
    g.t.edges = (1..101).map(|id| edge(0, id, EdgeKind::Contains)).collect();
    g.t.index();
    assert_eq!(
        symbols(&g, &[0], 0..1, &CancelAfter(AtomicUsize::new(5)))
            .err()
            .unwrap()
            .value["errorCode"],
        "cancelled"
    );
}

mod budget;
