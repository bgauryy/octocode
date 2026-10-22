use super::*;

#[test]
fn oversized_snapshot_is_rejected_before_allocation() {
    let dir = Fixture::new();
    let file = dir.0.as_path().join(GRAPH_FILE);
    std::fs::File::create(&file)
        .expect("file")
        .set_len(MAX_GRAPH_BYTES as u64 + 1)
        .expect("sparse fixture");
    assert!(read_graph_file(&file).is_err());
}
use crate::execution::cancel::NeverCancel;
use crate::tools::ast_graph::test_support::{Fixture, write_under};

fn fixture() -> Fixture {
    let dir = Fixture::new();
    std::fs::create_dir_all(dir.0.as_path().join(".git")).expect("git marker");
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
    write_under(&dir, "app/src", &files);
    dir
}

fn policy(root: &Path) -> PathPolicy {
    crate::tools::test_support::workspace_policy(root)
}

#[test]
fn default_workspace_stops_at_the_allowed_roots() {
    let dir = fixture();
    let sub = dir.0.as_path().join("app");
    assert_eq!(
        default_workspace(sub.clone(), &policy(dir.0.as_path())),
        dir.0.as_path()
    );
    // A repository root above the allowed root would be denied: use cwd.
    assert_eq!(default_workspace(sub.clone(), &policy(&sub)), sub);
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
        None,
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
    let out = ingest_fixture(dir.0.as_path(), None);
    assert_eq!(out.class, ExitClass::Success, "{}", out.value);
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
    let mut previous = bytes.clone();
    previous[8..12].copy_from_slice(&(format::FORMAT_VERSION - 1).to_le_bytes());
    assert!(
        format::decode(&previous)
            .unwrap_err()
            .contains(&format!("v{} is not supported", format::FORMAT_VERSION - 1))
    );
    let mut future = bytes;
    future[8] = 99;
    assert!(
        format::decode(&future)
            .unwrap_err()
            .contains("not supported")
    );
}

/// A credential in source text the graph keeps (here an import specifier,
/// stored as a package name) is redacted before the snapshot reaches the
/// disk; file digests stay as hashed, so staleness checks still hold.
#[test]
fn ingest_redacts_secrets_before_the_snapshot_reaches_the_disk() {
    let dir = Fixture::new();
    std::fs::create_dir_all(dir.0.as_path().join(".git")).expect("git marker");
    let token = format!(
        "ghp_{}",
        "a1B2c3D4e5F6g7H8i9J0".repeat(2).get(..36).expect("token")
    );
    write_under(
        &dir,
        "app/src",
        &[(
            "main.ts",
            &format!("import t from '{token}';\nexport const main = () => t;\n"),
        )],
    );
    let out = ingest_fixture(dir.0.as_path(), None);
    assert_eq!(out.class, ExitClass::Success, "{}", out.value);
    let id = out.value["id"].as_str().expect("id");
    let snapshot = dir.0.as_path().join(".octocode").join("graph").join(id);
    for file in [GRAPH_FILE, MANIFEST_FILE] {
        let bytes = std::fs::read(snapshot.join(file)).expect("snapshot file");
        assert!(
            !String::from_utf8_lossy(&bytes).contains(&token),
            "{file} holds the token"
        );
    }
    let stale = ask(dir.0.as_path(), "stale", None, |_| {});
    assert_eq!(stale.value["fresh"], true, "{}", stale.value);
}

#[test]
fn ingest_publishes_a_snapshot_under_the_workspace() {
    let dir = fixture();
    let out = ingest_fixture(dir.0.as_path(), None);
    assert_eq!(out.class, ExitClass::Success, "{}", out.value);
    let id = out.value["id"].as_str().expect("id");
    assert!(id.contains("-app-"), "{id}");
    let home = dir.0.as_path().join(".octocode").join("graph");
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
fn stale_detects_edits_and_prune_keeps_the_newest_snapshots() {
    let dir = fixture();
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let fresh = ask(dir.0.as_path(), "stale", None, |_| {});
    assert_eq!(
        (fresh.class, &fresh.value["fresh"]),
        (ExitClass::Success, &json!(true)),
        "{}",
        fresh.value
    );

    dir.write("app/src/a.ts", "export const a = 2;\n");
    let stale = ask(dir.0.as_path(), "stale", None, |_| {});
    assert_eq!(
        stale.value["results"],
        json!([{"file": "src/a.ts", "status": "changed"}])
    );

    for _ in 0..2 {
        assert_eq!(
            ingest_fixture(dir.0.as_path(), Some(1)).class,
            ExitClass::Success
        );
    }
    let home = dir.0.as_path().join(".octocode").join("graph");
    assert_eq!(list_snapshots(&home).len(), 1);
}

/// `fixture()` plus a manifest, an undeclared import, an orphan, a
/// generated file, and a test.
fn issues_fixture() -> Fixture {
    let dir = fixture();
    let app = dir.0.as_path().join("app");
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
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let root = dir.0.as_path();
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
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let root = dir.0.as_path();
    let out = ask(root, "issues", None, |o| o.limit = Some(200));
    assert_eq!(out.class, ExitClass::Success, "{}", out.value);

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
    assert_eq!(bad.class, ExitClass::InvalidInput);
}

/// Finding evidence lists every member, export and site it counts: a list
/// is never cut to a sample beside its count.
#[test]
fn issue_evidence_lists_every_item_it_counts() {
    let dir = issues_fixture();
    let src = dir.0.as_path().join("app/src");
    for n in 0..25 {
        std::fs::write(
            src.join(format!("ring{n}.ts")),
            format!(
                "import {{ r{next} }} from './ring{next}';\nimport pad from 'left-pad';\nexport const r{n} = () => r{next}() + pad('');\n",
                next = (n + 1) % 25
            ),
        )
        .expect("ring file");
    }
    let exports = (0..25)
        .map(|n| format!("export const unusedValue{n} = {n};\n"))
        .collect::<String>();
    std::fs::write(src.join("many.ts"), exports).expect("exports");
    // The entry reaches both: an unreachable file is one `unreachable-file`
    // finding, its exports not listed again.
    let main = std::fs::read_to_string(src.join("main.ts")).expect("main");
    std::fs::write(
        src.join("main.ts"),
        format!("import './many';\nimport './ring0';\n{main}"),
    )
    .expect("entry");
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let out = ask(dir.0.as_path(), "issues", None, |o| o.limit = Some(500));
    let rows = out.value["results"].as_array().cloned().unwrap_or_default();
    let finding = |detector: &str, subject: &str| {
        rows.iter()
            .find(|row| {
                row["detector"] == detector
                    && row["subject"].as_str().is_some_and(|s| s.contains(subject))
            })
            .cloned()
            .unwrap_or_else(|| panic!("{detector} {subject}: {}", out.value))
    };
    let cycle = finding("cycle", "ring");
    assert_eq!(cycle["evidence"]["size"], 25, "{cycle}");
    assert_eq!(
        cycle["evidence"]["members"].as_array().map(Vec::len),
        Some(25),
        "{cycle}"
    );
    let unused = finding("unused-export", "many.ts");
    assert_eq!(
        unused["evidence"]["exports"].as_array().map(Vec::len),
        Some(25),
        "{unused}"
    );
    let undeclared = finding("undeclared-dependency", "left-pad");
    let sites = undeclared["evidence"]["sites"]
        .as_array()
        .map(Vec::len)
        .unwrap_or(0);
    assert!(sites >= 26, "{undeclared}");
}

#[test]
fn issues_baseline_reports_new_and_resolved_findings() {
    let dir = issues_fixture();
    let first = ingest_fixture(dir.0.as_path(), None);
    let base = first.value["id"].as_str().expect("id").to_owned();
    dir.write("app/src/b.ts", "export const b = () => 1;\n");
    dir.write("app/src/orphan2.ts", "export const x = 1;\n");
    std::thread::sleep(std::time::Duration::from_millis(1100));
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let out = ask(dir.0.as_path(), "issues", None, |o| {
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
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let root = dir.0.as_path();

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
    let app = dir.0.as_path().join("app");
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
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    std::fs::write(
        app.join("src/fmt.ts"),
        "export const fmt = (s: string) => s;\n",
    )
    .expect("edit");
    let out = ask(dir.0.as_path(), "impact", None, |o| {
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
    let widened = ask(dir.0.as_path(), "impact", None, |o| {
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

/// `--since` runs git on a repository the caller controls; its config
/// must not run programs (an fsmonitor hook executes on every index read).
#[cfg(unix)]
#[test]
fn impact_since_does_not_run_repository_fsmonitor_hooks() {
    use std::os::unix::fs::PermissionsExt;
    let dir = issues_fixture();
    let app = dir.0.as_path().join("app");
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
    let identity = ["-c", "user.email=t@t", "-c", "user.name=t"];
    assert!(git(&[&identity[..], &["add", "-A"]].concat()));
    assert!(git(&[&identity[..], &["commit", "-qm", "base"]].concat()));
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let marker = dir.0.as_path().join("hook-ran");
    let hook = dir.0.as_path().join("fsmonitor.sh");
    std::fs::write(&hook, format!("#!/bin/sh\ntouch '{}'\n", marker.display())).expect("hook");
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    assert!(git(&[
        "config",
        "core.fsmonitor",
        hook.to_str().expect("utf-8 path")
    ]));
    std::fs::write(
        app.join("src/fmt.ts"),
        "export const fmt = (s: string) => s;\n",
    )
    .expect("edit");
    let out = ask(dir.0.as_path(), "impact", None, |o| {
        o.since = Some("HEAD".into())
    });
    assert!(
        impacted(&out).contains(&("src/fmt.ts".into(), 0)),
        "{}",
        out.value
    );
    assert!(!marker.exists(), "the repository fsmonitor hook ran");
}

#[test]
fn oversized_and_minified_files_stay_visible_without_noise() {
    let dir = fixture();
    let src = dir.0.as_path().join("app/src");
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
    let ingest = ingest_fixture(dir.0.as_path(), None);
    assert_eq!(ingest.class, ExitClass::Success, "{}", ingest.value);

    let huge = ask(dir.0.as_path(), "node", Some("src/huge.min.js"), |_| {});
    assert_eq!(huge.class, ExitClass::Success, "{}", huge.value);
    let deps = ask(dir.0.as_path(), "deps", Some("src/uses.ts"), |_| {});
    let rows = deps.value["results"].as_array().expect("rows");
    assert!(
        rows.iter()
            .any(|r| r["id"] == "src/huge.min.js" && r["via"] == "unparsed-target"),
        "{}",
        deps.value
    );
    let outline = ask(dir.0.as_path(), "symbols", Some("src/small.min.js"), |_| {});
    assert_eq!(
        outline.value["total"], 0,
        "minified symbols are dropped: {}",
        outline.value
    );

    let issues = ask(dir.0.as_path(), "issues", None, |o| o.limit = Some(500));
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
    let dir = Fixture::new();
    std::fs::create_dir_all(dir.0.as_path().join(".git")).expect("git");
    let krate = dir.0.as_path().join("app");
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
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let node = ask(
        dir.0.as_path(),
        "node",
        Some("src/lib.rs#tests.adds"),
        |_| {},
    );
    assert_eq!(node.value["node"]["test"], true, "{}", node.value);
    let out = ask(dir.0.as_path(), "impact", Some("src/lib.rs#add"), |_| {});
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

// ── Graph quality acceptance tests (docs/engine/CODE_GRAPH.md § Evaluation) ─

/// A repository root (a `.git` marker) holding `files` under `app/`.
fn git_app(files: &[(&str, &str)]) -> Fixture {
    let dir = Fixture::new();
    std::fs::create_dir_all(dir.0.as_path().join(".git")).expect("git");
    write_under(&dir, "app", files);
    dir
}

#[test]
fn plan_1_call_answers_report_coverage() {
    let dir = fixture();
    write_under(
        &dir,
        "app/src",
        &[(
            "opener.ts",
            "import { Box } from './lib';\nfunction make(): any { return null; }\nexport function run() {\n  const b = make();\n  b.open();\n  return new Box();\n}\n",
        )],
    );
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let out = ask(
        dir.0.as_path(),
        "callers",
        Some("src/lib.ts#Box.open"),
        |_| {},
    );
    let coverage = &out.value["coverage"];
    let recall = coverage["callInternalRecall"].as_f64().expect("recall");
    assert!(recall < 1.0, "{}", out.value);
    assert!(
        coverage["warning"].as_str().is_some_and(|w| !w.is_empty()),
        "{}",
        out.value
    );
    let impact = ask(
        dir.0.as_path(),
        "impact",
        Some("src/lib.ts#Box.open"),
        |_| {},
    );
    assert!(
        impact.value["coverage"]["callInternalRecall"].is_number(),
        "{}",
        impact.value
    );
}

#[test]
fn plan_2_string_referenced_files_are_low_tier_and_hidden_by_default() {
    let dir = issues_fixture();
    write_under(
        &dir,
        "app",
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
    // Reached from the entry: an unreachable file's exports are not judged
    // apart from the file.
    let main = dir.0.as_path().join("app/src/main.ts");
    let text = std::fs::read_to_string(&main).expect("main");
    std::fs::write(&main, format!("import './local';\n{text}")).expect("entry");
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let default = ask(dir.0.as_path(), "issues", None, |o| o.limit = Some(500));
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

    let all = ask(dir.0.as_path(), "issues", None, |o| {
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
    let dir = git_app(&[
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
    ]);
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let callees = ask(
        dir.0.as_path(),
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
    let dir = git_app(&[
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
    ]);
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let out = ask(dir.0.as_path(), "impact", Some("src/a.ts"), |_| {});
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
    let deep = ask(dir.0.as_path(), "impact", Some("src/a.ts"), |o| {
        o.depth = Some(10)
    });
    assert!(impacted(&deep).iter().any(|(f, _)| f == "src/c4.ts"));
}

#[test]
fn plan_9_unchanged_tree_reuses_the_latest_snapshot() {
    let dir = fixture();
    let first = ingest_fixture(dir.0.as_path(), None);
    assert_eq!(first.class, ExitClass::Success);
    let second = ingest_fixture(dir.0.as_path(), None);
    assert_eq!(second.value["reused"], true, "{}", second.value);
    assert_eq!(second.value["id"], first.value["id"]);
    dir.write("app/src/a.ts", "export const a = 5;\n");
    let third = ingest_fixture(dir.0.as_path(), None);
    assert_ne!(third.value["reused"], true, "{}", third.value);
    let forced = ingest(
        &IngestOptions {
            path: dir.0.as_path().join("app"),
            workspace: Some(dir.0.as_path().to_path_buf()),
            force: true,
            ..Default::default()
        },
        &policy(dir.0.as_path()),
        &ContentSecurity::new(),
        &NeverCancel,
        None,
    );
    assert_ne!(forced.value["reused"], true);
}

/// A snapshot of an older format (here v4, before diagnostics named their
/// target file) is never reused: the next ingest rebuilds it, and queries
/// answer from the rebuilt snapshot.
#[test]
fn an_older_format_snapshot_is_rebuilt_not_reused() {
    let dir = fixture();
    let first = ingest_fixture(dir.0.as_path(), None);
    assert_eq!(first.class, ExitClass::Success, "{}", first.value);
    let snapshot = PathBuf::from(first.value["dir"].as_str().expect("dir"));
    let older = format::FORMAT_VERSION - 1;
    let manifest_path = snapshot.join(MANIFEST_FILE);
    let mut manifest: Value =
        serde_json::from_slice(&std::fs::read(&manifest_path).expect("manifest")).expect("json");
    manifest["formatVersion"] = json!(older);
    std::fs::write(&manifest_path, manifest.to_string()).expect("older manifest");
    let mut bytes = std::fs::read(snapshot.join(GRAPH_FILE)).expect("graph.bin");
    bytes[8..12].copy_from_slice(&older.to_le_bytes());
    std::fs::write(snapshot.join(GRAPH_FILE), bytes).expect("older graph");
    let stale = ask(dir.0.as_path(), "stats", None, |_| {});
    assert!(
        stale.value["error"]
            .as_str()
            .is_some_and(|e| e.contains("re-run `octocode graph ingest`")),
        "{}",
        stale.value
    );
    let second = ingest_fixture(dir.0.as_path(), None);
    assert_eq!(second.class, ExitClass::Success, "{}", second.value);
    assert_ne!(second.value["reused"], true, "{}", second.value);
    assert_ne!(second.value["id"], first.value["id"]);
    let stats = ask(dir.0.as_path(), "stats", None, |_| {});
    assert_eq!(stats.class, ExitClass::Success, "{}", stats.value);
}

/// A settled file records its stamp, an equal stamp proves the bytes
/// without a read, and a same-size rewrite (new times) still rebuilds.
#[test]
fn settled_sources_reuse_by_stamp_and_a_same_size_edit_still_rebuilds() {
    let dir = fixture();
    octocode_engine::stamp::settle_written_so_far();
    let first = ingest_fixture(dir.0.as_path(), None);
    assert_eq!(first.class, ExitClass::Success, "{}", first.value);
    let graph_dir = PathBuf::from(first.value["dir"].as_str().expect("dir"));
    let bytes = std::fs::read(graph_dir.join(GRAPH_FILE)).expect("graph.bin");
    let (tables, _) = format::decode(&bytes).expect("decode");
    assert_eq!(tables.digests.len(), 4);
    assert_eq!(tables.stamps.len(), 4, "every settled source is stamped");
    // The unchanged-tree check decodes only the file index, the same one.
    let (files, files_digest) = format::decode_files(&bytes).expect("decode files");
    assert_eq!(files_digest, first.value["sha256"].as_str().expect("sha"));
    assert_eq!(files.strings, tables.strings);
    assert_eq!(files.digests, tables.digests);
    assert_eq!(files.stamps, tables.stamps);
    assert_eq!(files.nodes.len(), tables.nodes.len());
    assert!(files.edges.is_empty() && !tables.edges.is_empty());
    let (node, stamp) = tables
        .stamps
        .iter()
        .find(|(node, _)| tables.str(tables.nodes[*node as usize].key) == "src/a.ts")
        .expect("a.ts stamp");
    let a = dir.0.as_path().join("app/src/a.ts");
    // An equal stamp is trusted without reading: even a wrong digest passes.
    assert_eq!(
        source_matches(&a, "not-the-digest", Some(stamp)),
        Some(true)
    );
    let digest = tables
        .digests
        .iter()
        .find(|(id, _)| id == node)
        .map(|(_, digest)| tables.str(*digest))
        .expect("a.ts digest");
    assert_eq!(source_matches(&a, digest, None), Some(true));
    assert_eq!(
        source_matches(&dir.0.as_path().join("gone.ts"), digest, Some(stamp)),
        None
    );

    let second = ingest_fixture(dir.0.as_path(), None);
    assert_eq!(second.value["reused"], true, "{}", second.value);
    // Same length, new content: the stamp moves, so the hash decides.
    let before = std::fs::read_to_string(&a).expect("a.ts");
    let after = before.replace("b()", "c()");
    assert_eq!(before.len(), after.len());
    std::fs::write(&a, after).expect("edit");
    assert_eq!(source_matches(&a, digest, Some(stamp)), Some(false));
    let third = ingest_fixture(dir.0.as_path(), None);
    assert_eq!(third.class, ExitClass::Success, "{}", third.value);
    assert_ne!(third.value["reused"], true, "{}", third.value);
    let stale = ask(dir.0.as_path(), "stale", None, |_| {});
    assert_eq!(stale.value["fresh"], true, "{}", stale.value);
}

#[test]
fn plan_4_receiver_typed_member_calls_link() {
    let dir = git_app(&[
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
    ]);
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let rust = ask(
        dir.0.as_path(),
        "callees",
        Some("src/service.rs#run"),
        |_| {},
    );
    let rows = rust.value["results"].as_array().expect("rows");
    assert!(
        rows.iter()
            .any(|r| r["id"] == "src/store.rs#Store.save" && r["via"] == "receiver-type"),
        "{}",
        rust.value
    );
    assert!(!ids(&rust).contains(&"src/store.rs#Other.save".to_owned()));
    let typed = ask(
        dir.0.as_path(),
        "callees",
        Some("src/service.rs#typed"),
        |_| {},
    );
    assert!(
        ids(&typed).contains(&"src/store.rs#Store.save".to_owned()),
        "{}",
        typed.value
    );
    let ts = ask(dir.0.as_path(), "callees", Some("web/app.ts#go"), |_| {});
    assert!(
        ids(&ts).contains(&"web/store.ts#Cart.save".to_owned()),
        "{}",
        ts.value
    );
    assert!(!ids(&ts).contains(&"web/store.ts#Box.save".to_owned()));
}

#[test]
fn plan_4_typed_this_field_calls_are_member_calls_not_local_matches() {
    let dir = Fixture::new();
    std::fs::create_dir_all(dir.0.as_path().join(".git")).expect("git");
    write_under(
        &dir,
        "app/src",
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
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let out = ask(
        dir.0.as_path(),
        "callees",
        Some("src/svc.ts#Svc.run"),
        |_| {},
    );
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
    let dir = git_app(&[
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
    ]);
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    for caller in ["src/api.rs#handle", "src/api.rs#nested"] {
        let out = ask(dir.0.as_path(), "callees", Some(caller), |_| {});
        let rows = out.value["results"].as_array().expect("rows");
        assert!(
            rows.iter()
                .any(|r| r["id"] == "src/portable.rs#sanitize" && r["via"] == "module-path"),
            "{caller}: {}",
            out.value
        );
    }
}

/// `crate::a::f()` links through `pub use b::f;` in `a`: a module-path call
/// follows the re-export to the declaring file, like a namespace call does.
#[test]
fn rust_module_path_calls_follow_pub_use_reexports() {
    let dir = git_app(&[
        (
            "Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
        ),
        ("src/lib.rs", "pub mod a;\npub mod api;\n"),
        ("src/a/mod.rs", "mod b;\npub(crate) use b::f;\n"),
        ("src/a/b.rs", "pub fn f() -> u8 { 1 }\n"),
        ("src/api.rs", "pub fn g() -> u8 {\n    crate::a::f()\n}\n"),
    ]);
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let out = ask(dir.0.as_path(), "callees", Some("src/api.rs#g"), |_| {});
    let rows = out.value["results"].as_array().expect("rows");
    assert!(
        rows.iter()
            .any(|r| r["id"] == "src/a/b.rs#f" && r["via"] == "module-path"),
        "{}",
        out.value
    );
}

/// Every file edge carries the line of its own evidence: a Rust path-qualified
/// use (`crate::x::f()`) is its own line.
#[test]
fn path_qualified_rust_edges_keep_their_evidence_line() {
    let dir = git_app(&[
        (
            "Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
        ),
        ("src/lib.rs", "pub mod portable;\npub mod api;\n"),
        (
            "src/portable.rs",
            "pub const LIMIT: u8 = 1;\npub fn sanitize(x: u8) -> u8 { x }\n",
        ),
        (
            "src/api.rs",
            "pub fn handle() -> u8 {\n    crate::portable::sanitize(crate::portable::LIMIT)\n}\n",
        ),
    ]);
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let deps = ask(dir.0.as_path(), "deps", Some("src/api.rs"), |_| {});
    let rows = deps.value["results"].as_array().expect("rows");
    let row = rows
        .iter()
        .find(|r| r["id"] == "src/portable.rs")
        .unwrap_or_else(|| panic!("portable dep: {}", deps.value));
    assert_eq!(row["edgeLine"], json!(2), "{}", deps.value);
    // With a `use` and a later qualified path to one file, the row cites the
    // first reference: the `use` line.
    dir.write(
        "app/src/api.rs",
        "use crate::portable::LIMIT;\npub fn handle() -> u8 {\n    crate::portable::sanitize(LIMIT)\n}\n",
    );
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let deps = ask(dir.0.as_path(), "deps", Some("src/api.rs"), |_| {});
    let rows = deps.value["results"].as_array().expect("rows");
    let row = rows
        .iter()
        .find(|r| r["id"] == "src/portable.rs")
        .unwrap_or_else(|| panic!("portable dep: {}", deps.value));
    assert_eq!(row["edgeLine"], json!(1), "{}", deps.value);
}

/// `include!("gen/types.rs")` makes the included file a dependency of the
/// including one, resolved from the including file's directory.
#[test]
fn rust_include_links_the_included_file() {
    let dir = git_app(&[
        (
            "Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
        ),
        ("src/lib.rs", "pub mod api;\n"),
        (
            "src/api.rs",
            "include!(\"gen/types.rs\");\npub fn handle() -> u8 {\n    generated()\n}\n",
        ),
        ("src/gen/types.rs", "pub fn generated() -> u8 { 1 }\n"),
    ]);
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let deps = ask(dir.0.as_path(), "deps", Some("src/api.rs"), |_| {});
    assert!(
        ids(&deps).contains(&"src/gen/types.rs".to_owned()),
        "{}",
        deps.value
    );
}

/// A stored file's language is the engine registry's selector for its
/// grammar (the TSX grammar keeps `tsx`); an extension no grammar claims
/// keeps the parser's own label.
#[test]
fn stored_languages_are_the_registry_selectors() {
    for (file, raw, label) in [
        ("a.ts", "ts", "typescript"),
        ("a.mts", "", "typescript"),
        ("a.tsx", "tsx", "tsx"),
        ("a.jsx", "jsx", "javascript"),
        ("a.cjs", "", "javascript"),
        ("a.pyi", "", "python"),
        ("a.h", "", "c"),
        ("a.hh", "", "cpp"),
        ("a.s", "", "asm"),
        ("a.sbt", "", "scala"),
        ("a.cs", "", "csharp"),
        ("a.kt", "kotlin", "kotlin"),
        ("a.unknown", "raw", "raw"),
    ] {
        assert_eq!(
            super::tables::canonical_language(file, raw),
            label,
            "{file}"
        );
    }
}

/// One fixture per edge class a cycle can run through: a runtime import
/// loop, a type-only one, one closed by a dynamic `import()`, one closed by
/// a Python import inside a function (it runs on call), a Rust `use` loop,
/// and a Rust module tree (a parent calling its child, the child using
/// `super::`), which is no dependency cycle.
fn cycle_classes_app() -> Fixture {
    git_app(&[
        (
            "Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
        ),
        ("src/lib.rs", "pub mod ra;\npub mod rb;\npub mod tree;\n"),
        (
            "src/ra.rs",
            "use crate::rb::bee;\npub fn ay() -> u8 { bee() }\n",
        ),
        (
            "src/rb.rs",
            "use crate::ra::ay;\npub fn bee() -> u8 { ay() }\n",
        ),
        (
            "src/tree/mod.rs",
            "pub mod child;\npub fn top() -> u8 { child::leaf() }\n",
        ),
        (
            "src/tree/child.rs",
            "use super::top;\npub fn leaf() -> u8 { 1 }\npub fn up() -> u8 { top() }\n",
        ),
        (
            "web/a.ts",
            "import { b } from './b';\nexport const a = () => b();\n",
        ),
        (
            "web/b.ts",
            "import { a } from './a';\nexport const b = () => a();\n",
        ),
        (
            "web/t1.ts",
            "import type { T2 } from './t2';\nexport type T1 = { next: T2 };\n",
        ),
        (
            "web/t2.ts",
            "import type { T1 } from './t1';\nexport type T2 = { prev: T1 };\n",
        ),
        (
            "web/d1.ts",
            "import { d2 } from './d2';\nexport const d1 = () => d2();\n",
        ),
        (
            "web/d2.ts",
            "export const d2 = () => import('./d1').then((m) => m.d1);\n",
        ),
        (
            "p1.py",
            "from p2 import two\n\ndef one():\n    return two()\n",
        ),
        (
            "p2.py",
            "def two():\n    from p1 import one\n    return one\n",
        ),
    ])
}

/// Cycle groups as sorted member lists, with whether each is a runtime
/// (module-load) cycle.
type Groups = std::collections::BTreeSet<(Vec<String>, bool)>;

/// `graph query cycles` and the `cycle` detector of `graph query issues`
/// read one cycle engine: the same groups, the same runtime verdicts.
#[test]
fn every_cycle_surface_reports_the_same_groups() {
    let dir = cycle_classes_app();
    let root = dir.0.as_path();
    assert_eq!(ingest_fixture(root, None).class, ExitClass::Success);
    let group = |files: &[&str], runtime: bool| {
        (
            files.iter().map(|f| (*f).to_owned()).collect::<Vec<_>>(),
            runtime,
        )
    };
    let expected = Groups::from([
        group(&["p1.py", "p2.py"], false),
        group(&["src/ra.rs", "src/rb.rs"], false),
        group(&["web/a.ts", "web/b.ts"], true),
        group(&["web/d1.ts", "web/d2.ts"], false),
        group(&["web/t1.ts", "web/t2.ts"], false),
    ]);
    let listed = ask(root, "cycles", None, |_| {});
    let listed = listed.value["results"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|row| {
            let files = row["nodes"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|file| file.as_str().map(str::to_owned))
                .collect::<Vec<_>>();
            (files, row["runtime"] == true)
        })
        .collect::<Groups>();
    assert_eq!(listed, expected, "graph query cycles");
    let issues = ask(root, "issues", None, |o| {
        o.detectors = vec!["cycle".into()];
        o.limit = Some(50);
    });
    let found = issues.value["results"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|row| {
            let mut files = row["evidence"]["members"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|file| file.as_str().map(str::to_owned))
                .collect::<Vec<_>>();
            files.sort();
            (files, row["evidence"]["runtime"] == true)
        })
        .collect::<Groups>();
    assert_eq!(found, expected, "graph query issues: {}", issues.value);
}

/// A TypeScript package for the dead-code detectors: an entry, a
/// used module with one unused export, an orphan file, a pair of files only
/// each other import, a barrel, a default export, a test, and declaration
/// files (one beside a used module, one nothing imports).
fn dead_code_app() -> Fixture {
    git_app(&[
        ("package.json", r#"{"name":"app","main":"src/main.ts"}"#),
        (
            "src/main.ts",
            "import { used } from './lib';\nimport run from './runner';\nimport { fromBarrel } from './barrel';\nexport const main = () => used() + run() + fromBarrel();\n",
        ),
        (
            "src/lib.ts",
            "export function used() { return 1; }\nexport function unused() { return 2; }\n",
        ),
        (
            "src/runner.ts",
            "export default function run() { return 3; }\n",
        ),
        ("src/barrel.ts", "export * from './inner';\n"),
        (
            "src/inner.ts",
            "export const fromBarrel = () => 4;\nexport const alsoExported = 5;\n",
        ),
        ("src/orphan.ts", "export const lonely = 1;\n"),
        (
            "src/ping.ts",
            "import { pong } from './pong';\nexport const ping = () => pong();\n",
        ),
        (
            "src/pong.ts",
            "import { ping } from './ping';\nexport const pong = () => ping();\n",
        ),
        (
            "src/lib.test.ts",
            "import { used } from './lib';\nused();\n",
        ),
        (
            "src/runner.d.ts",
            "export default function run(): number;\n",
        ),
        ("types/extra.d.mts", "export declare const extra: number;\n"),
    ])
}

/// The store's `unreachable-file` and `unused-export` detectors judge the
/// dead-code fixture: the orphan and the pair only each other import are
/// unreachable, and only the exports no importer names are unused.
#[test]
fn dead_code_detectors_judge_one_fixture() {
    let dir = dead_code_app();
    let root = dir.0.as_path();
    assert_eq!(ingest_fixture(root, None).class, ExitClass::Success);
    let issues = ask(root, "issues", None, |o| {
        o.limit = Some(200);
        o.min_tier = Some(0);
    });
    let rows = issues.value["results"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let store_unreachable = rows
        .iter()
        .filter(|row| row["detector"] == "unreachable-file")
        .filter_map(|row| row["subject"].as_str().map(str::to_owned))
        .collect::<std::collections::BTreeSet<_>>();
    let store_unused = rows
        .iter()
        .filter(|row| row["detector"] == "unused-export")
        .flat_map(|row| {
            let file = row["subject"].as_str().unwrap_or_default().to_owned();
            row["evidence"]["exports"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|name| name.as_str().map(|name| format!("{file}#{name}")))
                .collect::<Vec<_>>()
        })
        .collect::<std::collections::BTreeSet<_>>();
    let set = |items: &[&str]| {
        items
            .iter()
            .map(|item| (*item).to_owned())
            .collect::<std::collections::BTreeSet<_>>()
    };
    assert_eq!(
        store_unreachable,
        set(&["src/orphan.ts", "src/ping.ts", "src/pong.ts"]),
        "{}",
        issues.value
    );
    assert_eq!(
        store_unused,
        set(&["src/inner.ts#alsoExported", "src/lib.ts#unused"]),
        "{}",
        issues.value
    );
}

fn ingest_root(dir: &Path, root: &Path, keep: Option<usize>, force: bool) -> GraphOutput {
    ingest(
        &IngestOptions {
            path: root.to_path_buf(),
            workspace: Some(dir.to_path_buf()),
            keep,
            force,
            ..Default::default()
        },
        &policy(dir),
        &ContentSecurity::new(),
        &NeverCancel,
        None,
    )
}

fn stale_rows(dir: &Path) -> (Value, Value) {
    let out = ask(dir, "stale", None, |_| {});
    (out.value["fresh"].clone(), out.value["results"].clone())
}

/// A linking config (tsconfig, package.json, ...) beside the sources or up
/// the repository is a build input: editing, adding or removing one makes
/// the snapshot stale and stops ingest from reusing it, like a source edit.
#[test]
fn config_inputs_make_a_snapshot_stale_and_block_reuse() {
    let dir = fixture();
    dir.write("app/tsconfig.json", "{\"compilerOptions\":{}}");
    dir.write("package.json", "{\"name\":\"repo\"}");
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    assert_eq!(stale_rows(dir.0.as_path()).0, json!(true));
    assert_eq!(ingest_fixture(dir.0.as_path(), None).value["reused"], true);

    dir.write(
        "app/tsconfig.json",
        "{\"compilerOptions\":{\"paths\":{\"@x/*\":[\"src/*\"]}}}",
    );
    assert_eq!(
        stale_rows(dir.0.as_path()),
        (
            json!(false),
            json!([{"file": "tsconfig.json", "status": "changed", "config": true}])
        )
    );
    let rebuilt = ingest_fixture(dir.0.as_path(), None);
    assert_ne!(rebuilt.value["reused"], true, "{}", rebuilt.value);
    assert_eq!(stale_rows(dir.0.as_path()).0, json!(true));

    dir.write(
        "app/package.json",
        "{\"name\":\"app\",\"main\":\"src/main.ts\"}",
    );
    std::fs::remove_file(dir.0.as_path().join("package.json")).expect("remove");
    assert_eq!(
        stale_rows(dir.0.as_path()).1,
        json!([
            {"file": "../package.json", "status": "missing", "config": true},
            {"file": "package.json", "status": "added", "config": true},
        ])
    );
    assert_ne!(ingest_fixture(dir.0.as_path(), None).value["reused"], true);
}

/// `stale` rescans the scope like ingest: a source added since ingest is
/// reported, so the snapshot is never called fresh while a re-ingest would
/// rebuild it.
#[test]
fn stale_reports_sources_added_since_ingest() {
    let dir = fixture();
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    dir.write("app/src/z.ts", "export const z = 1;\n");
    dir.write("app/src/deep/new/y.ts", "export const y = 1;\n");
    let out = ask(dir.0.as_path(), "stale", None, |_| {});
    assert_eq!(out.value["fresh"], false, "{}", out.value);
    assert_eq!(
        out.value["results"],
        json!([
            {"file": "src/deep/new/y.ts", "status": "added"},
            {"file": "src/z.ts", "status": "added"},
        ])
    );
    assert!(out.value["reingest"].is_string(), "{}", out.value);
    assert_ne!(ingest_fixture(dir.0.as_path(), None).value["reused"], true);
}

/// A snapshot whose scan stopped at maxFiles cannot tell an added file
/// from one past the cap, so it is never called fresh; its re-ingest keeps
/// the snapshot's exclusions and raises maxFiles, the limit that cut it.
#[test]
fn a_truncated_snapshot_is_never_fresh() {
    let dir = fixture();
    let out = ingest(
        &IngestOptions {
            path: dir.0.as_path().join("app"),
            workspace: Some(dir.0.as_path().to_path_buf()),
            max_files: Some(2),
            exclude_dir: vec!["gen".into()],
            ..Default::default()
        },
        &policy(dir.0.as_path()),
        &ContentSecurity::new(),
        &NeverCancel,
        None,
    );
    assert_eq!(out.value["scan"]["truncated"], true, "{}", out.value);
    let stale = ask(dir.0.as_path(), "stale", None, |_| {});
    assert_eq!(stale.value["fresh"], false, "{}", stale.value);
    assert!(
        stale.value["unchecked"]
            .as_str()
            .is_some_and(|why| why.contains("maxFiles (2)")),
        "{}",
        stale.value
    );
    // The cut's re-ingest is stated once, in the graph block.
    assert!(stale.value.get("reingest").is_none(), "{}", stale.value);
    let reingest = stale.value["graph"]["reingest"].as_str().expect("reingest");
    assert!(
        reingest.ends_with(" --max-files 4 --exclude gen"),
        "{reingest}"
    );
}

/// The edge cap does not cut the file scan: a capped snapshot whose files
/// were all scanned still reports files added since ingest, names no
/// maxFiles cut, and is fresh once nothing changed.
#[test]
fn an_edge_capped_snapshot_still_checks_added_files() {
    struct EdgeCap;
    impl Drop for EdgeCap {
        fn drop(&mut self) {
            crate::tools::ast_graph::graph::EDGE_CAP
                .with(|edges| edges.set(crate::tools::ast_graph::graph::MAX_GRAPH_EDGES));
        }
    }
    crate::tools::ast_graph::graph::EDGE_CAP.with(|edges| edges.set(1));
    let _restore = EdgeCap;
    let dir = fixture();
    let out = ingest_fixture(dir.0.as_path(), None);
    assert_eq!(out.value["scan"]["edgesCapped"], true, "{}", out.value);
    assert_eq!(out.value["scan"]["truncated"], false, "{}", out.value);
    let fresh = ask(dir.0.as_path(), "stale", None, |_| {});
    assert_eq!(fresh.value["fresh"], true, "{}", fresh.value);
    assert!(fresh.value.get("reingest").is_none(), "{}", fresh.value);
    dir.write("app/src/z.ts", "export const z = 1;\n");
    let stale = ask(dir.0.as_path(), "stale", None, |_| {});
    assert_eq!(
        stale.value["results"],
        json!([{"file": "src/z.ts", "status": "added"}]),
        "{}",
        stale.value
    );
    assert!(stale.value.get("unchecked").is_none(), "{}", stale.value);
    let reingest = stale.value["reingest"].as_str().expect("reingest");
    assert!(!reingest.contains("--max-files"), "{reingest}");
}

/// An export name of any length is text-scanned: a short name loaded
/// dynamically (`mod.run(...)`) is a text reference, not "no reference".
#[test]
fn short_export_names_are_text_checked() {
    let dir = fixture();
    dir.write(
        "app/src/main.ts",
        "import { other } from './cmd';\nexport function main() {\n  return other();\n}\n",
    );
    dir.write(
        "app/src/cmd.ts",
        "export function other() { return 1; }\nexport function run() { return 2; }\n",
    );
    dir.write(
        "app/src/loader.ts",
        "export async function load(file: string) {\n  const mod = await import(file);\n  return mod.run(1);\n}\n",
    );
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let all = ask(dir.0.as_path(), "issues", None, |o| {
        o.detectors = vec!["unused-export".into()];
        o.min_tier = Some(0);
        o.limit = Some(500);
    });
    let row = all.value["results"]
        .as_array()
        .expect("rows")
        .iter()
        .find(|row| row["subject"] == "src/cmd.ts")
        .cloned()
        .unwrap_or_else(|| panic!("cmd.ts unused export: {}", all.value));
    assert_eq!(row["evidence"]["exports"], json!(["run"]), "{row}");
    assert_eq!(row["tier"], 60, "{row}");
    assert_eq!(
        row["evidence"]["mentionedIn"]["run"],
        json!(["src/loader.ts:3"]),
        "{row}"
    );
}

/// Prose naming an export is not a reference to it: an export mentioned
/// only in docs stays tier 90 with every prose mention as context, and an
/// export a loader reaches keeps that code reference in `mentionedIn`
/// however many docs sort before it.
#[test]
fn prose_mentions_never_hide_an_unused_export() {
    let dir = fixture();
    dir.write(
        "app/src/main.ts",
        "import { other } from './cmd';\nimport { helper } from './tool';\nexport function main() {\n  return other() + helper();\n}\n",
    );
    dir.write(
        "app/src/cmd.ts",
        "export function other() { return 1; }\nexport function run() { return 2; }\n",
    );
    dir.write(
        "app/src/tool.ts",
        "export function helper() { return 1; }\nexport function go() { return 2; }\n",
    );
    dir.write(
        "app/src/loader.ts",
        "export async function load(file: string) {\n  const mod = await import(file);\n  return mod.go(1);\n}\n",
    );
    for doc in ["a/README.md", "b/NOTES.md", "c/OPS.md", "d/MORE.md"] {
        dir.write(
            &format!("app/docs/{doc}"),
            "# Usage\n\nThen run it, or go home.\n",
        );
    }
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let all = ask(dir.0.as_path(), "issues", None, |o| {
        o.detectors = vec!["unused-export".into()];
        o.min_tier = Some(0);
        o.limit = Some(500);
    });
    let row = |subject: &str| {
        all.value["results"]
            .as_array()
            .expect("rows")
            .iter()
            .find(|row| row["subject"] == subject)
            .cloned()
            .unwrap_or_else(|| panic!("{subject} unused export: {}", all.value))
    };
    let prose = json!([
        "docs/a/README.md:3",
        "docs/b/NOTES.md:3",
        "docs/c/OPS.md:3",
        "docs/d/MORE.md:3",
    ]);
    let cmd = row("src/cmd.ts");
    assert_eq!(cmd["evidence"]["exports"], json!(["run"]), "{cmd}");
    assert!(cmd["tier"].as_u64().is_some_and(|tier| tier >= 90), "{cmd}");
    assert!(cmd["evidence"].get("mentionedIn").is_none(), "{cmd}");
    assert_eq!(cmd["evidence"]["textMentions"]["run"], prose, "{cmd}");
    let tool = row("src/tool.ts");
    assert_eq!(tool["tier"], 60, "{tool}");
    assert_eq!(
        tool["evidence"]["mentionedIn"]["go"],
        json!(["src/loader.ts:3"]),
        "{tool}"
    );
    assert_eq!(tool["evidence"]["textMentions"]["go"], prose, "{tool}");
}

/// `--graph` takes an exact id, else the one root whose scope it names,
/// else the one id containing it; anything that names several snapshots
/// is ambiguous (exit 2, candidates listed), never a silent pick.
#[test]
fn graph_selection_is_exact_or_ambiguous() {
    let dir = fixture();
    let app = ingest_fixture(dir.0.as_path(), None);
    let src = ingest_root(
        dir.0.as_path(),
        &dir.0.as_path().join("app/src"),
        None,
        false,
    );
    let (app_id, src_id) = (
        app.value["id"].as_str().expect("id").to_owned(),
        src.value["id"].as_str().expect("id").to_owned(),
    );
    let stats = |spec: &str| {
        ask(dir.0.as_path(), "stats", None, |o| {
            o.graph = Some(spec.into())
        })
    };
    assert_eq!(
        stats("app").value["graph"]["id"],
        app_id.as_str(),
        "scope app is exact"
    );
    assert_eq!(stats("app-src").value["graph"]["id"], src_id.as_str());
    assert_eq!(stats(&app_id).value["graph"]["id"], app_id.as_str());
    let ambiguous = stats("ap");
    assert_eq!(
        ambiguous.class,
        ExitClass::InvalidInput,
        "{}",
        ambiguous.value
    );
    assert_eq!(
        ambiguous.value["candidates"],
        json!([app_id.clone(), src_id.clone()])
    );
    assert_eq!(
        stats(&src_id["20".len()..]).value["graph"]["id"],
        src_id.as_str(),
        "a substring of exactly one id"
    );
}

/// Snapshots are pruned per root: two roots whose names share one slug
/// (`a-b` and `a/b`) never delete each other's snapshots, and their ids
/// differ.
#[test]
fn prune_groups_snapshots_by_root_not_by_slug() {
    let dir = fixture();
    dir.write("a-b/x.ts", "export const x = 1;\n");
    dir.write("a/b/y.ts", "export const y = 1;\n");
    let dash = ingest_root(
        dir.0.as_path(),
        &dir.0.as_path().join("a-b"),
        Some(1),
        false,
    );
    let mut nested = Vec::new();
    for _ in 0..3 {
        let out = ingest_root(dir.0.as_path(), &dir.0.as_path().join("a/b"), Some(1), true);
        assert_eq!(out.class, ExitClass::Success, "{}", out.value);
        nested.push(out.value["id"].as_str().expect("id").to_owned());
    }
    let dash_id = dash.value["id"].as_str().expect("id");
    assert!(!nested.contains(&dash_id.to_owned()), "{nested:?}");
    let home = dir.0.as_path().join(".octocode").join("graph");
    let left = list_snapshots(&home);
    assert!(left.contains(&dash_id.to_owned()), "{left:?}");
    assert_eq!(left.len(), 2, "one per root: {left:?}");
}

/// The manifest root is outside the graph digest, so a query checks it
/// against the allowed roots before reading, hashing or scanning anything
/// under it.
#[test]
fn a_manifest_root_outside_the_allowed_roots_is_refused() {
    let dir = fixture();
    let out = ingest_fixture(dir.0.as_path(), None);
    let snapshot = PathBuf::from(out.value["dir"].as_str().expect("dir"));
    let outside = crate::tools::test_support::Fixture::new();
    outside.write("src/a.ts", "export const a = 1;\n");
    let manifest_path = snapshot.join(MANIFEST_FILE);
    let mut manifest: Value =
        serde_json::from_slice(&std::fs::read(&manifest_path).expect("manifest")).expect("json");
    manifest["root"] = json!(std::fs::canonicalize(outside.0.as_path()).expect("real"));
    std::fs::write(&manifest_path, serde_json::to_vec(&manifest).expect("json")).expect("tamper");
    for op in ["stale", "issues", "stats"] {
        let refused = ask(dir.0.as_path(), op, None, |_| {});
        assert_eq!(
            refused.value["errorCode"], "outsideAllowedRoots",
            "{op}: {}",
            refused.value
        );
        assert!(refused.value.get("results").is_none(), "{op}");
    }
}

/// `impact --since` with nothing changed is an empty answer (exit 1) that
/// says why, not an input error.
#[test]
fn impact_since_with_no_changes_is_empty() {
    let dir = fixture();
    let app = dir.0.as_path().join("app");
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .arg("-C")
            .arg(&app)
            .args(["-c", "user.email=t@t", "-c", "user.name=t"])
            .args(args)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    };
    if !git(&["init", "-q"]) {
        return; // git unavailable in this environment
    }
    assert!(git(&["add", "-A"]) && git(&["commit", "-qm", "base"]));
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let out = ask(dir.0.as_path(), "impact", None, |o| {
        o.since = Some("HEAD".into())
    });
    assert_eq!(out.class, ExitClass::Empty, "{}", out.value);
    assert_eq!(out.value["total"], 0, "{}", out.value);
    assert_eq!(
        out.value["summary"]["reason"], "no files changed since HEAD",
        "{}",
        out.value
    );
    let bare = ask(dir.0.as_path(), "impact", None, |_| {});
    assert_eq!(bare.class, ExitClass::InvalidInput, "{}", bare.value);
}

/// The pages of an `issues` listing slice one listing: page 1 is truncated
/// and the next page states the same total.
#[test]
fn issues_pages_slice_one_listing() {
    let dir = issues_fixture();
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let first = ask(dir.0.as_path(), "issues", None, |o| {
        o.limit = Some(1);
        o.min_tier = Some(0);
    });
    assert_eq!(first.value["truncated"], true, "{}", first.value);
    let second = ask(dir.0.as_path(), "issues", None, |o| {
        o.limit = Some(1);
        o.offset = Some(1);
        o.min_tier = Some(0);
    });
    assert_eq!(second.value["total"], first.value["total"]);
}

/// A workspace outside the allowed roots is refused with the shared
/// path-policy code, as the scan root is.
#[test]
fn ingest_denials_share_the_path_policy_codes() {
    let dir = fixture();
    let outside = crate::tools::test_support::Fixture::new();
    let denied = ingest(
        &IngestOptions {
            path: dir.0.as_path().join("app"),
            workspace: Some(outside.0.as_path().to_path_buf()),
            ..Default::default()
        },
        &policy(dir.0.as_path()),
        &ContentSecurity::new(),
        &NeverCancel,
        None,
    );
    assert_eq!(
        denied.value["errorCode"], "outsideAllowedRoots",
        "{}",
        denied.value
    );
    let scan = ingest(
        &IngestOptions {
            path: outside.0.as_path().to_path_buf(),
            workspace: Some(dir.0.as_path().to_path_buf()),
            ..Default::default()
        },
        &policy(dir.0.as_path()),
        &ContentSecurity::new(),
        &NeverCancel,
        None,
    );
    assert_eq!(
        scan.value["errorCode"], "outsideAllowedRoots",
        "{}",
        scan.value
    );
}

#[test]
fn authored_lib_javascript_keeps_its_symbols() {
    let dir = fixture();
    write_under(
        &dir,
        "app/lib",
        &[("math.js", "export function add(a, b) { return a + b; }\n")],
    );
    let ingested = ingest_fixture(dir.0.as_path(), None);
    assert_eq!(ingested.class, ExitClass::Success, "{}", ingested.value);
    let outline = ask(dir.0.as_path(), "symbols", Some("lib/math.js"), |_| {});
    assert_eq!(ids(&outline), vec!["lib/math.js#add"], "{}", outline.value);
}

// ── Drift, explicit entries, re-export dependents, scan cuts, parsers ───────

/// A file's dependents include the files that use its exports through a
/// re-exporting module, and only the ones naming one of those exports: an
/// importer of another item of the barrel does not depend on it.
#[test]
fn dependents_include_importers_through_a_reexporting_module() {
    let dir = git_app(&[
        (
            "src/a.ts",
            "export const a = 1;\nexport const unused = 2;\n",
        ),
        ("src/b.ts", "export const b = 2;\n"),
        (
            "src/index.ts",
            "export { a } from './a';\nexport { b } from './b';\n",
        ),
        (
            "src/main.ts",
            "import { a } from './index';\nconsole.log(a);\n",
        ),
        (
            "src/other.ts",
            "import { b } from './index';\nconsole.log(b);\n",
        ),
    ]);
    let root = dir.0.as_path();
    assert_eq!(ingest_fixture(root, None).class, ExitClass::Success);
    let out = ask(root, "dependents", Some("src/a.ts"), |_| {});
    assert_eq!(
        ids(&out),
        vec!["src/index.ts", "src/main.ts"],
        "{}",
        out.value
    );
    let through = &out.value["results"][1];
    assert_eq!(through["edge"], "uses", "{through}");
    assert_eq!(through["via"], "reexport", "{through}");
    assert_eq!(through["depth"], 1, "{through}");
    assert_eq!(through["edgeLine"], 1, "{through}");
    // An explicit edge filter walks exactly the edges it names.
    let imports = ask(root, "dependents", Some("src/a.ts"), |o| {
        o.edges = vec!["imports".into()]
    });
    assert_eq!(ids(&imports), vec!["src/index.ts"], "{}", imports.value);
}

/// `issues --entry` names the entrypoints in place of the inferred ones:
/// reachability and unused exports are judged from them, and an entry that
/// matches no node is a not-found error.
#[test]
fn issues_take_explicit_entrypoints_in_place_of_the_inferred_ones() {
    let dir = issues_fixture();
    let root = dir.0.as_path();
    assert_eq!(ingest_fixture(root, None).class, ExitClass::Success);
    let inferred = ask(root, "issues", None, |o| {
        o.detectors = vec!["unreachable-file".into(), "test-only".into()];
        o.limit = Some(200);
        o.min_tier = Some(0);
    });
    assert!(findings(&inferred, "unreachable-file").contains(&"src/orphan.ts".to_owned()));
    let explicit = ask(root, "issues", None, |o| {
        o.detectors = vec!["unreachable-file".into(), "test-only".into()];
        o.entries = vec!["src/orphan.ts".into()];
        o.limit = Some(200);
        o.min_tier = Some(0);
    });
    let unreachable = findings(&explicit, "unreachable-file");
    assert!(
        !unreachable.contains(&"src/orphan.ts".to_owned()),
        "{}",
        explicit.value
    );
    // main.ts is no entry now: only its test reaches it.
    assert!(
        findings(&explicit, "test-only").contains(&"src/main.ts".to_owned()),
        "{}",
        explicit.value
    );
    assert_eq!(
        explicit.value["summary"]["entrypoints"],
        json!({"total": 1, "byRule": {"explicit": 1}}),
        "{}",
        explicit.value
    );
    let next = explicit.value.get("next").and_then(Value::as_str);
    assert!(
        next.is_none_or(|next| next.contains("--entry src/orphan.ts")),
        "{next:?}"
    );
    let missing = ask(root, "issues", None, |o| {
        o.entries = vec!["src/nope.ts".into()]
    });
    assert_eq!(
        missing.class,
        ExitClass::Failed(crate::runtime::FailureKind::NotFound),
        "{}",
        missing.value
    );
}

/// `drift --baseline` diffs two snapshots by root-relative key: files
/// added, removed and edited, cycles added and resolved, imports that
/// turned dynamic, and relations added and removed.
#[test]
fn drift_diffs_two_snapshots_by_file_cycle_transition_and_relation() {
    let dir = fixture();
    let root = dir.0.as_path();
    let base = ingest_fixture(root, None);
    let base_id = base.value["id"].as_str().expect("id").to_owned();
    dir.write("app/src/b.ts", "export const b = () => 1;\n");
    dir.write(
        "app/src/main.ts",
        "import React from 'react';\nexport async function main() {\n  const { greet } = await import('./lib');\n  return greet('x');\n}\n",
    );
    dir.write("app/src/added.ts", "export const added = 1;\n");
    std::thread::sleep(std::time::Duration::from_millis(1100));
    let head = ingest_fixture(root, None);
    assert_eq!(head.class, ExitClass::Success, "{}", head.value);

    let out = ask(root, "drift", None, |o| {
        o.baseline = Some(base_id.clone());
        o.limit = Some(500);
    });
    assert_eq!(out.class, ExitClass::Success, "{}", out.value);
    let rows = out.value["results"].as_array().expect("rows");
    let has = |row: Value| rows.contains(&row);
    assert!(
        has(json!({"category":"file","change":"added","file":"src/added.ts"})),
        "{}",
        out.value
    );
    assert!(
        has(json!({"category":"file","change":"changed","file":"src/b.ts"})),
        "{}",
        out.value
    );
    assert!(
        has(json!({"category":"cycle","change":"resolved","files":["src/a.ts","src/b.ts"]})),
        "{}",
        out.value
    );
    assert!(
        has(
            json!({"category":"transition","change":"staticToDynamic","from":"src/main.ts","to":"src/lib.ts"})
        ),
        "{}",
        out.value
    );
    assert!(
        has(
            json!({"category":"relation","change":"removed","from":"src/b.ts","to":"src/a.ts","edge":"imports"})
        ),
        "{}",
        out.value
    );
    // A transition is not also an added and removed relation.
    assert!(
        !rows.iter().any(|row| row["category"] == "relation"
            && row["from"] == "src/main.ts"
            && row["to"] == "src/lib.ts"),
        "{}",
        out.value
    );
    let summary = &out.value["summary"];
    assert_eq!(summary["filesAdded"], 1, "{summary}");
    assert_eq!(summary["cyclesResolved"], 1, "{summary}");
    assert_eq!(summary["staticToDynamic"], 1, "{summary}");
    assert_eq!(
        out.value["baseline"]["id"],
        base_id.as_str(),
        "{}",
        out.value
    );
    // File rows lead.
    assert_eq!(rows[0]["category"], "file", "{}", out.value);

    let same = ask(root, "drift", None, |o| {
        o.graph = head.value["id"].as_str().map(str::to_owned);
        o.baseline = head.value["id"].as_str().map(str::to_owned);
    });
    assert_eq!(same.class, ExitClass::Empty, "{}", same.value);
    let missing = ask(root, "drift", None, |_| {});
    assert_eq!(missing.class, ExitClass::InvalidInput, "{}", missing.value);
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

/// `ingest --language-glob` picks a parser by root-relative glob, as for
/// astSearch: a C++ header named `.h` declares its class only when a glob
/// parses it as C++, and the globs are part of what a reuse compares.
#[test]
fn ingest_language_globs_choose_the_parser() {
    let dir = git_app(&[("include/widget.h", "namespace Space { class Widget {}; }\n")]);
    let root = dir.0.as_path();
    let with_globs = |globs: &[(&str, &str)]| {
        ingest(
            &IngestOptions {
                path: root.join("app"),
                workspace: Some(root.to_path_buf()),
                language_globs: globs
                    .iter()
                    .map(|(language, glob)| ((*language).to_owned(), vec![(*glob).to_owned()]))
                    .collect(),
                ..Default::default()
            },
            &policy(root),
            &ContentSecurity::new(),
            &NeverCancel,
            None,
        )
    };
    let widget = || {
        let found = ask(root, "find", Some("Widget"), |o| {
            o.kind = Some("symbol".into())
        });
        found.value["results"][0]["symbolKind"].clone()
    };
    let plain = with_globs(&[]);
    assert_eq!(plain.class, ExitClass::Success, "{}", plain.value);
    assert_ne!(widget(), "class", "C reads no class");
    let cpp = with_globs(&[("cpp", "include/**/*.h")]);
    assert_eq!(cpp.class, ExitClass::Success, "{}", cpp.value);
    assert!(
        cpp.value.get("reused").is_none(),
        "globs changed: {}",
        cpp.value
    );
    assert_eq!(
        cpp.value["scan"]["languageGlobs"],
        json!({"cpp": ["include/**/*.h"]}),
        "{}",
        cpp.value
    );
    assert_eq!(widget(), "class");
    let again = with_globs(&[("cpp", "include/**/*.h")]);
    assert_eq!(again.value["reused"], true, "{}", again.value);
    let stale = ask(root, "stale", None, |_| {});
    assert_eq!(stale.value["fresh"], true, "{}", stale.value);
}

/// A malformed `--language-glob` is refused, never matched as nothing.
#[test]
fn a_malformed_language_glob_is_an_invalid_glob_error() {
    let dir = git_app(&[("a.ts", "export const a = 1;\n")]);
    let root = dir.0.as_path();
    let out = ingest(
        &IngestOptions {
            path: root.join("app"),
            workspace: Some(root.to_path_buf()),
            language_globs: BTreeMap::from([("cpp".to_owned(), vec!["[bad".to_owned()])]),
            ..Default::default()
        },
        &policy(root),
        &ContentSecurity::new(),
        &NeverCancel,
        None,
    );
    assert_eq!(out.value["errorCode"], "invalidGlob", "{}", out.value);
    assert_eq!(out.class, ExitClass::InvalidInput, "{}", out.value);
}
