//! Unloaded C/C++ includes.
use super::*;
use octocode_test_support::json_fixture;

/// Build-script output the server does not load: the row keeps its reason,
/// offers no rerun (the server's own configuration decides what it loads),
/// and the reason is the whole statement.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn build_scripts_off_offers_no_rerun() {
    let dir = include_fixture();
    let root = dir.path().canonicalize().expect("root");
    let method = "impl R { fn include_diff(&self) -> bool { true } }\n";
    let generated = format!("include!(concat!(env!(\"OUT_DIR\"), \"/gen.rs\"));\n{method}");
    let row = rust_row(&root, &generated, json!({}), &["lib.rs"]).await;
    assert_eq!(
        row["partialReasons"],
        json!([BUILD_SCRIPTS_REASON]),
        "{row}"
    );
    assert!(row.get("notices").is_none(), "{row}");
    octocode_runtime_base::test_support::checks::assert_no_reason_meaning(&row);
    assert!(!offers(&row, "lspSearch"), "{row}");
}

/// Whether `row` offers a `tool` call.
fn offers(row: &Value, tool: &str) -> bool {
    octocode_runtime_base::test_support::checks::find_related_call(row, tool, |_| true).is_some()
}

/// `options` of a server configured to run build scripts.
fn build_scripts_on() -> Value {
    let mut options = octocode_engine::lsp::config::rust_analyzer_headless_options();
    options["cargo"]["buildScripts"]["enable"] = json!(true);
    options
}

/// A method reached through `Deref` to a type the package `include!`s
/// does not resolve while rust-analyzer cannot load that source: the
/// server answers with the declaration alone. Files that use the name in
/// code and are missing from that answer make the row partial; the
/// reason says which load is missing, the text lead reads the missed files,
/// and the static prose is stated once. A comment mention is no missed use.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unloaded_includes_flag_unanswered_files() {
    let dir = include_fixture();
    let root = dir.path().canonicalize().expect("root");
    let method = "impl R { fn include_diff(&self) -> bool { true } }\n";
    let generated = format!("include!(concat!(env!(\"OUT_DIR\"), \"/gen.rs\"));\n{method}");

    let first = rust_row(&root, &generated, json!({}), &["lib.rs"]).await;
    assert_eq!(
        first["partialReasons"],
        json!([BUILD_SCRIPTS_REASON]),
        "{first}"
    );
    // The code is the whole statement: its meaning is core's; the text lead
    // carries the call.
    assert!(first.get("notices").is_none(), "{first}");
    assert!(first.get("warnings").is_none(), "{first}");
    octocode_runtime_base::test_support::checks::assert_no_reason_meaning(&first);
    assert_eq!(
        first["coverage"]["textOnlyFiles"], 1,
        "comments and strings are no uses: {first}"
    );
    assert!(offers(&first, "localSearch"), "{first}");
    assert!(!offers(&first, "lspSearch"), "{first}");

    let later_fields = json_fixture(r#"{"page": 2}"#);
    let later = rust_row(&root, &generated, later_fields.clone(), &["lib.rs"]).await;
    assert_eq!(
        later["partialReasons"],
        json!([BUILD_SCRIPTS_REASON]),
        "{later}"
    );
    let shaped = octocode_runtime_base::response::rows::result_row(0, later.clone(), None);
    assert_eq!(
        shaped["data"], later,
        "row assembly preserves producer coverage guidance"
    );
    assert!(!offers(&later, "lspSearch"), "{later}");

    let complete = rust_row(&root, &generated, json!({}), &["lib.rs", "user.rs"]).await;
    assert!(complete.get("isPartial").is_none(), "{complete}");
    assert!(
        complete.get("next").is_none(),
        "no rerun lead on a complete row: {complete}"
    );
    assert_eq!(complete["coverage"]["textOnlyFiles"], 0, "{complete}");

    let ran = rust_row_configured(
        &root,
        &generated,
        json!({}),
        &["lib.rs"],
        build_scripts_on(),
    )
    .await;
    assert!(ran.get("isPartial").is_none(), "{ran}");
    // The server's own project speaks for itself: `user.rs` is settled, so a
    // complete row counts no file it does not lead to.
    assert_eq!(ran["coverage"]["textOnlyFiles"], 0, "{ran}");
}

/// The server's configured settings decide the coverage: one that runs build
/// scripts under every feature loads the generated source and the gated use
/// (complete); the headless defaults load neither (both reasons), and no
/// row offers a rerun.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rust_coverage_follows_the_configured_settings() {
    let dir = include_fixture();
    let root = dir.path().canonicalize().expect("root");
    std::fs::write(
        root.join("pkg/src/user.rs"),
        "#[cfg(feature = \"extra\")]\nfn u(q: &R) { q.include_diff(); }\n",
    )
    .expect("gated use");
    let lib = "include!(concat!(env!(\"OUT_DIR\"), \"/gen.rs\"));\nimpl R { fn include_diff(&self) {} }\n";
    let mut options = build_scripts_on();
    options["cargo"]["features"] = json!("all");
    options["procMacro"]["enable"] = json!(true);
    let inherited = rust_row_configured(&root, lib, json!({}), &["lib.rs"], options).await;
    assert!(inherited.get("partialReasons").is_none(), "{inherited}");
    assert_eq!(inherited["coverage"]["textOnlyFiles"], 0, "{inherited}");
    assert!(!offers(&inherited, "lspSearch"), "{inherited}");

    let headless = rust_row(&root, lib, json!({}), &["lib.rs"]).await;
    assert_eq!(
        headless["partialReasons"],
        json!([BUILD_SCRIPTS_REASON, "cfgGatedFiles"]),
        "{headless}"
    );
    assert!(!offers(&headless, "lspSearch"), "{headless}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn outside_includes_keep_partial_coverage_without_build_script_reruns() {
    let dir = include_fixture();
    let root = dir.path().canonicalize().expect("root");
    let method = "impl R { fn include_diff(&self) -> bool { true } }\n";
    let outside = format!("include!(\"../../contract/types.rs\");\n{method}");

    // No build context loads a file outside the package: a server that runs
    // build scripts stays partial, with the lexical lead and no rerun.
    let outside_row =
        rust_row_configured(&root, &outside, json!({}), &["lib.rs"], build_scripts_on()).await;
    assert_eq!(
        outside_row["partialReasons"],
        json!([OUTSIDE_INCLUDE_REASON]),
        "{outside_row}"
    );
    assert!(
        outside_row["notices"][0]
            .as_str()
            .is_some_and(|notice| notice.contains("contract/types.rs")),
        "{outside_row}"
    );
    assert!(offers(&outside_row, "localSearch"), "{outside_row}");
    assert!(!offers(&outside_row, "lspSearch"), "{outside_row}");

    // Both loads missing: which one hides a file is unknown; the text check
    // leads.
    let both = format!(
        "include!(concat!(env!(\"OUT_DIR\"), \"/gen.rs\"));\ninclude!(\"../../contract/types.rs\");\n{method}"
    );
    let both_row = rust_row(&root, &both, json!({}), &["lib.rs"]).await;
    assert_eq!(
        both_row["partialReasons"],
        json!([OUTSIDE_INCLUDE_REASON, BUILD_SCRIPTS_REASON]),
        "{both_row}"
    );
    assert!(offers(&both_row, "localSearch"), "{both_row}");
    assert!(!offers(&both_row, "lspSearch"), "{both_row}");
    // Both reasons are the whole statement; only the outside include's
    // files are this answer's own note.
    assert_eq!(
        both_row["notices"].as_array().map(Vec::len),
        Some(1),
        "{both_row}"
    );
    octocode_runtime_base::test_support::checks::assert_no_reason_meaning(&both_row);

    // A workspace symbol is a found declaration: no use is missing.
    let symbols = rust_row(
        &root,
        &outside,
        json_fixture(r#"{"operation": "workspaceSymbol"}"#),
        &["lib.rs"],
    )
    .await;
    assert!(symbols.get("isPartial").is_none(), "{symbols}");

    // A package whose includes all load: checked, complete, and says so; its
    // own files the server left out are settled, not counted.
    let plain = rust_row(&root, method, json!({}), &["lib.rs"]).await;
    assert!(plain.get("isPartial").is_none(), "{plain}");
    assert_eq!(plain["coverage"]["textOnlyFiles"], 0, "{plain}");
}

fn include_fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("dir");
    let root = dir.path().canonicalize().expect("root");
    let package = root.join("pkg");
    std::fs::create_dir_all(package.join("src")).expect("src");
    std::fs::create_dir_all(root.join("contract")).expect("contract");
    std::fs::write(package.join("Cargo.toml"), "[package]\nname = \"p\"\n").expect("manifest");
    std::fs::write(package.join("build.rs"), "fn main() {}\n").expect("build.rs");
    std::fs::write(
        package.join("src/user.rs"),
        "fn u(q: &R) { q.include_diff(); }\n",
    )
    .expect("user");
    std::fs::write(
        package.join("src/notes.rs"),
        "/// include_diff is documented here.\nfn n() { let _ = \"include_diff\"; }\n",
    )
    .expect("notes");
    dir
}
