use super::*;
use serde_json::json;

fn annotated_empty(
    query: &LspSearchQuery,
    category: &str,
    message: &str,
    available: bool,
) -> serde_json::Value {
    let config = super::server_context::rust_server_config(
        lsp_search_query::rust_context(query)
            .map_or(serde_json::Value::Null, |context| json!(context)),
    );
    let mut row = empty(query, category, message, available);
    annotate_empty_includes(&mut row, query, RustAnalysis::of(&config).build_scripts);
    row
}

#[test]
fn empty_and_failed_rows_keep_acquisition_separate_from_retry() {
    let q =
        query(json!({"operation":"definition","path":"/repo/a.ts","symbolName":"run","line":1}));
    for code in [
        ErrorCode::ServerUnavailable,
        ErrorCode::Timeout,
        ErrorCode::CapabilityUnavailable,
    ] {
        let row = failure(&q, "file:///repo/a.ts", code, "provider unavailable", false);
        assert_eq!(row["errorCode"], json!(code));
        let related = row["related"].as_array().expect("source acquisition");
        assert!(related.iter().all(|call| call["kind"] == "related"));
        assert_eq!(
            crate::tools::test_support::checks::find_related_call(&row, "lspSearch", |_| true,)
                .is_some(),
            code == ErrorCode::Timeout
        );
        if code == ErrorCode::Timeout {
            let retry =
                crate::tools::test_support::checks::related_call(&row, "lspSearch", |_| true);
            assert_eq!(retry["kind"], "related");
        }
        crate::contracts::validate_output("lspSearch", &public_row(&row))
            .expect("valid public channels");
    }
    let root =
        query(json!({"operation":"workspaceSymbol","workspaceRoot":"/repo","symbolQuery":"run"}));
    let row = with_next(
        &root,
        items_payload(&root, "symbols", json!([]), TEST_BUDGET),
    );
    assert_eq!(row["status"], "empty");
    assert!(row.get("related").is_none());
}

#[test]
fn empty_and_unavailable_rows_expose_status_and_recovery_next() {
    let query = query(serde_json::json!({
        "operation": "definition",
        "path": "/repo/src/lib.rs",
        "symbolName": "execute",
        "line": 10
    }));
    let empty = with_next(&query, empty(&query, "noLocations", "none", true));
    assert_eq!(empty["status"], "empty");
    assert_eq!(empty["related"][0]["kind"], "related");
    // A package that includes no unloaded source: no tip, and no static
    // advice beside the executable `next.read`.
    assert!(empty.get("hints").is_none(), "{empty}");
    // A package that includes build-script output: a name may be generated
    // by it (off by default).
    let package = tempfile::tempdir().expect("package");
    std::fs::create_dir_all(package.path().join("src")).expect("src");
    std::fs::write(
        package.path().join("Cargo.toml"),
        "[package]\nname = \"p\"\n",
    )
    .expect("manifest");
    std::fs::write(package.path().join("build.rs"), "fn main() {}\n").expect("build.rs");
    let unused_script = annotated_empty(
        &requery(
            &query,
            serde_json::json!({"path": package.path().join("src/lib.rs").to_string_lossy()}),
        ),
        "noLocations",
        "none",
        true,
    );
    assert!(
        unused_script.get("hints").is_none(),
        "a build script whose output nothing includes: {unused_script}"
    );
    std::fs::write(
        package.path().join("src/lib.rs"),
        "include!(concat!(env!(\"OUT_DIR\"), \"/gen.rs\"));\n",
    )
    .expect("lib");
    let scripted = requery(
        &query,
        serde_json::json!({"path": package.path().join("src/lib.rs").to_string_lossy()}),
    );
    // The executable rerun carries the call; one tip states its risk
    // (it runs repository code) without restating the call.
    let tipped = annotated_empty(&scripted, "noLocations", "none", true);
    assert_eq!(
        tipped["hints"].as_array().map(Vec::len),
        Some(1),
        "{tipped}"
    );
    assert!(
        tipped["hints"][0]
            .as_str()
            .is_some_and(|hint| hint.contains("repository code")
                && hint.contains("rustContext.buildScripts")),
        "{tipped}"
    );
    assert_eq!(
        tipped["related"][0]["input"]["queries"][0]["rustContext"],
        serde_json::json!({"buildScripts": true}),
        "{tipped}"
    );
    let built = requery(
        &scripted,
        serde_json::json!({"rustContext": {"buildScripts": true}}),
    );
    let built = annotated_empty(&built, "noLocations", "none", true);
    assert!(built.get("hints").is_none(), "{built}");
    let configured =
        super::server_context::rust_server_config(json!({"buildScripts":true,"procMacros":true}));
    let mut inherited = super::super::failure::empty(&scripted, "noLocations", "none", true);
    annotate_empty_includes(
        &mut inherited,
        &scripted,
        RustAnalysis::of(&configured).build_scripts,
    );
    assert!(inherited.get("hints").is_none(), "{inherited}");
    assert!(
        crate::tools::test_support::checks::find_related_call(&inherited, "lspSearch", |_| true)
            .is_none(),
        "{inherited}"
    );
    let mut unavailable = failure(
        &scripted,
        "",
        ErrorCode::ServerUnavailable,
        "provider unavailable",
        false,
    );
    let before = unavailable.clone();
    annotate_empty_includes(&mut unavailable, &scripted, false);
    assert_eq!(
        unavailable, before,
        "unavailable providers cannot report an empty semantic result or a build-script repair"
    );
    // Source included from outside the package never loads: its own tip,
    // and no rerun.
    std::fs::write(
        package.path().join("src/lib.rs"),
        "include!(\"../../elsewhere/types.rs\");\n",
    )
    .expect("lib");
    let outside = annotated_empty(&scripted, "noLocations", "none", true);
    assert!(
        outside["hints"][0]
            .as_str()
            .is_some_and(|hint| hint.contains("outside its directory")),
        "{outside}"
    );
    assert_eq!(
        outside["hints"].as_array().map(Vec::len),
        Some(1),
        "{outside}"
    );
    assert!(
        !outside["related"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|call| call["tool"] == "lspSearch"
                && call["input"]["queries"][0]["rustContext"]["buildScripts"] == true),
        "{outside}"
    );
    // Both: both tips (no rerun carries the build-script one), and no
    // ~2 GB rerun the outside include may defeat.
    std::fs::write(
        package.path().join("src/lib.rs"),
        "include!(concat!(env!(\"OUT_DIR\"), \"/gen.rs\"));\ninclude!(\"../../elsewhere/types.rs\");\n",
    )
    .expect("lib");
    let both = annotated_empty(&scripted, "noLocations", "none", true);
    assert_eq!(both["hints"].as_array().map(Vec::len), Some(2), "{both}");
    assert!(
        both["hints"][1]
            .as_str()
            .is_some_and(|hint| hint.contains("rustContext {buildScripts:true}")
                && hint.contains("repository code")),
        "{both}"
    );
    assert!(
        !both["related"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|call| call["tool"] == "lspSearch"
                && call["input"]["queries"][0]["rustContext"]["buildScripts"] == true),
        "{both}"
    );
}
/// A lead probes the configured server file only when the path policy
/// authorizes reading it.
#[test]
fn lead_discovery_drops_an_unauthorized_server_file() {
    let paths = PathPolicy::new(crate::policy::path::PathPolicyConfig::default()).expect("policy");
    let execution = LspExecutionConfig {
        env: std::collections::BTreeMap::from([("OCTOCODE_LEAD_PROBE".to_owned(), "x".to_owned())]),
        config_path: Some("/nonexistent/octocode-lsp.json".to_owned()),
        ..Default::default()
    };
    let discovery = execution.lead_discovery(&paths);
    assert_eq!(discovery.config_path, None);
    assert_eq!(
        discovery.env.get("OCTOCODE_LEAD_PROBE").map(String::as_str),
        Some("x")
    );
}

/// rust-analyzer indexes no Rust file outside a Cargo package (or
/// `rust-project.json`), whatever other marker (`.git`) sits above it: an
/// empty answer there is `noProject`, partial, not an absence. Every empty
/// server answer about the file says so: locations, hierarchy items, hover.
#[test]
fn a_rust_file_outside_a_cargo_package_answers_no_project_not_absence() {
    let repo = tempfile::tempdir().expect("repo");
    std::fs::create_dir_all(repo.path().join(".git")).expect("git dir");
    std::fs::create_dir_all(repo.path().join("src")).expect("src");
    let file = repo.path().join("src/lib.rs");
    std::fs::write(&file, "pub fn add() {}\npub fn caller() { add() }\n").expect("lib.rs");
    let at = |operation: &str| {
        query(json!({
            "operation": operation, "path": file.to_string_lossy(),
            "symbolName": "add", "line": 1
        }))
    };
    let rows = [
        items_payload(&at("callers"), "callers", json!([]), TEST_BUDGET),
        no_answer(
            &at("references"),
            "noLocations",
            "referencesProvider returned no locations",
        ),
        no_answer(
            &at("hover"),
            "noHover",
            "hoverProvider returned no hover at this position",
        ),
    ];
    for row in &rows {
        assert_eq!(row["status"], "empty", "{row}");
        assert_eq!(row["category"], "noProject", "{row}");
        assert_eq!(row["isPartial"], true, "{row}");
        assert_eq!(row["partialReasons"], json!(["noProject"]), "{row}");
        crate::contracts::validate_output("lspSearch", &public_row(row))
            .expect("the noProject row satisfies the output contract");
    }
    // Inside a Cargo package the same empty answer is the server's absence.
    std::fs::write(repo.path().join("Cargo.toml"), "[package]\nname = \"p\"\n").expect("manifest");
    let row = items_payload(&at("callers"), "callers", json!([]), TEST_BUDGET);
    assert_eq!(row["category"], "noLocations", "{row}");
    assert!(row.get("isPartial").is_none(), "{row}");
}
