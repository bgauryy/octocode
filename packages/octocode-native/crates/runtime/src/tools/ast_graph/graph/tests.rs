use crate::tools::ast_graph::test_support::known;
use crate::tools::ast_graph::types::DiagnosticCode;

mod cargo;
mod linking;
mod module_context;
mod prune;
mod resolution;

/// The graph of `root`, skipping the `exclude` globs too.
fn fixture_graph(root: &std::path::Path, exclude: &[&str]) -> super::BuiltGraph {
    let paths = crate::tools::test_support::workspace_policy(root);
    super::build_graph(
        &super::GraphScan {
            root: root.to_path_buf(),
            exclude: exclude.iter().map(|glob| (*glob).to_owned()).collect(),
            ..Default::default()
        },
        &paths,
        &crate::security::ContentSecurity::new(),
        &crate::execution::cancel::NeverCancel,
    )
    .unwrap()
}

/// A scan whose source correspondence must exceed its item budget stops
/// with `scopeTooBroad` as soon as that is certain, and its hints name the
/// directories that hold the files, largest first, as narrower ingests.
#[test]
fn an_over_budget_scan_stops_early_and_names_narrower_scopes() {
    let dir = crate::tools::test_support::Fixture::new();
    for index in 0..6 {
        dir.write(&format!("big/m{index}.ts"), "export const a = () => 1;\n");
    }
    dir.write("small/one.ts", "export const b = () => 2;\n");
    let root = dir.0.as_path();
    let error = super::build_graph(
        &super::GraphScan {
            root: root.to_path_buf(),
            limits: crate::tools::ast_graph::limits::Limits {
                source_bytes: 1,
                ..crate::tools::ast_graph::limits::LIMITS
            },
            ..Default::default()
        },
        &crate::tools::test_support::workspace_policy(root),
        &crate::security::ContentSecurity::new(),
        &crate::execution::cancel::NeverCancel,
    )
    .expect_err("over budget");
    assert_eq!(
        error.code,
        octocode_contract_types::ErrorCode::ScopeTooBroad
    );
    assert!(
        !error.message.contains("LimitExceeded("),
        "{}",
        error.message
    );
    let hints = error.hints.join("\n");
    let (big, small) = (hints.find("big"), hints.find("small"));
    assert!(big.is_some() && small.is_some() && big < small, "{hints}");
    assert!(hints.contains("graphBuild {"), "{hints}");
}

/// A file whose source correspondence outgrows the per-file budget is a scan
/// gap, like a file past the byte limit: the build keeps every other file and
/// names the skipped one, never failing the scan or dropping it silently.
#[test]
fn a_file_past_the_per_file_source_budget_is_a_named_scan_gap() {
    let dir = crate::tools::test_support::Fixture::new();
    let dense = (0..14_000)
        .map(|id| {
            format!(
                "export function f{id}(a, b) {{ return f{}(b, a); }}\n",
                id + 1
            )
        })
        .collect::<String>();
    dir.write("dense.ts", dense);
    dir.write("small.ts", "export const small = () => 1;\n");
    let built = fixture_graph(dir.0.as_path(), &[]);
    assert!(built.facts.contains_key("small.ts"));
    assert!(!built.facts.contains_key("dense.ts"));
    assert!(
        built
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.file == "dense.ts"
                && diagnostic.code == DiagnosticCode::ScanSkip
                && diagnostic
                    .message
                    .contains("graph.scan.sourceBudgetExceeded")),
        "{:?}",
        built.diagnostics
    );
}

/// Linking keeps the argument slots projection reads and counts the rest.
#[test]
fn linking_keeps_only_the_argument_slots_projection_links() {
    use octocode_engine::graph::{GraphArgumentGroupObservation, GraphArgumentObservation};
    let dir = crate::tools::test_support::Fixture::new();
    dir.write(
        "a.js",
        "function handler(){}\nfunction run(){ none(); literal(1); both(handler, 2); }\n",
    );
    let built = fixture_graph(&dir.0, &[]);
    let facts = &built.facts["a.js"];
    let counts = facts.argument_counts;
    assert_eq!(
        (counts.sites, counts.groups, counts.slots),
        (3, 3, 3),
        "{counts:?}"
    );
    assert_eq!(
        (counts.direct_references, counts.non_reference_values),
        (1, 2),
        "{counts:?}"
    );
    assert_eq!(facts.argument_sites.len(), 1, "only `both` has a reference");
    let GraphArgumentGroupObservation::Slots { slots } =
        &facts.argument_sites[0].groups[0].observation
    else {
        panic!("slots");
    };
    assert_eq!(slots.len(), 1);
    assert!(matches!(
        slots[0].observation,
        GraphArgumentObservation::Reference { .. }
    ));
}
