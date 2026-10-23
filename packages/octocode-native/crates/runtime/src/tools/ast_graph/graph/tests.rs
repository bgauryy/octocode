use crate::tools::ast_graph::test_support::{Fixture, known};
use crate::tools::ast_graph::types::DiagnosticCode;

mod cargo;
mod linking;
mod prune;
mod resolution;

/// The graph of `root`, pruning `exclude_dir` too.
fn fixture_graph(root: &std::path::Path, exclude_dir: &[&str]) -> super::BuiltGraph {
    let paths = crate::tools::test_support::workspace_policy(root);
    super::build_graph(
        &super::GraphScan {
            root: root.to_path_buf(),
            exclude_dir: exclude_dir.iter().map(|dir| (*dir).to_owned()).collect(),
            max_files: 50_000,
            ..Default::default()
        },
        &paths,
        &crate::security::ContentSecurity::new(),
        &crate::execution::cancel::NeverCancel,
    )
    .unwrap()
}
