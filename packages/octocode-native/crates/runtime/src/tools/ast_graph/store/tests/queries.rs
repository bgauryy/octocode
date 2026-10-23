//! Query operations: answers, references, paging, flags and leads.
use super::*;

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
    assert_eq!(callers.value["results"][0]["via"], "import");
    assert_eq!(callers.value["results"][0]["confidence"], "high");

    let callees = ask(root, "callees", Some("src/lib.ts#greet"), |_| {});
    assert_eq!(ids(&callees), vec!["src/lib.ts#shout"]);
    assert_eq!(callees.value["results"][0]["via"], "local");

    let member = ask(root, "callees", Some("Box.open"), |_| {});
    assert_eq!(
        ids(&member),
        vec!["src/lib.ts#Box.close"],
        "{}",
        member.value
    );

    let cycles = ask(root, "cycles", None, |_| {});
    assert_eq!(cycles.value["total"], 1, "{}", cycles.value);
    assert_eq!(
        cycles.value["results"][0]["nodes"],
        json!(["src/a.ts", "src/b.ts"])
    );

    let path = ask(root, "path", Some("src/main.ts"), |o| {
        o.to = Some("src/lib.ts".into())
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
        o.kind = Some("symbol".into())
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
        o.graph = Some("no-such-graph".into())
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
        o.limit = Some(2)
    });
    assert_eq!(first.class, ExitClass::Success, "paging is success");
    assert_eq!(first.value["truncated"], true);
    let next = first.value["next"].as_str().expect("next");
    assert!(
        next.contains("--offset 2") && next.contains("--limit 2"),
        "{next}"
    );
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
        o.edges = vec!["inherits".into()]
    });
    let rows = bases.value["results"].as_array().expect("rows");
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
    assert_eq!(renders.value["results"][0]["via"], "renders:local");
}

/// Every answer over a snapshot whose scan was cut says so in its graph
/// block: the cut and, for a file-scan cut, the re-ingest that lifts it.
#[test]
fn answers_over_a_cut_snapshot_name_the_cut_and_the_reingest() {
    let dir = fixture();
    let root = dir.0.as_path();
    let out = ingest(
        &IngestOptions {
            path: root.join("app"),
            workspace: Some(root.to_path_buf()),
            max_files: Some(2),
            ..Default::default()
        },
        &policy(root),
        &ContentSecurity::new(),
        &NeverCancel,
        None,
    );
    assert_eq!(out.value["scan"]["truncated"], true, "{}", out.value);
    for op in ["cycles", "stats", "issues"] {
        let answer = ask(root, op, None, |_| {});
        let graph = &answer.value["graph"];
        assert_eq!(
            graph["partialReasons"],
            json!(["maxFiles"]),
            "{op}: {graph}"
        );
        let reingest = graph["reingest"].as_str().expect("reingest");
        assert!(reingest.ends_with(" --max-files 4"), "{op}: {reingest}");
    }
    let full = ingest_root(root, &root.join("app"), None, true);
    assert_eq!(full.value["scan"]["truncated"], false, "{}", full.value);
    let graph = &ask(root, "cycles", None, |_| {}).value["graph"];
    assert!(graph.get("partialReasons").is_none(), "{graph}");
    assert!(graph.get("reingest").is_none(), "{graph}");
}

/// Flags outside their documented range are bad input (exit 2), never
/// silently clamped; a page past the last row is empty (exit 1).
#[test]
fn out_of_range_flags_are_rejected_and_a_page_past_the_end_is_empty() {
    let dir = fixture();
    let root = dir.0.as_path();
    assert_eq!(ingest_fixture(root, None).class, ExitClass::Success);
    for (depth, limit) in [
        (Some(0), None),
        (Some(21), None),
        (None, Some(0)),
        (None, Some(1001)),
    ] {
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
        o.offset = Some(500)
    });
    assert_eq!(past.class, ExitClass::Empty, "{}", past.value);
    assert!(past.value["total"].as_u64().is_some_and(|total| total > 0));
}

/// An unknown `--edge` names every edge kind `--edge` accepts.
#[test]
fn unknown_edge_kind_lists_every_kind() {
    let dir = fixture();
    let root = dir.0.as_path();
    assert_eq!(ingest_fixture(root, None).class, ExitClass::Success);
    let out = ask(root, "walk", Some("src/lib.ts"), |o| {
        o.edges = vec!["bogus".into()]
    });
    assert_eq!(out.class, ExitClass::InvalidInput);
    let error = out.value["error"].as_str().unwrap_or_default();
    for kind in ["contains", "imports", "calls", "uses", "inherits"] {
        assert!(error.contains(kind), "{error}");
    }
}

/// Every lead runs against the snapshot that produced it: `node` hints and
/// `issues` verify commands carry `--graph <id> --workspace <dir>`, like
/// `next`, and verify steps are commands, not prose.
#[test]
fn node_leads_and_verify_commands_are_pinned_to_the_snapshot() {
    let dir = issues_fixture();
    let root = dir.0.as_path();
    let ingested = ingest_fixture(root, None);
    let id = ingested.value["id"].as_str().expect("id").to_owned();
    let pinned = format!("--graph {id} --workspace ");
    let node = ask(root, "node", Some("src/lib.ts#greet"), |_| {});
    let hints = node.value["node"]["next"].as_array().expect("next");
    assert!(!hints.is_empty());
    for hint in hints {
        let hint = hint.as_str().unwrap_or_default();
        assert!(hint.contains(&pinned), "{hint}");
    }
    let issues = ask(root, "issues", None, |o| o.limit = Some(200));
    let steps = issues.value["results"]
        .as_array()
        .expect("rows")
        .iter()
        .flat_map(|row| row["verify"].as_array().cloned().unwrap_or_default())
        .collect::<Vec<_>>();
    assert!(!steps.is_empty(), "{}", issues.value);
    for step in &steps {
        let step = step.as_str().unwrap_or_default();
        assert!(step.starts_with("octocode "), "{step}");
        if step.starts_with("octocode graph query ") {
            assert!(step.contains(&pinned), "{step}");
        }
    }
}

/// An ambiguous reference lists the first candidates, says how many match,
/// and offers the `find` command that pages them all; `--kind` is offered
/// only when it can narrow them.
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
    assert_eq!(out.value["total"], 25);
    assert_eq!(out.value["truncated"], true);
    let next = out.value["next"].as_str().expect("next");
    assert!(
        next.starts_with("octocode graph query find run ") && next.contains(&id),
        "{next}"
    );
    let error = out.value["error"].as_str().unwrap_or_default();
    assert!(
        !error.contains("--kind"),
        "every candidate is a symbol: {error}"
    );
    let found = ask(root, "find", Some("run"), |o| o.limit = Some(100));
    assert_eq!(found.value["total"], 25, "{}", found.value);
}
