use super::*;

mod includes;
mod python;
mod rust;
mod text_leads;

/// A workspace-symbol row is partial because the protocol has no
/// completeness marker: the row says so once (a notice), says no page
/// completes it, and leads to the lexical search that checks the rest.
#[test]
fn workspace_coverage_preserves_errors_and_avoids_a_directory_document_hint() {
    let scope = Scope::new("/w".into(), vec!["*.ts".into()]);
    let mut stale = json!({"status":"error","errorCode":"staleSnapshot"});
    let before = stale.clone();
    workspace_symbols(&mut stale, "Shared", &scope, Some("/w/a.ts"), None);
    assert_eq!(stale, before);
    let mut empty = json!({"kind":"empty"});
    workspace_symbols(&mut empty, "Shared", &scope, None, None);
    assert_eq!(empty["isPartial"], true);
    assert_eq!(
        empty["partialReasons"],
        json!(["workspaceSymbolCoverageUnknown"])
    );
    let notice = empty["notices"][0].as_str().unwrap_or_default();
    assert!(
        notice.contains("no completeness marker") && notice.contains("localSearch"),
        "{empty}"
    );
    assert!(empty.get("warnings").is_none(), "{empty}");
    let text = &empty["next"]["findText"]["query"]["queries"][0];
    assert_eq!(text["path"], "/w", "{empty}");
    assert_eq!(text["include"], json!(["*.ts"]), "{empty}");
    // The query term is searched as typed: a partial term (servers match
    // it as a prefix) is no whole word.
    let mut partial = json!({"kind":"symbols"});
    workspace_symbols(&mut partial, "ToolRunt", &scope, None, None);
    let text = &partial["next"]["findText"]["query"]["queries"][0];
    assert_eq!(text["matchString"], "ToolRunt", "{partial}");
    assert!(
        text.get("regex").is_none() && text.get("caseMode").is_none(),
        "a literal term, smart case: {partial}"
    );
    assert!(
        empty["next"].get("inspectDocumentSymbols").is_none(),
        "a directory is not a document"
    );
    assert!(empty.get("terminalLimit").is_none());
}

#[test]
fn gate_scan_reports_unvisited_files_after_its_cap() {
    let root = tempfile::tempdir().expect("root");
    std::fs::write(root.path().join("a.rs"), "pub fn other() {}\n").expect("first");
    std::fs::write(
        root.path().join("z.rs"),
        "#[cfg(feature = \"extra\")] pub fn wanted() {}\n",
    )
    .expect("second");
    let scan = scan_gates(root.path(), "wanted", Gate::Feature, 1);
    assert!(scan.files.is_empty());
    assert!(scan.has_more);
}

/// A gated file counts only when it uses the name in code and the
/// server's answer did not cover it: a doc link or a comment is no use,
/// and an answered file was analyzed.
#[test]
fn gate_scan_ignores_comment_mentions_and_answered_files() {
    let dir = tempfile::tempdir().expect("dir");
    let root = dir.path().canonicalize().expect("root");
    std::fs::write(
        root.join("doc.rs"),
        "//! See [`Continuation`].\n#[cfg(feature = \"x\")]\nfn other() {}\n",
    )
    .expect("doc");
    let used = root.join("used.rs");
    std::fs::write(&used, "#[cfg(feature = \"x\")]\nfn f(c: Continuation) {}\n").expect("used");
    let scope = Scope::new(root.to_string_lossy().into_owned(), vec!["*.rs".into()]);
    let count = gated_files_mentioning(&scope, &scope.root, "Continuation", Gate::Feature);
    assert_eq!(count.scan.files, vec![used.to_string_lossy().into_owned()]);
    assert_eq!(count.matches, 1);
    scope.answer([used.to_string_lossy()]);
    assert_eq!(
        gated_files_mentioning(&scope, &scope.root, "Continuation", Gate::Feature).matches,
        0
    );
}

fn query(value: Value) -> LspSearchQuery {
    serde_json::from_value(value).expect("query")
}

/// A Rust row of `symbol` in a package at `root/pkg` whose `src/lib.rs`
/// is `lib`, after the server answered `answered` (files under `pkg/src`).
async fn rust_row(root: &Path, lib: &str, query_fields: Value, answered: &[&str]) -> Value {
    let package = root.join("pkg");
    std::fs::write(package.join("src/lib.rs"), lib).expect("lib");
    let lib_path = package.join("src/lib.rs").to_string_lossy().into_owned();
    let root_text = root.to_string_lossy().into_owned();
    let policy = crate::tools::test_support::workspace_policy(root);
    let scope = Scope::new(root_text.clone(), vec!["*.rs".into()]);
    scope
        .text_files(
            "include_diff",
            &policy,
            &crate::execution::cancel::NeverCancel,
        )
        .await
        .expect("scan");
    scope.answer(answered.iter().map(|file| {
        package
            .join("src")
            .join(file)
            .to_string_lossy()
            .into_owned()
    }));
    let mut q = json!({"path": lib_path, "operation": "references",
        "symbolName": "include_diff", "line": 1});
    for (key, value) in query_fields.as_object().into_iter().flatten() {
        q[key] = value.clone();
    }
    if q["operation"] == "workspaceSymbol" {
        q.as_object_mut().expect("query").remove("line");
    }
    let mut row = json!({"kind":"references","matches":[{"path":"pkg/src/lib.rs"}]});
    annotate(
        &mut row,
        &query(q),
        Some("rust"),
        Server {
            anchor: &lib_path,
            root: &root_text,
        },
        &scope,
    );
    row
}

#[test]
fn text_search_lead_targets_repo_scope_with_language_include() {
    let mut row = json!({"kind":"references","matches":[{"path":"a.py"}]});
    let q = query(
        json!({"path":"/repo/pkg/a.py","operation":"references","symbolName":"get","line":3}),
    );
    let scope = Scope::new("/repo".into(), vec!["*.py".into(), "*.pyi".into()]);
    annotate(
        &mut row,
        &q,
        Some("python"),
        Server {
            anchor: "/w/a.py",
            root: "/w",
        },
        &scope,
    );
    let lead = &row["next"]["findText"]["query"]["queries"][0];
    assert_eq!(lead["path"], "/repo", "{row}");
    assert_eq!(lead["include"], json!(["*.py", "*.pyi"]), "{row}");
}

#[test]
fn gate_and_word_detection() {
    assert!(has_feature_gate("#[cfg(all(test, feature = \"x\"))]"));
    assert!(has_feature_gate("#![cfg(feature=\"full\")]"));
    assert!(!has_feature_gate("#[cfg(test)]\nlet feature = 1;"));
    assert!(mentions_word("a(spawn_blocking)", "spawn_blocking"));
    assert!(!mentions_word("spawn_blocking_extra", "spawn_blocking"));
}
