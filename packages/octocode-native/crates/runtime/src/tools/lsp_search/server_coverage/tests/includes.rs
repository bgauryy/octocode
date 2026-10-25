//! Unloaded C/C++ includes.
use super::*;

/// A method reached through `Deref` to a type the package `include!`s
/// does not resolve while rust-analyzer cannot load that source: the
/// server answers with the declaration alone. Files that use the name in
/// code and are missing from that answer make the row partial; the
/// reason says which load is missing, the build-script rerun is offered
/// only where it helps and only on a partial first page, and the static
/// prose is stated once. A comment mention is no missed use.
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
    // The prose is the one statement of the fact and the rerun's cost
    // and risk; the lead carries the call, so the prose does not repeat it.
    let notice = first["notices"][0].as_str().unwrap_or_default();
    assert!(
        notice.contains("rustContext.buildScripts") && notice.contains("repository code"),
        "{first}"
    );
    assert_eq!(
        first["notices"].as_array().map(Vec::len),
        Some(1),
        "{first}"
    );
    assert!(first.get("warnings").is_none(), "{first}");
    assert_eq!(
        first["coverage"]["textOnlyFiles"], 1,
        "comments and strings are no uses: {first}"
    );
    let rerun = &crate::tools::test_support::checks::related_call(&first, "lspSearch", |input| {
        input["queries"][0]["rustContext"]["buildScripts"] == true
    })["input"]["queries"][0];
    assert_eq!(
        rerun["rustContext"],
        json!({"buildScripts": true}),
        "{first}"
    );
    assert_eq!(rerun["symbolName"], "include_diff", "{first}");

    let later_fields = json!({"page": 2, });
    let later = rust_row(&root, &generated, later_fields.clone(), &["lib.rs"]).await;
    assert_eq!(
        later["partialReasons"],
        json!([BUILD_SCRIPTS_REASON]),
        "{later}"
    );
    let shaped = crate::response::rows::result_row(0, later.clone(), None);
    assert_eq!(
        shaped["data"], later,
        "row assembly preserves producer coverage guidance"
    );
    assert!(
        !later["related"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|call| call["tool"] == "lspSearch"
                && call["input"]["queries"][0]["rustContext"]["buildScripts"] == true),
        "{later}"
    );

    let complete = rust_row(&root, &generated, json!({}), &["lib.rs", "user.rs"]).await;
    assert!(complete.get("isPartial").is_none(), "{complete}");
    assert!(
        complete.get("next").is_none(),
        "no rerun lead on a complete row: {complete}"
    );
    assert_eq!(complete["coverage"]["textOnlyFiles"], 0, "{complete}");

    let ran = rust_row(
        &root,
        &generated,
        json!({"rustContext": {"buildScripts": true}}),
        &["lib.rs"],
    )
    .await;
    assert!(ran.get("isPartial").is_none(), "{ran}");
    assert_eq!(ran["coverage"]["textOnlyFiles"], 1, "{ran}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn outside_includes_keep_partial_coverage_without_build_script_reruns() {
    let dir = include_fixture();
    let root = dir.path().canonicalize().expect("root");
    let method = "impl R { fn include_diff(&self) -> bool { true } }\n";
    let outside = format!("include!(\"../../contract/types.rs\");\n{method}");

    // No build context loads a file outside the package: the rerun
    // stays partial, with the lexical lead and no rerun of its own.
    let outside_row = rust_row(
        &root,
        &outside,
        json!({"rustContext": {"buildScripts": true}}),
        &["lib.rs"],
    )
    .await;
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
    assert!(
        outside_row["related"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|call| call["tool"] == "localSearch"),
        "{outside_row}"
    );
    assert!(
        !outside_row["related"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|call| call["tool"] == "lspSearch"
                && call["input"]["queries"][0]["rustContext"]["buildScripts"] == true),
        "{outside_row}"
    );

    // Both loads missing: which one hides a file is unknown, so the
    // text check leads and the ~2 GB rerun is named, not offered.
    let both = format!(
        "include!(concat!(env!(\"OUT_DIR\"), \"/gen.rs\"));\ninclude!(\"../../contract/types.rs\");\n{method}"
    );
    let both_row = rust_row(&root, &both, json!({}), &["lib.rs"]).await;
    assert_eq!(
        both_row["partialReasons"],
        json!([OUTSIDE_INCLUDE_REASON, BUILD_SCRIPTS_REASON]),
        "{both_row}"
    );
    assert!(
        both_row["related"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|call| call["tool"] == "localSearch"),
        "{both_row}"
    );
    assert!(
        !both_row["related"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|call| call["tool"] == "lspSearch"
                && call["input"]["queries"][0]["rustContext"]["buildScripts"] == true),
        "{both_row}"
    );
    assert!(
        both_row["notices"][1]
            .as_str()
            .is_some_and(|notice| notice.contains("first")
                && notice.contains("buildScripts:true")
                && notice.contains("repository code")),
        "{both_row}"
    );

    // A workspace symbol is a found declaration: no use is missing.
    let symbols = rust_row(
        &root,
        &outside,
        json!({"operation": "workspaceSymbol"}),
        &["lib.rs"],
    )
    .await;
    assert!(symbols.get("isPartial").is_none(), "{symbols}");

    // A package whose includes all load: checked, complete, and says so.
    let plain = rust_row(&root, method, json!({}), &["lib.rs"]).await;
    assert!(plain.get("isPartial").is_none(), "{plain}");
    assert_eq!(plain["coverage"]["textOnlyFiles"], 1, "{plain}");
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
