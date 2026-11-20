//! Rust rows: feature-gated mentions and negated gates.
use super::*;
use octocode_test_support::json_fixture;

/// The headless defaults with every feature on (`cargo.features: "all"`).
fn all_features() -> Value {
    let mut options = octocode_engine::lsp::config::rust_analyzer_headless_options();
    options["cargo"]["features"] = json!("all");
    options
}

/// Whether `row` offers an `lspSearch` call: no rerun changes the server's
/// feature set.
fn offers_rerun(row: &Value) -> bool {
    octocode_runtime_base::test_support::checks::find_related_call(row, "lspSearch", |_| true)
        .is_some()
}

/// Feature-gated mentions the answer missed: one reason and their count,
/// no prose and no rerun, on every page of the walk; a server configured
/// for every feature is not flagged.
#[test]
fn rust_rows_flag_feature_gated_mentions() {
    let dir = tempfile::tempdir().expect("dir");
    std::fs::write(dir.path().join("lib.rs"), "pub fn spawn_blocking() {}\n").expect("lib");
    std::fs::write(
        dir.path().join("gated.rs"),
        "#![cfg(feature = \"full\")]\nfn t() { spawn_blocking(); }\n",
    )
    .expect("gated");
    std::fs::write(
        dir.path().join("other.rs"),
        "#[cfg(feature = \"x\")]\nfn spawn_blocking_extra() {}\n",
    )
    .expect("other");
    let root = dir.path().canonicalize().expect("root");
    let root = root.to_string_lossy().into_owned();
    let scope = scanned(
        Scope::new(root.clone(), vec!["*.rs".into()]),
        "spawn_blocking",
    );
    let row_query = json!({"path":format!("{root}/lib.rs"),"operation":"references","symbolName":"spawn_blocking","line":1});
    let mut row = json_fixture(r#"{"operation": "references", "matches": [{"path": "lib.rs"}]}"#);
    annotate(
        &mut row,
        &query(row_query.clone()),
        Some("rust"),
        Server {
            anchor: &format!("{root}/lib.rs"),
            root: &root,
        },
        &scope,
    );
    // Gated files found: one reason and their count, no prose.
    assert_eq!(row["partialReasons"], json!([CFG_REASON]), "{row}");
    assert_eq!(row["coverage"]["cfgGatedFiles"], 1, "{row}");
    assert!(
        row.get("coverage")
            .is_none_or(|c| c.get("reason").is_none()),
        "{row}"
    );
    assert!(row.get("warnings").is_none(), "{row}");
    assert!(!offers_rerun(&row), "{row}");
    // A later page of the same walk keeps the reason.
    for later in [
        json_fixture(r#"{"page": 2, "snapshot": "lsp-v3:abc"}"#),
        json_fixture(r#"{"importerPage": 2, "snapshot": "lsp-v3:abc"}"#),
    ] {
        let mut later_query = row_query.clone();
        later_query
            .as_object_mut()
            .expect("query")
            .extend(later.as_object().expect("fields").clone());
        let mut page =
            json_fixture(r#"{"operation": "references", "matches": [{"path": "lib.rs"}]}"#);
        annotate(
            &mut page,
            &query(later_query),
            Some("rust"),
            Server {
                anchor: &format!("{root}/lib.rs"),
                root: &root,
            },
            &scope,
        );
        assert_eq!(page["partialReasons"], json!([CFG_REASON]), "{page}");
        assert!(!offers_rerun(&page), "{page}");
    }
    // A server configured for every feature is not flagged.
    let mut all = json_fixture(r#"{"operation": "references", "matches": []}"#);
    annotate_with(
        &mut all,
        &query(row_query),
        Some("rust"),
        Server {
            anchor: &format!("{root}/lib.rs"),
            root: &root,
        },
        &scope,
        all_features(),
    );
    assert!(all.get("isPartial").is_none(), "{all}");
}

#[test]
fn all_features_run_flags_negated_feature_gates() {
    let dir = tempfile::tempdir().expect("dir");
    std::fs::write(dir.path().join("lib.rs"), "pub fn spawn_blocking() {}\n").expect("lib");
    std::fs::write(
        dir.path().join("fallback.rs"),
        "#[cfg(not(feature = \"rt\"))]\nfn t() { spawn_blocking(); }\n",
    )
    .expect("negated");
    let root = dir.path().canonicalize().expect("root");
    let root = root.to_string_lossy().into_owned();
    let scope = scanned(
        Scope::new(root.clone(), vec!["*.rs".into()]),
        "spawn_blocking",
    );
    let mut row = json_fixture(r#"{"operation": "references", "matches": []}"#);
    let q = query(
        json!({"path":format!("{root}/lib.rs"),"operation":"references",
        "symbolName":"spawn_blocking","line":1}),
    );
    annotate_with(
        &mut row,
        &q,
        Some("rust"),
        Server {
            anchor: &format!("{root}/lib.rs"),
            root: &root,
        },
        &scope,
        all_features(),
    );
    assert_eq!(row["partialReasons"], json!([CFG_NEGATED_REASON]), "{row}");
    assert_eq!(
        octocode_runtime_base::test_support::checks::related_call(&row, "localSearch", |_| true)["tool"],
        "localSearch",
        "{row}"
    );
    assert!(!offers_rerun(&row), "{row}");
    assert!(has_negated_feature_gate(
        "#[cfg(all(unix, not( feature = \"x\")))]"
    ));
    assert!(!has_negated_feature_gate("#[cfg(feature = \"x\")]"));
}

/// An incomplete gate scan's caveat reads the same on every row of a
/// response: it is said once, through the notices channel.
#[test]
fn an_incomplete_gate_scan_is_noted_once_per_response() {
    let dir = tempfile::tempdir().expect("dir");
    std::fs::write(dir.path().join("lib.rs"), "pub fn once_noted() {}\n").expect("lib");
    let root = dir.path().to_string_lossy().into_owned();
    // No text scan ran (it failed): the check is incomplete.
    let scope = Scope::new(root.clone(), vec!["*.rs".into()]);
    let row = || {
        let mut row =
            json_fixture(r#"{"operation": "references", "matches": [{"path": "lib.rs"}]}"#);
        annotate(
            &mut row,
            &query(
                json!({"path":format!("{root}/lib.rs"),"operation":"references","symbolName":"once_noted","line":1}),
            ),
            Some("rust"),
            Server {
                anchor: &format!("{root}/lib.rs"),
                root: &root,
            },
            &scope,
        );
        json!({"index":0,"data":row})
    };
    let mut response = json!({"results":[row(), row()]});
    octocode_runtime_base::response::notices::place(&mut response, "results");
    let said = response
        .to_string()
        .matches("The gate scan was incomplete")
        .count();
    assert_eq!(said, 1, "{response}");
    for row in response["results"].as_array().expect("rows") {
        assert_eq!(row["data"]["partialReasons"], json!([CFG_REASON]), "{row}");
    }
}

/// A file of the server's own Cargo project that spells the name in code and
/// is out of the answer holds no use: the server indexed its project, and a
/// `.rs` file no module declares is not compiled. A complete row then counts
/// no such file: `coverage.textOnlyFiles` counts only files it flags and leads
/// to, never a number with no way to the files.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_own_project_file_left_out_is_not_counted_on_a_complete_row() {
    let dir = tempfile::tempdir().expect("dir");
    let root = dir.path().canonicalize().expect("root");
    let package = root.join("pkg");
    std::fs::create_dir_all(package.join("src")).expect("src");
    // Outside git, generated-output dirs (`build/`, `dist/`) are pruned from
    // the scan; an orphan anywhere else is read.
    std::fs::create_dir_all(package.join("scratch")).expect("scratch");
    std::fs::write(package.join("Cargo.toml"), "[package]\nname = \"p\"\n").expect("manifest");
    std::fs::write(
        package.join("scratch/copy.rs"),
        "fn copy() { crate::include_diff(); }\n",
    )
    .expect("orphan");
    let row = rust_row(&root, "pub fn include_diff() {}\n", json!({}), &["lib.rs"]).await;
    assert!(row.get("isPartial").is_none(), "{row}");
    assert_eq!(
        row["coverage"]["textOnlyFiles"], 0,
        "a counted file must be flagged and led to: {row}"
    );
}

/// The row of `references` on `pub fn shared` in package `pkg` (the server's
/// root) of a repository that gitignores `scratch/`, after `prepare` laid out
/// files there. The server answered only the declaration's file; the check
/// runs as a request runs it: dependents, text scan, code uses.
async fn ignored_row(prepare: impl FnOnce(&Path)) -> (tempfile::TempDir, Value) {
    let dir = tempfile::tempdir().expect("dir");
    let root = dir.path().canonicalize().expect("root");
    let write = |path: &Path, text: &str| {
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
        std::fs::write(path, text).expect("write");
    };
    std::fs::create_dir_all(root.join(".git")).expect("repository marker");
    write(&root.join(".gitignore"), "scratch/\n");
    let package = root.join("pkg");
    write(
        &package.join("Cargo.toml"),
        "[package]\nname = \"pkg\"\nversion = \"0.1.0\"\n",
    );
    let lib = package.join("src/lib.rs");
    write(&lib, "pub fn shared() {}\n");
    prepare(&root);
    let (lib_text, package_text) = (
        lib.to_string_lossy().into_owned(),
        package.to_string_lossy().into_owned(),
    );
    let policy = octocode_runtime_base::test_support::workspace_policy(&root);
    let cancel = octocode_runtime_base::execution::cancel::NeverCancel;
    let scope = Scope::new(root.to_string_lossy().into_owned(), vec!["*.rs".into()]);
    scope.anchor_site(&lib_text, 0, 7);
    scope.asks(crate::lsp_search::scope::Question::Uses);
    scope
        .dependents(&package_text, &lib_text, "rs", &cancel)
        .await
        .expect("dependents");
    scope
        .text_files("shared", &policy, &cancel)
        .await
        .expect("scan");
    scope.answer([lib_text.as_str()]);
    scope
        .check_code_uses("shared", &cancel)
        .await
        .expect("code uses");
    let q = query(json!({"path": lib_text, "operation": "references",
        "symbolName": "shared", "line": 1}));
    let mut row = json_fixture(
        r#"{
            "operation": "references",
            "matches": [{"path": "pkg/src/lib.rs"}]
        }"#,
    );
    annotate(
        &mut row,
        &q,
        Some("rust"),
        Server {
            anchor: &lib_text,
            root: &package_text,
        },
        &scope,
    );
    (dir, row)
}

/// Gitignored files are out of the scan, as ripgrep and editor search leave
/// them out: a gitignored crate whose path dependency is this `pkg` and that
/// calls it is neither counted nor named, and the row stays whole.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn gitignored_files_are_outside_the_scan() {
    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
        std::fs::write(path, text).expect("write");
    }
    let (_dir, row) = ignored_row(|root| {
        write(&root.join("scratch/loose.rs"), "fn l() {\n    shared();\n}\n");
        write(
            &root.join("scratch/real/Cargo.toml"),
            "[package]\nname = \"real\"\nversion = \"0.1.0\"\n\n[dependencies]\npkg = { path = \"../../pkg\" }\n",
        );
        write(
            &root.join("scratch/real/src/lib.rs"),
            "fn r() {\n    pkg::shared();\n}\n",
        );
    })
    .await;
    assert!(row.get("isPartial").is_none(), "{row}");
    assert!(row["coverage"].get("ignoredFiles").is_none(), "{row}");
    assert!(!row.to_string().contains("scratch"), "{row}");
}

/// A module whose `mod` declaration carries a gate the analysis leaves out
/// (a feature of a default build, a cfg option no build setting sets) is
/// compiled out whole, though the module file carries no gate itself: its
/// uses are missing, so the row is partial and counts every such module.
/// A gate the analysis sets (`cfg(test)`) leaves nothing out.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_module_gated_at_its_declaration_is_counted() {
    let dir = tempfile::tempdir().expect("dir");
    let root = dir.path().canonicalize().expect("root");
    let source = root.join("pkg/src");
    std::fs::create_dir_all(&source).expect("src");
    std::fs::write(
        root.join("pkg/Cargo.toml"),
        "[package]\nname = \"p\"\n\n[features]\nbright = []\n",
    )
    .expect("manifest");
    let gates = [
        ("bright", Some("feature = \"bright\"")),
        ("custom", Some("transport_cfg")),
        (
            "nested",
            Some("all(feature = \"bright\", not(feature = \"dim\"))"),
        ),
        ("tested", Some("test")),
        ("always", None),
    ];
    let mut lib = String::from("pub fn gated_target() {}\n");
    for (module, gate) in gates {
        if let Some(gate) = gate {
            lib.push_str(&format!("#[cfg({gate})]\n"));
        }
        lib.push_str(&format!("mod {module};\n"));
        std::fs::write(
            source.join(format!("{module}.rs")),
            "pub fn call() { super::gated_target(); }\n",
        )
        .expect("module");
    }
    std::fs::write(source.join("lib.rs"), &lib).expect("lib");
    let policy = octocode_runtime_base::test_support::workspace_policy(&root);
    let scope = Scope::new(root.to_string_lossy().into_owned(), vec!["*.rs".into()]);
    scope
        .text_files(
            "gated_target",
            &policy,
            &octocode_runtime_base::execution::cancel::NeverCancel,
        )
        .await
        .expect("scan");
    // The server compiled the crate root and the modules the analysis keeps.
    let compiled = ["lib", "tested", "always"];
    scope.answer(compiled.map(|module| {
        source
            .join(format!("{module}.rs"))
            .to_string_lossy()
            .into_owned()
    }));
    let lib_path = source.join("lib.rs").to_string_lossy().into_owned();
    let q = query(json!({"path": lib_path, "operation": "references",
        "symbolName": "gated_target", "line": 1}));
    let mut row =
        json_fixture(r#"{"operation": "references", "matches": [{"path": "pkg/src/lib.rs"}]}"#);
    let package = root.join("pkg").to_string_lossy().into_owned();
    let config = configured_rust(octocode_engine::lsp::config::rust_analyzer_headless_options());
    super::super::annotate(
        &mut row,
        &q,
        Some("rust"),
        Server {
            anchor: &lib_path,
            root: &package,
        },
        &scope,
        crate::lsp_search::receipt::RustAnalysis::of(&config),
        &|| false,
    );
    let left_out = gates
        .iter()
        .filter(|(module, _)| !compiled.contains(module))
        .count();
    assert_eq!(row["partialReasons"], json!([CFG_REASON]), "{row}");
    assert_eq!(row["coverage"]["cfgGatedFiles"], left_out, "{row}");
}

/// The gate scan counts what the request's question counts: a gated file
/// that only takes `&dyn Check` holds no implementation of `Check`, so an
/// implementations row neither counts nor blames it; a gated `impl` still
/// counts.
#[test]
fn the_gate_scan_counts_only_files_that_may_answer_the_question() {
    let dir = tempfile::tempdir().expect("dir");
    let root = dir.path().canonicalize().expect("root");
    let write = |name: &str, text: &str| {
        let path = root.join(name);
        std::fs::write(&path, text).expect("source");
        path.to_string_lossy().into_owned()
    };
    let declared = write("check.rs", "pub trait Check {}\n");
    write(
        "held.rs",
        "#[cfg(feature = \"x\")]\nfn f(c: &dyn Check) {}\n",
    );
    let implemented = write("never.rs", "#[cfg(feature = \"x\")]\nimpl Check for S {}\n");
    let scope = scanned(
        Scope::new(root.to_string_lossy().into_owned(), vec!["*.rs".into()]),
        "Check",
    );
    scope.asks(crate::lsp_search::scope::Question::Heritage);
    scope.answer([declared]);
    let found = gated_files_mentioning(
        &scope,
        &scope.root,
        "Check",
        (Gate::Off, &super::headless_configuration()),
        &|| false,
    );
    assert_eq!(found.scan.files, vec![implemented]);
}
