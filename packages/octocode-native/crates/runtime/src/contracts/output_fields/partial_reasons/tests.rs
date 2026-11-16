use super::*;
use std::path::{Path, PathBuf};

/// The source trees whose non-test code builds a tool's rows, relative to
/// the workspace's `crates/` directory. A tree shared by several tools
/// (`source_view`, the GitHub search rule) is listed for each of them.
fn trees(tool: ToolId) -> &'static [&'static str] {
    const SOURCE_VIEW: &str = "runtime/src/tools/source_view";
    const GITHUB_SEARCH: &str = "runtime/src/tools/gh_shared/search.rs";
    match tool {
        GraphBuild => &[
            "runtime/src/tools/ast_graph/store/ingest",
            "runtime/src/tools/ast_graph/graph",
        ],
        GraphQuery => &["runtime/src/tools/ast_graph/store/query"],
        LocalSearch => &[
            "runtime/src/tools/local_search",
            "runtime/src/tools/large_source",
            "engine/src/search/text_search",
            SOURCE_VIEW,
        ],
        LocalFetch => &[
            "runtime/src/tools/local_fetch",
            "runtime/src/tools/large_source",
            "runtime/src/tools/source_capture",
            SOURCE_VIEW,
        ],
        GhGetFileContent => &["runtime/src/tools/gh_get_file_content", SOURCE_VIEW],
        AstSearch => &["runtime/src/tools/ast_search"],
        LspSearch => &["runtime/src/tools/lsp_search"],
        StructureSearch => &["runtime/src/tools/structure_search"],
        GhSearchCode => &["runtime/src/tools/gh_search_code", GITHUB_SEARCH],
        GhSearchRepo => &["runtime/src/tools/gh_search_repo", GITHUB_SEARCH],
        GhSearchHistory => &["runtime/src/tools/gh_search_history", GITHUB_SEARCH],
        GhStructure => &["runtime/src/tools/gh_structure"],
        GhGetHistoryItem => &["runtime/src/tools/gh_get_history_item"],
        ArtifactSearch => &[
            "runtime/src/tools/artifact_search",
            "runtime/src/providers/artifact",
        ],
        SecurityCheck => &["runtime/src/tools/security_check"],
        // Neither lists coverage reasons.
        ToolId::Clasify | ToolId::GhCloneRepo => &[],
    }
}

fn crates_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates directory")
        .to_path_buf()
}

/// Whether `path` holds test code (a `tests` tree or a `*_tests` file).
fn is_test(path: &Path) -> bool {
    path.components().any(|part| {
        let part = part.as_os_str().to_string_lossy();
        part == "tests"
            || part.ends_with("_tests")
            || part.ends_with("_tests.rs")
            || part == "tests.rs"
    })
}

fn sources(tool: ToolId) -> String {
    fn walk(path: &Path, out: &mut String) {
        if is_test(path) {
            return;
        }
        if path.is_dir() {
            let mut entries: Vec<_> = std::fs::read_dir(path)
                .expect("read dir")
                .flatten()
                .map(|entry| entry.path())
                .collect();
            entries.sort();
            entries.iter().for_each(|entry| walk(entry, out));
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push_str(&std::fs::read_to_string(path).expect("source"));
            out.push('\n');
        }
    }
    let mut text = String::new();
    for tree in trees(tool) {
        walk(&crates_dir().join(tree), &mut text);
    }
    text
}

/// How code spells a reason: its wire name, its `PartialReason` variant, or
/// the engine's `CapReason` that names it.
fn spellings(reason: &str) -> Vec<String> {
    let mut variant = reason.to_owned();
    variant[..1].make_ascii_uppercase();
    // `"cancelled"` is also the text of internal stop errors, so only the
    // typed spellings count for it.
    let mut spelled = vec![format!("PartialReason::{variant}")];
    if reason != "cancelled" {
        spelled.push(format!("\"{reason}\""));
    }
    let cap = match reason {
        "fileTooLarge" => Some("MaxFileSize"),
        "cancelled" => Some("Cancelled"),
        "binaryQuit" => Some("BinaryQuit"),
        _ => None,
    };
    spelled.extend(cap.map(|cap| format!("CapReason::{cap}")));
    if reason == "securitySelectedViewSizeLimit" {
        spelled.push("PartialReason::SecuritySelectedView".to_owned());
    }
    spelled
}

fn emits(source: &str, reason: &str) -> bool {
    spellings(reason)
        .iter()
        .any(|spelling| source.contains(spelling.as_str()))
}

fn dictionary() -> Map<String, Value> {
    crate::contracts::partial_reasons()
        .and_then(Value::as_object)
        .expect("the contract's partialReasons dictionary")
        .clone()
}

/// The registry is the emit sites: each listed tool's code names its reason,
/// and no tool's code names a reason the registry does not list it for.
#[test]
fn every_listed_reason_is_emittable_by_its_tool_and_no_other() {
    let sources: Vec<(ToolId, String)> = ToolId::ALL
        .into_iter()
        .map(|tool| (tool, sources(tool)))
        .collect();
    for (reason, _) in dictionary() {
        let listed = EMITTERS
            .iter()
            .find(|(name, _)| *name == reason)
            .unwrap_or_else(|| panic!("{reason} is in the dictionary but not the registry"))
            .1;
        for (tool, source) in &sources {
            assert_eq!(
                emits(source, &reason),
                listed.contains(tool),
                "{reason} for {tool:?}: {}",
                if listed.contains(tool) {
                    "listed but no code of the tool names it"
                } else {
                    "the tool's code names it but the registry does not list it"
                }
            );
        }
    }
    for (reason, _) in EMITTERS {
        assert!(
            dictionary().contains_key(*reason),
            "{reason} is registered but core's dictionary lacks it"
        );
    }
}

/// A tool's reference serves its own reasons and none of another tool's.
#[test]
fn a_tool_is_served_only_the_reasons_it_emits() {
    let all = dictionary();
    let git = emitted_by(GhSearchRepo, &all);
    assert!(git.contains_key("providerResultCap"));
    assert!(!git.contains_key("serverUnhealthy"), "an lspSearch reason");
    let lsp = emitted_by(LspSearch, &all);
    assert!(lsp.contains_key("serverUnhealthy") && !lsp.contains_key("providerResultCap"));
    assert!(git.len() < all.len() / 4 && lsp.len() < all.len());
}
