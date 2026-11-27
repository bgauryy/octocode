use crate::ast_graph::test_support::known;
use crate::ast_graph::types::DiagnosticCode;

mod cargo;
mod linking;
mod module_context;
mod prune;
mod resolution;

/// The graph of `root`, skipping the `exclude` globs too.
fn fixture_graph(root: &std::path::Path, exclude: &[&str]) -> super::BuiltGraph {
    let paths = octocode_runtime_base::test_support::workspace_policy(root);
    super::build_graph(
        &super::GraphScan {
            root: root.to_path_buf(),
            exclude: exclude.iter().map(|glob| (*glob).to_owned()).collect(),
            ..Default::default()
        },
        &paths,
        &octocode_runtime_base::security::ContentSecurity::new(),
        &octocode_runtime_base::execution::cancel::NeverCancel,
    )
    .unwrap()
}

/// A scan whose files must project more rows than a snapshot table holds
/// stops with `scopeTooBroad` as soon as that is certain, and its hints name
/// the directories that hold the files, largest first, as narrower ingests.
#[test]
fn an_over_budget_scan_stops_early_and_names_narrower_scopes() {
    let dir = octocode_runtime_base::test_support::Fixture::new();
    for index in 0..6 {
        dir.write(&format!("big/m{index}.ts"), "export const a = () => 1;\n");
    }
    dir.write("small/one.ts", "export const b = () => 2;\n");
    let root = dir.0.as_path();
    let error = super::build_graph(
        &super::GraphScan {
            root: root.to_path_buf(),
            limits: crate::ast_graph::limits::Limits {
                table_items: 3,
                ..*crate::ast_graph::limits::LIMITS
            },
            ..Default::default()
        },
        &octocode_runtime_base::test_support::workspace_policy(root),
        &octocode_runtime_base::security::ContentSecurity::new(),
        &octocode_runtime_base::execution::cancel::NeverCancel,
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

/// A narrower ingest is a lead for the same scan: it carries the walk the
/// refused build was given, or it would scan another scope.
#[test]
fn the_narrower_scope_hints_carry_the_builds_walk() {
    use crate::ast_graph::walk_scope::WalkScope;
    use octocode_engine::types::IgnorePolicy;
    let dir = octocode_runtime_base::test_support::Fixture::new();
    for index in 0..6 {
        dir.write(&format!("big/m{index}.ts"), "export const a = () => 1;\n");
    }
    dir.write("big/.hid/h.ts", "export const h = 1;\n");
    dir.write("small/one.ts", "export const b = () => 2;\n");
    let root = dir.0.as_path();
    let hints = |walk| {
        super::build_graph(
            &super::GraphScan {
                root: root.to_path_buf(),
                walk,
                limits: crate::ast_graph::limits::Limits {
                    table_items: 3,
                    ..*crate::ast_graph::limits::LIMITS
                },
                ..Default::default()
            },
            &octocode_runtime_base::test_support::workspace_policy(root),
            &octocode_runtime_base::security::ContentSecurity::new(),
            &octocode_runtime_base::execution::cancel::NeverCancel,
        )
        .expect_err("over budget")
        .hints
        .join("\n")
    };
    let hidden = WalkScope {
        ignore: IgnorePolicy {
            include_ignored: false,
            include_hidden: true,
        },
        ..WalkScope::default()
    };
    let lines = |hints: &str| {
        hints
            .lines()
            .filter(|line| line.starts_with("graphBuild {"))
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let (lifted, plain) = (hints(hidden), hints(WalkScope::default()));
    assert!(!lines(&lifted).is_empty(), "{lifted}");
    for line in lines(&lifted) {
        assert!(line.contains(r#""includeHidden":true"#), "{line}");
    }
    for line in lines(&plain) {
        assert!(!line.contains("include"), "{line}");
    }
}

/// A scan whose kept files would pass the build's memory budget stops with
/// `scopeTooBroad` as soon as the next file passes it, during extraction and
/// before linking: the message names how far it got and both sizes, and the
/// hints name the narrower scopes, largest first.
#[test]
fn a_scan_past_the_build_budget_stops_early_and_names_narrower_scopes() {
    let dir = octocode_runtime_base::test_support::Fixture::new();
    for index in 0..6 {
        dir.write(&format!("big/m{index}.ts"), "export const a = () => 1;\n");
    }
    dir.write("small/one.ts", "export const b = () => 2;\n");
    let root = dir.0.as_path();
    let build = |build_bytes| {
        super::build_graph(
            &super::GraphScan {
                root: root.to_path_buf(),
                limits: crate::ast_graph::limits::Limits {
                    build_bytes,
                    ..*crate::ast_graph::limits::LIMITS
                },
                ..Default::default()
            },
            &octocode_runtime_base::test_support::workspace_policy(root),
            &octocode_runtime_base::security::ContentSecurity::new(),
            &octocode_runtime_base::execution::cancel::NeverCancel,
        )
    };
    // A budget under one declaration's charge but over each source: every
    // file is read (only a source larger than the whole budget is a skip),
    // and the first one kept passes it.
    let budget = crate::ast_graph::limits::build_charge(0, 1, 0) - 1;
    assert!(std::fs::metadata(root.join("big/m0.ts")).unwrap().len() < budget as u64);
    let error = build(budget).expect_err("past the build budget");
    assert_eq!(
        error.code,
        octocode_contract_types::ErrorCode::ScopeTooBroad
    );
    assert!(
        error.message.contains("the first 1 of 7 files") && error.message.contains("build budget"),
        "{}",
        error.message
    );
    let hints = error.hints.join("\n");
    let (big, small) = (hints.find("big"), hints.find("small"));
    assert!(big.is_some() && small.is_some() && big < small, "{hints}");
    assert!(hints.contains("graphBuild {"), "{hints}");
    // The default budget, half the host's memory, holds this scope.
    assert_eq!(
        build(crate::ast_graph::limits::LIMITS.build_bytes)
            .expect("within the build budget")
            .facts
            .len(),
        7
    );
}

/// Source correspondence alone never refuses a scope: readers admit its
/// origins a block at a time, so a scan whose summed correspondence passes
/// the source budget still builds every file.
#[test]
fn summed_source_correspondence_never_refuses_a_scope() {
    let dir = octocode_runtime_base::test_support::Fixture::new();
    for index in 0..6 {
        dir.write(&format!("big/m{index}.ts"), "export const a = () => 1;\n");
    }
    let root = dir.0.as_path();
    let built = super::build_graph(
        &super::GraphScan {
            root: root.to_path_buf(),
            limits: crate::ast_graph::limits::Limits {
                source_bytes: 1,
                ..*crate::ast_graph::limits::LIMITS
            },
            ..Default::default()
        },
        &octocode_runtime_base::test_support::workspace_policy(root),
        &octocode_runtime_base::security::ContentSecurity::new(),
        &octocode_runtime_base::execution::cancel::NeverCancel,
    )
    .expect("summed correspondence is no snapshot bound");
    assert_eq!(built.facts.len(), 6);
}

use super::peaks::{dense_source, file_peak};
use octocode_test_support::json_fixture;

fn build_within(
    root: &std::path::Path,
    build_bytes: usize,
) -> Result<super::BuiltGraph, octocode_runtime_base::response::result::ToolError> {
    super::build_graph(
        &super::GraphScan {
            root: root.to_path_buf(),
            limits: crate::ast_graph::limits::Limits {
                build_bytes,
                ..*crate::ast_graph::limits::LIMITS
            },
            ..Default::default()
        },
        &octocode_runtime_base::test_support::workspace_policy(root),
        &octocode_runtime_base::security::ContentSecurity::new(),
        &octocode_runtime_base::execution::cancel::NeverCancel,
    )
}

/// A file whose source correspondence alone outgrows the build's memory
/// budget, with nothing else kept, is a scan gap like a file past the byte
/// limit: named with the limit, its ceiling and its use, never failing the
/// scan or dropping it silently.
#[test]
fn a_file_past_the_build_budget_in_source_correspondence_is_a_named_scan_gap() {
    let dir = octocode_runtime_base::test_support::Fixture::new();
    let dense = dense_source("f", 2_000);
    let peak = file_peak(&dense, "dense.ts");
    // It enters the build's memory; its correspondence does not fit.
    let build_bytes = peak.in_flight() * 3 / 4;
    assert!(build_bytes > peak.entry);
    dir.write("dense.ts", dense);
    let built =
        build_within(dir.0.as_path(), build_bytes).expect("a declined file never fails the build");
    assert!(!built.facts.contains_key("dense.ts"));
    let skip = built
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.file == "dense.ts")
        .unwrap_or_else(|| panic!("{:?}", built.diagnostics));
    assert_eq!(skip.code, DiagnosticCode::ScanSkip);
    assert!(
        skip.message.contains("graph.scan.sourceBudgetExceeded")
            && skip.message.contains("build memory budget")
            && skip.message.contains(&build_bytes.to_string()),
        "{}",
        skip.message
    );
}

/// RV-GHOME R3-1: whether a file is skipped depends on the file alone. A
/// dense file whose correspondence no build of this budget could hold is a
/// named skip beside kept files, wherever it falls in the walk and however
/// the workers interleave; the small files are kept on every run.
#[test]
fn a_file_no_build_could_hold_is_skipped_beside_kept_files_in_any_order() {
    let dense = dense_source("f", 2_000);
    let build_bytes = file_peak(&dense, "dense.ts").in_flight() * 3 / 4;
    for (dense_name, smalls) in [("a_dense.ts", 4), ("z_dense.ts", 4), ("m_dense.ts", 16)] {
        for _ in 0..3 {
            let dir = octocode_runtime_base::test_support::Fixture::new();
            dir.write(dense_name, &dense);
            for index in 0..smalls {
                let name = if index % 2 == 0 { "b" } else { "y" };
                dir.write(
                    &format!("{name}{index}.ts"),
                    format!("export const s{index} = () => {index};\n"),
                );
            }
            let built = build_within(dir.0.as_path(), build_bytes)
                .unwrap_or_else(|error| panic!("{dense_name}: {}", error.message));
            assert!(!built.facts.contains_key(dense_name), "{dense_name}");
            assert_eq!(
                built.facts.len(),
                smalls,
                "{dense_name}: {:?}",
                built.diagnostics
            );
            assert!(
                built
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.file == dense_name
                        && diagnostic
                            .message
                            .contains("graph.scan.sourceBudgetExceeded")),
                "{dense_name}: {:?}",
                built.diagnostics
            );
        }
    }
}

/// RV-GHOME F-B: the files in flight share the build's memory. Two dense
/// files whose peaks fit the budget one at a time, but not one beside the
/// other kept, are never both held, however many workers run them: the
/// scope is refused (`scopeTooBroad`). With room for both, both are kept.
#[test]
fn files_in_flight_share_the_build_budget() {
    let dir = octocode_runtime_base::test_support::Fixture::new();
    let (a, b) = (dense_source("a", 2_000), dense_source("b", 2_000));
    let (peak_a, peak_b) = (file_peak(&a, "a.ts"), file_peak(&b, "b.ts"));
    let kept = peak_a.kept.max(peak_b.kept);
    let in_flight = peak_a.in_flight().max(peak_b.in_flight());
    dir.write("a.ts", a);
    dir.write("b.ts", b);
    let root = dir.0.as_path();
    // Room for either file alone, kept or in flight; not for one in flight
    // beside the other kept.
    let tight = kept.max(in_flight) + in_flight / 2;
    assert!(tight < kept + in_flight);
    let refused = build_within(root, tight).expect_err("not both at once");
    assert_eq!(
        refused.code,
        octocode_contract_types::ErrorCode::ScopeTooBroad,
        "{}",
        refused.message
    );
    assert!(
        refused.message.contains("build budget")
            && refused.message.contains("could not be held beside"),
        "{}",
        refused.message
    );
    let built = build_within(root, 3 * (kept + in_flight)).expect("room for both");
    assert_eq!(built.facts.len(), 2, "{:?}", built.diagnostics);
}

/// Linking keeps the argument slots projection reads and counts the rest.
#[test]
fn linking_keeps_only_the_argument_slots_projection_links() {
    use octocode_engine::graph::{GraphArgumentGroupObservation, GraphArgumentObservation};
    let dir = octocode_runtime_base::test_support::Fixture::new();
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

/// Parallel workers admit files in any order: once one passes the build
/// budget, every later admission reports that same refusal, so the answer
/// names the files that passed it, never a count a slower worker reached.
#[test]
fn every_admission_after_the_budget_passes_reports_the_first_refusal() {
    let root = std::path::Path::new("/graph-admission");
    let entries = (0..3)
        .map(|index| octocode_engine::types::FileSystemEntry {
            path: format!("/graph-admission/m{index}.ts"),
            relative_path: format!("m{index}.ts"),
            name: format!("m{index}.ts"),
            entry_type: octocode_engine::types::FileKind::File,
            size: Some(1),
            modified_time: None,
            walk_position: index + 1,
            stamp: None,
        })
        .collect::<Vec<_>>();
    let scan = super::GraphScan {
        limits: crate::ast_graph::limits::Limits {
            build_bytes: 1,
            ..*crate::ast_graph::limits::LIMITS
        },
        ..Default::default()
    };
    let admission = super::scope::Admission::new(&scan, root, &entries);
    let first = admission.admit(0, 2).expect_err("past the build budget");
    assert!(
        first.message.contains("the first 1 of 3 files"),
        "{}",
        first.message
    );
    let later = admission.admit(0, 2).expect_err("still past it");
    assert_eq!(later, first);
}

/// A declaration's stored `end_line` is the last line of its own source,
/// however tall its header: Rust attributes and Python decorators above it
/// do not move the end.
#[test]
fn a_declaration_header_does_not_move_its_stored_end_line() {
    let dir = octocode_runtime_base::test_support::Fixture::new();
    dir.write(
        "a.rs",
        "#[derive(Debug)]\n#[repr(C)]\npub struct Alpha {\n    x: u8,\n}\n\nfn plain() {\n}\n",
    );
    dir.write(
        "b.py",
        "@one\n@two\ndef run():\n    pass\n\ndef plain():\n    pass\n",
    );
    let built = fixture_graph(&dir.0, &[]);
    let ends = |file: &str| -> Vec<(String, u32, u32)> {
        built.facts[file]
            .declarations
            .iter()
            .map(|d| (d.name.clone(), d.line, d.end_line))
            .collect()
    };
    let rust = ends("a.rs");
    assert!(rust.contains(&("Alpha".into(), 3, 5)), "{rust:?}");
    assert!(rust.contains(&("plain".into(), 7, 8)), "{rust:?}");
    let python = ends("b.py");
    assert!(python.contains(&("run".into(), 3, 4)), "{python:?}");
    assert!(python.contains(&("plain".into(), 6, 7)), "{python:?}");
}

/// A narrower ingest is a lead for the same scan: it keeps `exclude` and
/// `parserGlobs`, their root-relative globs rebased onto the narrower root
/// (a glob that cannot match under it is dropped, one that can is kept).
#[test]
fn the_narrower_scope_hints_carry_exclude_and_parser_globs() {
    let dir = octocode_runtime_base::test_support::Fixture::new();
    for index in 0..6 {
        dir.write(&format!("big/m{index}.ts"), "export const a = () => 1;\n");
    }
    dir.write("small/one.ts", "export const b = () => 2;\n");
    let root = dir.0.as_path();
    let error = super::build_graph(
        &super::GraphScan {
            root: root.to_path_buf(),
            exclude: vec!["*.gen.ts".into(), "/big/skip".into(), "/small/other".into()],
            language_globs: std::collections::BTreeMap::from([(
                "cpp".to_owned(),
                vec!["big/inc/*.h".to_owned(), "small/*.h".to_owned()],
            )]),
            limits: crate::ast_graph::limits::Limits {
                table_items: 3,
                ..*crate::ast_graph::limits::LIMITS
            },
            ..Default::default()
        },
        &octocode_runtime_base::test_support::workspace_policy(root),
        &octocode_runtime_base::security::ContentSecurity::new(),
        &octocode_runtime_base::execution::cancel::NeverCancel,
    )
    .expect_err("over budget");
    let row_of = |dir: &str| {
        let hint = error
            .hints
            .iter()
            .find(|hint| hint.contains(&format!("/{dir}\"")))
            .unwrap_or_else(|| panic!("no hint for {dir}: {:?}", error.hints));
        let start = hint.find('{').expect("a row");
        serde_json::Deserializer::from_str(&hint[start..])
            .into_iter::<serde_json::Value>()
            .next()
            .expect("row")
            .expect("json")
    };
    let (big, small) = (row_of("big"), row_of("small"));
    assert_eq!(
        big["exclude"],
        json_fixture(r#"["*.gen.ts", "/skip"]"#),
        "{big}"
    );
    assert_eq!(
        big["parserGlobs"],
        json_fixture(r#"{"cpp": ["/inc/*.h"]}"#),
        "{big}"
    );
    assert_eq!(
        small["exclude"],
        json_fixture(r#"["*.gen.ts", "/other"]"#),
        "{small}"
    );
    assert_eq!(
        small["parserGlobs"],
        json_fixture(r#"{"cpp": ["/*.h"]}"#),
        "{small}"
    );
}
