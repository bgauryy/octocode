use super::*;
use octocode_test_support::json_fixture;
use serde_json::json;

/// `annotate_empty_includes` on an empty row, under the headless defaults
/// (no build scripts).
fn annotated_empty(
    query: &LspSearchQuery,
    category: &str,
    message: &str,
    available: bool,
) -> serde_json::Value {
    let config = super::server_context::rust_server_config(json!({}));
    let mut row = empty(query, category, message, available);
    annotate_empty_includes(&mut row, query, RustAnalysis::of(&config), None, &|| false);
    row
}

/// Whether `row` offers any `lspSearch` call.
fn offers_lsp_search(row: &serde_json::Value) -> bool {
    octocode_runtime_base::test_support::checks::find_related_call(row, "lspSearch", |_| true)
        .is_some()
}

#[test]
fn empty_and_failed_rows_keep_acquisition_separate_from_retry() {
    let q = query(json_fixture(
        r#"{
            "operation": "definition",
            "path": "/repo/a.ts",
            "symbolName": "run",
            "line": 1
        }"#,
    ));
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
            octocode_runtime_base::test_support::checks::find_related_call(
                &row,
                "lspSearch",
                |_| true,
            )
            .is_some(),
            code == ErrorCode::Timeout
        );
        if code == ErrorCode::Timeout {
            let retry = octocode_runtime_base::test_support::checks::related_call(
                &row,
                "lspSearch",
                |_| true,
            );
            assert_eq!(retry["kind"], "related");
        }
        octocode_runtime_base::contracts::validate_output("lspSearch", &public_row(&row))
            .expect("valid public channels");
    }
    let root = query(json_fixture(
        r#"{
            "operation": "workspaceSymbol",
            "workspaceRoot": "/repo",
            "symbolFuzzy": "run"
        }"#,
    ));
    let row = with_next(
        &root,
        items_payload(&root, "symbols", json!([]), TEST_BUDGET),
    );
    assert_eq!(row["status"], "empty");
    assert!(row.get("related").is_none());
}

#[test]
fn empty_and_unavailable_rows_expose_status_and_recovery_next() {
    let query = query(json_fixture(
        r#"{
            "operation": "definition",
            "path": "/repo/src/lib.rs",
            "symbolName": "execute",
            "line": 10
        }"#,
    ));
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
    // A name the package's own source declares is no generated name: the
    // package-level build script is no evidence for this anchor.
    std::fs::write(
        package.path().join("src/declared.rs"),
        "pub fn execute() {}\n",
    )
    .expect("declared");
    let declared = annotated_empty(&scripted, "noLocations", "none", true);
    assert!(
        declared.get("hints").is_none() && declared.get("related").is_none(),
        "{declared}"
    );
    std::fs::remove_file(package.path().join("src/declared.rs")).expect("remove");
    // The cause is the row's partial reason, as on every operation, and the
    // whole statement (its meaning is core's): no hint, and no rerun.
    let tipped = annotated_empty(&scripted, "noLocations", "none", true);
    assert!(tipped.get("hints").is_none(), "{tipped}");
    octocode_runtime_base::test_support::checks::assert_no_reason_meaning(&tipped);
    assert!(!offers_lsp_search(&tipped), "{tipped}");
    assert_eq!(tipped["isPartial"], true, "{tipped}");
    assert_eq!(
        tipped["partialReasons"],
        json_fixture(r#"["buildScriptsOff"]"#),
        "{tipped}"
    );
    // A server configured to run build scripts loads their output: no
    // reason, no hint, no rerun.
    let configured = super::server_context::rust_server_config(json_fixture(
        r#"{"cargo": {"buildScripts": {"enable": true}}, "procMacro": {"enable": true}}"#,
    ));
    let mut inherited = super::super::failure::empty(&scripted, "noLocations", "none", true);
    annotate_empty_includes(
        &mut inherited,
        &scripted,
        RustAnalysis::of(&configured),
        None,
        &|| false,
    );
    assert!(inherited.get("hints").is_none(), "{inherited}");
    assert!(inherited.get("partialReasons").is_none(), "{inherited}");
    assert!(!offers_lsp_search(&inherited), "{inherited}");
    let mut unavailable = failure(
        &scripted,
        "",
        ErrorCode::ServerUnavailable,
        "provider unavailable",
        false,
    );
    let before = unavailable.clone();
    annotate_empty_includes(
        &mut unavailable,
        &scripted,
        RustAnalysis::of(&super::server_context::rust_server_config(json!({}))),
        None,
        &|| false,
    );
    assert_eq!(
        unavailable, before,
        "unavailable providers cannot report an empty semantic result or a build-script repair"
    );
}

/// Source a package `include!`s from outside its directory never loads:
/// its own partial reason, and no rerun.
#[test]
fn an_outside_include_is_partial_without_a_build_script_rerun() {
    let package = tempfile::tempdir().expect("package");
    std::fs::create_dir_all(package.path().join("src")).expect("src");
    std::fs::write(
        package.path().join("Cargo.toml"),
        "[package]\nname = \"p\"\n",
    )
    .expect("manifest");
    std::fs::write(package.path().join("build.rs"), "fn main() {}\n").expect("build.rs");
    let scripted = query(json!({
        "operation": "definition",
        "path": package.path().join("src/lib.rs").to_string_lossy(),
        "symbolName": "execute",
        "line": 10
    }));
    std::fs::write(
        package.path().join("src/lib.rs"),
        "include!(\"../../elsewhere/types.rs\");\n",
    )
    .expect("lib");
    let outside = annotated_empty(&scripted, "noLocations", "none", true);
    assert!(outside.get("hints").is_none(), "{outside}");
    octocode_runtime_base::test_support::checks::assert_no_reason_meaning(&outside);
    assert_eq!(
        outside["partialReasons"],
        json_fixture(r#"["includeOutsidePackage"]"#),
        "{outside}"
    );
    assert!(!offers_lsp_search(&outside), "{outside}");
    // Both: both reasons, and no rerun.
    std::fs::write(
        package.path().join("src/lib.rs"),
        "include!(concat!(env!(\"OUT_DIR\"), \"/gen.rs\"));\ninclude!(\"../../elsewhere/types.rs\");\n",
    )
    .expect("lib");
    let both = annotated_empty(&scripted, "noLocations", "none", true);
    assert!(both.get("hints").is_none(), "{both}");
    assert_eq!(
        both["partialReasons"],
        json_fixture(r#"["includeOutsidePackage", "buildScriptsOff"]"#),
        "{both}"
    );
    octocode_runtime_base::test_support::checks::assert_no_reason_meaning(&both);
    assert!(!offers_lsp_search(&both), "{both}");
}
/// A name a dependency crate `include!`s from outside its directory (D2:
/// `SkippedPaths`, from `contract-types`) has no location for that reason,
/// not for the anchor crate's build scripts: the outside-include tip with
/// the include target as the lead.
#[test]
fn a_name_from_a_dependency_s_outside_include_is_attributed_to_that_include() {
    let root = tempfile::tempdir().expect("workspace");
    let root = root.path().canonicalize().expect("canonical");
    let write = |path: &str, text: &str| {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dir");
        std::fs::write(path, text).expect("write");
    };
    write(
        "app/Cargo.toml",
        "[package]\nname = \"app\"\n\n[dependencies]\ntypes = { path = \"../types\" }\n",
    );
    write("app/build.rs", "fn main() {}\n");
    write(
        "app/src/lib.rs",
        "use types::SkippedPaths;\ninclude!(concat!(env!(\"OUT_DIR\"), \"/gen.rs\"));\n",
    );
    write("types/Cargo.toml", "[package]\nname = \"types\"\n");
    write("types/src/lib.rs", "include!(\"../../shared/types.rs\");\n");
    write("shared/types.rs", "pub struct SkippedPaths;\n");
    let q = query(json!({
        "operation": "definition",
        "path": root.join("app/src/lib.rs").to_string_lossy(),
        "symbolName": "SkippedPaths",
        "line": 1
    }));
    let row = annotated_empty(&q, "noLocations", "none", true);
    assert_eq!(
        row["partialReasons"],
        json_fixture(r#"["includeOutsidePackage"]"#),
        "{row}"
    );
    let hints = row["hints"].as_array().expect("hints");
    assert_eq!(hints.len(), 1, "{row}");
    assert!(
        hints[0].as_str().is_some_and(
            |hint| hint.contains("crate types include!s") && !hint.contains("build script")
        ),
        "{row}"
    );
    let lead =
        octocode_runtime_base::test_support::checks::related_call(&row, "localSearch", |_| true);
    assert_eq!(
        lead["input"]["queries"][0]["path"],
        root.join("shared/types.rs").to_string_lossy().as_ref(),
        "{row}"
    );
    assert!(
        octocode_runtime_base::test_support::checks::find_related_call(&row, "lspSearch", |_| true)
            .is_none(),
        "{row}"
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
        assert_eq!(
            row["partialReasons"],
            json_fixture(r#"["noProject"]"#),
            "{row}"
        );
        octocode_runtime_base::contracts::validate_output("lspSearch", &public_row(row))
            .expect("the noProject row satisfies the output contract");
    }
    // Inside a Cargo package the same empty answer is the server's absence.
    std::fs::write(repo.path().join("Cargo.toml"), "[package]\nname = \"p\"\n").expect("manifest");
    let row = items_payload(&at("callers"), "callers", json!([]), TEST_BUDGET);
    assert_eq!(row["category"], "noLocations", "{row}");
    assert!(row.get("isPartial").is_none(), "{row}");
}
