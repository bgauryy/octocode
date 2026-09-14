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
    }
    Ok(())
}
