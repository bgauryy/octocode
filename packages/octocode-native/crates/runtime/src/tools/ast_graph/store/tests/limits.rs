//! Snapshot bounds refuse a build as soon as its snapshot cannot publish:
//! at scan, at link, or on the projection before redaction. A build exactly
//! at a bound publishes.
use super::super::ingest::{REDACTIONS, Scan, build_for_ingest, project_graph};
use super::*;
use crate::tools::ast_graph::limits::{LIMITS, Limits, Tally};
use octocode_engine::signatures::languages::RegistryView;

fn options(dir: &Path) -> IngestOptions {
    IngestOptions {
        path: dir.join("app"),
        workspace: Some(dir.to_path_buf()),
        refresh: true,
        ..Default::default()
    }
}

fn ingest_within(dir: &Path, limits: &Limits) -> GraphOutput {
    super::super::ingest::ingest_with_registry(
        &options(dir),
        &workspace_policy(dir),
        &ContentSecurity::new(),
        &NeverCancel,
        None,
        RegistryView::installed(),
        limits,
    )
}

fn edges(edges: usize) -> Limits {
    Limits { edges, ..LIMITS }
}

fn redactions() -> usize {
    REDACTIONS.with(std::cell::Cell::get)
}

/// `graph.bin` files under the workspace's graph home, staged or published.
fn graph_files(dir: &Path) -> usize {
    fn walk(path: &Path) -> usize {
        std::fs::read_dir(path).map_or(0, |entries| {
            entries
                .flatten()
                .map(|entry| {
                    let path = entry.path();
                    if path.is_dir() {
                        walk(&path)
                    } else {
                        usize::from(entry.file_name() == GRAPH_FILE)
                    }
                })
                .sum()
        })
    }
    walk(&dir.join(".octocode/graph"))
}

/// A refusal: `scopeTooBroad` naming narrower scopes, nothing redacted and
/// nothing written.
fn assert_refused(dir: &Path, out: &GraphOutput, redacted: usize) -> String {
    assert_eq!(
        out.value["errorCode"],
        json!(ErrorCode::ScopeTooBroad),
        "{}",
        out.value
    );
    assert!(
        out.value["hints"]
            .as_array()
            .is_some_and(|hints| !hints.is_empty()),
        "{}",
        out.value
    );
    assert_eq!(redactions(), redacted, "refused before redaction");
    assert_eq!(graph_files(dir), 0, "no snapshot is staged");
    out.value["error"].as_str().unwrap_or_default().to_owned()
}

/// A bound exactly at the projected EDGE rows publishes; one below refuses.
fn assert_edge_bound_is_exact(dir: &Path) {
    let whole = ingest_within(dir, &LIMITS);
    assert_eq!(whole.class, ExitClass::Success, "{}", whole.value);
    let rows = whole.value["counts"]["edges"].as_u64().expect("edges") as usize;
    let at = ingest_within(dir, &edges(rows));
    assert_eq!(at.class, ExitClass::Success, "{}", at.value);
    assert_eq!(at.value["counts"], whole.value["counts"]);
    let below = ingest_within(dir, &edges(rows - 1));
    assert_eq!(
        below.value["errorCode"],
        json!(ErrorCode::ScopeTooBroad),
        "{}",
        below.value
    );
}

#[test]
fn an_edge_bound_trips_during_the_scan() {
    let files = (0..12)
        .map(|index| {
            (
                format!("{}/m{index}.ts", if index % 2 == 0 { "a" } else { "b" }),
                "export function f() {}\nexport function g() {}\n",
            )
        })
        .collect::<Vec<_>>();
    let files = files
        .iter()
        .map(|(path, text)| (path.as_str(), *text))
        .collect::<Vec<_>>();
    let dir = git_app(&files);
    let redacted = redactions();
    let out = ingest_within(dir.0.as_path(), &edges(3));
    let message = assert_refused(dir.0.as_path(), &out, redacted);
    assert!(message.contains("EDGE"), "{message}");
    // The scan stopped once the refusal was certain, before every file.
    let charged = message
        .strip_prefix("the first ")
        .and_then(|rest| rest.split(' ').next())
        .and_then(|count| count.parse::<usize>().ok())
        .expect("charged file count");
    assert!(message.contains("of 12 files"), "{message}");
    assert!(charged < 12, "{message}");
}

#[test]
fn an_edge_bound_trips_while_linking() {
    let leaves = ["a", "b", "c", "d", "e"];
    let mut files = leaves
        .iter()
        .map(|leaf| (format!("{leaf}.ts"), "console.log(1);\n".to_owned()))
        .collect::<Vec<_>>();
    files.push((
        "hub.ts".into(),
        leaves
            .iter()
            .map(|leaf| format!("import './{leaf}';\n"))
            .collect(),
    ));
    let files = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect::<Vec<_>>();
    let dir = git_app(&files);
    let redacted = redactions();
    let out = ingest_within(dir.0.as_path(), &edges(4));
    let message = assert_refused(dir.0.as_path(), &out, redacted);
    assert!(message.contains("linking"), "{message}");
    assert!(message.contains("4 EDGE rows"), "{message}");
    assert_edge_bound_is_exact(dir.0.as_path());
}

/// A file naming its one dependency many times is one edge. The tally is
/// asserted directly: the projected EDGE rows also hold `uses` edges the
/// tally never charges, so a bound on them alone cannot see a repeat.
#[test]
fn repeated_references_to_one_target_count_once() {
    let dir = git_app(&[
        (
            "a.ts",
            "import { b } from './b'\nimport { c } from './b'\nexport const a = b + c\n",
        ),
        ("b.ts", "export const b = 1\nexport const c = 2\n"),
    ]);
    let dir = dir.0.as_path();
    let built = build(dir, &workspace_policy(dir), &ContentSecurity::new());
    // Two file nodes and three symbols; three `contains` and one import.
    assert_eq!(built.tally, Tally { nodes: 5, edges: 4 });
    // With no `uses` edges the one import is the only EDGE row, so a
    // one-row bound publishes only when the repeats count once.
    let dir = git_app(&[
        ("a.ts", "import './b';\nimport './b';\nimport './b';\n"),
        ("b.ts", "console.log(1);\n"),
    ]);
    let out = ingest_within(dir.0.as_path(), &edges(1));
    assert_eq!(out.class, ExitClass::Success, "{}", out.value);
    assert_eq!(out.value["counts"]["edges"], json!(1), "{}", out.value);
}

#[test]
fn a_build_exactly_at_the_edge_bound_publishes() {
    let dir = fixture();
    assert_edge_bound_is_exact(dir.0.as_path());
}

/// Redaction rewrites strings, never rows: a projection past a table bound
/// is refused before that pass. (The scan charges source items against the
/// same row bound, so the build runs under the default bounds here.)
#[test]
fn row_counts_are_checked_before_redaction() {
    let dir = git_app(&[
        ("a.ts", "import './b';\nimport './b';\nimport './b';\n"),
        ("b.ts", "console.log(1);\n"),
    ]);
    let dir = dir.0.as_path();
    let (paths, security) = (workspace_policy(dir), ContentSecurity::new());
    let built = build(dir, &paths, &security);
    let limits = Limits {
        table_items: 2,
        ..LIMITS
    };
    let redacted = redactions();
    let Err(out) = project_graph(&built, &paths, &security, &NeverCancel, &limits) else {
        panic!("three witnesses pass a two-row bound");
    };
    assert_eq!(out.value["errorCode"], json!(ErrorCode::ScopeTooBroad));
    let message = out.value["error"].as_str().unwrap_or_default();
    assert!(message.contains("WITN"), "{message}");
    assert_eq!(redactions(), redacted, "refused before redaction");
    let Ok(_) = project_graph(&built, &paths, &security, &NeverCancel, &LIMITS) else {
        panic!("the default bounds hold it");
    };
    assert_eq!(redactions(), redacted + 1);
}

fn build(
    dir: &Path,
    paths: &PathPolicy,
    security: &ContentSecurity,
) -> crate::tools::ast_graph::types::BuiltGraph {
    let scan = Scan {
        cargo: None,
        registry: RegistryView::installed(),
        limits: &LIMITS,
    };
    let Ok(built) = build_for_ingest(&options(dir), paths, security, &NeverCancel, &scan) else {
        panic!("build");
    };
    built
}

/// What a build tallies is a lower bound of the rows it projects, so a
/// tally past a bound never refuses a snapshot that would publish.
#[test]
fn the_build_tally_is_a_lower_bound_of_the_projection() {
    let fixtures = [
        fixture(),
        issues_fixture(),
        git_app(&[
            ("lib.rs", "mod util;\npub fn run() { util::help(); }\n"),
            ("util.rs", "pub fn help() {}\n"),
            ("main.py", "from pkg import thing\nthing()\n"),
            ("pkg/__init__.py", "def thing():\n    pass\n"),
            (
                "Main.java",
                "class Main { void go() { new Main().go(); } }\n",
            ),
        ]),
    ];
    for dir in fixtures {
        let dir = dir.0.as_path();
        let (paths, security) = (workspace_policy(dir), ContentSecurity::new());
        let built = build(dir, &paths, &security);
        let Ok(projection) = project_graph(&built, &paths, &security, &NeverCancel, &LIMITS) else {
            panic!("projection");
        };
        let tables = &projection.tables;
        assert!(
            built.tally.nodes > 0 && built.tally.edges > 0,
            "{:?}",
            built.tally
        );
        assert!(built.tally.nodes <= tables.nodes.len(), "{:?}", built.tally);
        assert!(built.tally.edges <= tables.edges.len(), "{:?}", built.tally);
    }
}
