//! Empty and unavailable rows: status, recovery leads and workspace-root
//! failures.

use super::*;

/// A resolved anchor with no subtypes says so, instead of asking the caller
/// to verify an anchor that was found.
#[test]
fn an_empty_hierarchy_row_says_the_relation_is_empty() {
    for (operation, wording) in [
        ("subtypes", "extends or implements"),
        ("supertypes", "base"),
    ] {
        let query: LspSearchQuery = serde_json::from_value(serde_json::json!({
            "operation": operation, "path": "src/a.ts", "symbolName": "A", "line": 1
        }))
        .expect("hierarchy query");
        let row = with_stage_hints(&query, &empty(&query, "noLocations", "no results", true));
        let hint = row["hints"][0].as_str().expect("hint");
        assert!(hint.contains(wording), "{operation}: {hint}");
        assert!(!hint.contains("Verify the symbol"), "{hint}");
        assert!(hint.chars().count() <= 120, "{hint}");
    }
    let query: LspSearchQuery = serde_json::from_value(serde_json::json!({
        "operation": "references", "path": "src/a.ts", "symbolName": "A", "line": 1
    }))
    .expect("references query");
    // The row's executable `next.read` is the recovery; the general
    // advice lives once in the tool description.
    let row = with_stage_hints(
        &query,
        &with_next(&query, empty(&query, "noLocations", "no results", true)),
    );
    assert!(row.get("hints").is_none(), "{row}");
}

#[test]
fn empty_and_unavailable_rows_expose_status_and_recovery_next() {
    let query = query(serde_json::json!({
        "operation": "definition", "mainGoal": "test", "reasoning": "test",
        "path": "/repo/src/lib.rs",
        "symbolName": "execute",
        "line": 10
    }));
    let empty = with_next(&query, empty(&query, "noLocations", "none", true));
    assert_eq!(empty["status"], "empty");
    assert!(empty["next"]["read"].get("confidence").is_none());
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
    let unused_script = super::super::failure::empty(
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
    let tipped = super::super::failure::empty(&scripted, "noLocations", "none", true);
    assert_eq!(
        tipped["hints"].as_array().map(Vec::len),
        Some(1),
        "{tipped}"
    );
    assert!(
        tipped["hints"][0]
            .as_str()
            .is_some_and(|hint| hint.contains("repository code")
                && hint.contains("includeBuildScripts")
                && !hint.contains("buildScripts:true")),
        "{tipped}"
    );
    assert_eq!(
        tipped["next"]["includeBuildScripts"]["query"]["queries"][0]["rustContext"],
        serde_json::json!({"buildScripts": true}),
        "{tipped}"
    );
    let built = requery(
        &scripted,
        serde_json::json!({"rustContext": {"buildScripts": true}}),
    );
    let built = super::super::failure::empty(&built, "noLocations", "none", true);
    assert!(built.get("hints").is_none(), "{built}");
    // Source included from outside the package never loads: its own tip,
    // and no rerun.
    std::fs::write(
        package.path().join("src/lib.rs"),
        "include!(\"../../elsewhere/types.rs\");\n",
    )
    .expect("lib");
    let outside = super::super::failure::empty(&scripted, "noLocations", "none", true);
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
        outside["next"].get("includeBuildScripts").is_none(),
        "{outside}"
    );
    // Both: both tips (no rerun carries the build-script one), and no
    // ~2 GB rerun the outside include may defeat.
    std::fs::write(
        package.path().join("src/lib.rs"),
        "include!(concat!(env!(\"OUT_DIR\"), \"/gen.rs\"));\ninclude!(\"../../elsewhere/types.rs\");\n",
    )
    .expect("lib");
    let both = super::super::failure::empty(&scripted, "noLocations", "none", true);
    assert_eq!(both["hints"].as_array().map(Vec::len), Some(2), "{both}");
    assert!(
        both["hints"][1]
            .as_str()
            .is_some_and(|hint| hint.contains("rustContext {buildScripts:true}")
                && hint.contains("repository code")),
        "{both}"
    );
    assert!(both["next"].get("includeBuildScripts").is_none(), "{both}");
    let down = failure(
        &query,
        "file:///repo/src/lib.rs",
        ErrorCode::ServerUnavailable,
        "missing",
        false,
    );
    assert_eq!(down["status"], "error");
    assert_eq!(down["errorCode"], "serverUnavailable");
    assert_eq!(down["next"]["read"]["tool"], "localFetch");
    // The fallback is an executable lexical lead, not a static tip.
    let text = &down["next"]["findText"];
    assert_eq!(text["tool"], "localSearch", "{down}");
    let search = &text["query"]["queries"][0];
    assert_eq!(search["path"], "/repo/src", "{down}");
    assert_eq!(search["matchString"], "\\bexecute\\b", "{down}");
    assert_eq!(search["regex"], "rust", "{down}");
    assert_eq!(search["caseMode"], "sensitive", "{down}");
    // The row's own leads say it all.
    assert!(
        with_stage_hints(&query, &down).get("hints").is_none(),
        "{down}"
    );
    crate::contracts::validate_output("lspSearch", &public_row(&down))
        .expect("unavailable row satisfies the output contract");

    let timeout = failure(
        &query,
        "file:///repo/src/lib.rs",
        ErrorCode::Timeout,
        "timed out",
        false,
    );
    assert_eq!(timeout["next"]["retry"]["tool"], "lspSearch");
    assert_eq!(timeout["next"]["read"]["tool"], "localFetch");

    let explicitly_scoped = requery(&query, serde_json::json!({"workspaceRoot": "/repo"}));
    let scoped = failure(
        &explicitly_scoped,
        "file:///repo/src/lib.rs",
        ErrorCode::CapabilityUnavailable,
        "unsupported",
        true,
    );
    assert_eq!(scoped["next"]["read"]["tool"], "localFetch");
}

/// A directory scope is searched for the name: the lead is `findText`
/// (a localSearch), never a `read`.
#[test]
fn a_directory_scope_recovers_with_a_text_search_lead() {
    let dir = tempfile::tempdir().expect("dir");
    let query = query(serde_json::json!({
        "operation": "workspaceSymbol", "mainGoal": "test", "reasoning": "test",
        "path": dir.path().to_string_lossy(), "symbolName": "execute"
    }));
    let row = with_next(&query, empty(&query, "noLocations", "none", true));
    assert_eq!(row["next"]["findText"]["tool"], "localSearch", "{row}");
    assert!(row["next"].get("read").is_none(), "{row}");
}

#[test]
fn workspace_root_failures_emit_a_string_path_without_directory_read_recovery() {
    let query: LspSearchQuery = serde_json::from_value(serde_json::json!({
        "operation": "workspaceSymbol", "mainGoal": "test", "reasoning": "test",
        "workspaceRoot": "/repo",
        "symbolName": "execute"
    }))
    .expect("workspace-symbol query");

    let down = failure(
        &query,
        "file:///repo",
        ErrorCode::ServerUnavailable,
        "missing",
        false,
    );

    assert!(down["path"].is_string(), "{down}");
    assert!(down.get("next").is_none(), "{down}");
    let hinted = with_stage_hints(&query, &down);
    assert!(
        hinted["hints"][0]
            .as_str()
            .is_some_and(|hint| hint.contains("Provide path")),
        "{hinted}"
    );
    crate::contracts::validate_output("lspSearch", &public_row(&down))
        .expect("workspace-root failure must satisfy the internal output contract");
}

#[test]
fn workspace_root_empty_results_do_not_emit_a_pathless_read_recovery() {
    let q = query(serde_json::json!({
        "operation": "workspaceSymbol", "mainGoal": "test", "reasoning": "test",
        "workspaceRoot": "/repo",
        "symbolName": "nothing"
    }));
    let row = with_next(
        &q,
        items_payload(&q, "symbols", serde_json::json!([]), TEST_BUDGET),
    );
    assert_eq!(row["status"], "empty");
    assert!(row.get("next").is_none(), "{row}");
    crate::contracts::validate_output("lspSearch", &public_row(&row))
        .expect("empty workspace-root row satisfies the output contract");
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
