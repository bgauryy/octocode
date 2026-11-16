//! Ingest: redaction, roles and parsers; `denials` holds workspace roots and refusals.
use super::*;

mod denials;

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
    let vias = deps.value["rows"]
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
    assert_eq!(ids(&uses), vec!["app/src/main.ts"], "{}", uses.value);
}

/// Minified sources are graphed like any other (no size cut of the
/// scan's own). A 1.28 MB one-line bundle of 160,000 `var a` declarations
/// is either graphed whole or declined by a named budget, explicit either
/// way; a small bundle keeps its declarations and call sites.
#[test]
fn admitted_minified_evidence_is_preserved_and_oversize_is_explicit() {
    let dir = fixture();
    let src = dir.0.as_path().join("app/src");
    let repeats = 160_000;
    let big = format!("!function(){{{}}}();\n", "var a=1;".repeat(repeats));
    std::fs::write(src.join("huge.min.js"), &big).expect("huge");
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
    let rows = deps.value["rows"].as_array().expect("rows");
    assert!(
        rows.iter().any(|r| r["id"] == "app/src/huge.min.js"),
        "{}",
        deps.value
    );
    let symbols = ask(dir.0.as_path(), "symbols", Some("src/huge.min.js"), |o| {
        o.limit = Some(1);
    });
    let graphed = symbols.value["pagination"]["total"] == repeats;
    let diagnostics = ask(dir.0.as_path(), "diagnostics", None, |o| {
        o.limit = Some(100);
    });
    let declined = diagnostics.value["rows"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|row| {
            row["path"] == "app/src/huge.min.js"
                && row["message"]
                    .as_str()
                    .is_some_and(|message| message.contains("budget"))
        });
    assert!(
        graphed != declined,
        "graphed whole or declined by a named budget: {} {}",
        symbols.value,
        diagnostics.value
    );
    let outline = ask(dir.0.as_path(), "symbols", Some("src/small.min.js"), |_| {});
    assert_eq!(
        outline.value["pagination"]["total"], 400,
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
    assert_eq!(calls.value["pagination"]["total"], 200, "{}", calls.value);

    let issues = ask(dir.0.as_path(), "issues", None, |o| o.limit = Some(500));
    let subjects = issues.value["rows"]
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
        !issues.value["rows"]
            .as_array()
            .expect("rows")
            .iter()
            .any(|r| r["detector"] == "unresolved-import" && r["subject"] == "app/src/uses.ts"),
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
            crate::tools::ast_graph::languages::canonical_language(file, raw),
            label,
            "{file}"
        );
    }
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
/// A glob names a grammar and nothing else: a TypeScript file named for the
/// parser it already gets keeps the extraction it gets without one, so
/// `import type` stays erased and a re-export keeps its importer.
#[test]
fn a_language_glob_keeps_the_extraction_of_its_grammar() {
    let dir = git_app(&[
        (
            "src/spec.ts",
            "import type { Mapper } from './map';\nexport interface Spec { map: Mapper }\n",
        ),
        (
            "src/map.ts",
            "import type { Spec } from './spec';\nexport type Mapper = (spec: Spec) => void;\n",
        ),
        ("src/index.ts", "export * from './spec';\n"),
    ]);
    let root = dir.0.as_path();
    let graph_with = |globs: BTreeMap<String, Vec<String>>| {
        let built = ingest(
            &IngestOptions {
                path: root.join("app"),
                workspace: Some(root.to_path_buf()),
                language_globs: globs,
                ..Default::default()
            },
            &workspace_policy(root),
            &ContentSecurity::new(),
            &NeverCancel,
            None,
        );
        assert_eq!(built.class, ExitClass::Success, "{}", built.value);
        let cycles = ask(root, "cycles", None, |_| {}).value["rows"].clone();
        let mut dependents = ids(&ask(root, "dependents", Some("src/spec.ts"), |_| {}));
        dependents.sort();
        (cycles, dependents)
    };
    let plain = graph_with(BTreeMap::new());
    assert!(
        plain.1.iter().any(|id| id.ends_with("src/index.ts")),
        "a re-export is an importer: {:?}",
        plain.1
    );
    let named = graph_with(BTreeMap::from([(
        "ts".to_owned(),
        vec!["src/**/*.ts".to_owned()],
    )]));
    assert_eq!(named, plain);
    let family = graph_with(BTreeMap::from([(
        "TypeScript".to_owned(),
        vec!["src/**/*.ts".to_owned()],
    )]));
    assert_eq!(family, plain);
}

/// astSearch: a `.h` header holding C++ reads as C++ (its class is a class)
/// until a glob parses it as C; every build applies the selected globs.
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
        found.value["rows"][0]["kind"].clone()
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
    assert_fresh_build(&again);
    assert_eq!(again.value["sha256"], c.value["sha256"]);
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
    assert_eq!(
        out.class,
        ExitClass::Failed(crate::id::error_codes::ErrorClass::CorrectInput),
        "{}",
        out.value
    );
}

/// `stats` names each language once, by the id every per-language count
/// uses (`linkageByLanguage`): `.ts`, `.mts` and `.cts` are all TypeScript.
#[test]
fn stats_languages_use_the_per_language_count_ids() {
    let dir = git_app(&[
        ("a.ts", "export function a() { return 1; }\n"),
        (
            "b.ts",
            "import { a } from './a';\nexport const b = () => a();\n",
        ),
        ("c.mts", "export const c = 1;\n"),
        ("d.cts", "export const d = 1;\n"),
        ("e.js", "export const e = 1;\n"),
    ]);
    assert_eq!(ingest_fixture(&dir.0, None).class, ExitClass::Success);
    let stats = ask(&dir.0, "stats", None, |_| {});
    let languages = stats.value["languages"]
        .as_array()
        .unwrap_or_else(|| panic!("{}", stats.value))
        .iter()
        .map(|row| (row["language"].to_string(), row["files"].clone()))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(
        languages,
        BTreeMap::from([
            (json!("javascript").to_string(), json!(1)),
            (json!("typescript").to_string(), json!(4)),
        ]),
        "{}",
        stats.value
    );
    for language in stats.value["linkageByLanguage"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(language, _)| json!(language).to_string())
    {
        assert!(
            languages.contains_key(&language),
            "{language}: {}",
            stats.value
        );
    }
}

#[test]
fn a_published_graph_keeps_extraction_for_changed_builds() {
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
    assert!(job.exists(), "publication keeps its extraction as a cache");
    std::fs::write(root.join("new.ts"), "export const added = 1;\n").unwrap();
    let rebuilt = ingest_root(&dir.0, &root, None, false);
    assert_eq!(rebuilt.class, ExitClass::Success, "{}", rebuilt.value);
    assert_eq!(
        rebuilt.value["extraction"]["parsedFiles"], 1,
        "the new file"
    );
    assert_eq!(rebuilt.value["extraction"]["reusedFiles"], 1, "the old one");
    assert_eq!(rebuilt.value["extraction"]["resumedFiles"], 0);
    assert!(job.exists());
}

/// `includeIgnored`, `includeHidden` and `defaultExcludes:false` widen the
/// build's walk: the files they admit are scanned, the manifest records the
/// walk, and a snapshot of another walk is never reused for the request.
#[test]
fn the_walk_flags_widen_the_scan_and_are_part_of_the_snapshot_identity() {
    use crate::tools::ast_graph::walk_scope::WalkScope;
    use octocode_engine::types::IgnorePolicy;
    let dir = git_app(&[
        ("src/main.ts", "export const main = 1;\n"),
        (".gitignore", "generated/\n"),
        ("generated/api.ts", "export const api = 1;\n"),
        (".hidden/tool.ts", "export const tool = 1;\n"),
    ]);
    let root = dir.0.as_path();
    let build_in = |root: &Path, walk: WalkScope| {
        let out = ingest(
            &IngestOptions {
                path: root.join("app"),
                workspace: Some(root.to_path_buf()),
                walk,
                ..Default::default()
            },
            &workspace_policy(root),
            &ContentSecurity::new(),
            &NeverCancel,
            None,
        );
        assert_eq!(out.class, ExitClass::Success, "{}", out.value);
        out.value
    };
    let build = |walk: WalkScope| build_in(root, walk);
    let scanned = |out: &Value| out["scan"]["filesScanned"].as_u64().expect("count");
    let plain = build(WalkScope::default());
    let ignored = WalkScope {
        ignore: IgnorePolicy {
            include_ignored: true,
            include_hidden: false,
        },
        ..WalkScope::default()
    };
    let with_ignored = build(ignored);
    assert_eq!(
        scanned(&with_ignored),
        scanned(&plain) + 1,
        "{with_ignored}"
    );
    assert_ne!(with_ignored["id"], plain["id"]);
    let hidden = build(WalkScope {
        ignore: IgnorePolicy {
            include_ignored: false,
            include_hidden: true,
        },
        ..WalkScope::default()
    });
    assert_eq!(scanned(&hidden), scanned(&plain) + 1, "{hidden}");
    // Outside a git work tree the default prune drops `node_modules`.
    let loose = Fixture::new();
    write_under(
        &loose,
        "app",
        &[
            ("src/main.ts", "export const main = 1;\n"),
            ("node_modules/dep/index.ts", "export const dep = 1;\n"),
        ],
    );
    let pruned = build_in(loose.0.as_path(), WalkScope::default());
    let unpruned = build_in(
        loose.0.as_path(),
        WalkScope {
            default_excludes: false,
            ..WalkScope::default()
        },
    );
    assert_eq!(scanned(&unpruned), scanned(&pruned) + 1, "{unpruned}");
    // Reuse answers the newest snapshot only: the same walk twice in a row.
    let first = build(ignored);
    let again = build(ignored);
    assert_eq!(again["id"], first["id"], "same walk reuses: {again}");
}
