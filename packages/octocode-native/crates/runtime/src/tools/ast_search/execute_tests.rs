//! Typed rows through `execute_ast`: shape and cancellation refusals, the
//! syntaxTree surface, and the missing-path recovery.
use serde_json::json;

use super::*;
mod sorting;
use crate::{
    execution::cancel::CancellationCheck,
    policy::path::{PathPolicy, PathPolicyConfig},
    security::ContentSecurity,
};

struct Cancelled;
impl CancellationCheck for Cancelled {
    fn check(&self) -> Result<(), String> {
        Err("cancelled by test".to_owned())
    }
}

/// Tests speak JSON rows; the runtime owns the typed parse.
fn execute_row(
    query: Value,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancellation: &dyn CancellationCheck,
) -> Result<Value, ToolError> {
    let query: AstSearchQuery = serde_json::from_value(query).expect("typed astSearch row");
    execute_ast(&query, paths, security, cancellation, None)
}

fn context() -> (PathPolicy, ContentSecurity) {
    (
        PathPolicy::new(PathPolicyConfig::default()).expect("default path policy"),
        ContentSecurity::new(),
    )
}

#[test]
fn input_shape_and_cancellation_are_rejected() {
    let (paths, security) = context();
    // Shape mismatches never reach the tool: the runtime's typed parse
    // rejects them (reported as `invalidInput`).
    for row in [
        json!({"path":"."}),
        json!({"operation":"symbols","path":".","unknown":true}),
    ] {
        assert!(serde_json::from_value::<AstSearchQuery>(row).is_err());
    }

    let cancelled = execute_row(
        json!({"operation":"symbols","path":"."}),
        &paths,
        &security,
        &Cancelled,
    )
    .expect_err("cancelled before filesystem access");
    assert_eq!(cancelled.code, ErrorCode::Cancelled);
}

#[test]
fn syntax_tree_is_the_only_tree_surface() {
    let root = tempfile::tempdir().expect("fixture directory");
    let source = root.path().join("fixture.ts");
    std::fs::write(&source, "export const value = 1;\n").expect("fixture source");
    let (paths, security) = crate::tools::test_support::simple_policy(root.path());

    let syntax = execute_row(
        json!({"operation":"syntaxTree","path":source.to_string_lossy()}),
        &paths,
        &security,
        &crate::execution::cancel::NeverCancel,
    )
    .expect("syntax tree");
    assert_eq!(syntax["operation"], "syntaxTree");
    assert!(syntax.get("treeKind").is_none(), "{syntax}");
    // One compact row per node: id, kind (an anonymous token quoted),
    // 1-based line:column span, and `^parent`, paired with its byte span.
    let nodes = syntax["nodes"].as_array().expect("node rows");
    assert_eq!(nodes[0]["node"], "0 program 1:1-2:1", "{syntax}");
    assert_eq!(
        nodes[1]["node"], "1 export_statement 1:1-1:24 ^0",
        "{syntax}"
    );
    // A node that fills a grammar field names it before its kind: the
    // name a rule's `has`/`inside` `field` matches.
    assert!(
        nodes.iter().any(|row| row["node"]
            .as_str()
            .is_some_and(|row| row.contains(" name:identifier 1:14-1:19 "))),
        "{syntax}"
    );
    assert!(syntax.get("nextOffset").is_none(), "{syntax}");
    let tokens = execute_row(
        json!({"operation":"syntaxTree","path":source.to_string_lossy(),"namedOnly":false}),
        &paths,
        &security,
        &crate::execution::cancel::NeverCancel,
    )
    .expect("token tree");
    assert_eq!(
        tokens["nodes"][2]["node"], "2 \"export\" 1:1-1:7 ^1",
        "{tokens}"
    );
    assert_eq!(
        nodes[0]["sourceRange"],
        json!({"startByte":0,"endByte":24}),
        "{syntax}"
    );
    assert_eq!(
        tokens["nodes"][2]["sourceRange"],
        json!({"startByte":0,"endByte":6}),
        "{tokens}"
    );
    assert!(syntax.get("byteRanges").is_none(), "{syntax}");

    for retired in [
        json!({"operation":"tree","treeKind":"filesystem","path":root.path().to_string_lossy()}),
        json!({"operation":"tree","treeKind":"syntax","path":source.to_string_lossy()}),
        json!({"operation":"files","path":root.path().to_string_lossy()}),
        json!({"operation":"syntaxTree","path":source.to_string_lossy(),"entryType":"f"}),
        json!({"operation":"syntaxTree","path":source.to_string_lossy(),"sort":{"by":"size","order":"asc"}}),
        json!({"operation":"topology","analysis":"dependencies","path":root.path().to_string_lossy(),"file":"fixture.ts"}),
    ] {
        assert!(
            serde_json::from_value::<AstSearchQuery>(retired).is_err(),
            "retired astSearch surface must be rejected"
        );
    }
}

/// A relative path that does not exist leads to the tree of its nearest
/// existing parent, in every operation, as in every local tool; the
/// message names no absolute filesystem path.
#[test]
fn a_missing_relative_path_leads_to_its_parent_without_an_absolute_root() {
    let root = tempfile::tempdir().expect("root");
    std::fs::create_dir(root.path().join("src")).expect("src");
    let paths = crate::tools::test_support::workspace_policy(root.path());
    let security = ContentSecurity::new();
    for row in [
        json!({"operation":"symbols","path":"src/nope.rs"}),
        json!({"operation":"syntaxTree","path":"src/nope.rs"}),
        json!({"operation":"match","path":"src/nope.rs","rule":{"pattern":"fn $A() {}"}}),
    ] {
        let error = execute_row(
            row.clone(),
            &paths,
            &security,
            &crate::execution::cancel::NeverCancel,
        )
        .expect_err("missing path");
        assert_eq!(error.code, ErrorCode::PathNotFound, "{row}");
        assert!(
            !error
                .message
                .contains(&root.path().to_string_lossy().into_owned()),
            "{row}: {}",
            error.message
        );
        let lead = &error.calls.as_ref().expect("lead")["viewTree"];
        assert_eq!(lead["tool"], "structureSearch", "{row}");
        assert_eq!(lead["input"]["queries"][0]["path"], "src", "{row}");
    }
}

#[test]
fn delivered_paths_resolve_to_the_same_source_inside_and_outside_workspace() {
    let root = tempfile::tempdir().expect("workspace");
    let outside = tempfile::tempdir().expect("additional root");
    std::fs::create_dir(root.path().join("src")).expect("src");
    let inside = root.path().join("src/item.ts");
    let external = outside.path().join("item.ts");
    for path in [&inside, &external] {
        std::fs::write(path, "export function item() { return 1; }\n").expect("source");
    }
    let paths = PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(root.path().to_path_buf()),
        additional_roots: vec![outside.path().to_path_buf()],
        ..Default::default()
    })
    .expect("path policy");
    for source in [&inside, &external] {
        let canonical = source.canonicalize().expect("canonical source");
        let expected = if source == &inside {
            "src/item.ts".to_owned()
        } else {
            canonical.to_string_lossy().into_owned()
        };
        for row in [
            json!({"operation":"symbols","path":source}),
            json!({"operation":"symbols","path":source.parent()}),
            json!({"operation":"syntaxTree","path":source}),
            json!({"operation":"match","path":source,"rule":{"pattern":"return $V"}}),
            json!({"operation":"match","path":source.parent(),"rule":{"pattern":"return $V"}}),
        ] {
            let out = execute_row(
                row.clone(),
                &paths,
                &ContentSecurity::new(),
                &crate::execution::cancel::NeverCancel,
            )
            .expect("AST result");
            let shown = out
                .get("path")
                .unwrap_or(&out["files"][0]["path"])
                .as_str()
                .expect("source path");
            assert_eq!(shown, expected, "{row}: {out}");
            assert_eq!(
                paths
                    .validate_read(shown)
                    .expect("executable result path")
                    .canonical,
                canonical
            );
        }
    }
    for directory in [root.path(), outside.path()] {
        let source = directory.join("Cargo.toml");
        std::fs::write(&source, "[package]\nname=\"fixture\"\n").expect("document");
        let out = execute_row(
            json!({"operation":"symbols","path":source}),
            &paths,
            &ContentSecurity::new(),
            &crate::execution::cancel::NeverCancel,
        )
        .expect("terminal error row");
        assert_eq!(out["status"], "error");
        let shown = out["path"].as_str().expect("error source path");
        assert_eq!(
            std::path::Path::new(shown).is_relative(),
            directory == root.path()
        );
        assert_eq!(
            paths
                .validate_read(shown)
                .expect("resolvable error path")
                .canonical,
            source.canonicalize().expect("canonical document")
        );
    }
}
