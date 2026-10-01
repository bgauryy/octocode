use super::*;
use crate::policy::path::PathPolicyConfig;
use crate::tools::cancel::NeverCancel;

fn fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("fixture");
    let src = dir.path().join("app").join("src");
    std::fs::create_dir_all(&src).expect("src");
    std::fs::create_dir_all(dir.path().join(".git")).expect("git marker");
    let files = [
        (
            "main.ts",
            "import { greet } from './lib';\nimport React from 'react';\nexport function main() {\n  return greet('x');\n}\n",
        ),
        (
            "lib.ts",
            "export function greet(name: string) {\n  return shout(name);\n}\nfunction shout(v: string) {\n  return v;\n}\nexport class Box {\n  open() { return this.close(); }\n  close() { return 1; }\n}\n",
        ),
        (
            "a.ts",
            "import { b } from './b';\nexport const a = () => b();\n",
        ),
        (
            "b.ts",
            "import { a } from './a';\nexport const b = () => a();\n",
        ),
    ];
    for (name, text) in files {
        std::fs::write(src.join(name), text).expect("fixture file");
    }
    dir
}

fn policy(root: &Path) -> PathPolicy {
    PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(root.to_path_buf()),
        ..Default::default()
    })
    .expect("path policy")
}

fn ingest_fixture(dir: &Path, keep: Option<usize>) -> GraphOutput {
    let options = IngestOptions {
        path: dir.join("app"),
        workspace: Some(dir.to_path_buf()),
        keep,
        ..Default::default()
    };
    ingest(
        &options,
        &policy(dir),
        &ContentSecurity::new(),
        &NeverCancel,
    )
}

fn ask(
    dir: &Path,
    op: &str,
    target: Option<&str>,
    tweak: impl FnOnce(&mut QueryOptions),
) -> GraphOutput {
    let mut options = QueryOptions {
        op: op.into(),
        target: target.map(str::to_owned),
        workspace: Some(dir.to_path_buf()),
        ..Default::default()
    };
    tweak(&mut options);
    query(&options, &policy(dir))
}

fn ids(out: &GraphOutput) -> Vec<String> {
    out.value["results"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|row| row["id"].as_str().unwrap_or_default().to_owned())
        .collect()
}

#[test]
fn format_round_trips_deterministically_and_rejects_corruption() {
    let dir = fixture();
    let out = ingest_fixture(dir.path(), None);
    assert_eq!(out.exit, 0, "{}", out.value);
    let graph_dir = PathBuf::from(out.value["dir"].as_str().expect("dir"));
    let bytes = std::fs::read(graph_dir.join(GRAPH_FILE)).expect("graph.bin");
    let (tables, digest) = format::decode(&bytes).expect("decode");
    assert_eq!(digest, out.value["sha256"].as_str().expect("sha"));
    let (again, _) = format::encode(&tables);
    assert_eq!(
        again, bytes,
        "re-encoding a decoded snapshot is byte-identical"
    );

    let mut corrupt = bytes.clone();
    let last = corrupt.len() - 1;
    corrupt[last] ^= 0xff;
    assert!(format::decode(&corrupt).unwrap_err().contains("checksum"));
    assert!(
        format::decode(b"not a graph")
            .unwrap_err()
            .contains("magic")
    );
    let mut future = bytes;
    future[8] = 99;
    assert!(
        format::decode(&future)
            .unwrap_err()
            .contains("not supported")
    );
}

#[test]
fn ingest_publishes_a_snapshot_under_the_workspace() {
    let dir = fixture();
    let out = ingest_fixture(dir.path(), None);
    assert_eq!(out.exit, 0, "{}", out.value);
    let id = out.value["id"].as_str().expect("id");
    assert!(id.ends_with("-app"), "{id}");
    let home = dir.path().join(".octocode").join("graph");
    assert_eq!(
        std::fs::read_to_string(home.join("latest")).expect("latest"),
        id
    );
    let manifest: Value = serde_json::from_slice(
        &std::fs::read(home.join(id).join(MANIFEST_FILE)).expect("manifest"),
    )
    .expect("json");
    assert_eq!(manifest["kind"], MANIFEST_KIND);
    assert_eq!(manifest["scan"]["filesScanned"], 4);
    assert_eq!(manifest["counts"]["nodesByKind"]["package"], 1);
    assert!(
        std::fs::read_dir(&home)
            .expect("home")
            .flatten()
            .all(|entry| !entry.file_name().to_string_lossy().starts_with(".tmp-")),
        "temp build directories never survive a publish"
    );
}

#[test]
fn queries_answer_imports_calls_cycles_and_packages() {
    let dir = fixture();
    assert_eq!(ingest_fixture(dir.path(), None).exit, 0);
    let root = dir.path();

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
    assert_eq!(ingest_fixture(dir.path(), None).exit, 0);
    let missing = ask(dir.path(), "node", Some("nope"), |_| {});
    assert_eq!(missing.exit, 3);
    assert_eq!(missing.value["errorCode"], "graph.nodeNotFound");

    let bad = ask(dir.path(), "explode", None, |_| {});
    assert_eq!(bad.exit, 2);

    let none = ask(dir.path(), "stats", None, |o| {
        o.graph = Some("no-such-graph".into())
    });
    assert_eq!(none.exit, 3, "{}", none.value);
}

#[test]
fn list_operations_page_with_a_runnable_next_command() {
    let dir = fixture();
    assert_eq!(ingest_fixture(dir.path(), None).exit, 0);
    let first = ask(dir.path(), "symbols", Some("src/lib.ts"), |o| {
        o.limit = Some(2)
    });
    assert_eq!(first.exit, 6);
    assert_eq!(first.value["truncated"], true);
    let next = first.value["next"].as_str().expect("next");
    assert!(
        next.contains("--offset 2") && next.contains("--limit 2"),
        "{next}"
    );
    let second = ask(dir.path(), "symbols", Some("src/lib.ts"), |o| {
        o.limit = Some(2);
        o.offset = Some(2);
    });
    assert_eq!(ids(&second), vec!["src/lib.ts#Box", "src/lib.ts#Box.open"]);
}

#[test]
fn stale_detects_edits_and_prune_keeps_the_newest_snapshots() {
    let dir = fixture();
    assert_eq!(ingest_fixture(dir.path(), None).exit, 0);
    let fresh = ask(dir.path(), "stale", None, |_| {});
    assert_eq!(
        (fresh.exit, &fresh.value["fresh"]),
        (0, &json!(true)),
        "{}",
        fresh.value
    );

    std::fs::write(dir.path().join("app/src/a.ts"), "export const a = 2;\n").expect("edit");
    let stale = ask(dir.path(), "stale", None, |_| {});
    assert_eq!(
        stale.value["results"],
        json!([{"file": "src/a.ts", "status": "changed"}])
    );

    for _ in 0..2 {
        assert_eq!(ingest_fixture(dir.path(), Some(1)).exit, 0);
    }
    let home = dir.path().join(".octocode").join("graph");
    assert_eq!(list_snapshots(&home).len(), 1);
}

/// `fixture()` plus a manifest, an undeclared import, an orphan, a
/// generated file, and a test.
fn issues_fixture() -> tempfile::TempDir {
    let dir = fixture();
    let app = dir.path().join("app");
    std::fs::write(
        app.join("package.json"),
        r#"{"name":"app","main":"dist/main.js","dependencies":{"react":"18"},"devDependencies":{"vitest":"1"}}"#,
    )
    .expect("manifest");
    let src = app.join("src");
    std::fs::write(src.join("orphan.ts"), "export const lonely = 1;\n").expect("orphan");
    std::fs::write(
        src.join("fmt.ts"),
        "import pad from 'left-pad';\nimport { expect } from 'vitest';\nexport const fmt = (s: string) => pad(s);\nexport const check = () => expect(1);\n",
    )
    .expect("fmt");
    std::fs::write(
        src.join("client.gen.ts"),
        "// Code generated by openapi. DO NOT EDIT.\nexport const unused = 1;\n",
    )
    .expect("generated");
    std::fs::write(
        src.join("main.test.ts"),
        "import { main } from './main';\nimport { fmt } from './fmt';\nmain(); fmt('x');\n",
    )
    .expect("test");
    dir
}

fn findings(out: &GraphOutput, detector: &str) -> Vec<String> {
    out.value["results"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|row| row["detector"] == detector)
        .map(|row| row["subject"].as_str().unwrap_or_default().to_owned())
        .collect()
}

#[test]
fn ingest_classifies_roles_entries_and_declared_dependencies() {
    let dir = issues_fixture();
    assert_eq!(ingest_fixture(dir.path(), None).exit, 0);
    let root = dir.path();
    let stats = ask(root, "issues", None, |o| o.limit = Some(200));
    let roles = &stats.value["summary"]["fileRoles"];
    assert_eq!(roles["generated"], 1, "{}", stats.value["summary"]);
    assert_eq!(roles["test"], 1);
    assert_eq!(roles["entry"], 1, "dist/main.js maps back to src/main.ts");

    let deps = ask(root, "deps", Some("src/fmt.ts"), |_| {});
    let vias = deps.value["results"]
        .as_array()
        .expect("rows")
        .iter()
        .map(|row| {
            (
                row["id"].as_str().unwrap_or_default().to_owned(),
                row["via"].as_str().unwrap_or_default().to_owned(),
            )
        })
        .collect::<Vec<_>>();
    assert!(
        vias.contains(&("pkg:left-pad".into(), "external-undeclared".into())),
        "{vias:?}"
    );
    assert!(
        vias.contains(&("pkg:vitest".into(), "external-dev".into())),
        "{vias:?}"
    );

    let uses = ask(root, "dependents", Some("src/lib.ts#greet"), |o| {
        o.edges = vec!["uses".into()]
    });
    assert_eq!(ids(&uses), vec!["src/main.ts"], "{}", uses.value);
}

#[test]
fn issues_rank_hypotheses_with_evidence_and_controls() {
    let dir = issues_fixture();
    assert_eq!(ingest_fixture(dir.path(), None).exit, 0);
    let root = dir.path();
    let out = ask(root, "issues", None, |o| o.limit = Some(200));
    assert!(matches!(out.exit, 0 | 6), "{}", out.value);

    assert_eq!(findings(&out, "cycle"), vec!["src/a.ts"], "{}", out.value);
    let cycle = out.value["results"]
        .as_array()
        .expect("rows")
        .iter()
        .find(|row| row["detector"] == "cycle")
        .expect("cycle finding");
    assert_eq!(cycle["evidence"]["cutCount"], 1);
    assert_eq!(
        cycle["evidence"]["witness"].as_array().map(Vec::len),
        Some(2)
    );

    let unreachable = findings(&out, "unreachable-file");
    assert!(
        unreachable.contains(&"src/orphan.ts".to_owned()),
        "{unreachable:?}"
    );
    assert!(
        !unreachable.contains(&"src/client.gen.ts".to_owned()),
        "generated files are never subjects"
    );
    assert!(!unreachable.contains(&"src/main.ts".to_owned()));

    assert_eq!(findings(&out, "test-only"), vec!["src/fmt.ts"]);
    assert_eq!(
        findings(&out, "undeclared-dependency"),
        vec!["app::left-pad"]
    );
    assert_eq!(
        findings(&out, "dev-dependency-in-production"),
        vec!["app::vitest"]
    );

    let unused = findings(&out, "unused-export");
    assert!(
        unused.contains(&"src/lib.ts".to_owned()),
        "Box is never imported: {unused:?}"
    );

    let scores = out.value["results"]
        .as_array()
        .expect("rows")
        .iter()
        .map(|row| row["score"].as_f64().unwrap_or_default())
        .collect::<Vec<_>>();
    assert!(
        scores.windows(2).all(|w| w[0] >= w[1]),
        "ranked by score: {scores:?}"
    );

    let only = ask(root, "issues", None, |o| o.detectors = vec!["cycle".into()]);
    assert_eq!(only.value["total"], 1);
    let bad = ask(root, "issues", None, |o| o.detectors = vec!["nope".into()]);
    assert_eq!(bad.exit, 2);
}

#[test]
fn issues_baseline_reports_new_and_resolved_findings() {
    let dir = issues_fixture();
    let first = ingest_fixture(dir.path(), None);
    let base = first.value["id"].as_str().expect("id").to_owned();
    std::fs::write(
        dir.path().join("app/src/b.ts"),
        "export const b = () => 1;\n",
    )
    .expect("break cycle");
    std::fs::write(
        dir.path().join("app/src/orphan2.ts"),
        "export const x = 1;\n",
    )
    .expect("orphan");
    std::thread::sleep(std::time::Duration::from_millis(1100));
    assert_eq!(ingest_fixture(dir.path(), None).exit, 0);
    let out = ask(dir.path(), "issues", None, |o| {
        o.baseline = Some(base.clone());
        o.limit = Some(200);
    });
    let baseline = &out.value["baseline"];
    assert!(
        baseline["resolved"].as_u64().unwrap_or(0) >= 1,
        "{baseline}"
    );
    assert!(
        baseline["resolvedFindings"]
            .as_array()
            .expect("resolved")
            .iter()
            .any(|f| f["detector"] == "cycle"),
        "{baseline}"
    );
    let new = out.value["results"]
        .as_array()
        .expect("rows")
        .iter()
        .filter(|row| row["status"] == "new")
        .map(|row| row["subject"].as_str().unwrap_or_default())
        .collect::<Vec<_>>();
    assert!(new.contains(&"src/orphan2.ts"), "{new:?}");
}

#[test]
fn heritage_and_jsx_become_inherits_and_renders_edges() {
    let dir = fixture();
    let src = dir.path().join("app/src");
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
    assert_eq!(ingest_fixture(dir.path(), None).exit, 0);
    let bases = ask(dir.path(), "deps", Some("Crate"), |o| {
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

    let renders = ask(dir.path(), "callers", Some("src/view.tsx#Card"), |_| {});
    assert_eq!(
        ids(&renders),
        vec!["src/view.tsx#Page"],
        "{}",
        renders.value
    );
    assert_eq!(renders.value["results"][0]["via"], "renders:local");
}

fn impacted(out: &GraphOutput) -> Vec<(String, u64)> {
    out.value["results"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|row| {
            (
                row["id"].as_str().unwrap_or_default().to_owned(),
                row["depth"].as_u64().unwrap_or(99),
            )
        })
        .collect()
}

#[test]
fn impact_is_symbol_precise_for_symbols_and_transitive_for_files() {
    let dir = issues_fixture();
    assert_eq!(ingest_fixture(dir.path(), None).exit, 0);
    let root = dir.path();

    let symbol = ask(root, "impact", Some("src/lib.ts#shout"), |_| {});
    assert_eq!(
        symbol.value["summary"]["precision"], "symbol",
        "{}",
        symbol.value
    );
    let files = impacted(&symbol);
    assert!(files.contains(&("src/lib.ts".into(), 0)), "{files:?}");
    assert!(
        files.iter().any(|(f, _)| f == "src/main.ts"),
        "greet calls shout, main calls greet: {files:?}"
    );
    assert!(
        !files.iter().any(|(f, _)| f == "src/fmt.ts"),
        "fmt never touches lib: {files:?}"
    );
    assert_eq!(
        symbol.value["summary"]["testsToRun"],
        json!(["src/main.test.ts"])
    );

    // Box is exported but nobody binds it: symbol precision keeps main.ts out.
    let unused = ask(root, "impact", Some("src/lib.ts#Box"), |_| {});
    assert_eq!(
        impacted(&unused),
        vec![("src/lib.ts".to_owned(), 0)],
        "{}",
        unused.value
    );

    let file = ask(root, "impact", Some("src/lib.ts"), |_| {});
    assert_eq!(file.value["summary"]["precision"], "file");
    let files = impacted(&file);
    assert!(files.contains(&("src/main.ts".into(), 1)), "{files:?}");
    assert!(files.contains(&("src/main.test.ts".into(), 2)), "{files:?}");
    assert_eq!(
        file.value["summary"]["affectedEntrypoints"],
        json!(["src/main.ts"])
    );
}

#[test]
fn impact_since_reads_git_and_widens_on_config_changes() {
    let dir = issues_fixture();
    let app = dir.path().join("app");
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .arg("-C")
            .arg(&app)
            .args(args)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    };
    if !git(&["init", "-q"]) {
        return; // git unavailable in this environment
    }
    assert!(git(&[
        "-c",
        "user.email=t@t",
        "-c",
        "user.name=t",
        "add",
        "-A"
    ]));
    assert!(git(&[
        "-c",
        "user.email=t@t",
        "-c",
        "user.name=t",
        "commit",
        "-qm",
        "base"
    ]));
    assert_eq!(ingest_fixture(dir.path(), None).exit, 0);
    std::fs::write(
        app.join("src/fmt.ts"),
        "export const fmt = (s: string) => s;\n",
    )
    .expect("edit");
    let out = ask(dir.path(), "impact", None, |o| {
        o.since = Some("HEAD".into())
    });
    let files = impacted(&out);
    assert!(files.contains(&("src/fmt.ts".into(), 0)), "{}", out.value);
    assert!(files.contains(&("src/main.test.ts".into(), 1)), "{files:?}");
    assert!(!files.iter().any(|(f, _)| f == "src/lib.ts"));

    std::fs::write(
        app.join("package.json"),
        r#"{"name":"app","main":"dist/main.js"}"#,
    )
    .expect("manifest");
    let widened = ask(dir.path(), "impact", None, |o| {
        o.since = Some("HEAD".into())
    });
    assert!(
        widened.value["summary"]["configChanges"][0]["file"] == "package.json",
        "{}",
        widened.value["summary"]
    );
    assert!(
        impacted(&widened)
            .iter()
            .any(|(f, d)| f == "src/lib.ts" && *d == 0)
    );
}

#[test]
fn oversized_and_minified_files_stay_visible_without_noise() {
    let dir = fixture();
    let src = dir.path().join("app/src");
    // Over the 1 MB parse bound: kept as an `unparsed` file node.
    let big = format!("!function(){{{}}}();\n", "var a=1;".repeat(160_000));
    std::fs::write(src.join("huge.min.js"), &big).expect("huge");
    // Under the bound but minified: parsed, symbols dropped.
    let small = format!("{}\n", "function a(){return 1}var b=a();".repeat(200));
    std::fs::write(src.join("small.min.js"), small).expect("small");
    std::fs::write(
        src.join("uses.ts"),
        "import './huge.min.js';\nimport './small.min.js';\nexport const x = 1;\n",
    )
    .expect("importer");
    let ingest = ingest_fixture(dir.path(), None);
    assert_eq!(ingest.exit, 0, "{}", ingest.value);

    let huge = ask(dir.path(), "node", Some("src/huge.min.js"), |_| {});
    assert_eq!(huge.exit, 0, "{}", huge.value);
    let deps = ask(dir.path(), "deps", Some("src/uses.ts"), |_| {});
    let rows = deps.value["results"].as_array().expect("rows");
    assert!(
        rows.iter()
            .any(|r| r["id"] == "src/huge.min.js" && r["via"] == "unparsed-target"),
        "{}",
        deps.value
    );
    let outline = ask(dir.path(), "symbols", Some("src/small.min.js"), |_| {});
    assert_eq!(
        outline.value["total"], 0,
        "minified symbols are dropped: {}",
        outline.value
    );

    let issues = ask(dir.path(), "issues", None, |o| o.limit = Some(500));
    let subjects = issues.value["results"]
        .as_array()
        .expect("rows")
        .iter()
        .map(|r| r["subject"].as_str().unwrap_or_default().to_owned())
        .collect::<Vec<_>>();
    assert!(
        !subjects.iter().any(|s| s.contains(".min.js")),
        "{subjects:?}"
    );
    assert!(
        !issues.value["results"]
            .as_array()
            .expect("rows")
            .iter()
            .any(|r| r["detector"] == "unresolved-import" && r["subject"] == "src/uses.ts"),
        "import of an unparsed file is linked, not unresolved"
    );
}

#[test]
fn impact_lists_inline_rust_test_functions() {
    let dir = tempfile::tempdir().expect("dir");
    std::fs::create_dir_all(dir.path().join(".git")).expect("git");
    let krate = dir.path().join("app");
    std::fs::create_dir_all(krate.join("src")).expect("src");
    std::fs::write(
        krate.join("Cargo.toml"),
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
    )
    .expect("manifest");
    std::fs::write(
        krate.join("src/lib.rs"),
        "pub fn add(a: u8, b: u8) -> u8 {\n    a + b\n}\n\npub fn twice(a: u8) -> u8 {\n    add(a, a)\n}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n\n    #[test]\n    fn adds() {\n        let _ = add(1, 2);\n    }\n\n    #[test]\n    fn doubles() {\n        let _ = twice(2);\n    }\n}\n",
    )
    .expect("lib");
    assert_eq!(ingest_fixture(dir.path(), None).exit, 0);
    let node = ask(dir.path(), "node", Some("src/lib.rs#tests.adds"), |_| {});
    assert_eq!(node.value["node"]["test"], true, "{}", node.value);
    let out = ask(dir.path(), "impact", Some("src/lib.rs#add"), |_| {});
    let tests = &out.value["summary"]["testFunctions"];
    assert!(
        tests
            .as_array()
            .is_some_and(|t| t.contains(&json!("src/lib.rs#tests.adds"))),
        "{}",
        out.value["summary"]
    );
    assert!(
        tests
            .as_array()
            .is_some_and(|t| t.contains(&json!("src/lib.rs#tests.doubles"))),
        "transitively via twice: {}",
        out.value["summary"]
    );
    assert_eq!(out.value["summary"]["testsToRun"], json!(["src/lib.rs"]));
}

// ── Quality plan acceptance tests (CODE_GRAPH_QUALITY_PLAN.md) ─────────────

fn write_all(root: &Path, files: &[(&str, &str)]) {
    for (path, text) in files {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
        std::fs::write(path, text).expect("write");
    }
}

#[test]
fn plan_1_call_answers_report_coverage() {
    let dir = fixture();
    write_all(
        &dir.path().join("app/src"),
        &[(
            "opener.ts",
            "import { Box } from './lib';\nfunction make(): any { return null; }\nexport function run() {\n  const b = make();\n  b.open();\n  return new Box();\n}\n",
        )],
    );
    assert_eq!(ingest_fixture(dir.path(), None).exit, 0);
    let out = ask(dir.path(), "callers", Some("src/lib.ts#Box.open"), |_| {});
    let coverage = &out.value["coverage"];
    let recall = coverage["callInternalRecall"].as_f64().expect("recall");
    assert!(recall < 1.0, "{}", out.value);
    assert!(
        coverage["warning"].as_str().is_some_and(|w| !w.is_empty()),
        "{}",
        out.value
    );
    let impact = ask(dir.path(), "impact", Some("src/lib.ts#Box.open"), |_| {});
    assert!(
        impact.value["coverage"]["callInternalRecall"].is_number(),
        "{}",
        impact.value
    );
}

#[test]
fn plan_2_string_referenced_files_are_low_tier_and_hidden_by_default() {
    let dir = issues_fixture();
    write_all(
        &dir.path().join("app"),
        &[
            ("src/worker.ts", "export const work = () => 1;\n"),
            (
                "src/spawn.ts",
                "export const spawn = () => new Worker(new URL('./worker.ts', import.meta.url));\n",
            ),
            ("src/tool.ts", "export const tool = 1;\n"),
            ("build.config.json", "{\"entry\": \"./src/tool.ts\"}\n"),
            (
                "src/local.ts",
                "export function helper() { return 1; }\nexport const value = helper();\n",
            ),
        ],
    );
    assert_eq!(ingest_fixture(dir.path(), None).exit, 0);
    let default = ask(dir.path(), "issues", None, |o| o.limit = Some(500));
    let shown = findings(&default, "unreachable-file");
    assert!(
        !shown.contains(&"src/worker.ts".to_owned()),
        "string-referenced worker hidden: {shown:?}"
    );
    assert!(
        !shown.contains(&"src/tool.ts".to_owned()),
        "config-referenced file hidden: {shown:?}"
    );
    assert!(
        shown.contains(&"src/orphan.ts".to_owned()),
        "true orphan still shown: {shown:?}"
    );
    let orphan = default.value["results"]
        .as_array()
        .expect("rows")
        .iter()
        .find(|r| r["subject"] == "src/orphan.ts" && r["detector"] == "unreachable-file")
        .expect("orphan");
    assert!(orphan["tier"].as_u64().unwrap_or(0) >= 90, "{orphan}");

    let all = ask(dir.path(), "issues", None, |o| {
        o.limit = Some(500);
        o.min_tier = Some(0);
    });
    let worker = all.value["results"]
        .as_array()
        .expect("rows")
        .iter()
        .find(|r| r["subject"] == "src/worker.ts" && r["detector"] == "unreachable-file")
        .expect("worker listed at tier 60");
    assert_eq!(worker["tier"], 60, "{worker}");
    assert!(
        worker["evidence"]["mentionedIn"]
            .as_array()
            .is_some_and(|m| !m.is_empty()),
        "{worker}"
    );

    // An export used only inside its own file is "could be un-exported".
    assert!(
        findings(&all, "export-only-local").contains(&"src/local.ts".to_owned()),
        "{}",
        all.value["summary"]
    );
    let unused = findings(&all, "unused-export");
    assert!(
        !unused.contains(&"src/local.ts".to_owned()) || {
            let row = all.value["results"]
                .as_array()
                .expect("rows")
                .iter()
                .find(|r| r["subject"] == "src/local.ts" && r["detector"] == "unused-export")
                .expect("row");
            !row["evidence"]["exports"]
                .as_array()
                .expect("exports")
                .contains(&json!("helper"))
        },
        "helper is used locally, not unused"
    );
}

#[test]
fn plan_3_type_qualified_calls_and_scoped_disambiguation_link() {
    let dir = tempfile::tempdir().expect("dir");
    std::fs::create_dir_all(dir.path().join(".git")).expect("git");
    write_all(
        &dir.path().join("app"),
        &[
            (
                "com/shop/Cart.java",
                "package com.shop;\npublic class Cart {\n  public static Cart empty() { return new Cart(); }\n  public void add() {}\n}\n",
            ),
            (
                "com/shop/Checkout.java",
                "package com.shop;\npublic class Checkout {\n  public void run() {\n    Cart c = Cart.empty();\n    Helper.assist();\n  }\n}\n",
            ),
            (
                "com/shop/Helper.java",
                "package com.shop;\npublic class Helper {\n  public static void assist() {}\n}\n",
            ),
            (
                "com/other/Helper.java",
                "package com.other;\npublic class Helper {\n  public static void assist() {}\n}\n",
            ),
        ],
    );
    assert_eq!(ingest_fixture(dir.path(), None).exit, 0);
    let callees = ask(
        dir.path(),
        "callees",
        Some("com/shop/Checkout.java#Checkout.run"),
        |_| {},
    );
    let got = ids(&callees);
    assert!(
        got.contains(&"com/shop/Cart.java#Cart.empty".to_owned()),
        "{}",
        callees.value
    );
    assert!(
        got.contains(&"com/shop/Helper.java#Helper.assist".to_owned()),
        "same-package wins: {got:?}"
    );
    assert!(
        !got.contains(&"com/other/Helper.java#Helper.assist".to_owned()),
        "{got:?}"
    );
}

#[test]
fn plan_5_impact_defaults_to_depth_three_and_passes_through_barrels() {
    let dir = tempfile::tempdir().expect("dir");
    std::fs::create_dir_all(dir.path().join(".git")).expect("git");
    write_all(
        &dir.path().join("app"),
        &[
            ("src/a.ts", "export const a = () => 1;\n"),
            ("src/b.ts", "export const b = () => 2;\n"),
            (
                "src/index.ts",
                "export { a } from './a';\nexport { b } from './b';\n",
            ),
            (
                "src/useA.ts",
                "import { a } from './index';\nexport const ua = () => a();\n",
            ),
            (
                "src/useB.ts",
                "import { b } from './index';\nexport const ub = () => b();\n",
            ),
            (
                "src/c1.ts",
                "import { ua } from './useA';\nexport const c1 = () => ua();\n",
            ),
            (
                "src/c2.ts",
                "import { c1 } from './c1';\nexport const c2 = () => c1();\n",
            ),
            (
                "src/c3.ts",
                "import { c2 } from './c2';\nexport const c3 = () => c2();\n",
            ),
            (
                "src/c4.ts",
                "import { c3 } from './c3';\nexport const c4 = () => c3();\n",
            ),
        ],
    );
    assert_eq!(ingest_fixture(dir.path(), None).exit, 0);
    let out = ask(dir.path(), "impact", Some("src/a.ts"), |_| {});
    let files = impacted(&out);
    assert!(files.iter().any(|(f, _)| f == "src/useA.ts"), "{files:?}");
    assert!(
        !files.iter().any(|(f, _)| f == "src/useB.ts"),
        "barrel passes through: {files:?}"
    );
    assert_eq!(
        out.value["summary"]["maxDepth"], 3,
        "{}",
        out.value["summary"]
    );
    assert!(
        !files.iter().any(|(f, _)| f == "src/c4.ts"),
        "depth 3 by default: {files:?}"
    );
    assert!(
        out.value["summary"]["willBreak"].is_array(),
        "{}",
        out.value["summary"]
    );
    let deep = ask(dir.path(), "impact", Some("src/a.ts"), |o| {
        o.depth = Some(10)
    });
    assert!(impacted(&deep).iter().any(|(f, _)| f == "src/c4.ts"));
}

#[test]
fn plan_9_unchanged_tree_reuses_the_latest_snapshot() {
    let dir = fixture();
    let first = ingest_fixture(dir.path(), None);
    assert_eq!(first.exit, 0);
    let second = ingest_fixture(dir.path(), None);
    assert_eq!(second.value["reused"], true, "{}", second.value);
    assert_eq!(second.value["id"], first.value["id"]);
    std::fs::write(dir.path().join("app/src/a.ts"), "export const a = 5;\n").expect("edit");
    let third = ingest_fixture(dir.path(), None);
    assert_ne!(third.value["reused"], true, "{}", third.value);
    let forced = ingest(
        &IngestOptions {
            path: dir.path().join("app"),
            workspace: Some(dir.path().to_path_buf()),
            force: true,
            ..Default::default()
        },
        &policy(dir.path()),
        &ContentSecurity::new(),
        &NeverCancel,
    );
    assert_ne!(forced.value["reused"], true);
}

#[test]
fn plan_4_receiver_typed_member_calls_link() {
    let dir = tempfile::tempdir().expect("dir");
    std::fs::create_dir_all(dir.path().join(".git")).expect("git");
    write_all(
        &dir.path().join("app"),
        &[
            (
                "Cargo.toml",
                "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
            ),
            ("src/lib.rs", "pub mod store;\npub mod service;\n"),
            (
                "src/store.rs",
                "pub struct Store;\nimpl Store {\n    pub fn new() -> Self { Store }\n    pub fn save(&self) {}\n}\npub struct Other;\nimpl Other {\n    pub fn save(&self) {}\n}\n",
            ),
            (
                "src/service.rs",
                "use crate::store::Store;\npub fn run() {\n    let s = Store::new();\n    s.save();\n}\npub fn typed(s: &Store) {\n    s.save();\n}\n",
            ),
            (
                "web/store.ts",
                "export class Cart {\n  save() { return 1; }\n}\nexport class Box {\n  save() { return 2; }\n}\n",
            ),
            (
                "web/app.ts",
                "import { Cart } from './store';\nexport function go() {\n  const c = new Cart();\n  c.save();\n}\n",
            ),
        ],
    );
    assert_eq!(ingest_fixture(dir.path(), None).exit, 0);
    let rust = ask(dir.path(), "callees", Some("src/service.rs#run"), |_| {});
    let rows = rust.value["results"].as_array().expect("rows");
    assert!(
        rows.iter()
            .any(|r| r["id"] == "src/store.rs#Store.save" && r["via"] == "receiver-type"),
        "{}",
        rust.value
    );
    assert!(!ids(&rust).contains(&"src/store.rs#Other.save".to_owned()));
    let typed = ask(dir.path(), "callees", Some("src/service.rs#typed"), |_| {});
    assert!(
        ids(&typed).contains(&"src/store.rs#Store.save".to_owned()),
        "{}",
        typed.value
    );
    let ts = ask(dir.path(), "callees", Some("web/app.ts#go"), |_| {});
    assert!(
        ids(&ts).contains(&"web/store.ts#Cart.save".to_owned()),
        "{}",
        ts.value
    );
    assert!(!ids(&ts).contains(&"web/store.ts#Box.save".to_owned()));
}

#[test]
fn plan_4_typed_this_field_calls_are_member_calls_not_local_matches() {
    let dir = tempfile::tempdir().expect("dir");
    std::fs::create_dir_all(dir.path().join(".git")).expect("git");
    write_all(
        &dir.path().join("app/src"),
        &[
            (
                "repo.ts",
                "export class Repo {\n  load() { return 1; }\n}\n",
            ),
            (
                "svc.ts",
                "import { Repo } from './repo';\nfunction load() { return 0; }\nexport class Svc {\n  private repo: Repo = new Repo();\n  run() {\n    return this.repo.load();\n  }\n}\nexport const unused = load();\n",
            ),
        ],
    );
    assert_eq!(ingest_fixture(dir.path(), None).exit, 0);
    let out = ask(dir.path(), "callees", Some("src/svc.ts#Svc.run"), |_| {});
    let got = ids(&out);
    assert!(
        got.contains(&"src/repo.ts#Repo.load".to_owned()),
        "{}",
        out.value
    );
    assert!(
        !got.contains(&"src/svc.ts#load".to_owned()),
        "typed member call must not match the local fn: {got:?}"
    );
}

#[test]
fn plan_3_rust_module_path_calls_link() {
    let dir = tempfile::tempdir().expect("dir");
    std::fs::create_dir_all(dir.path().join(".git")).expect("git");
    write_all(
        &dir.path().join("app"),
        &[
            (
                "Cargo.toml",
                "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
            ),
            ("src/lib.rs", "pub mod portable;\npub mod api;\n"),
            ("src/portable.rs", "pub fn sanitize(x: u8) -> u8 { x }\n"),
            (
                "src/api.rs",
                "pub fn handle() -> u8 {\n    crate::portable::sanitize(1)\n}\npub fn nested() -> u8 {\n    super::portable::sanitize(2)\n}\n",
            ),
        ],
    );
    assert_eq!(ingest_fixture(dir.path(), None).exit, 0);
    for caller in ["src/api.rs#handle", "src/api.rs#nested"] {
        let out = ask(dir.path(), "callees", Some(caller), |_| {});
        let rows = out.value["results"].as_array().expect("rows");
        assert!(
            rows.iter()
                .any(|r| r["id"] == "src/portable.rs#sanitize" && r["via"] == "module-path"),
            "{caller}: {}",
            out.value
        );
    }
}

/// One engine, two front ends: the persisted `graph query` file answers and
/// the astTopology analyses must name the same files on the same tree.
fn topology(dir: &Path, query: Value) -> Value {
    let mut row = query;
    row["goal"] = json!("parity");
    row["reasoning"] = json!("parity");
    row["path"] = json!(dir.join("app"));
    let query: crate::tools::ast_graph::AstTopologyQuery =
        serde_json::from_value(row).expect("topology query");
    crate::tools::ast_graph::execute_topology(
        &query,
        &policy(dir),
        &ContentSecurity::new(),
        &NeverCancel,
    )
    .expect("topology")
}

fn file_ids(out: &GraphOutput) -> std::collections::BTreeSet<String> {
    ids(out)
        .into_iter()
        .filter(|id| !id.starts_with("pkg:") && !id.contains('#'))
        .collect()
}

fn topology_files(out: &Value) -> std::collections::BTreeSet<String> {
    out["results"]
        .as_array()
        .into_iter()
        .flatten()
        // Re-export consumers are astTopology's addition over the edge walk,
        // and the graph store models a Rust `mod` declaration as containment.
        .filter(|row| row.get("reexportVia").is_none() && row["edgeKinds"] != json!(["rust-module"]))
        .filter_map(|row| row["file"].as_str().map(str::to_owned))
        .collect()
}

fn assert_parity(dir: &Path, from: &str, to: &str) {
    assert_eq!(ingest_fixture(dir, None).exit, 0);
    for (op, analysis, file) in [
        ("deps", "dependencies", from),
        ("dependents", "dependents", to),
    ] {
        let graph = ask(dir, op, Some(file), |_| {});
        let topo = topology(dir, json!({"analysis":analysis,"file":file}));
        assert!(!file_ids(&graph).is_empty(), "{op} {file}: {}", graph.value);
        assert_eq!(
            file_ids(&graph),
            topology_files(&topo),
            "{op} {file}: {} vs {topo}",
            graph.value
        );
    }
    let graph = ask(dir, "path", Some(from), |o| o.to = Some(to.into()));
    let topo = topology(dir, json!({"analysis":"path","file":from,"target":to}));
    let hops = graph.value["path"]
        .as_array()
        .expect("graph path")
        .iter()
        .map(|hop| hop["id"].clone())
        .collect::<Vec<_>>();
    assert_eq!(json!(hops), topo["results"][0]["files"], "{topo}");
    let graph = ask(dir, "cycles", None, |_| {});
    let topo = topology(dir, json!({"analysis":"cycles"}));
    let graph_cycles = graph.value["results"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|row| row["nodes"].clone())
        .collect::<Vec<_>>();
    let topo_cycles = topo["results"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|row| row["files"].clone())
        .collect::<Vec<_>>();
    assert_eq!(graph_cycles, topo_cycles, "{} vs {topo}", graph.value);
}

#[test]
fn graph_query_and_ast_topology_agree_on_typescript_files() {
    let dir = fixture();
    assert_parity(dir.path(), "src/main.ts", "src/lib.ts");
}

#[test]
fn graph_query_and_ast_topology_agree_on_rust_files() {
    let dir = tempfile::tempdir().expect("dir");
    std::fs::create_dir_all(dir.path().join(".git")).expect("git");
    write_all(
        &dir.path().join("app"),
        &[
            (
                "Cargo.toml",
                "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
            ),
            ("src/lib.rs", "pub mod a;\npub mod b;\npub mod api;\n"),
            ("src/a.rs", "use crate::b::bee;\npub fn ay() { bee() }\n"),
            ("src/b.rs", "use crate::a::ay;\npub fn bee() { ay() }\n"),
            (
                "src/api.rs",
                "use crate::a::ay;\npub fn handle() { ay() }\n",
            ),
        ],
    );
    assert_parity(dir.path(), "src/api.rs", "src/a.rs");
}
