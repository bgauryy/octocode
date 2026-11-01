//! Query operations: answers, references, paging, flags and leads.
use super::*;
use crate::tools::ast_graph::test_support::{next_offset, next_query};

#[test]
fn queries_answer_imports_calls_cycles_and_packages() {
    let dir = fixture();
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let root = dir.0.as_path();

    let deps = ask(root, "deps", Some("src/main.ts"), |_| {});
    assert_eq!(
        ids(&deps),
        vec!["src/lib.ts", "pkg:react"],
        "{}",
        deps.value
    );

    let callers = ask(root, "callers", Some("greet"), |_| {});
    assert_eq!(ids(&callers), vec!["src/main.ts#main"], "{}", callers.value);
    assert_eq!(callers.value["rows"][0]["via"], "import");
    assert_eq!(callers.value["rows"][0]["confidence"], "high");

    let callees = ask(root, "callees", Some("src/lib.ts#greet"), |_| {});
    assert_eq!(ids(&callees), vec!["src/lib.ts#shout"]);
    assert_eq!(callees.value["rows"][0]["via"], "local");

    let member = ask(root, "callees", Some("Box.open"), |_| {});
    assert_eq!(
        ids(&member),
        vec!["src/lib.ts#Box.close"],
        "{}",
        member.value
    );

    let cycles = ask(root, "cycles", None, |_| {});
    assert_eq!(cycles.value["pagination"]["totalItems"], 1, "{}", cycles.value);
    assert_eq!(
        cycles.value["rows"][0]["nodes"],
        json!(["src/a.ts", "src/b.ts"])
    );

    let path = ask(root, "path", Some("src/main.ts"), |o| {
        o.to = Some("src/lib.ts".into());
    });
    assert_eq!(path.value["found"], true);
    assert_eq!(path.value["length"], 1);

    let react = ask(root, "dependents", Some("pkg:react"), |_| {});
    assert_eq!(ids(&react), vec!["src/main.ts"]);

    let outline = ask(root, "symbols", Some("src/lib.ts"), |_| {});
    assert_eq!(
        ids(&outline),
        vec![
            "src/lib.ts#greet",
            "src/lib.ts#shout",
            "src/lib.ts#Box",
            "src/lib.ts#Box.open",
            "src/lib.ts#Box.close"
        ]
    );

    let absolute = root.join("app/src/lib.ts");
    let by_path = ask(root, "node", Some(&absolute.to_string_lossy()), |_| {});
    assert_eq!(
        by_path.value["node"]["id"], "src/lib.ts",
        "{}",
        by_path.value
    );

    let found = ask(root, "find", Some("gre"), |o| {
        o.kind = Some("symbol".into());
    });
    assert_eq!(ids(&found), vec!["src/lib.ts#greet"]);
}

#[test]
fn references_report_ambiguity_and_absence_with_exit_codes() {
    let dir = fixture();
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let missing = ask(dir.0.as_path(), "node", Some("nope"), |_| {});
    assert_eq!(
        missing.class,
        ExitClass::Failed(crate::runtime::FailureKind::NotFound)
    );
    assert_eq!(missing.value["errorCode"], "notFound");

    let bad = ask(dir.0.as_path(), "explode", None, |_| {});
    assert_eq!(bad.class, ExitClass::InvalidInput);

    let none = ask(dir.0.as_path(), "stats", None, |o| {
        o.graph = Some("no-such-graph".into());
    });
    assert_eq!(
        none.class,
        ExitClass::Failed(crate::runtime::FailureKind::NotFound),
        "{}",
        none.value
    );
}

#[test]
fn list_operations_page_with_a_runnable_next_command() {
    let dir = fixture();
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let first = ask(dir.0.as_path(), "symbols", Some("src/lib.ts"), |o| {
        o.limit = Some(2);
    });
    assert_eq!(first.class, ExitClass::Success, "paging is success");
    assert_eq!(first.value["pagination"]["hasMore"], true);
    assert_eq!(next_offset(&first), Some(2), "{}", first.value);
    let second = ask(dir.0.as_path(), "symbols", Some("src/lib.ts"), |o| {
        o.limit = Some(2);
        o.offset = Some(2);
    });
    assert_eq!(ids(&second), vec!["src/lib.ts#Box", "src/lib.ts#Box.open"]);
}

#[test]
fn heritage_and_jsx_become_inherits_and_renders_edges() {
    let dir = fixture();
    let src = dir.0.as_path().join("app/src");
    std::fs::write(
        src.join("shapes.ts"),
        "import { Box } from './lib';\nexport interface Shape { area(): number }\nexport class Crate extends Box implements Shape {\n  area() { return 1; }\n}\n",
    )
    .expect("shapes");
    std::fs::write(
        src.join("view.tsx"),
        "import { Crate } from './shapes';\nexport function Card() { return null; }\nexport function Page() {\n  return <Card />;\n}\n",
    )
    .expect("view");
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let bases = ask(dir.0.as_path(), "deps", Some("Crate"), |o| {
        o.edges = vec!["inherits".into()];
    });
    let rows = bases.value["rows"].as_array().expect("rows");
    let got = rows
        .iter()
        .map(|r| {
            (
                r["id"].as_str().unwrap_or_default(),
                r["via"].as_str().unwrap_or_default(),
            )
        })
        .collect::<Vec<_>>();
    assert!(
        got.contains(&("src/lib.ts#Box", "extends:import")),
        "{got:?}"
    );
    assert!(
        got.contains(&("src/shapes.ts#Shape", "implements:local")),
        "{got:?}"
    );

    let renders = ask(
        dir.0.as_path(),
        "callers",
        Some("src/view.tsx#Card"),
        |_| {},
    );
    assert_eq!(
        ids(&renders),
        vec!["src/view.tsx#Page"],
        "{}",
        renders.value
    );
    assert_eq!(renders.value["rows"][0]["via"], "renders:local");
}

/// Flags outside their documented range are bad input (exit 2), never
/// silently clamped; a page past the last row is empty (exit 1).
#[test]
fn out_of_range_flags_are_rejected_and_a_page_past_the_end_is_empty() {
    let dir = fixture();
    let root = dir.0.as_path();
    assert_eq!(ingest_fixture(root, None).class, ExitClass::Success);
    // No depth bound but the graph's own: a deep walk is admitted.
    let deep = ask(root, "callers", Some("src/lib.ts#greet"), |o| {
        o.depth = Some(21);
    });
    assert_ne!(deep.class, ExitClass::InvalidInput, "{}", deep.value);
    // No limit bound either: pages and responseChars bound a page.
    let wide = ask(root, "callers", Some("src/lib.ts#greet"), |o| {
        o.limit = Some(1001);
    });
    assert_ne!(wide.class, ExitClass::InvalidInput, "{}", wide.value);
    for (depth, limit) in [(Some(0), None), (None, Some(0))] {
        let out = ask(root, "callers", Some("src/lib.ts#greet"), |o| {
            o.depth = depth;
            o.limit = limit;
        });
        assert_eq!(out.class, ExitClass::InvalidInput, "{}", out.value);
        assert!(
            out.value["error"]
                .as_str()
                .is_some_and(|error| error.contains("out of range")),
            "{}",
            out.value
        );
    }
    let past = ask(root, "symbols", Some("src/lib.ts"), |o| {
        o.offset = Some(500);
    });
    assert_eq!(past.class, ExitClass::Empty, "{}", past.value);
    assert!(past.value["pagination"]["totalItems"].as_u64().is_some_and(|total| total > 0));
}

/// An unknown `--edge` names every edge kind `--edge` accepts.
#[test]
fn unknown_edge_kind_lists_every_kind() {
    let dir = fixture();
    let root = dir.0.as_path();
    assert_eq!(ingest_fixture(root, None).class, ExitClass::Success);
    let out = ask(root, "walk", Some("src/lib.ts"), |o| {
        o.edges = vec!["bogus".into()];
    });
    assert_eq!(out.class, ExitClass::InvalidInput);
    let error = out.value["error"].as_str().unwrap_or_default();
    for kind in ["contains", "imports", "calls", "uses", "inherits"] {
        assert!(error.contains(kind), "{error}");
    }
}

/// Every lead runs against the snapshot that produced it: `node` hints and
/// `issues` verify steps pin its id, like `next`, and verify steps are tool
/// calls (a tool name and its query row), not prose.
#[test]
fn node_leads_and_verify_commands_are_pinned_to_the_snapshot() {
    let dir = issues_fixture();
    let root = dir.0.as_path();
    let ingested = ingest_fixture(root, None);
    let id = ingested.value["id"].as_str().expect("id").to_owned();
    let issues = ask(root, "issues", None, |o| o.limit = Some(200));
    let steps = issues.value["rows"]
        .as_array()
        .expect("rows")
        .iter()
        .flat_map(|row| row["verify"].as_array().cloned().unwrap_or_default())
        .collect::<Vec<_>>();
    assert!(!steps.is_empty(), "{}", issues.value);
    for step in &steps {
        // Each step is a runnable related call, not text.
        assert_eq!(step["kind"], "related", "{step}");
        let row = &step["input"]["queries"][0];
        if step["tool"] == "graphQuery" {
            assert_eq!(row["graph"], id, "{step}");
        } else {
            assert_eq!(step["tool"], "lspSearch", "{step}");
            assert!(row["path"].is_string(), "{step}");
        }
    }
}

/// An ambiguous reference lists the first candidates, says how many match,
/// and continues the exact candidate set after the delivered preview.
#[test]
fn ambiguous_reference_pages_every_candidate() {
    let dir = Fixture::new();
    std::fs::create_dir_all(dir.0.as_path().join(".git")).expect("git marker");
    let files = (0..25)
        .map(|index| {
            (
                format!("f{index}.ts"),
                "export function run() {}\n".to_owned(),
            )
        })
        .collect::<Vec<_>>();
    let files = files
        .iter()
        .map(|(name, text)| (name.as_str(), text.as_str()))
        .collect::<Vec<_>>();
    write_under(&dir, "app/src", &files);
    let root = dir.0.as_path();
    let ingested = ingest_fixture(root, None);
    let id = ingested.value["id"].as_str().expect("id").to_owned();
    let out = ask(root, "callers", Some("run"), |_| {});
    assert_eq!(out.class, ExitClass::InvalidInput, "{}", out.value);
    assert_eq!(out.value["candidates"].as_array().map(Vec::len), Some(20));
    assert_eq!(out.value["pagination"]["totalItems"], 25);
    assert_eq!(out.value["pagination"]["hasMore"], true);
    let next = next_query(&out).expect("structured continuation");
    assert_eq!(
        (next.op.as_str(), next.target.as_deref(), next.offset),
        ("candidates", Some("run"), Some(20)),
        "{next:?}"
    );
    let error = out.value["error"].as_str().unwrap_or_default();
    assert!(
        !error.contains("kind"),
        "every candidate is a symbol: {error}"
    );
    let mut continued = next.clone();
    continued.graph = Some(id);
    let rest = query(&continued, &workspace_policy(root));
    assert_eq!(rest.value["rows"].as_array().unwrap().len(), 5);
    let union = out.value["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .chain(rest.value["rows"].as_array().unwrap())
        .map(|row| row["id"].as_str().unwrap())
        .collect::<BTreeSet<_>>();
    assert_eq!(union.len(), 25);
    let found = ask(root, "find", Some("run"), |o| o.limit = Some(100));
    assert_eq!(found.value["pagination"]["totalItems"], 25, "{}", found.value);
}
