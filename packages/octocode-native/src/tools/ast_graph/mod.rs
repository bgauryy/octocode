mod algorithms;
mod analysis;
mod build;
mod types;

use crate::{
    policy::path::PathPolicy, security::ContentSecurity, tools::local_fetch::CancellationCheck,
};

pub use types::{AstGraphError, AstGraphQuery, AstGraphResult, GraphAnalysis};

/// Execute the public `astSearch` topology variant through the portable native
/// fact scanner and Rust-owned graph algorithms.
pub fn execute_topology(
    query: &AstGraphQuery,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancel: &dyn CancellationCheck,
) -> AstGraphResult {
    validate_query(query)?;
    if query.analysis == GraphAnalysis::Drift {
        return analysis::drift(query, paths, security, cancel);
    }
    let built = build::build_graph(query, paths, security, cancel)?;
    analysis::analyze(built, query, security, cancel)
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
                return Err(AstGraphError::new("invalidGraphQuery", "drift requires path"));
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
    fn drift_reports_resolved_cycle_and_removed_relation_between_two_roots() {
        let temp = tempfile::TempDir::new().expect("temp");
        let root = temp.path();
        // Baseline: a <-> b import cycle.
        std::fs::create_dir_all(root.join("base")).unwrap();
        std::fs::write(root.join("base/a.ts"), "import { b } from './b';\nexport const a = () => b();\n").unwrap();
        std::fs::write(root.join("base/b.ts"), "import { a } from './a';\nexport const b = () => a();\n").unwrap();
        // Head: cycle resolved — b no longer imports a.
        std::fs::create_dir_all(root.join("head")).unwrap();
        std::fs::write(root.join("head/a.ts"), "import { b } from './b';\nexport const a = () => b();\n").unwrap();
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
}
