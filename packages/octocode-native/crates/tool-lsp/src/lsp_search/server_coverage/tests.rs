use super::*;
use octocode_test_support::json_fixture;
use serde_json::json;

mod health;
mod includes;
mod python;
mod questions;
mod rust;
mod targets;
mod text_leads;

/// An empty workspace-symbol answer leads to the text search alone: its
/// anchor file only picked the project, so neither its outline nor a read
/// of it stands in for the search (local-flows F10).
#[test]
fn an_empty_workspace_symbol_answer_leads_only_to_the_text_search() {
    let scope = Scope::new("/w".into(), vec!["*.ts".into()]);
    let query: LspSearchQuery = serde_json::from_value(json!({
        "operation": "workspaceSymbol", "symbolFuzzy": "walkSkipLedger", "path": "/w/a.ts"
    }))
    .expect("query");
    let mut empty = json_fixture(r#"{"status": "empty", "category": "noLocations"}"#);
    workspace_symbols(
        &mut empty,
        &query,
        "walkSkipLedger",
        &scope,
        Some("/w/a.ts"),
        None,
    );
    let empty = crate::lsp_search::failure::with_next(&query, empty);
    let tools = empty["related"]
        .as_array()
        .expect("leads")
        .iter()
        .map(|call| call["tool"].as_str().expect("tool"))
        .collect::<Vec<_>>();
    assert_eq!(tools, ["localSearch"], "{empty}");
    // An answer with symbols still offers the anchor's outline.
    let mut found = json_fixture(r#"{"matches": [{"symbolName": "walkSkipLedger"}]}"#);
    workspace_symbols(
        &mut found,
        &query,
        "walkSkipLedger",
        &scope,
        Some("/w/a.ts"),
        None,
    );
    assert!(found.to_string().contains("documentSymbols"), "{found}");
}

/// The protocol has no completeness marker on any workspace-symbol answer:
/// the tool description says so once, so the row is no partial and carries
/// no note of it; it leads to the lexical search that checks the rest.
#[test]
fn workspace_coverage_preserves_errors_and_avoids_a_directory_document_hint() {
    let scope = Scope::new("/w".into(), vec!["*.ts".into()]);
    let mut stale = json_fixture(r#"{"status": "error", "errorCode": "staleSnapshot"}"#);
    let before = stale.clone();
    workspace_symbols(
        &mut stale,
        &first_page("Shared"),
        "Shared",
        &scope,
        Some("/w/a.ts"),
        None,
    );
    assert_eq!(stale, before);
    let mut empty = json_fixture(r#"{"status": "empty"}"#);
    workspace_symbols(
        &mut empty,
        &first_page("Shared"),
        "Shared",
        &scope,
        None,
        None,
    );
    assert!(empty.get("isPartial").is_none(), "{empty}");
    assert!(empty.get("partialReasons").is_none(), "{empty}");
    assert!(empty.get("notices").is_none(), "{empty}");
    assert!(empty.get("warnings").is_none(), "{empty}");
    let text =
        &octocode_runtime_base::test_support::checks::related_call(&empty, "localSearch", |_| true)
            ["input"]["queries"][0];
    assert_eq!(text["path"], "/w", "{empty}");
    assert_eq!(text["include"], json_fixture(r#"["*.ts"]"#), "{empty}");
    // The query term is searched as typed: a partial term (servers match
    // it as a prefix) is no whole word.
    let mut partial = json_fixture(r#"{"kind": "symbols"}"#);
    workspace_symbols(
        &mut partial,
        &first_page("ToolRunt"),
        "ToolRunt",
        &scope,
        None,
        None,
    );
    let text =
        &octocode_runtime_base::test_support::checks::related_call(&partial, "localSearch", |_| {
            true
        })["input"]["queries"][0];
    assert_eq!(
        text["matchString"],
        json_fixture(r#"["ToolRunt"]"#),
        "{partial}"
    );
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

/// A first-page workspaceSymbol query for `term`.
fn first_page(term: &str) -> LspSearchQuery {
    serde_json::from_value(json!({
        "operation": "workspaceSymbol", "symbolFuzzy": term, "workspaceRoot": "/w"
    }))
    .expect("query")
}

/// A later page of a workspace-symbol answer repeats none of the leads its
/// first page carried, and no page carries a note; a server cap
/// stays, a fact of every page.
#[test]
fn a_later_workspace_symbol_page_repeats_no_note_or_lead() {
    let scope = Scope::new("/w".into(), vec!["*.ts".into()]);
    let later: LspSearchQuery = serde_json::from_value(json_fixture(
        r#"{
            "operation": "workspaceSymbol",
            "symbolFuzzy": "Shared",
            "workspaceRoot": "/w",
            "page": 2,
            "snapshot": "lsp-v3:abc"
        }"#,
    ))
    .expect("query");
    let mut row = json_fixture(r#"{"kind": "symbols"}"#);
    workspace_symbols(
        &mut row,
        &later,
        "Shared",
        &scope,
        Some("/w/a.ts"),
        Some((100, "raise it")),
    );
    assert!(row.get("notices").is_none(), "{row}");
    assert!(row.get("related").is_none(), "{row}");
    assert_eq!(
        row["partialReasons"],
        json_fixture(r#"["workspaceSymbolCoverageUnknown"]"#),
        "{row}"
    );
    let mut first = json_fixture(r#"{"kind": "symbols"}"#);
    workspace_symbols(
        &mut first,
        &first_page("Shared"),
        "Shared",
        &scope,
        Some("/w/a.ts"),
        None,
    );
    assert!(first.get("notices").is_none(), "{first}");
    assert!(first.get("related").is_some(), "{first}");
}

/// `scope` after its text scan of `name`, as a request runs it before
/// coverage annotations.
fn scanned(scope: Scope, name: &str) -> Scope {
    let policy = octocode_runtime_base::test_support::workspace_policy(Path::new(&scope.root));
    tokio::runtime::Runtime::new()
        .expect("rt")
        .block_on(scope.text_files(
            name,
            &policy,
            &octocode_runtime_base::execution::cancel::NeverCancel,
        ))
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
        document_language_ids: Default::default(),
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
        super::super::receipt::RustAnalysis::of(&config).allowing_build_scripts(true),
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
    rust_row_gated(root, lib, query_fields, answered, options, true).await
}

/// [`rust_row_configured`] where `allowed` is the host's
/// `lsp.allowBuildScripts`.
async fn rust_row_gated(
    root: &Path,
    lib: &str,
    query_fields: Value,
    answered: &[&str],
    options: Value,
    allowed: bool,
) -> Value {
    let package = root.join("pkg");
    std::fs::write(package.join("src/lib.rs"), lib).expect("lib");
    let lib_path = package.join("src/lib.rs").to_string_lossy().into_owned();
    let root_text = root.to_string_lossy().into_owned();
    let policy = octocode_runtime_base::test_support::workspace_policy(root);
    let scope = Scope::new(root_text.clone(), vec!["*.rs".into()]);
    scope
        .text_files(
            "include_diff",
            &policy,
            &octocode_runtime_base::execution::cancel::NeverCancel,
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
        q["symbolFuzzy"] = name;
    }
    let mut row = json_fixture(
        r#"{
            "operation": "references",
            "matches": [{"path": "pkg/src/lib.rs"}]
        }"#,
    );
    let q = query(q);
    let config = configured_rust(&q, options);
    // rust-analyzer roots at the package's Cargo workspace: here the package.
    let package_text = package.to_string_lossy().into_owned();
    super::annotate(
        &mut row,
        &q,
        Some("rust"),
        Server {
            anchor: &lib_path,
            root: &package_text,
        },
        &scope,
        super::super::receipt::RustAnalysis::of(&config).allowing_build_scripts(allowed),
        &|| false,
    );
    row
}

#[test]
fn text_search_lead_targets_repo_scope_with_language_include() {
    let mut row = json_fixture(r#"{"operation": "references", "matches": [{"path": "a.py"}]}"#);
    let q = query(json_fixture(
        r#"{
            "path": "/repo/pkg/a.py",
            "operation": "references",
            "symbolName": "get",
            "line": 3
        }"#,
    ));
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
    let lead = &octocode_runtime_base::test_support::checks::related_call(
        &row,
        "localSearch",
        |_| true,
    )["input"]["queries"][0];
    assert_eq!(lead["path"], "/repo", "{row}");
    assert_eq!(
        lead["include"],
        json_fixture(r#"["*.py", "*.pyi"]"#),
        "{row}"
    );
}

#[test]
fn gate_detection() {
    assert!(has_feature_gate("#[cfg(all(test, feature = \"x\"))]"));
    assert!(has_feature_gate("#![cfg(feature=\"full\")]"));
    assert!(!has_feature_gate("#[cfg(test)]\nlet feature = 1;"));
}

/// A row names no use when it is empty or lists only declarations.
#[test]
fn a_row_of_declarations_names_no_use() {
    assert!(lists_no_use(&json_fixture(r#"{"status": "empty"}"#)));
    assert!(lists_no_use(&json_fixture(
        r#"{"files": [{"matches": [{"role": "declaration"}]}]}"#
    )));
    assert!(!lists_no_use(&json_fixture(
        r#"{
            "files": [{"matches": [{"role": "declaration"}, {"role": "call"}]}]
        }"#
    )));
    // A view without match rows lists its files as uses.
    assert!(!lists_no_use(&json_fixture(
        r#"{"files": [{"path": "a.go"}, {"path": "b.go"}]}"#
    )));
    assert!(!lists_no_use(&json_fixture(r#"{"files": []}"#)));
    // A caller row has no role: it is a use.
    assert!(!lists_no_use(&json_fixture(
        r#"{"files": [{"matches": [{"symbolName": "f"}]}]}"#
    )));
}

/// With nothing above the project to say where it ends, no answer can speak for
/// a module beside it: a row that lists a use (gopls without `go.work` lists
/// `a/a.go` and never loads `b/`) is as partial as an empty one.
#[test]
fn an_unbounded_scope_keeps_every_answer_partial() {
    let q = query(json_fixture(
        r#"{
            "path": "/w/a.go",
            "operation": "callers",
            "symbolName": "F",
            "line": 1
        }"#,
    ));
    let mut scope = Scope::new("/w".into(), vec!["*.go".into()]);
    scope.unbounded = true;
    let run = |scope: &Scope, mut row: Value| {
        annotate(
            &mut row,
            &q,
            Some("go"),
            Server {
                anchor: "/w/a.go",
                root: "/w",
            },
            scope,
        );
        row
    };
    let empty = run(&scope, json_fixture(r#"{"status": "empty"}"#));
    assert_eq!(
        empty["partialReasons"],
        json!([UNANSWERED_REASON]),
        "{empty}"
    );
    let used = run(
        &scope,
        json_fixture(r#"{"files": [{"matches": [{"symbolName": "g"}]}]}"#),
    );
    assert_eq!(used["partialReasons"], json!([UNANSWERED_REASON]), "{used}");
    // A bounded scope leaves a row listing a use alone.
    scope.unbounded = false;
    let bounded = run(
        &scope,
        json_fixture(r#"{"files": [{"matches": [{"symbolName": "g"}]}]}"#),
    );
    assert!(bounded.get("isPartial").is_none(), "{bounded}");
}

/// A stopped walk proves no absence: the row says the projects beside the root
/// were not looked for. One that found none leaves no gap: the answer stands.
#[test]
fn an_unfinished_sibling_walk_claims_no_absence() {
    let q = query(json_fixture(
        r#"{
            "path": "/w/a/a.go",
            "operation": "references",
            "symbolName": "F",
            "line": 1
        }"#,
    ));
    let annotated = |scope: &Scope, mut row: Value| {
        annotate(
            &mut row,
            &q,
            Some("go"),
            Server {
                anchor: "/w/a/a.go",
                root: "/w/a",
            },
            scope,
        );
        row
    };
    let mut unwalked = Scope::new("/w/a".into(), vec!["*.go".into()]);
    unwalked.unbounded = true;
    let unknown =
        annotated(&unwalked, json_fixture(r#"{"status": "empty"}"#))["warnings"].to_string();
    assert!(unknown.contains("not looked for"), "{unknown}");
    let mut walked = Scope::new("/w/a".into(), vec!["*.go".into()]);
    walked.unbounded = true;
    let _ = walked.siblings.set(Vec::new());
    let used = annotated(
        &walked,
        json_fixture(r#"{"files": [{"matches": [{"role": "call"}]}]}"#),
    );
    assert!(
        used.get("isPartial").is_none() && used.get("warnings").is_none(),
        "{used}"
    );
}

/// An unbounded answer names the projects beside the server's root and says to
/// widen `workspaceRoot` to their parent, and its text lead searches them, not
/// the root already searched.
#[test]
fn an_unbounded_answer_names_the_projects_beside_it_and_where_to_widen() {
    let q = query(json_fixture(
        r#"{
            "path": "/w/a/a.go",
            "operation": "references",
            "symbolName": "F",
            "line": 1
        }"#,
    ));
    let mut scope = Scope::new("/w/a".into(), vec!["*.go".into()]);
    scope.unbounded = true;
    let _ = scope.siblings.set(vec!["/w/b".into(), "/w/c".into()]);
    let mut row = json_fixture(r#"{"files": [{"matches": [{"role": "call"}]}]}"#);
    annotate(
        &mut row,
        &q,
        Some("go"),
        Server {
            anchor: "/w/a/a.go",
            root: "/w/a",
        },
        &scope,
    );
    let warnings = row["warnings"].to_string();
    assert!(
        warnings.contains("b, c") && warnings.contains("Widen (not narrow) workspaceRoot to /w"),
        "{row}"
    );
    let lead = &octocode_runtime_base::test_support::checks::related_call(
        &row,
        "localSearch",
        |_| true,
    )["input"]["queries"];
    let searched = lead
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|search| search["path"].as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        searched,
        ["/w/b", "/w/c"],
        "the root is not searched again: {row}"
    );
    let includes = lead
        .as_array()
        .into_iter()
        .flatten()
        .map(|search| search["include"].clone())
        .collect::<Vec<_>>();
    assert!(
        includes
            .iter()
            .all(|include| include == &json_fixture(r#"["*.go"]"#)),
        "a sibling search keeps to the family's files: {row}"
    );
}

/// A C-family row never reads complete while a file spelling the name is
/// unanswered, and an answered scan leaves it alone.
#[test]
fn a_c_family_row_with_an_unanswered_spelling_is_partial() {
    let q = query(json_fixture(
        r#"{
            "path": "/w/a.c",
            "operation": "callers",
            "symbolName": "F",
            "line": 1
        }"#,
    ));
    let scope = Scope::new("/w".into(), vec!["*.c".into()]);
    let mut row = json_fixture(r#"{"files": [{"matches": [{"symbolName": "g"}]}]}"#);
    annotate(
        &mut row,
        &q,
        Some("c"),
        Server {
            anchor: "/w/a.c",
            root: "/w",
        },
        &scope,
    );
    assert!(
        row.get("isPartial").is_none(),
        "no scan, nothing to say: {row}"
    );
}

/// Every later flag on the row keeps the lead to the projects beside the root:
/// a server-health report re-flags the row and must not replace that lead with
/// the root search the unbounded note says already ran.
#[test]
fn a_later_flag_keeps_the_lead_to_the_sibling_projects() {
    use octocode_engine::lsp::client::{HealthLevel, ServerHealth};
    let q = query(json_fixture(
        r#"{
            "path": "/w/a/a.go",
            "operation": "references",
            "symbolName": "F",
            "line": 1
        }"#,
    ));
    let mut scope = Scope::new("/w/a".into(), vec!["*.go".into()]);
    scope.unbounded = true;
    let _ = scope.siblings.set(vec!["/w/b".into()]);
    let mut row = json_fixture(r#"{"files": [{"matches": [{"role": "call"}]}]}"#);
    let server = Server {
        anchor: "/w/a/a.go",
        root: "/w/a",
    };
    annotate(&mut row, &q, Some("go"), server, &scope);
    let health = ServerHealth {
        level: HealthLevel::Warning,
        message: Some("loading".into()),
        project: None,
    };
    flag_server_health(&mut row, &q, Some(health), "/w/a/a.go", &scope);
    let lead = &octocode_runtime_base::test_support::checks::related_call(
        &row,
        "localSearch",
        |_| true,
    )["input"]["queries"];
    let searched = lead
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|search| search["path"].as_str())
        .collect::<Vec<_>>();
    assert_eq!(searched, ["/w/b"], "{row}");
}
