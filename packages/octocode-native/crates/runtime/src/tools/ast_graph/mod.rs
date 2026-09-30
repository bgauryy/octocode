mod algorithms;
mod aliases;
mod analysis;
mod cargo;
mod graph;
mod packages;
pub mod store;
mod types;

use crate::{
    policy::path::PathPolicy, security::ContentSecurity, tools::cancel::CancellationCheck,
};

pub use types::{AstGraphError, AstGraphResult, AstTopologyQuery, GraphAnalysis};

/// Execute public `astTopology` through the portable native
/// fact scanner and Rust-owned graph algorithms.
pub fn execute_topology(
    query: &AstTopologyQuery,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancel: &dyn CancellationCheck,
) -> AstGraphResult {
    // The builder and graph algorithms uphold map-index invariants with
    // `expect()`; a corrupt or adversarial input tripping one must surface as
    // a structured tool error, not tear down the host process.
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        validate_query(query)?;
        if query.analysis() == GraphAnalysis::Drift {
            return analysis::drift(query, paths, security, cancel);
        }
        let mut built = graph::build_graph(query, paths, security, cancel)?;
        analysis::analyze(&mut built, query, security, cancel)
    }))
    .unwrap_or_else(|panic| {
        let detail = panic
            .downcast_ref::<&str>()
            .map(|message| (*message).to_owned())
            .or_else(|| panic.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "graph analysis panicked".to_owned());
        Err(AstGraphError::new(
            "ast.graph.internal",
            format!("Graph analysis failed internally: {detail}"),
        ))
    })
}

/// Per-analysis field sets are enforced by the generated wire type; these are
/// the numeric bounds it does not encode.
fn validate_query(query: &AstTopologyQuery) -> Result<(), AstGraphError> {
    if query.page() > 1000 || query.diagnostic_page() > 1000 {
        return Err(AstGraphError::new(
            "ast.input.invalid",
            "page fields must be between 1 and 1000",
        ));
    }
    if query.depth().is_some_and(|x| x > 50) {
        return Err(AstGraphError::new(
            "ast.input.invalid",
            "depth must be between 1 and 50",
        ));
    }
    if query.page_size().is_some_and(|x| x > 100)
        || query.diagnostic_page_size().is_some_and(|x| x > 100)
    {
        return Err(AstGraphError::new(
            "ast.input.invalid",
            "pageSize fields must be between 1 and 100",
        ));
    }
    if query.max_files().is_some_and(|x| x > 50_000) || query.limit().is_some_and(|x| x > 5_000) {
        return Err(AstGraphError::new(
            "ast.input.invalid",
            "graph bounds exceed the public schema",
        ));
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod drift_tests {
    use super::*;
    use crate::policy::path::{PathPolicy, PathPolicyConfig};
    use serde_json::{Value, json};

    struct Active;
    impl CancellationCheck for Active {
        fn check(&self) -> Result<(), String> {
            Ok(())
        }
    }

    fn run(query: Value, root: &std::path::Path) -> AstGraphResult {
        let paths = PathPolicy::new(PathPolicyConfig {
            workspace_root: Some(root.to_path_buf()),
            ..Default::default()
        })
        .expect("path policy");
        let security = ContentSecurity::new();
        let parsed: AstTopologyQuery = serde_json::from_value(query).expect("query");
        execute_topology(&parsed, &paths, &security, &Active)
    }

    #[test]
    fn rust_dead_code_credits_module_declarations_and_qualified_path_calls() {
        let root = tempfile::tempdir().expect("fixture directory");
        let write = |path: &str, text: &str| {
            let path = root.path().join(path);
            std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
            std::fs::write(path, text).expect("write");
        };
        write(
            "Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
        );
        // Only a live caller credits a qualified-path callee, so the root
        // calls `handle`, which calls the two items by path.
        write(
            "src/lib.rs",
            "pub mod tools;\npub mod portable;\npub mod api;\npub fn start() { api::handle(); }\n",
        );
        write("src/tools/mod.rs", "pub mod inner;\n");
        write("src/tools/inner.rs", "pub fn used() {}\n");
        write("src/portable.rs", "pub fn sanitize() {}\n");
        write(
            "src/api.rs",
            "pub fn handle() {\n    crate::portable::sanitize();\n    crate::tools::inner::used();\n}\n",
        );
        let query = json!({"goal":"find dead code","reasoning":"test","analysis":"deadCode","path":root.path(),"rustWorkspace":"cargo"});
        let out = run(query, root.path()).expect("dead code");
        let names = out["results"]
            .as_array()
            .expect("rows")
            .iter()
            .map(|row| row["name"].as_str().unwrap_or_default().to_owned())
            .collect::<Vec<_>>();
        assert!(
            !names.contains(&"inner".to_owned()),
            "a `mod` declaration is not an export: {names:?}"
        );
        assert!(
            !names.contains(&"sanitize".to_owned()),
            "called by qualified path: {names:?}"
        );
        assert!(
            !names.contains(&"used".to_owned()),
            "called by qualified path: {names:?}"
        );
    }

    #[test]
    fn oversized_implicit_scan_is_refused_with_narrower_roots() {
        let root = tempfile::tempdir().expect("fixture directory");
        let over = graph::SCOPE_ADMISSION_FILES as usize + 1;
        for (package, count) in [("big", over - 10), ("small", 10)] {
            let dir = root.path().join("packages").join(package);
            std::fs::create_dir_all(&dir).expect("package dir");
            for i in 0..count {
                std::fs::write(dir.join(format!("f{i}.ts")), "export const x = 1;\n")
                    .expect("fixture file");
            }
        }
        let query =
            json!({"goal": "test", "reasoning":"test","analysis":"cycles","path":root.path()});
        let error = run(query.clone(), root.path()).expect_err("scope refused");
        assert_eq!(error.code, "ast.graph.scopeTooBroad");
        assert!(
            error
                .message
                .contains("packages/big (4991), packages/small (10)"),
            "{}",
            error.message
        );
        assert_eq!(error.hints.len(), 1);
        let next = error.next.expect("continuations");
        let narrow = &next["narrowScope"]["query"];
        assert!(
            narrow["path"].as_str().unwrap().ends_with("packages/big"),
            "{next}"
        );
        assert_eq!(next["expandScan"]["query"]["maxFiles"], 20_000);

        let mut explicit = query;
        explicit["maxFiles"] = json!(20_000);
        assert!(
            run(explicit, root.path()).is_ok(),
            "explicit maxFiles opts in"
        );
    }

    #[test]
    fn topology_passes_path_scoped_header_parser_to_the_shared_scan() {
        let root = std::env::temp_dir().join(format!(
            "octocode-topology-header-glob-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(root.join("include")).expect("fixture directory");
        std::fs::write(
            root.join("include/widget.h"),
            "namespace Space { class Widget {}; }\n",
        )
        .expect("fixture header");
        let paths = PathPolicy::new(PathPolicyConfig {
            workspace_root: Some(root.clone()),
            ..Default::default()
        })
        .expect("paths");
        let security = ContentSecurity::new();
        let query: AstTopologyQuery = serde_json::from_value(json!({"goal": "test", "reasoning":"test","analysis":"dependencies","path":root,"file":"include/widget.h","languageGlobs":{"cpp":["include/**/*.h"]}})).expect("query");
        let built = graph::build_graph(&query, &paths, &security, &Active).expect("graph");
        assert!(
            built
                .facts
                .get("include/widget.h")
                .is_some_and(|facts| facts
                    .declarations
                    .iter()
                    .any(|declaration| declaration.name == "Widget")),
            "{built:?}"
        );
        let public = execute_topology(&query, &paths, &security, &Active).expect("public topology");
        assert!(public.is_object());
    }

    #[test]
    fn drift_reports_resolved_cycle_and_removed_relation_between_two_roots() {
        let temp = tempfile::TempDir::new().expect("temp");
        let root = temp.path();
        // Baseline: a <-> b import cycle.
        std::fs::create_dir_all(root.join("base")).unwrap();
        std::fs::write(
            root.join("base/a.ts"),
            "import { b } from './b';\nexport const a = () => b();\n",
        )
        .unwrap();
        std::fs::write(
            root.join("base/b.ts"),
            "import { a } from './a';\nexport const b = () => a();\n",
        )
        .unwrap();
        // Head: cycle resolved — b no longer imports a.
        std::fs::create_dir_all(root.join("head")).unwrap();
        std::fs::write(
            root.join("head/a.ts"),
            "import { b } from './b';\nexport const a = () => b();\n",
        )
        .unwrap();
        std::fs::write(root.join("head/b.ts"), "export const b = () => 1;\n").unwrap();

        let head = root.join("head");
        let base = root.join("base");
        let out = run(
            json!({
                "goal": "test", "reasoning":"test","analysis":"drift",
                "path": head.to_string_lossy(),
                "baseline": base.to_string_lossy()
            }),
            root,
        )
        .expect("drift result");

        assert_eq!(out["analysis"], "drift");
        assert_eq!(out["summary"]["comparable"], json!(true));
        assert_eq!(out["summary"]["cyclesResolved"], json!(1));
        assert!(
            out["summary"]["relationsRemoved"].as_u64().unwrap_or(0) >= 1,
            "the removed b->a import is a removed relation: {out}"
        );
        let resolved = out["results"]
            .as_array()
            .expect("results")
            .iter()
            .any(|row| row["category"] == "cycle" && row["change"] == "resolved");
        assert!(resolved, "a resolved-cycle row is present: {out}");
    }

    #[test]
    fn drift_rejects_baseline_on_non_drift_and_requires_it_on_drift() {
        let temp = tempfile::TempDir::new().expect("temp");
        let root = temp.path();
        std::fs::write(root.join("a.ts"), "export const a = 1;\n").unwrap();

        // The wire contract scopes baseline to drift and requires it there.
        let parse = |query: Value| serde_json::from_value::<AstTopologyQuery>(query);
        assert!(
            parse(json!({"goal": "test", "reasoning":"test","analysis":"cycles","path":root.to_string_lossy(),"baseline":root.to_string_lossy()}))
                .is_err(),
            "baseline rejected on cycles"
        );
        assert!(
            parse(json!({"goal": "test", "reasoning":"test","analysis":"drift","path":root.to_string_lossy()}))
                .is_err(),
            "drift requires baseline"
        );
    }

    #[test]
    fn cycles_on_unresolvable_rust_crate_imports_degrades_instead_of_a_confident_zero() {
        // A Rust source tree whose `crate::` imports cannot be resolved (no
        // Cargo.toml, syntax mode). Before the fix this returned `cycleCount:0`
        // with no confidence marker — a confident-looking zero on a graph with
        // zero resolved edges. It must now degrade honestly.
        let temp = tempfile::TempDir::new().expect("temp");
        let root = temp.path();
        std::fs::write(
            root.join("foo.rs"),
            "use crate::bar::thing;\npub fn foo() { thing(); }\n",
        )
        .unwrap();
        std::fs::write(root.join("bar.rs"), "pub fn thing() {}\n").unwrap();

        let out = run(
            json!({"goal": "test", "reasoning":"test","analysis":"cycles","path":root.to_string_lossy()}),
            root,
        )
        .expect("cycles result");

        // The zero is still reported, but no longer as a confident answer.
        assert_eq!(out["summary"]["cycleCount"], json!(0));
        assert_eq!(
            out["confidence"], "low",
            "unresolved edges must lower confidence: {out}"
        );
        assert_eq!(
            out["summary"]["importResolution"]["status"], "failed",
            "zero resolved imports is a failed resolution: {out}"
        );
        assert_eq!(out["summary"]["importResolution"]["resolved"], json!(0));
        let reasons = out["completeness"]["coverageGapReasons"]
            .as_array()
            .expect("coverageGapReasons");
        assert!(
            reasons.iter().any(|r| r == "unresolvedImports"),
            "unresolved crate:: imports must be flagged: {out}"
        );
        // A coverage gap is not truncation: nothing more is reachable by paging.
        assert!(out.get("truncated").is_none(), "{out}");
        assert!(out.get("terminalLimit").is_none(), "{out}");
    }

    #[test]
    fn non_code_imports_are_not_unresolved_coverage_gaps() {
        let temp = tempfile::TempDir::new().expect("temp");
        let root = temp.path();
        std::fs::write(
            root.join("a.ts"),
            "import pkg from './package.json';\nimport './a.css';\nimport logo from './logo.svg?url';\nimport { b } from './b';\nexport const a = b;\n",
        )
        .unwrap();
        std::fs::write(
            root.join("b.ts"),
            "import { a } from './a';\nexport const b = 1;\n",
        )
        .unwrap();

        let out = run(
            json!({"goal": "test", "reasoning":"test","analysis":"cycles","path":root.to_string_lossy()}),
            root,
        )
        .expect("cycles result");

        assert_eq!(out["coverage"]["imports"]["unresolvedInternal"], 0, "{out}");
        assert_eq!(out["coverage"]["imports"]["nonCode"], 3, "{out}");
        assert!(out.get("confidence").is_none(), "{out}");
        assert!(out.get("truncated").is_none(), "{out}");
        assert!(out.get("terminalLimit").is_none(), "{out}");
        assert!(
            out["completeness"].get("coverageGapReasons").is_none(),
            "{out}"
        );
        // Empty diagnostics and a single diagnostic page carry no envelope.
        assert!(out["coverage"].get("diagnostics").is_none(), "{out}");
        assert!(out["coverage"].get("diagnosticCounts").is_none(), "{out}");
        assert!(
            out["coverage"].get("diagnosticsPagination").is_none(),
            "{out}"
        );
        let row = &out["results"][0];
        assert_eq!(row["files"], json!(["a.ts", "b.ts"]), "{out}");
        assert!(row.get("size").is_none(), "size duplicates files: {out}");
        assert!(row.get("outgoingComponentCount").is_none(), "{out}");
    }

    #[test]
    fn go_package_imports_link_every_package_file_with_its_import_line() {
        let temp = tempfile::TempDir::new().expect("temp");
        let root = temp.path();
        std::fs::write(root.join("go.mod"), "module example.com/app\n").unwrap();
        std::fs::create_dir_all(root.join("tsdb")).unwrap();
        std::fs::write(
            root.join("main.go"),
            "package main\n\nimport (\n\t\"fmt\"\n\t\"example.com/app/tsdb\"\n)\n\nfunc main() { fmt.Println(tsdb.Open()) }\n",
        )
        .unwrap();
        std::fs::write(
            root.join("tsdb/db.go"),
            "package tsdb\n\nfunc Open() int { return 1 }\n",
        )
        .unwrap();
        std::fs::write(
            root.join("tsdb/head.go"),
            "package tsdb\n\ntype Head struct{}\n",
        )
        .unwrap();
        std::fs::write(root.join("tsdb/db_test.go"), "package tsdb\n").unwrap();

        let out = run(
            json!({"goal": "test", "reasoning":"test","analysis":"dependencies","path":root.to_string_lossy(),"file":"main.go"}),
            root,
        )
        .expect("dependencies");
        let rows = out["results"].as_array().expect("rows");
        let files = rows
            .iter()
            .map(|r| r["file"].as_str().unwrap_or_default())
            .collect::<Vec<_>>();
        assert_eq!(files, vec!["tsdb/db.go", "tsdb/head.go"], "{out}");
        assert!(rows.iter().all(|r| r["importLine"] == 5), "{out}");

        let dependents = run(
            json!({"goal": "test", "reasoning":"test","analysis":"dependents","path":root.to_string_lossy(),"file":"tsdb/head.go"}),
            root,
        )
        .expect("dependents");
        assert_eq!(dependents["results"][0]["file"], "main.go", "{dependents}");
    }

    /// A scan rooted below the module root still reads the enclosing
    /// `go.mod`, so module-path imports into the scanned subtree link.
    #[test]
    fn go_subdirectory_roots_resolve_through_the_enclosing_go_mod() {
        let temp = tempfile::TempDir::new().expect("temp");
        let root = temp.path();
        std::fs::write(root.join("go.mod"), "module example.com/app\n").unwrap();
        std::fs::create_dir_all(root.join("tsdb/chunkenc")).unwrap();
        std::fs::write(
            root.join("tsdb/chunkenc/chunk.go"),
            "package chunkenc\n\nfunc New() int { return 1 }\n",
        )
        .unwrap();
        std::fs::write(
            root.join("tsdb/db.go"),
            "package tsdb\n\nimport \"example.com/app/tsdb/chunkenc\"\n\nfunc Open() int { return chunkenc.New() }\n",
        )
        .unwrap();
        let out = run(
            json!({"goal":"test","reasoning":"test","analysis":"dependents","path":root.join("tsdb").to_string_lossy(),"file":"chunkenc/chunk.go"}),
            root,
        )
        .expect("dependents");
        assert_eq!(out["results"][0]["file"], "db.go", "{out}");
    }

    /// A scan rooted at a Python package resolves absolute imports that name
    /// that package (`from pkg.utils.text import x` under root `pkg/`).
    #[test]
    fn python_package_roots_resolve_imports_through_their_package_name() {
        let temp = tempfile::TempDir::new().expect("temp");
        let root = temp.path();
        std::fs::create_dir_all(root.join("pkg/utils")).unwrap();
        std::fs::write(root.join("pkg/__init__.py"), "").unwrap();
        std::fs::write(root.join("pkg/utils/__init__.py"), "").unwrap();
        std::fs::write(
            root.join("pkg/utils/text.py"),
            "def slug(x):\n    return x\n",
        )
        .unwrap();
        std::fs::write(
            root.join("pkg/admin.py"),
            "from pkg.utils.text import slug\n\nprint(slug('a'))\n",
        )
        .unwrap();
        let out = run(
            json!({"goal":"test","reasoning":"test","analysis":"dependents","path":root.join("pkg").to_string_lossy(),"file":"utils/text.py"}),
            root,
        )
        .expect("dependents");
        assert_eq!(out["results"][0]["file"], "admin.py", "{out}");
    }

    /// Java classes of one package use each other without an import.
    #[test]
    fn java_same_package_uses_link_without_an_import() {
        let temp = tempfile::TempDir::new().expect("temp");
        let root = temp.path();
        let dir = root.join("src/com/acme");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("Lists.java"),
            "package com.acme;\n\npublic final class Lists {\n  public static int transform(int x) { return x; }\n}\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("Multimaps.java"),
            "package com.acme;\n\nfinal class Multimaps {\n  int run() { return Lists.transform(1); }\n}\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("Other.java"),
            "package com.acme;\n\nfinal class Other {\n  int run() { return 2; }\n}\n",
        )
        .unwrap();
        let out = run(
            json!({"goal":"test","reasoning":"test","analysis":"dependents","path":root.to_string_lossy(),"file":"src/com/acme/Lists.java"}),
            root,
        )
        .expect("dependents");
        let files = out["results"]
            .as_array()
            .expect("rows")
            .iter()
            .map(|row| row["file"].as_str().unwrap_or_default())
            .collect::<Vec<_>>();
        assert_eq!(files, ["src/com/acme/Multimaps.java"], "{out}");
    }

    #[test]
    fn java_class_imports_link_to_the_class_file() {
        let temp = tempfile::TempDir::new().expect("temp");
        let root = temp.path();
        let dir = root.join("src/com/acme");
        std::fs::create_dir_all(dir.join("util")).unwrap();
        std::fs::write(
            dir.join("App.java"),
            "package com.acme;\n\nimport com.acme.util.Strings;\nimport java.util.List;\n\nclass App {}\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("util/Strings.java"),
            "package com.acme.util;\n\npublic class Strings {}\n",
        )
        .unwrap();

        let out = run(
            json!({"goal": "test", "reasoning":"test","analysis":"dependencies","path":root.to_string_lossy(),"file":"src/com/acme/App.java"}),
            root,
        )
        .expect("dependencies");
        assert_eq!(
            out["results"][0]["file"], "src/com/acme/util/Strings.java",
            "{out}"
        );
        assert_eq!(out["results"][0]["importLine"], 3, "{out}");
    }

    #[test]
    fn path_edges_carry_import_lines() {
        let temp = tempfile::TempDir::new().expect("temp");
        let root = temp.path();
        std::fs::write(
            root.join("a.ts"),
            "// head\nimport { b } from './b';\nexport const a = b;\n",
        )
        .unwrap();
        std::fs::write(root.join("b.ts"), "export const b = 1;\n").unwrap();

        let out = run(
            json!({"goal": "test", "reasoning":"test","analysis":"path","path":root.to_string_lossy(),"file":"a.ts","target":"b.ts"}),
            root,
        )
        .expect("path result");

        assert_eq!(out["results"][0]["edges"][0]["importLine"], 2, "{out}");
    }

    #[test]
    fn resolved_entrypoints_are_emitted_on_the_first_page_only() {
        let temp = tempfile::TempDir::new().expect("temp");
        let root = temp.path();
        std::fs::write(root.join("package.json"), r#"{"name":"x","main":"a.js"}"#).unwrap();
        for i in 0..3 {
            std::fs::write(root.join(format!("f{i}.js")), "export const x = 1;\n").unwrap();
        }
        std::fs::write(root.join("a.js"), "export const a = 1;\n").unwrap();
        let query = |page: u32| json!({"goal": "test", "reasoning":"test","analysis":"reachability","path":root.to_string_lossy(),"pageSize":2,"page":page});

        let first = run(query(1), root).expect("page 1");
        let second = run(query(2), root).expect("page 2");

        assert_eq!(
            first["summary"]["entrypointsResolved"],
            json!(["a.js"]),
            "{first}"
        );
        assert!(
            second["summary"].get("entrypointsResolved").is_none(),
            "{second}"
        );
        assert_eq!(second["summary"]["entrypointsResolvedCount"], 1, "{second}");
    }

    #[test]
    fn skipped_import_target_is_not_reported_as_a_resolved_dependency() {
        let temp = tempfile::TempDir::new().expect("temp");
        let root = temp.path();
        std::fs::write(root.join("entry.ts"), "import './unread';\n").unwrap();
        std::fs::write(root.join("unread.ts"), [0xff]).unwrap();

        let out = run(
            json!({"goal": "test", "reasoning":"test","analysis":"dependencies","path":root.to_string_lossy(),"file":"entry.ts"}),
            root,
        )
        .expect("dependencies result");

        assert_eq!(out["filesSkipped"], 1);
        assert_eq!(out["coverage"]["imports"]["resolved"], 0);
        assert_eq!(out["coverage"]["imports"]["unresolvedInternal"], 1);
        assert!(out["results"].as_array().unwrap().is_empty(), "{out}");
    }

    #[test]
    fn nested_workspace_package_import_resolves_to_scanned_source() {
        let temp = tempfile::TempDir::new().expect("temp");
        let root = temp.path();
        std::fs::create_dir_all(root.join("packages/lib/src")).unwrap();
        std::fs::write(
            root.join("packages/lib/package.json"),
            r#"{"name":"@fixture/lib","exports":"./src/index.ts"}"#,
        )
        .unwrap();
        std::fs::write(
            root.join("packages/lib/src/index.ts"),
            "export const value = 1;\n",
        )
        .unwrap();
        std::fs::write(
            root.join("entry.ts"),
            "import { value } from '@fixture/lib';\nconsole.log(value);\n",
        )
        .unwrap();

        let out = run(
            json!({"goal": "test", "reasoning":"test","analysis":"dependencies","path":root.to_string_lossy(),"file":"entry.ts"}),
            root,
        )
        .expect("dependencies result");

        assert!(
            out["results"]
                .as_array()
                .unwrap()
                .iter()
                .any(|row| row["file"] == "packages/lib/src/index.ts"),
            "workspace package manifest should resolve its source: {out}"
        );
    }

    #[test]
    fn syntax_basis_is_not_repeated_as_a_diagnostic_for_every_file() {
        let temp = tempfile::TempDir::new().expect("temp");
        let root = temp.path();
        std::fs::write(root.join("one.py"), "def one(): pass\n").unwrap();
        std::fs::write(root.join("two.py"), "def two(): pass\n").unwrap();

        let out = run(
            json!({"goal": "test", "reasoning":"test","analysis":"cycles","path":root.to_string_lossy()}),
            root,
        )
        .expect("cycles result");

        assert_eq!(out["coverage"]["basis"], "syntactic");
        assert!(
            out["coverage"]["diagnosticCounts"]["syntax-only"].is_null(),
            "{out}"
        );
        assert!(out["coverage"].get("diagnostics").is_none(), "{out}");
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod dead_code_root_tests {
    use super::*;
    use crate::policy::path::{PathPolicy, PathPolicyConfig};
    use serde_json::{Value, json};

    struct Active;
    impl CancellationCheck for Active {
        fn check(&self) -> Result<(), String> {
            Ok(())
        }
    }

    fn run(query: Value, root: &std::path::Path) -> AstGraphResult {
        let paths = PathPolicy::new(PathPolicyConfig {
            workspace_root: Some(root.to_path_buf()),
            ..Default::default()
        })
        .expect("path policy");
        let security = ContentSecurity::new();
        let parsed: AstTopologyQuery = serde_json::from_value(query).expect("query");
        execute_topology(&parsed, &paths, &security, &Active)
    }

    // Regression: dead-code root inference must not be package.json-only. A Rust
    // crate must infer `src/main.rs` as a root so the helper it calls reads as
    // reachable rather than dead.
    #[test]
    fn rust_dead_code_infers_main_root_and_keeps_helper_reachable() {
        let temp = tempfile::TempDir::new().expect("temp");
        let root = temp.path();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("src/main.rs"),
            "use crate::helper;\nfn main() { helper::run(); }\n",
        )
        .unwrap();
        std::fs::write(root.join("src/helper.rs"), "pub fn run() {}\n").unwrap();

        let out = run(
            json!({"goal": "test", "reasoning":"test","analysis":"deadCode","path":root.to_string_lossy()}),
            root,
        )
        .expect("dead code result");

        assert!(
            out["summary"]["entrypointsResolvedCount"]
                .as_u64()
                .unwrap_or(0)
                >= 1,
            "src/main.rs must be inferred as a root: {out}"
        );
        let dead = out["results"].as_array().cloned().unwrap_or_default();
        // The reachability defect: with a real root the helper is reached
        // through main's `use crate::helper`, so no file may be reported as an
        // unreachable file or dead cluster. (Export-level name-usage heuristics
        // are a separate, lower-confidence signal outside this fix's scope.)
        assert!(
            !dead
                .iter()
                .any(|r| r["reason"] == "unreachable-file" || r["reason"] == "dead-cluster"),
            "no file should be unreachable/dead-clustered when main is a root: {out}"
        );
        assert_eq!(
            out["summary"]["deadClusterCount"],
            json!(0),
            "a reachable helper must not form a dead cluster: {out}"
        );
    }

    // Regression: when no roots resolve for the detected languages the dead-code
    // verdict must be hard-gated (empty dead list + low confidence), not report
    // every export in the tree as dead.
    #[test]
    fn dead_code_hard_gates_when_no_roots_resolve() {
        let temp = tempfile::TempDir::new().expect("temp");
        let root = temp.path();
        // A Rust source file with no main.rs/lib.rs/bin and no Cargo.toml: no
        // entrypoint can be inferred.
        std::fs::write(root.join("util.rs"), "pub fn util() {}\n").unwrap();

        let out = run(
            json!({"goal": "test", "reasoning":"test","analysis":"deadCode","path":root.to_string_lossy()}),
            root,
        )
        .expect("dead code result");

        assert_eq!(
            out["summary"]["deadExportCount"],
            json!(0),
            "with no resolvable roots the dead list must be suppressed, not everything flagged: {out}"
        );
        assert_eq!(
            out["confidence"], "low",
            "an ungated dead-code verdict without roots must degrade to low confidence: {out}"
        );
    }

    /// Unreferenced-export rows for a TS fixture rooted at `main.ts`, as
    /// `(name, exportedAs)` pairs.
    fn dead_exports(files: &[(&str, &str)]) -> Vec<(String, Value)> {
        let temp = tempfile::TempDir::new().expect("temp");
        let root = temp.path();
        for (name, content) in files {
            std::fs::write(root.join(name), content).unwrap();
        }
        let out = run(
            json!({"goal": "test", "reasoning":"test","analysis":"deadCode","path":root.to_string_lossy(),"entrypoints":["main.ts"]}),
            root,
        )
        .expect("dead code result");
        let rows = out["results"].as_array().cloned().unwrap_or_default();
        assert_eq!(
            out["summary"]["deadExportCount"].as_u64(),
            Some(rows.len() as u64),
            "{out}"
        );
        rows.iter()
            .map(|r| {
                assert_eq!(r["reason"], "unreferenced-export", "{out}");
                (
                    r["name"].as_str().unwrap().to_owned(),
                    r.get("exportedAs").cloned().unwrap_or(Value::Null),
                )
            })
            .collect()
    }

    #[test]
    fn renamed_export_is_reported_under_its_local_name_with_the_public_alias() {
        // `bar` (a different local function) is not exported; the
        // unused renamed export `foo` is dead and says it is public `bar`.
        let dead = dead_exports(&[
            (
                "mod.ts",
                "export function kept() { return 1 }\nfunction foo() { return 2 }\nfunction bar() { return 3 }\nexport { foo as bar }\n",
            ),
            ("main.ts", "import { kept } from './mod'\nkept()\n"),
        ]);
        assert_eq!(dead, vec![("foo".to_owned(), json!(["bar"]))]);
    }

    #[test]
    fn importing_the_public_alias_keeps_the_local_declaration_live() {
        // `export { foo as bar }`: importing `bar` consumes `foo`, and the
        // unrelated local `bar` is not exported.
        let dead = dead_exports(&[
            (
                "mod.ts",
                "export function kept() { return 1 }\nfunction foo() { return 2 }\nfunction bar() { return 3 }\nexport { foo as bar }\n",
            ),
            ("main.ts", "import { bar } from './mod'\nbar()\n"),
        ]);
        assert_eq!(dead, vec![("kept".to_owned(), Value::Null)]);
    }

    /// Rows of a Cargo fixture as `(file, name, viaHeuristic)`.
    fn rust_dead_rows(files: &[(&str, &str)]) -> Vec<(String, String, Value)> {
        let temp = tempfile::TempDir::new().expect("temp");
        let root = temp.path();
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        for (name, content) in files {
            let path = root.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, content).unwrap();
        }
        let out = run(
            json!({"goal": "test", "reasoning":"test","analysis":"deadCode","path":root.to_string_lossy(),"rustWorkspace":"cargo"}),
            root,
        )
        .expect("dead code result");
        out["results"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .map(|r| {
                (
                    r["file"].as_str().unwrap().to_owned(),
                    r["name"].as_str().unwrap().to_owned(),
                    r.get("viaHeuristic").cloned().unwrap_or(Value::Null),
                )
            })
            .collect()
    }

    #[test]
    fn rust_unreachable_caller_does_not_keep_its_qualified_path_callee_live() {
        // `buried` is never called, so `crate::portable::secret()` inside it
        // credits nothing; `used` is called from the live `handle`.
        let rows = rust_dead_rows(&[
            (
                "src/lib.rs",
                "pub mod api;\npub mod portable;\npub fn start() { api::handle(); }\n",
            ),
            (
                "src/api.rs",
                "pub fn handle() { crate::portable::used(); }\nfn buried() { crate::portable::secret(); }\n",
            ),
            ("src/portable.rs", "pub fn used() {}\npub fn secret() {}\n"),
        ]);
        assert_eq!(
            rows,
            vec![(
                "src/portable.rs".to_owned(),
                "secret".to_owned(),
                json!("syntax-references")
            )]
        );
    }

    #[test]
    fn rust_qualified_calls_resolve_through_the_imported_binding() {
        // `h::run()` names `run` in the file `use crate::helper as h` binds;
        // the same-named `run` elsewhere is not credited by it.
        let rows = rust_dead_rows(&[
            (
                "src/main.rs",
                "mod helper;\nmod other;\nuse crate::helper as h;\nfn main() { h::run(); }\n",
            ),
            ("src/helper.rs", "pub fn run() {}\npub fn idle() {}\n"),
            ("src/other.rs", "pub fn run() {}\n"),
        ]);
        let names = rows
            .iter()
            .map(|(file, name, _)| format!("{file}:{name}"))
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["src/helper.rs:idle", "src/other.rs:run"]);
    }

    #[test]
    fn rust_unresolved_qualified_name_credit_is_only_a_heuristic() {
        // `unknown::sanitize()` cannot be resolved to a file: the name match
        // no longer proves `sanitize` live, it labels the candidate.
        let rows = rust_dead_rows(&[
            (
                "src/lib.rs",
                "pub mod portable;\npub fn start() { unknown::sanitize(); }\n",
            ),
            ("src/portable.rs", "pub fn sanitize() {}\n"),
        ]);
        assert_eq!(
            rows,
            vec![(
                "src/portable.rs".to_owned(),
                "sanitize".to_owned(),
                json!("qualified-path-name")
            )]
        );
    }

    #[test]
    fn default_import_keeps_the_default_export_declaration_live() {
        // `import foo from` consumes the module's default export.
        let def =
            "export default function foo() { return 1 }\nexport function other() { return 2 }\n";
        let dead = dead_exports(&[
            ("def.ts", def),
            (
                "main.ts",
                "import foo from './def'\nimport { other } from './def'\nfoo(); other()\n",
            ),
        ]);
        assert!(dead.is_empty(), "{dead:?}");
        let dead = dead_exports(&[
            ("def.ts", def),
            ("main.ts", "import { other } from './def'\nother()\n"),
        ]);
        assert_eq!(dead, vec![("foo".to_owned(), json!(["default"]))]);
    }

    #[test]
    fn same_named_method_does_not_share_liveness_with_a_live_function() {
        // The live exported `run` and the unreachable method
        // `Calls.run` are different callers, so `secret` is dead.
        let dead = dead_exports(&[
            (
                "calls.ts",
                "export function publicA() { return 1 }\nexport function secret() { return 2 }\nexport function run() { return publicA() }\nclass Calls { run() { return secret() } }\n",
            ),
            ("main.ts", "import { run } from './calls'\nrun()\n"),
        ]);
        assert_eq!(dead, vec![("secret".to_owned(), Value::Null)]);
    }

    #[test]
    fn unreachable_caller_does_not_keep_its_exported_callee_live() {
        // `buried` is never called, so its callee `secret` is dead;
        // `publicA` stays live through the imported `start`.
        let dead = dead_exports(&[
            (
                "calls.ts",
                "export function publicA() { return 1 }\nexport function secret() { return 2 }\nexport function start() { return publicA() }\nfunction buried() { return secret() }\n",
            ),
            ("main.ts", "import { start } from './calls'\nstart()\n"),
        ]);
        assert_eq!(dead, vec![("secret".to_owned(), Value::Null)]);
    }

    #[test]
    fn names_only_in_comments_or_strings_do_not_keep_exports_live() {
        let dead = dead_exports(&[
            (
                "lib.ts",
                "export function inComment() { return 1 }\nexport function inString() { return 2 }\nexport function passed() { return 3 }\nexport function assigned() { return 4 }\nexport function shadowed() { return 5 }\n// inComment is only documented here\nexport const label = 'inString'\nregister(passed)\nconst slot = assigned\nexport function make() { const shadowed = () => 0; return [shadowed, slot] }\n",
            ),
            (
                "main.ts",
                "import { make, label } from './lib'\nmake(); label\n",
            ),
        ]);
        assert_eq!(
            dead,
            vec![
                ("inComment".to_owned(), Value::Null),
                ("inString".to_owned(), Value::Null),
                ("shadowed".to_owned(), Value::Null),
            ]
        );
    }

    #[test]
    fn unreferenced_export_rows_name_their_reference_basis() {
        let temp = tempfile::TempDir::new().expect("temp");
        let root = temp.path();
        std::fs::write(root.join("mod.ts"), "export function unused() {}\n").unwrap();
        std::fs::write(root.join("main.ts"), "import './mod'\n").unwrap();
        let out = run(
            json!({"goal": "test", "reasoning":"test","analysis":"deadCode","path":root.to_string_lossy(),"entrypoints":["main.ts"]}),
            root,
        )
        .expect("dead code result");
        assert_eq!(out["results"][0]["name"], "unused", "{out}");
        assert_eq!(
            out["results"][0]["viaHeuristic"], "semantic-references",
            "{out}"
        );
    }

    #[test]
    fn import_used_only_in_an_unreachable_function_does_not_keep_its_target_live() {
        // `buried` is never called: its `helper()` is the only use of the
        // import, so the import credits nothing.
        let helper = "export function helper() { return 1 }\n";
        let dead = dead_exports(&[
            ("helper.ts", helper),
            (
                "app.ts",
                "import { helper } from './helper'\nexport function start() { return 1 }\nfunction buried() { return helper() }\n",
            ),
            ("main.ts", "import { start } from './app'\nstart()\n"),
        ]);
        assert_eq!(dead, vec![("helper".to_owned(), Value::Null)]);
        // The same import used from the live `start` keeps `helper` live.
        let dead = dead_exports(&[
            ("helper.ts", helper),
            (
                "app.ts",
                "import { helper } from './helper'\nexport function start() { return helper() }\nfunction buried() { return helper() }\n",
            ),
            ("main.ts", "import { start } from './app'\nstart()\n"),
        ]);
        assert!(dead.is_empty(), "{dead:?}");
    }

    #[test]
    fn import_used_as_a_value_by_a_live_declaration_keeps_its_target_live() {
        // Value, type and export-clause uses credit the import too.
        let dead = dead_exports(&[
            (
                "helper.ts",
                "export function cb() { return 1 }\nexport interface Shape {}\nexport function passed() { return 2 }\n",
            ),
            (
                "app.ts",
                "import { cb, Shape, passed } from './helper'\nexport function start(s: Shape) { return register(cb) }\nexport { passed }\n",
            ),
            (
                "main.ts",
                "import { start, passed } from './app'\nstart(passed)\n",
            ),
        ]);
        assert!(dead.is_empty(), "{dead:?}");
    }

    #[test]
    fn namespace_default_and_reexported_imports_stay_live() {
        let dead = dead_exports(&[
            (
                "lib.ts",
                "export default function run() { return 1 }\nexport function viaNs() { return 2 }\nexport function viaChain() { return 3 }\nexport function viaClause() { return 4 }\n",
            ),
            ("barrel.ts", "export { viaChain } from './lib'\n"),
            (
                "clause.ts",
                "import { viaClause } from './lib'\nexport { viaClause }\n",
            ),
            (
                "app.ts",
                "import run from './lib'\nimport * as ns from './lib'\nexport function start() { run(); return ns.viaNs() }\n",
            ),
            (
                "main.ts",
                "import { start } from './app'\nimport { viaChain } from './barrel'\nimport { viaClause } from './clause'\nstart(); viaChain(); viaClause()\n",
            ),
        ]);
        assert!(dead.is_empty(), "{dead:?}");
        // A namespace import cannot be resolved to one member: even from an
        // unreachable function it keeps the whole module public.
        let dead = dead_exports(&[
            ("lib.ts", "export function viaNs() { return 2 }\n"),
            (
                "app.ts",
                "import * as ns from './lib'\nexport function start() { return 1 }\nfunction buried() { return ns.viaNs() }\n",
            ),
            ("main.ts", "import { start } from './app'\nstart()\n"),
        ]);
        assert!(dead.is_empty(), "{dead:?}");
    }

    #[test]
    fn rust_use_binding_only_in_an_unreachable_fn_does_not_keep_its_target_live() {
        let main = "mod app;\nmod util;\nfn main() { app::start(); }\n";
        let rows = rust_dead_rows(&[
            ("src/main.rs", main),
            (
                "src/app.rs",
                "use crate::util::run;\npub fn start() {}\nfn buried() { run(); }\n",
            ),
            ("src/util.rs", "pub fn run() {}\n"),
        ]);
        assert_eq!(
            rows,
            vec![(
                "src/util.rs".to_owned(),
                "run".to_owned(),
                json!("syntax-references")
            )]
        );
        // Used from the live `start`, or straight from `main`: live.
        let rows = rust_dead_rows(&[
            ("src/main.rs", main),
            (
                "src/app.rs",
                "use crate::util::run;\npub fn start() { run(); }\nfn buried() { run(); }\n",
            ),
            ("src/util.rs", "pub fn run() {}\n"),
        ]);
        assert!(rows.is_empty(), "{rows:?}");
        let rows = rust_dead_rows(&[
            (
                "src/main.rs",
                "mod util;\nuse crate::util::run;\nfn main() { run(); }\n",
            ),
            ("src/util.rs", "pub fn run() {}\n"),
        ]);
        assert!(rows.is_empty(), "{rows:?}");
    }

    #[test]
    fn rust_uses_that_cannot_be_ordered_by_liveness_stay_conservative() {
        // A `pub use` re-exports, a trait impl method and a test run without
        // a by-name caller: each use keeps its target live.
        let rows = rust_dead_rows(&[
            (
                "src/lib.rs",
                "pub mod app;\nmod util;\npub use crate::util::exported;\n",
            ),
            (
                "src/app.rs",
                "use crate::util::{fmt_helper, test_helper};\nstruct Shown;\nimpl std::fmt::Display for Shown {\n    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { fmt_helper(f) }\n}\n#[cfg(test)]\nmod tests {\n    use super::*;\n    #[test]\n    fn t() { test_helper(); }\n}\n#[test]\nfn direct() { test_helper(); }\n",
            ),
            (
                "src/util.rs",
                "pub fn exported() {}\npub fn fmt_helper(_: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { Ok(()) }\npub fn test_helper() {}\n",
            ),
        ]);
        assert!(rows.is_empty(), "{rows:?}");
    }

    #[test]
    fn value_escapes_and_module_level_calls_keep_callees_live() {
        // Reachability stays conservative: a function passed as a value, a
        // module-level call and a method of a used class all keep callees live.
        let dead = dead_exports(&[
            (
                "lib.ts",
                "export function a() { return 1 }\nexport function b() { return 2 }\nexport function c() { return 3 }\nexport function d() { return 4 }\nfunction handler() { return a() }\nregister(handler)\nfunction boot() { return b() }\nboot()\nclass Svc { go() { return c() } }\nexport function make() { return new Svc() }\n",
            ),
            ("main.ts", "import { make } from './lib'\nmake()\n"),
        ]);
        assert_eq!(dead, vec![("d".to_owned(), Value::Null)]);
    }
}
