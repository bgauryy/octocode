mod algorithms;
mod analysis;
mod graph;
mod types;

use crate::{
    policy::path::PathPolicy, security::ContentSecurity, tools::local_fetch::CancellationCheck,
};

pub use types::{AstGraphError, AstGraphQuery, AstGraphResult, GraphAnalysis};

/// Execute public `astTopology` through the portable native
/// fact scanner and Rust-owned graph algorithms.
pub fn execute_topology(
    query: &AstGraphQuery,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancel: &dyn CancellationCheck,
) -> AstGraphResult {
    // The builder and graph algorithms uphold map-index invariants with
    // `expect()`; a corrupt or adversarial input tripping one must surface as
    // a structured tool error, not tear down the host process.
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        validate_query(query)?;
        if query.analysis == GraphAnalysis::Drift {
            return analysis::drift(query, paths, security, cancel);
        }
        let built = graph::build_graph(query, paths, security, cancel)?;
        analysis::analyze(built, query, security, cancel)
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

fn validate_query(query: &AstGraphQuery) -> Result<(), AstGraphError> {
    if query.operation != "topology" {
        return Err(AstGraphError::new(
            "ast.input.invalid",
            "operation must be topology",
        ));
    }
    if query.page == 0
        || query.page > 1000
        || query.diagnostic_page == 0
        || query.diagnostic_page > 1000
    {
        return Err(AstGraphError::new(
            "ast.input.invalid",
            "page fields must be between 1 and 1000",
        ));
    }
    if query.depth.is_some_and(|x| x == 0 || x > 50) {
        return Err(AstGraphError::new(
            "ast.input.invalid",
            "depth must be between 1 and 50",
        ));
    }
    if query.page_size.is_some_and(|x| x == 0 || x > 100)
        || query
            .diagnostic_page_size
            .is_some_and(|x| x == 0 || x > 100)
    {
        return Err(AstGraphError::new(
            "ast.input.invalid",
            "pageSize fields must be between 1 and 100",
        ));
    }
    if query.max_files.is_some_and(|x| x == 0 || x > 50_000)
        || query.limit.is_some_and(|x| x == 0 || x > 5_000)
    {
        return Err(AstGraphError::new(
            "ast.input.invalid",
            "graph bounds exceed the public schema",
        ));
    }
    if query
        .rust_workspace
        .as_deref()
        .is_some_and(|value| !matches!(value, "syntax" | "cargo"))
    {
        return Err(AstGraphError::new(
            "ast.input.invalid",
            "rustWorkspace must be syntax or cargo",
        ));
    }
    match query.analysis {
        GraphAnalysis::Dependencies | GraphAnalysis::Dependents => {
            if query.file.is_none() {
                return Err(AstGraphError::new(
                    "invalidGraphQuery",
                    format!("{} requires file", query.analysis.as_str()),
                ));
            }
            if query.target.is_some()
                || query.entrypoints.is_some()
                || query.include_tests.is_some()
            {
                return Err(AstGraphError::new(
                    "invalidGraphQuery",
                    "traversal analyses reject target, entrypoints, and includeTests",
                ));
            }
        }
        GraphAnalysis::Path => {
            if query.file.is_none() || query.target.is_none() {
                return Err(AstGraphError::new(
                    "invalidGraphQuery",
                    "path requires file and target",
                ));
            }
            if query.depth.is_some() || query.entrypoints.is_some() || query.include_tests.is_some()
            {
                return Err(AstGraphError::new(
                    "invalidGraphQuery",
                    "path rejects depth, entrypoints, and includeTests",
                ));
            }
        }
        GraphAnalysis::Cycles => {
            if query.path.is_none() {
                return Err(AstGraphError::new(
                    "invalidGraphQuery",
                    "cycles requires path",
                ));
            }
            if query.file.is_some()
                || query.target.is_some()
                || query.depth.is_some()
                || query.entrypoints.is_some()
                || query.include_tests.is_some()
            {
                return Err(AstGraphError::new(
                    "invalidGraphQuery",
                    "cycles rejects file, target, depth, entrypoints, and includeTests",
                ));
            }
        }
        GraphAnalysis::Reachability | GraphAnalysis::DeadCode => {
            if query.file.is_some() || query.target.is_some() || query.depth.is_some() {
                return Err(AstGraphError::new(
                    "invalidGraphQuery",
                    "reachability analyses reject file, target, and depth",
                ));
            }
        }
        GraphAnalysis::Drift => {
            if query.path.is_none() {
                return Err(AstGraphError::new(
                    "invalidGraphQuery",
                    "drift requires path",
                ));
            }
            if query.baseline.is_none() {
                return Err(AstGraphError::new(
                    "invalidGraphQuery",
                    "drift requires baseline",
                ));
            }
            if query.file.is_some()
                || query.target.is_some()
                || query.depth.is_some()
                || query.entrypoints.is_some()
                || query.include_tests.is_some()
            {
                return Err(AstGraphError::new(
                    "invalidGraphQuery",
                    "drift rejects file, target, depth, entrypoints, and includeTests",
                ));
            }
        }
    }
    if query.baseline.is_some() && query.analysis != GraphAnalysis::Drift {
        return Err(AstGraphError::new(
            "invalidGraphQuery",
            "baseline is only valid for drift",
        ));
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod drift_tests {
    use super::*;
    use crate::{
        policy::path::{PathPolicy, PathPolicyConfig},
        security::SecurityRegistry,
    };
    use serde_json::{Value, json};
    use std::sync::Arc;

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
        let security = ContentSecurity::new(Arc::new(SecurityRegistry::default()));
        let parsed: AstGraphQuery = serde_json::from_value(query).expect("query");
        execute_topology(&parsed, &paths, &security, &Active)
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
        let security = ContentSecurity::new(Arc::new(SecurityRegistry::default()));
        let query: AstGraphQuery = serde_json::from_value(json!({"operation":"topology","analysis":"dependencies","path":root,"file":"include/widget.h","languageGlobs":{"cpp":["include/**/*.h"]}})).expect("query");
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
                "operation":"topology","analysis":"drift",
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

        // baseline on a non-drift analysis is rejected.
        let rejected = run(
            json!({"operation":"topology","analysis":"cycles","path":root.to_string_lossy(),"baseline":root.to_string_lossy()}),
            root,
        )
        .expect_err("baseline rejected on cycles");
        assert_eq!(rejected.code, "invalidGraphQuery");

        // drift without baseline is rejected.
        let missing = run(
            json!({"operation":"topology","analysis":"drift","path":root.to_string_lossy()}),
            root,
        )
        .expect_err("drift requires baseline");
        assert_eq!(missing.code, "invalidGraphQuery");
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
            json!({"operation":"topology","analysis":"cycles","path":root.to_string_lossy()}),
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
        let reasons = out["partialReasons"].as_array().expect("partialReasons");
        assert!(
            reasons.iter().any(|r| r == "unresolvedImports"),
            "unresolved crate:: imports must be flagged: {out}"
        );
    }

    #[test]
    fn skipped_import_target_is_not_reported_as_a_resolved_dependency() {
        let temp = tempfile::TempDir::new().expect("temp");
        let root = temp.path();
        std::fs::write(root.join("entry.ts"), "import './unread';\n").unwrap();
        std::fs::write(root.join("unread.ts"), [0xff]).unwrap();

        let out = run(
            json!({"operation":"topology","analysis":"dependencies","path":root.to_string_lossy(),"file":"entry.ts"}),
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
            json!({"operation":"topology","analysis":"dependencies","path":root.to_string_lossy(),"file":"entry.ts"}),
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
            json!({"operation":"topology","analysis":"cycles","path":root.to_string_lossy()}),
            root,
        )
        .expect("cycles result");

        assert_eq!(out["coverage"]["basis"], "syntactic");
        assert!(
            out["coverage"]["diagnosticCounts"]["syntax-only"].is_null(),
            "{out}"
        );
        assert!(
            out["coverage"]["diagnostics"]
                .as_array()
                .unwrap()
                .is_empty(),
            "{out}"
        );
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod dead_code_root_tests {
    use super::*;
    use crate::{
        policy::path::{PathPolicy, PathPolicyConfig},
        security::SecurityRegistry,
    };
    use serde_json::{Value, json};
    use std::sync::Arc;

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
        let security = ContentSecurity::new(Arc::new(SecurityRegistry::default()));
        let parsed: AstGraphQuery = serde_json::from_value(query).expect("query");
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
            json!({"operation":"topology","analysis":"deadCode","path":root.to_string_lossy()}),
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
            json!({"operation":"topology","analysis":"deadCode","path":root.to_string_lossy()}),
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
}
