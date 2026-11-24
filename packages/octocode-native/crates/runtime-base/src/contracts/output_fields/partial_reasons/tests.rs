use super::*;
use std::path::{Path, PathBuf};

/// The source trees whose non-test code builds a tool's rows, relative to
/// the workspace's `crates/` directory. A tree shared by several tools
/// (`source_view`, the GitHub search rule) is listed for each of them.
fn trees(tool: ToolId) -> &'static [&'static str] {
    const SOURCE_VIEW: &str = "tools-shared/src/source_view";
    const GITHUB_SEARCH: &str = "tools-shared/src/gh_shared/search.rs";
    match tool {
        GraphBuild => &[
            "tool-graph/src/ast_graph/store/ingest",
            "tool-graph/src/ast_graph/graph",
        ],
        GraphQuery => &["tool-graph/src/ast_graph/store/query"],
        LocalSearch => &[
            "tool-local/src/local_search",
            "tools-shared/src/large_source",
            "engine/src/search/text_search",
            SOURCE_VIEW,
        ],
        LocalFetch => &[
            "tool-local/src/local_fetch",
            "tools-shared/src/large_source",
            "tools-shared/src/source_capture",
            SOURCE_VIEW,
        ],
        GhGetFileContent => &["tool-github/src/gh_get_file_content", SOURCE_VIEW],
        AstSearch => &["tool-ast/src/ast_search"],
        LspSearch => &["tool-lsp/src/lsp_search"],
        StructureSearch => &["tool-local/src/structure_search"],
        GhSearchCode => &["tool-github/src/gh_search_code", GITHUB_SEARCH],
        GhSearchRepo => &["tool-github/src/gh_search_repo", GITHUB_SEARCH],
        GhSearchHistory => &["tool-github/src/gh_search_history", GITHUB_SEARCH],
        GhStructure => &["tool-github/src/gh_structure"],
        GhGetHistoryItem => &["tool-github/src/gh_get_history_item"],
        ArtifactSearch => &[
            "tool-packages/src/artifact_search",
            "tool-packages/src/artifact",
        ],
        SecurityCheck => &["tool-packages/src/security_check"],
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

/// The non-test Rust source under `path`, appended to `out`.
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

/// The non-test source of `trees` (relative to `crates/`); a moved tree
/// would leave its tool's reasons unread.
fn read_trees(tool: ToolId, trees: &[&str]) -> String {
    let mut text = String::new();
    for tree in trees {
        let path = crates_dir().join(tree);
        assert!(path.exists(), "{tool:?} source tree {tree} does not exist");
        walk(&path, &mut text);
    }
    text
}

fn sources(tool: ToolId) -> String {
    read_trees(tool, trees(tool))
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

/// Reasons a shared tree writes that a tool's own code removes from every
/// row it serves, with that tool's own tree: ghSearchRepo states the
/// provider cap once, as a warning (`state_cap_once`), so its rows never
/// carry `providerResultCap`.
const STRIPPED: &[(ToolId, &str, &str)] = &[(
    GhSearchRepo,
    "providerResultCap",
    "tool-github/src/gh_search_repo",
)];

/// Whether `tool` serves `reason`: its code names it and its own code does
/// not strip it ([`STRIPPED`]).
fn serves(tool: ToolId, source: &str, reason: &str) -> bool {
    emits(source, reason)
        && !STRIPPED
            .iter()
            .any(|(stripper, stripped, _)| *stripper == tool && *stripped == reason)
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
                serves(*tool, source, &reason),
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

/// A stripped reason's strip site is still in its tool's own code: the
/// tool names the reason it removes. Without it the shared tree's reason
/// reaches the tool's rows, and the registry must list the tool again.
#[test]
fn every_stripped_reason_is_removed_by_its_tools_own_code() {
    for (tool, reason, tree) in STRIPPED {
        let own = read_trees(*tool, &[tree]);
        assert!(
            own.contains(&format!("\"{reason}\"")),
            "{tool:?} no longer strips {reason}: list it as an emitter again"
        );
        assert!(
            emitted_by(*tool, &dictionary()).get(*reason).is_none(),
            "{tool:?} strips {reason}, so its reference must not list it"
        );
    }
}

/// A tool's reference serves its own reasons and none of another tool's.
#[test]
fn a_tool_is_served_only_the_reasons_it_emits() {
    let all = dictionary();
    let code = emitted_by(GhSearchCode, &all);
    assert!(code.contains_key("providerResultCap"));
    assert!(!code.contains_key("serverUnhealthy"), "an lspSearch reason");
    let lsp = emitted_by(LspSearch, &all);
    assert!(lsp.contains_key("serverUnhealthy") && !lsp.contains_key("providerResultCap"));
    assert!(code.len() < all.len() / 4 && lsp.len() < all.len());
    // ghSearchRepo states its provider cap as a warning, never as a reason.
    assert!(!emitted_by(GhSearchRepo, &all).contains_key("providerResultCap"));
}
