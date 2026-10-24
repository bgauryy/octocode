//! One failure, one code and recovery in every local walking tool.

use super::*;
use octocode_contract_types::{AstSearchQuery, LocalSearchQuery, StructureSearchQuery};

/// Each local walking tool's error code for a looping root, by its public
/// entry.
#[cfg(unix)]
fn loop_codes(root: &Path) -> Vec<(&'static str, octocode_contract_types::ErrorCode)> {
    let looping = root.join("a");
    let (paths, security) = simple_policy(root);
    let local: LocalSearchQuery =
        serde_json::from_value(row(&looping, json!({"matchString": ["x"]}))).expect("query");
    let local = crate::tools::local_search::execute_local_search(
        &local,
        &paths,
        &security,
        &NeverCancel,
        None,
        None,
        None,
    )
    .expect_err("loop");
    let structure: StructureSearchQuery =
        serde_json::from_value(row(&looping, json!({"operation": "tree"}))).expect("query");
    let structure = crate::tools::structure_search::execute_structure(
        &structure,
        &paths,
        &security,
        &NeverCancel,
        None,
    )
    .expect_err("loop");
    let ast: AstSearchQuery =
        serde_json::from_value(row(&looping, json!({"operation": "symbols"}))).expect("query");
    let ast = crate::tools::ast_search::execute_ast(&ast, &paths, &security, &NeverCancel, None)
        .expect_err("loop");
    vec![
        ("localSearch", local.code),
        ("structureSearch", structure.code),
        ("astSearch", ast.code),
    ]
}

/// One failure has one `errorCode` in every local tool: a symlink loop is a
/// path that cannot be resolved, whichever tool met it.
#[cfg(unix)]
#[test]
fn a_symlink_loop_root_has_one_code_in_every_local_tool() {
    let root = Fixture::new();
    std::os::unix::fs::symlink(root.0.join("b"), root.0.join("a")).expect("a");
    std::os::unix::fs::symlink(root.0.join("a"), root.0.join("b")).expect("b");
    for (tool, code) in loop_codes(&root.0) {
        assert_eq!(
            code,
            octocode_contract_types::ErrorCode::PathValidationFailed,
            "{tool}"
        );
    }
}

/// A binary file named as astSearch's path is refused as binary, the code
/// localFetch gives the same file.
#[test]
fn a_binary_file_is_binary_file_unsupported_in_ast_search() {
    let root = Fixture::new();
    std::fs::write(root.0.join("blob.rs"), b"fn a() {}\0\x01\x02").expect("blob");
    let query: AstSearchQuery = serde_json::from_value(row(
        &root.0.join("blob.rs"),
        json!({"operation": "match", "rule": {"pattern": "fn a() {}"}}),
    ))
    .expect("query");
    let (paths, security) = simple_policy(&root.0);
    let error =
        crate::tools::ast_search::execute_ast(&query, &paths, &security, &NeverCancel, None)
            .expect_err("binary");
    assert_eq!(
        error.code,
        octocode_contract_types::ErrorCode::BinaryFileUnsupported,
        "{error:?}"
    );
}

/// A directory the walk cannot read is disclosed by every walking tool, and
/// leaves absence unproven: astSearch (match and symbols) marks the result
/// partial instead of answering complete, as structureSearch warns of it.
#[cfg(unix)]
#[test]
fn an_unreadable_directory_leaves_every_walking_tool_partial() {
    use std::os::unix::fs::PermissionsExt;
    let root = Fixture::new();
    std::fs::write(root.0.join("a.ts"), "function needle() {}\n").expect("visible");
    std::fs::create_dir(root.0.join("locked")).expect("locked");
    std::fs::write(root.0.join("locked/z.ts"), "function needle() {}\n").expect("hidden");
    std::fs::set_permissions(
        root.0.join("locked"),
        std::fs::Permissions::from_mode(0o000),
    )
    .expect("chmod");
    if std::fs::read_dir(root.0.join("locked")).is_ok() {
        let _ = std::fs::set_permissions(
            root.0.join("locked"),
            std::fs::Permissions::from_mode(0o755),
        );
        return;
    }
    let outs = [ast_match(), ast_symbols(), structure("files", &FLAGGED)]
        .into_iter()
        .map(|tool| (tool.name, (tool.run)(&root.0, &(tool.query)(&root.0))))
        .collect::<Vec<_>>();
    let _ = std::fs::set_permissions(
        root.0.join("locked"),
        std::fs::Permissions::from_mode(0o755),
    );
    // One typed disclosure in every walking tool: the read error at the
    // walked scope, in `skippedPaths`.
    for (name, out) in outs {
        let unreadable = out["skippedPaths"]["entries"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|entry| entry["cause"] == "unreadable")
            .collect::<Vec<_>>();
        assert_eq!(unreadable.len(), 1, "{name}: {out}");
        assert_eq!(
            unreadable[0]["reason"], "permission denied",
            "{name}: {out}"
        );
        if name.starts_with("astSearch") {
            assert_eq!(out["isPartial"], json!(true), "{name}: {out}");
            assert_ne!(out["coverage"]["state"], json!("complete"), "{name}: {out}");
        }
    }
}

/// A missing path gets the same recovery in every local walking tool:
/// `pathNotFound` with the viewTree lead, and no absolute filesystem path
/// in the message.
#[test]
fn a_missing_path_has_one_recovery_and_no_absolute_root_in_every_tool() {
    let root = Fixture::new();
    std::fs::create_dir(root.0.join("src")).expect("src");
    std::fs::write(root.0.join("src/recover.ts"), "source").expect("source");
    std::fs::write(root.0.join("outside.ts"), "outside").expect("outside");
    let (paths, security) = simple_policy(&root.0);
    let missing = json!("src/nope");
    let mut errors = Vec::new();
    let fields = |extra: Value| {
        let mut row = row(&root.0, extra);
        row["path"] = missing.clone();
        row
    };
    let local: LocalSearchQuery =
        serde_json::from_value(fields(json!({"matchString": ["x"]}))).expect("query");
    errors.push((
        "localSearch",
        crate::tools::local_search::execute_local_search(
            &local,
            &paths,
            &security,
            &NeverCancel,
            None,
            None,
            None,
        )
        .expect_err("missing"),
    ));
    let structure: StructureSearchQuery =
        serde_json::from_value(fields(json!({"operation": "tree"}))).expect("query");
    errors.push((
        "structureSearch",
        crate::tools::structure_search::execute_structure(
            &structure,
            &paths,
            &security,
            &NeverCancel,
            None,
        )
        .expect_err("missing"),
    ));
    for operation in [
        json!({"operation": "symbols"}),
        json!({"operation": "match", "rule": {"pattern": "x"}}),
    ] {
        let ast: AstSearchQuery = serde_json::from_value(fields(operation)).expect("query");
        errors.push((
            "astSearch",
            crate::tools::ast_search::execute_ast(&ast, &paths, &security, &NeverCancel, None)
                .expect_err("missing"),
        ));
    }
    let absolute = root.0.to_string_lossy().into_owned();
    for (tool, error) in errors {
        assert_eq!(
            error.code,
            octocode_contract_types::ErrorCode::PathNotFound,
            "{tool}"
        );
        assert!(
            !error.message.contains(&absolute),
            "{tool}: {}",
            error.message
        );
        let calls = error.calls.as_deref().expect("related request");
        let related = &calls["viewTree"];
        assert_eq!(related["kind"], "related", "{tool}: {calls}");
        crate::contracts::validate("structureSearch", related["input"].clone())
            .expect("public recovery input");
        let query: StructureSearchQuery =
            serde_json::from_value(related["input"]["queries"][0].clone()).expect("tree query");
        let out = crate::tools::structure_search::execute_structure(
            &query,
            &paths,
            &security,
            &NeverCancel,
            None,
        )
        .expect("scoped recovery executes");
        assert!(out.to_string().contains("recover.ts"), "{tool}: {out}");
        assert!(!out.to_string().contains("outside.ts"), "{tool}: {out}");
    }
}
