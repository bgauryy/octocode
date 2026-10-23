use super::*;

/// One code, one recovery hint. A missing path gets the same hint
/// on every tool that reports `pathNotFound`.
#[test]
fn path_errors_have_one_hint_on_every_tool() {
    for (code, expected) in [
        (
            ErrorCode::PathNotFound,
            crate::response::output::VERIFY_PATH_HINT,
        ),
        (
            ErrorCode::PathPolicyDenied,
            crate::policy::discovery::WITHHELD_HINT,
        ),
    ] {
        let hints: std::collections::BTreeSet<String> = [
            "localFetch",
            "localSearch",
            "structureSearch",
            "astSearch",
            "lspSearch",
            "ghCloneRepo",
        ]
        .into_iter()
        .map(|tool| {
            let mut row = json!({
                "index": 0,
                "status": "error",
                "data": {"error":"withheld","errorCode":code}
            });
            apply_hint_policy(
                &mut row,
                ToolId::from_name(tool).expect("known tool"),
                &json!({"path":"/repo/missing.rs"}),
            );
            row["data"]["hints"][0].as_str().expect("hint").to_owned()
        })
        .collect();
        assert_eq!(hints.into_iter().collect::<Vec<_>>(), [expected], "{code}");
    }
}

#[test]
fn invalid_input_and_missing_path_errors_do_not_suggest_broadening() {
    for (tool, code, expected) in [
        (
            "structureSearch",
            "invalidInput",
            "Correct the rejected field",
        ),
        ("ghSearchRepo", "invalidInput", "Correct the rejected field"),
        ("structureSearch", "pathNotFound", "Verify the path exists"),
        ("localSearch", "pathNotFound", "Verify the path exists"),
        ("structureSearch", "outsideAllowedRoots", "allowed roots"),
    ] {
        let mut row = json!({
            "index": 0,
            "status": "error",
            "data": {"error":"failed","errorCode":code}
        });
        apply_hint_policy(
            &mut row,
            ToolId::from_name(tool).expect("known tool"),
            &json!({"path":"/repo/nope"}),
        );
        let hint = row["data"]["hints"][0].as_str().expect("hint");
        assert!(hint.contains(expected), "{code}: {hint}");
        assert!(!hint.contains("Broaden"), "{code}: {hint}");
    }
    assert!(is_invalid_input_code("invalidPattern"));
    assert!(!is_invalid_input_code("fileAccessFailed"));
    // astSearch rejects a missing, unknown, or mismatched grammar and an
    // unparseable pattern: the caller must fix the request (exit 2).
    for code in [
        "languageRequired",
        "languageUnsupported",
        "languageMismatch",
        "languageFileRequired",
        "languageDirectoryRequired",
        "invalidPattern",
        "invalidGlob",
        "invalidInput",
    ] {
        assert!(is_invalid_input_code(code), "{code}");
    }
    for code in [
        "timeout",
        "executionFailed",
        "fileTooLarge",
        "pathNotFound",
        "fileAccessFailed",
        // Undeclared spellings are no class at all, never invalid input.
        "structural.language.unsupported",
    ] {
        assert!(!is_invalid_input_code(code), "{code}");
    }
}

#[test]
fn ast_search_error_hints_are_code_specific() {
    let query = json!({"operation":"match","path":"/repo","pattern":"foo($A)"});
    for (code, expected) in [
        ("outsideAllowedRoots", "allowed root"),
        ("structural.query.compileFailed", "syntaxTree"),
        ("languageRequired", "language"),
        ("languageUnsupported", "language"),
        ("languageFileRequired", "languageGlobs"),
        ("languageDirectoryRequired", "language"),
    ] {
        let mut row = json!({
            "index": 0,
            "status": "error",
            "data": {"error":"failed","errorCode":code}
        });
        apply_hint_policy(
            &mut row,
            ToolId::from_name("astSearch").expect("known tool"),
            &query,
        );
        let hint = row["data"]["hints"][0].as_str().expect("hint");
        assert!(hint.contains(expected), "{code}: {hint}");
        assert!(!hint.contains("Broaden the syntax"), "{code}: {hint}");
    }
    // A parse limit is its typed error alone: no fallback text.
    let mut row = json!({"index":0,"status":"error",
        "data":{"error":"failed","errorCode":"fileTooLarge","terminalLimit":true}});
    apply_hint_policy(&mut row, ToolId::AstSearch, &query);
    assert!(row["data"].get("hints").is_none(), "{row}");
}

/// A page past the end is one shared outcome on every tool: status
/// `empty`, a `reason` naming the page and the page count, and a
/// `next.restart` at page 1 without the page cursors.
/// The empty fallback names the filter that most likely hid the
/// answer, in each tool's own vocabulary.
#[test]
fn empty_fallbacks_name_the_tools_own_filters() {
    let hint = |tool: ToolId, query: Value| tool.output().fallback_hint(&query);
    for (tool, query, needle) in [
        (
            ToolId::GhSearchCode,
            json!({"extensions":["rs"]}),
            "extensions",
        ),
        (ToolId::GhSearchCode, json!({"matchIn":"path"}), "path"),
        (
            ToolId::GhSearchCode,
            json!({"keywords":["x"]}),
            "unproven absence",
        ),
        (
            ToolId::GhSearchCode,
            json!({"keywords":["x","y"]}),
            "keywords",
        ),
        // A repository search's filters drop through its `broadenSearch`
        // lead, so its fallback text only names keywords.
        (ToolId::GhSearchRepo, json!({"keywords":["x"]}), "keywords"),
        (ToolId::GhStructure, json!({"include":["*.rs"]}), "include"),
        (ToolId::GhStructure, json!({"path":"src"}), "path"),
        (
            ToolId::ArtifactSearch,
            json!({"packageName":"x"}),
            "packageName",
        ),
        (
            ToolId::ArtifactSearch,
            json!({"keywords":["x"]}),
            "keywords",
        ),
        (
            ToolId::StructureSearch,
            json!({"operation":"files","extensions":["rs"]}),
            "extensions",
        ),
        (
            ToolId::StructureSearch,
            json!({"operation":"tree","path":"."}),
            "path",
        ),
    ] {
        let text = hint(tool, query.clone());
        assert!(text.contains(needle), "{tool:?} {query}: {text}");
        assert!(
            text.chars().count() <= MAX_GUIDANCE_CHARS,
            "{tool:?}: {text}"
        );
    }
    // Filters differ, so the advice differs.
    assert_ne!(
        hint(ToolId::GhSearchCode, json!({"extensions":["rs"]})),
        hint(ToolId::GhSearchCode, json!({"keywords":["x"]}))
    );
}
