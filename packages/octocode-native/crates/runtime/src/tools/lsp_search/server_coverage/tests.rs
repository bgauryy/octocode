use super::*;

mod includes;
mod python;
mod rust;
mod text_leads;

/// The protocol has no completeness marker on any workspace-symbol answer:
/// the row says so once (a notice, not a partial flag), says no page
/// completes it, and leads to the lexical search that checks the rest.
#[test]
fn workspace_coverage_preserves_errors_and_avoids_a_directory_document_hint() {
    let scope = Scope::new("/w".into(), vec!["*.ts".into()]);
    let mut stale = json!({"status":"error","errorCode":"staleSnapshot"});
    let before = stale.clone();
    workspace_symbols(&mut stale, "Shared", &scope, Some("/w/a.ts"), None);
    assert_eq!(stale, before);
    let mut empty = json!({"status":"empty"});
    workspace_symbols(&mut empty, "Shared", &scope, None, None);
    assert!(empty.get("isPartial").is_none(), "{empty}");
    assert!(empty.get("partialReasons").is_none(), "{empty}");
    let notice = empty["notices"][0].as_str().unwrap_or_default();
    assert!(
        notice.contains("no completeness marker") && notice.contains("localSearch"),
        "{empty}"
    );
    assert!(empty.get("warnings").is_none(), "{empty}");
    let text = &crate::tools::test_support::checks::related_call(&empty, "localSearch", |_| true)["input"]
        ["queries"][0];
    assert_eq!(text["path"], "/w", "{empty}");
    assert_eq!(text["include"], json!(["*.ts"]), "{empty}");
    // The query term is searched as typed: a partial term (servers match
    // it as a prefix) is no whole word.
    let mut partial = json!({"kind":"symbols"});
    workspace_symbols(&mut partial, "ToolRunt", &scope, None, None);
    let text = &crate::tools::test_support::checks::related_call(&partial, "localSearch", |_| true)
        ["input"]["queries"][0];
    assert_eq!(text["matchString"], json!(["ToolRunt"]), "{partial}");
    assert!(
        text.get("regex").is_none() && text.get("caseMode").is_none(),
        "a literal term, smart case: {partial}"
    );
    assert!(
        !empty["related"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|call| call["tool"] == "lspSearch"
                && call["input"]["queries"][0]["operation"] == "documentSymbols"),
        "a directory is not a document"
    );
    assert!(empty.get("terminalLimit").is_none());
}

/// `scope` after its text scan of `name`, as a request runs it before
/// coverage annotations.
fn scanned(scope: Scope, name: &str) -> Scope {
    let policy = crate::tools::test_support::workspace_policy(Path::new(&scope.root));
    tokio::runtime::Runtime::new()
        .expect("rt")
        .block_on(scope.text_files(name, &policy, &crate::execution::cancel::NeverCancel))
        .expect("scan");
    scope
}

/// The gate check reads only the files the request's text scan found
/// under the server root, never every Rust file: among a thousand files,
/// the one gated file that names the symbol is found by reading the files
/// that mention it. Only the request (its cancellation or deadline) stops
/// it early, which it discloses; a failed text scan checked nothing.
#[test]
fn gate_check_reads_only_the_files_that_mention_the_name() {
    let dir = tempfile::tempdir().expect("root");
    let root = dir.path().canonicalize().expect("canonical");
    for n in 0..1000 {
        std::fs::write(root.join(format!("a{n:04}.rs")), "pub fn other() {}\n").expect("file");
    }
    std::fs::write(root.join("lib.rs"), "pub fn wanted() {}\n").expect("lib");
    std::fs::write(
        root.join("z.rs"),
        "#[cfg(feature = \"extra\")] fn f() { wanted(); }\n",
    )
    .expect("gated");
    let text = root.to_string_lossy().into_owned();
    let scope = scanned(Scope::new(text.clone(), vec!["*.rs".into()]), "wanted");
    let mentions = scope.scanned_files("wanted").expect("scan").len();
    assert_eq!(mentions, 2, "lib.rs and z.rs spell the name");
    GATE_READS.with(|reads| reads.set(0));
    let whole = gated_files_mentioning(&scope, &text, "wanted", Gate::Feature, &|| false);
    assert_eq!(whole.matches, 1, "the gated file is found");
    assert!(!whole.incomplete());
    assert_eq!(
        GATE_READS.with(std::cell::Cell::get),
        mentions,
        "files read"
    );
    let stopped = gated_files_mentioning(&scope, &text, "wanted", Gate::Feature, &|| true);
    assert!(stopped.scan.files.is_empty() && stopped.scan.has_more);
    let unscanned = Scope::new(text.clone(), vec!["*.rs".into()]);
    let unknown = gated_files_mentioning(&unscanned, &text, "wanted", Gate::Feature, &|| false);
    assert!(unknown.incomplete() && unknown.matches == 0);
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
    let scope = scanned(
        Scope::new(root.to_string_lossy().into_owned(), vec!["*.rs".into()]),
        "Continuation",
    );
    let count = gated_files_mentioning(&scope, &scope.root, "Continuation", Gate::Feature, &|| {
        false
    });
    assert_eq!(count.scan.files, vec![used.to_string_lossy().into_owned()]);
    assert_eq!(count.matches, 1);
    scope.answer([used.to_string_lossy()]);
    assert_eq!(
        gated_files_mentioning(&scope, &scope.root, "Continuation", Gate::Feature, &|| {
            false
        })
        .matches,
        0
    );
}

fn query(value: Value) -> LspSearchQuery {
    serde_json::from_value(value).expect("query")
}

fn configured_rust(
    query: &LspSearchQuery,
    options: Value,
) -> octocode_engine::lsp::types::LspLanguageServerConfig {
    let mut config = octocode_engine::lsp::types::LspLanguageServerConfig {
        command: "rust-analyzer".into(),
        args: None,
        workspace_root: "/repo".into(),
        language_id: Some("rust".into()),
        initialization_options: Some(options),
        env: None,
        max_memory_mb: None,
        source: Default::default(),
    };
    super::super::receipt::apply_rust_context(&mut config, query).expect("Rust context");
    config
}

fn annotate(
    row: &mut Value,
    query: &LspSearchQuery,
    language_id: Option<&str>,
    server: Server<'_>,
    scope: &Scope,
) {
    let config = configured_rust(
        query,
        octocode_engine::lsp::config::rust_analyzer_headless_options(),
    );
    super::annotate(
        row,
        query,
        language_id,
        server,
        scope,
        super::super::receipt::RustAnalysis::of(&config),
        &|| false,
    );
}

/// A Rust row of `symbol` in a package at `root/pkg` whose `src/lib.rs`
/// is `lib`, after the server answered `answered` (files under `pkg/src`).
async fn rust_row(root: &Path, lib: &str, query_fields: Value, answered: &[&str]) -> Value {
    rust_row_configured(
        root,
        lib,
        query_fields,
        answered,
        octocode_engine::lsp::config::rust_analyzer_headless_options(),
    )
    .await
}

async fn rust_row_configured(
    root: &Path,
    lib: &str,
    query_fields: Value,
    answered: &[&str],
    options: Value,
) -> Value {
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
        let name = q
            .as_object_mut()
            .expect("query")
            .remove("symbolName")
            .expect("name");
        q["symbolQuery"] = name;
    }
    let mut row = json!({"operation":"references","matches":[{"path":"pkg/src/lib.rs"}]});
    let q = query(q);
    let config = configured_rust(&q, options);
    super::annotate(
        &mut row,
        &q,
        Some("rust"),
        Server {
            anchor: &lib_path,
            root: &root_text,
        },
        &scope,
        super::super::receipt::RustAnalysis::of(&config),
        &|| false,
    );
    row
}

#[test]
fn text_search_lead_targets_repo_scope_with_language_include() {
    let mut row = json!({"operation":"references","matches":[{"path":"a.py"}]});
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
    let lead = &crate::tools::test_support::checks::related_call(&row, "localSearch", |_| true)["input"]
        ["queries"][0];
    assert_eq!(lead["path"], "/repo", "{row}");
    assert_eq!(lead["include"], json!(["*.py", "*.pyi"]), "{row}");
}

#[test]
fn gate_detection() {
    assert!(has_feature_gate("#[cfg(all(test, feature = \"x\"))]"));
    assert!(has_feature_gate("#![cfg(feature=\"full\")]"));
    assert!(!has_feature_gate("#[cfg(test)]\nlet feature = 1;"));
}
