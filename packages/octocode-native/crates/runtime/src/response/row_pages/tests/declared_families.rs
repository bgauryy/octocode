use super::*;
use crate::id::ToolId;
use crate::tools::test_support::assert_row_contract;
use octocode_io::cancellation::Cancellation;

fn pages(tool: ToolId, data: Value) -> Vec<Value> {
    assert_row_contract(tool, &data);
    let plan = PreparedRows::new(vec![json!({"index":0,"data":data})], tool)
        .plan(700, &Cancellation::new())
        .expect("valid plan");
    assert!(plan.total_pages > 1);
    (0..plan.total_pages)
        .flat_map(|page| {
            plan.build_page(page, &[None], &Cancellation::new())
                .expect("valid build_page")
        })
        .map(|row| {
            assert_row_contract(tool, &row["data"]);
            row["data"].clone()
        })
        .collect()
}

fn names(prefix: &str) -> Vec<Value> {
    (0..80)
        .map(|index| json!(format!("{prefix}-{index}-{}", "name".repeat(12))))
        .collect()
}

/// Named rows (`{name, <value>}`): branches and engines, whose names are data
/// and never JSON keys.
fn named(prefix: &str, value: &str) -> Vec<Value> {
    (0..80)
        .map(|index| format!("{prefix}-{index}"))
        .chain(["feature/a~b".to_owned(), "0".to_owned()])
        .map(|name| json!({"name": name, value: "value".repeat(12)}))
        .collect()
}

#[test]
fn clone_symlinks_and_sparse_scope_each_page_once() {
    let symlinks = names("link");
    let requested = json!(["src/a", "src/b"]);
    let data = json!({"bytes":123,"location":{"kind":"tree","localPath":"/workspace",
        "commitSha":"fixed","resolvedRef":"main","requestedPaths":requested,"symlinks":symlinks}});
    let mut seen = Vec::new();
    let mut scopes = Vec::new();
    for row in pages(ToolId::GhCloneRepo, data) {
        // Evidence, not page context: whole, on the row's first part only.
        scopes.extend(row["location"].get("requestedPaths").cloned());
        seen.extend(
            row["location"]["symlinks"]
                .as_array()
                .expect("symlinks")
                .iter()
                .cloned(),
        );
    }
    assert_eq!(seen, symlinks);
    assert_eq!(scopes, [requested]);
}

#[test]
fn structure_branch_records_and_tags_are_each_delivered_once() {
    let branches = named("branch", "sha");
    let tags = names("tag");
    let data = json!({"defaultBranch":"main","commitSha":"fixed","branches":branches,"tags":tags});
    let (mut seen_branches, mut seen_tags) = (Vec::new(), Vec::new());
    for row in pages(ToolId::GhStructure, data) {
        seen_branches.extend(row["branches"].as_array().into_iter().flatten().cloned());
        seen_tags.extend(row["tags"].as_array().into_iter().flatten().cloned());
    }
    assert_eq!(seen_branches, branches);
    assert_eq!(seen_tags, tags);
}

#[test]
fn artifact_dependencies_and_engine_records_are_each_delivered_once() {
    let dependencies = names("dependency");
    let engines = named("runtime", "range");
    let data = json!({"artifacts":[{"packageName":"package","version":"1.0.0","dependencies":dependencies,"engines":engines}]});
    let (mut seen_engines, mut seen_dependencies) = (Vec::new(), Vec::new());
    for row in pages(ToolId::ArtifactSearch, data) {
        for artifact in row["artifacts"].as_array().expect("artifacts") {
            assert_eq!(artifact["packageName"], "package");
            seen_engines.extend(
                artifact["engines"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .cloned(),
            );
            seen_dependencies.extend(
                artifact["dependencies"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .cloned(),
            );
        }
    }
    assert_eq!(seen_engines, engines);
    assert_eq!(seen_dependencies, dependencies);
}

#[test]
fn artifact_original_empty_dependencies_remain_distinct_from_unknown() {
    for known in [true, false] {
        let mut artifact = json!({"packageName":"package","engines":named("engine", "range")});
        if known {
            artifact["dependencies"] = json!([]);
            artifact["peerDependencies"] = json!([]);
        }
        for row in pages(ToolId::ArtifactSearch, json!({"artifacts":[artifact]})) {
            let artifact = &row["artifacts"][0];
            for field in ["dependencies", "peerDependencies"] {
                if known {
                    assert_eq!(artifact[field], json!([]));
                } else {
                    assert!(artifact.get(field).is_none());
                }
            }
        }
    }
}

#[test]
fn independently_paged_dependency_lists_preserve_known_empty_peer_context() {
    let dependencies = names("required");
    let optional = names("optional");
    let data = json!({"artifacts":[{"packageName":"package","dependencies":dependencies,
        "optionalDependencies":optional,"peerDependencies":[]}]});
    let (mut seen_dependencies, mut seen_optional) = (Vec::new(), Vec::new());
    for row in pages(ToolId::ArtifactSearch, data) {
        for artifact in row["artifacts"].as_array().expect("artifacts") {
            assert_eq!(artifact["peerDependencies"], json!([]));
            seen_dependencies.extend(
                artifact["dependencies"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .cloned(),
            );
            seen_optional.extend(
                artifact["optionalDependencies"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .cloned(),
            );
        }
    }
    assert_eq!(seen_dependencies, dependencies);
    assert_eq!(seen_optional, optional);
}

#[test]
fn shared_skipped_entries_page_once_with_required_coverage_context() {
    let entries = (0..80)
        .map(|index| {
            json!({"path":format!("path-{index}-{}","directory".repeat(8)),
        "cause":"ignored","reason":"fixture ignore rule"})
        })
        .collect::<Vec<_>>();
    for tool in [
        ToolId::StructureSearch,
        ToolId::AstSearch,
        ToolId::LocalSearch,
    ] {
        let mut seen = Vec::new();
        for row in pages(
            tool,
            json!({"files":[],"skippedPaths":{"entries":entries,"sampled":false}}),
        ) {
            assert_eq!(row["skippedPaths"]["sampled"], false);
            seen.extend(
                row["skippedPaths"]["entries"]
                    .as_array()
                    .expect("required entries")
                    .iter()
                    .cloned(),
            );
        }
        assert_eq!(seen, entries);
    }
}

/// The skipped entries a page carries, after checking it never repeats an
/// empty `skippedPaths` shell and keeps `sampled` beside its entries.
fn skipped_entries(row: &Value) -> Vec<Value> {
    let Some(skipped) = row.get("skippedPaths") else {
        return Vec::new();
    };
    assert_eq!(skipped["sampled"], false, "{row}");
    let entries = skipped["entries"].as_array().expect("required entries");
    assert!(!entries.is_empty(), "empty skippedPaths shell: {row}");
    entries.clone()
}

#[test]
fn file_pages_carry_skipped_entries_only_where_they_are_delivered() {
    let files = (0..80)
        .map(|index| json!({"path":format!("file-{index}.rs")}))
        .collect::<Vec<_>>();
    let entries = json!([{"path":"ignored","cause":"ignored"}]);
    for tool in [
        ToolId::StructureSearch,
        ToolId::AstSearch,
        ToolId::LocalSearch,
    ] {
        let (mut seen_files, mut seen_entries) = (Vec::new(), Vec::new());
        let rows = pages(
            tool,
            json!({"files":files,"skippedPaths":{"entries":entries,"sampled":false}}),
        );
        for row in &rows {
            seen_files.extend(row["files"].as_array().expect("files").iter().cloned());
            seen_entries.extend(skipped_entries(row));
        }
        assert!(rows.iter().any(|row| row.get("skippedPaths").is_none()));
        assert_eq!(seen_files, files);
        assert_eq!(seen_entries, *entries.as_array().expect("entries"));
    }
}

#[test]
fn early_and_lean_parts_drop_the_skipped_shell_losslessly() {
    let names = (0..14)
        .map(|index| json!(format!("file-{index}-{}.rs", "segment".repeat(6))))
        .collect::<Vec<_>>();
    let entries = (0..80)
        .map(|index| json!({"path":format!("skipped-{index}-{}", "directory".repeat(8)),"cause":"ignored"}))
        .collect::<Vec<_>>();
    let data = json!({"files":[{"dir":"src","files":names}],
        "skippedPaths":{"entries":entries,"sampled":false}});
    let rows = pages(ToolId::StructureSearch, data);
    let (mut seen_names, mut seen_entries) = (Vec::new(), Vec::new());
    for row in &rows {
        for group in row["files"].as_array().expect("anchor files") {
            assert_eq!(group["dir"], "src");
            seen_names.extend(group["files"].as_array().expect("names").iter().cloned());
        }
        seen_entries.extend(skipped_entries(row));
    }
    assert!(
        rows.iter()
            .take_while(|row| row.get("skippedPaths").is_none())
            .count()
            > 0,
        "the lean rest split leads with name-only parts"
    );
    assert_eq!(seen_names, names);
    assert_eq!(seen_entries, entries);
}
