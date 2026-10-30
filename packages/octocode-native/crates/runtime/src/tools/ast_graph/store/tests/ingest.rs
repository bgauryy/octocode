//! Ingest: workspace roots, redaction, roles, parsers and denials.
use super::*;

#[test]
fn default_workspace_stops_at_the_allowed_roots() {
    let dir = fixture();
    let sub = dir.0.as_path().join("app");
    assert_eq!(
        default_workspace(sub.clone(), &workspace_policy(dir.0.as_path())),
        dir.0.as_path()
    );
    // A repository root above the allowed root would be denied: use cwd.
    assert_eq!(default_workspace(sub.clone(), &workspace_policy(&sub)), sub);
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
        o.edges = vec!["uses".into()];
    });
    assert_eq!(ids(&uses), vec!["src/main.ts"], "{}", uses.value);
}

#[test]
fn admitted_minified_evidence_is_preserved_and_oversize_is_explicit() {
    let dir = fixture();
    let src = dir.0.as_path().join("app/src");
    // Over the 1 MB parse bound: kept as an `unparsed` file node.
    let big = format!("!function(){{{}}}();\n", "var a=1;".repeat(160_000));
    std::fs::write(src.join("huge.min.js"), &big).expect("huge");
    // Under the bound, minified source retains declarations and call sites.
    let small = (0..200)
        .map(|i| format!("function a{i}(){{return 1}}var b{i}=a{i}();"))
        .collect::<String>();
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
        outline.value["total"], 400,
        "admitted minified declarations must remain queryable: {}",
        outline.value
    );

    let calls = ask(
        dir.0.as_path(),
        "witnesses",
        Some("src/small.min.js"),
        |o| {
            o.edges = vec!["calls".into()];
        },
    );
    assert_eq!(calls.value["total"], 200, "{}", calls.value);

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
            super::super::tables::canonical_language(file, raw),
            label,
            "{file}"
        );
    }
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
        &workspace_policy(dir.0.as_path()),
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
        &workspace_policy(dir.0.as_path()),
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

/// `ingest --language-glob` picks a parser by root-relative glob, as for
/// astSearch: a `.h` header holding C++ reads as C++ (its class is a class)
/// until a glob parses it as C, and the globs are part of what a reuse
/// compares.
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
            &workspace_policy(root),
            &ContentSecurity::new(),
            &NeverCancel,
            None,
        )
    };
    let widget = || {
        let found = ask(root, "find", Some("Widget"), |o| {
            o.kind = Some("symbol".into());
        });
        found.value["results"][0]["symbolKind"].clone()
    };
    let plain = with_globs(&[]);
    assert_eq!(plain.class, ExitClass::Success, "{}", plain.value);
    assert_eq!(widget(), "class", "a C++ header reads as C++");
    let c = with_globs(&[("c", "include/**/*.h")]);
    assert_eq!(c.class, ExitClass::Success, "{}", c.value);
    assert!(
        c.value.get("reused").is_none(),
        "globs changed: {}",
        c.value
    );
    assert_eq!(
        c.value["scan"]["languageGlobs"],
        json!({"c": ["include/**/*.h"]}),
        "{}",
        c.value
    );
    assert_ne!(widget(), "class", "C reads no class");
    let again = with_globs(&[("c", "include/**/*.h")]);
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
        &workspace_policy(root),
        &ContentSecurity::new(),
        &NeverCancel,
        None,
    );
    assert_eq!(out.value["errorCode"], "invalidGlob", "{}", out.value);
    assert_eq!(out.class, ExitClass::InvalidInput, "{}", out.value);
}

#[test]
fn a_published_graph_keeps_completed_extraction_for_reuse() {
    let dir = git_app(&[(
        "a.ts",
        "export function a() { return b(); }\nfunction b() {}\n",
    )]);
    let built = ingest_root(&dir.0, &dir.0.join("app"), None, false);
    assert_eq!(built.class, ExitClass::Success, "{}", built.value);
    let root = dir.0.join("app").canonicalize().unwrap();
    let job = super::super::super::graph::checkpoint::job_path(
        &dir.0.join(".octocode/graph/.facts"),
        &root,
    );
    let manifest: Value =
        serde_json::from_slice(&std::fs::read(job.join("job.json")).unwrap()).unwrap();
    assert_eq!(manifest["stage"], "complete");
    let artifact = job.join(format!("{}.fact", octocode_engine::digest::sha256(b"a.ts")));
    assert!(artifact.is_file());
    std::fs::write(root.join("new.ts"), "export const added = 1;\n").unwrap();
    let rebuilt = ingest_root(&dir.0, &root, None, false);
    assert_eq!(rebuilt.class, ExitClass::Success, "{}", rebuilt.value);
    assert_eq!(rebuilt.value["extraction"]["parsedFiles"], 1);
    assert_eq!(rebuilt.value["extraction"]["reusedFiles"], 1);
    assert_eq!(rebuilt.value["extraction"]["resumedFiles"], 0);
}
