use super::*;
use crate::id::ToolId;
use crate::tools::test_support::assert_part_contract;
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
            assert_part_contract(tool, &row);
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
fn advisory_versions_references_and_credits_are_each_delivered_once() {
    let versions = names("version");
    let other_versions = names("other");
    let references: Vec<Value> = (0..80)
        .map(|index| json!({"type":"WEB","url":format!("https://example.org/{index}/{}", "ref".repeat(12))}))
        .collect();
    let credits: Vec<Value> = (0..30)
        .map(|index| json!({"login":format!("user-{index}"),"type":"FINDER"}))
        .collect();
    let data = json!({"answer":"affected","sources":[{"source":"osv","status":"found"},{"source":"github","status":"found"}],
        "advisories":[{"id":"GHSA-1","link":"https://osv.dev/vulnerability/GHSA-1",
        "details":"detail ".repeat(300),"credits":credits,"references":references,
        "affected":[{"packageName":"a","versions":versions},
                    {"packageName":"b","versions":other_versions}]}]});
    let (mut seen_references, mut seen_credits) = (Vec::new(), Vec::new());
    // `affected` pages as a collection: an entry alone on a later page is
    // at position 0 there, so its package name identifies it.
    let mut seen_versions: [Vec<Value>; 2] = Default::default();
    for row in pages(ToolId::SecurityCheck, data) {
        for advisory in row["advisories"].as_array().into_iter().flatten() {
            seen_references.extend(
                advisory["references"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .cloned(),
            );
            seen_credits.extend(
                advisory["credits"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .cloned(),
            );
            for affected in advisory["affected"].as_array().into_iter().flatten() {
                let slot = match affected["packageName"].as_str() {
                    Some("a") => 0,
                    Some("b") => 1,
                    other => panic!("unexpected affected entry {other:?}"),
                };
                seen_versions[slot].extend(
                    affected["versions"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .cloned(),
                );
            }
        }
    }
    assert_eq!(seen_references, references);
    assert_eq!(seen_credits, credits);
    assert_eq!(seen_versions, [versions, other_versions]);
}

/// The shape of a real GHSA advisory: windowable free text (`answer`,
/// `details`) beside `affected` entries whose `versions` plan before the
/// paged references. Every declared collection arrives exactly once: the
/// references never join a string window that has no slot for them. Every
/// page fits: `affected` pages as a collection whose entries name their
/// package, and the free text is windowed in parts of its own instead of
/// enlarging the first. Later reference parts do not repeat the entries.
#[test]
fn advisory_text_windows_beside_nested_collections_keep_every_item() {
    let tool = ToolId::SecurityCheck;
    let budget = 2_000;
    let references: Vec<Value> = (0..80)
        .map(|index| json!({"type":"WEB","url":format!("https://example.org/advisory/{index}/{}", "r".repeat(20))}))
        .collect();
    let credits = vec![json!({"login":"finder","type":"FINDER"})];
    // Many entries: their context alone outgrows a later page.
    for packages in [2, 30] {
        let affected: Vec<Value> = (0..packages)
            .map(|package| {
                // One list large enough to plan before the references.
                let count = if package == 0 { 80 } else { 3 };
                let versions: Vec<Value> = (0..count)
                    .map(|index| json!(format!("{package}.{index}.0")))
                    .collect();
                json!({"packageName":format!("package-{package}"),
                    "ranges":[{"type":"ECOSYSTEM","events":[{"introduced":"0"},{"fixed":"9.9.9"}]}],
                    "versions":versions})
            })
            .collect();
        let versions: Vec<Value> = affected
            .iter()
            .map(|entry| entry["versions"].clone())
            .collect();
        // Several prose lengths, so some final string window leaves room
        // that the next split could try to fill.
        for (answer, details) in [(300, 450), (330, 520), (360, 610), (390, 700)] {
            let case = format!("{packages} packages, {answer}/{details}");
            let data = json!({"answer":"answer words ".repeat(answer),
                "sources":[{"source":"osv","status":"found"}],
                "advisories":[{"id":"GHSA-1","link":"https://osv.dev/vulnerability/GHSA-1",
                "details":"detail words ".repeat(details),"credits":credits,
                "affected":affected,"references":references}]});
            assert_row_contract(tool, &data);
            let cancelled = Cancellation::new();
            let plan = PreparedRows::new(vec![json!({"index":0,"data":data})], tool)
                .plan(budget, &cancelled)
                .expect("valid plan");
            assert!(plan.total_pages > 1);
            let (mut seen_references, mut seen_credits) = (Vec::new(), Vec::new());
            let mut seen_versions = vec![Vec::new(); versions.len()];
            for page in 0..plan.total_pages {
                let rows = plan
                    .build_page(page, &[None], &cancelled)
                    .expect("valid page");
                let page_chars = super::evidence_closure::chars(&json!(rows));
                for row in rows {
                    assert_part_contract(tool, &row);
                    for advisory in row["data"]["advisories"].as_array().into_iter().flatten() {
                        seen_references.extend(
                            advisory["references"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .cloned(),
                        );
                        seen_credits.extend(
                            advisory["credits"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .cloned(),
                        );
                        for entry in advisory["affected"].as_array().into_iter().flatten() {
                            let slot = entry["packageName"]
                                .as_str()
                                .and_then(|name| name.strip_prefix("package-"))
                                .and_then(|index| index.parse::<usize>().ok())
                                .expect("affected entries keep their package name");
                            let delivered = entry["versions"].as_array().into_iter().flatten();
                            seen_versions[slot].extend(delivered.cloned());
                        }
                    }
                }
                assert!(
                    page_chars <= budget,
                    "{case}: page {page} is {page_chars} chars"
                );
            }
            assert_eq!(seen_references, references, "{case}: references");
            assert_eq!(seen_credits, credits, "{case}: credits");
            for (slot, expected) in versions.iter().enumerate() {
                assert_eq!(
                    &json!(seen_versions[slot]),
                    expected,
                    "{case}: versions {slot}"
                );
            }
        }
    }
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
