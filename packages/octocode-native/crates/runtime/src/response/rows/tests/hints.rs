use super::*;
use crate::response::output::SANDBOX_HINT;

/// A cancelled call has one recovery on every tool: retry only when the
/// result is still needed and what cancelled it changed, else report it.
#[test]
fn a_cancellation_bounds_its_retry_and_otherwise_reports() {
    for tool in [ToolId::LocalSearch, ToolId::Clasify, ToolId::GhSearchCode] {
        assert_eq!(
            error_code_hint(tool, ErrorCode::Cancelled, &Value::Null),
            Some(CANCELLED_HINT)
        );
    }
    for slot in ["still needed", "changed", "otherwise report"] {
        assert!(CANCELLED_HINT.contains(slot), "{slot}: {CANCELLED_HINT}");
    }
    assert!(CANCELLED_HINT.chars().count() <= MAX_GUIDANCE_CHARS);
}

#[test]
fn relative_row_paths_drop_dot_segments_only() {
    let row = |path: &str| json!({"data": {"path": path, "matches": [{"path": path}]}});
    let value = envelope(
        ToolId::LocalSearch,
        vec![row("./src/a.ts"), row("src/./b/../c.ts"), row(".")],
    );
    let paths = value["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["data"]["path"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(paths, ["src/a.ts", "src/b/../c.ts", "."]);
    assert_eq!(
        value["results"][0]["data"]["matches"][0]["path"],
        "src/a.ts"
    );
}

fn error_hint(tool: &str, query: &Value, code: &str, message: &str) -> String {
    let mut row = json!({
        "index": 0,
        "status": "error",
        "data": {"error": message, "errorCode": code}
    });
    apply_hint_policy(
        &mut row,
        ToolId::from_name(tool).expect("known tool"),
        query,
    );
    row["data"]["hints"][0]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

#[test]
fn error_hints_match_exact_codes_not_substrings() {
    let query = json!({"owner":"o","keywords":["octocode"]});
    assert!(error_hint("ghSearchCode", &query, "rateLimited", "slow down").contains("Retry-After"));
    // A code merely containing "auth" is not an authentication failure.
    // The tool's fallback names its own advice.
    let fallback = error_hint("ghSearchCode", &query, "authorizationPending", "x");
    assert!(fallback.contains("unproven absence"), "{fallback}");
    // The sandbox hint keys on the dedicated code, never on message text.
    for tool in ["localSearch", "localFetch", "lspSearch"] {
        assert!(
            error_hint(
                tool,
                &json!({"path":"/etc"}),
                "outsideAllowedRoots",
                "denied"
            )
            .contains("ALLOWED_PATHS"),
            "{tool}"
        );
    }
    assert!(
        SANDBOX_HINT.contains("~/.octocode/.env")
            && SANDBOX_HINT.contains("not a workspace file")
            && SANDBOX_HINT.chars().count() <= MAX_GUIDANCE_CHARS,
        "the hint names the trusted place to widen roots and is delivered uncut: {SANDBOX_HINT}"
    );
    assert!(
        !error_hint(
            "localSearch",
            &json!({"path":"/etc"}),
            "fileAccessFailed",
            "Path '/etc' is outside allowed directories"
        )
        .contains("ALLOWED_PATHS")
    );
}

/// Debug diagnostics (`meta`) never take the row's one recovery hint: an
/// empty code search keeps its cause-specific tip.
#[test]
fn diagnostic_hints_never_displace_the_rows_recovery_tip() {
    let mut row = json!({"index":0,"status":"empty",
        "meta":{"diagnostics":{"hints":["GitHub reported an incomplete search index result; retry."]}},
        "data":{"hints":["The repository is missing, private, or hidden from this token; check owner/repo spelling."]}});
    apply_hint_policy(&mut row, ToolId::GhSearchCode, &json!({}));
    assert_eq!(
        row["data"]["hints"],
        json!([
            "The repository is missing, private, or hidden from this token; check owner/repo spelling."
        ])
    );
    assert_eq!(
        row["meta"]["diagnostics"]["hints"].as_array().map(Vec::len),
        Some(1)
    );
}

/// `hints.text` is an ordered list: the recovery first, then each further
/// finding in the order the tool wrote it, a repeat once, none lost.
#[test]
fn hints_are_an_ordered_list_that_keeps_every_finding() {
    let mut row = json!({"index":0,"status":"empty",
    "data":{"hints":[
        "No matches under src.",
        "2 entries sit behind skipped paths (build/); the lead lists them.",
        "No matches under src.",
        "Another finding."
    ]}});
    apply_hint_policy(&mut row, ToolId::LocalSearch, &json!({}));
    assert_eq!(
        row["data"]["hints"],
        json!([
            "No matches under src.",
            "2 entries sit behind skipped paths (build/); the lead lists them.",
            "Another finding."
        ])
    );
}

#[test]
fn error_fallbacks_are_failure_class_aware_and_preserve_domain_recovery() {
    let query = json!({"owner":"o","keywords":["octocode"]});
    let mut timeout = json!({
        "index": 0,
        "status": "error",
        "data": {"error":"timed out","errorCode":"timeout"}
    });
    apply_hint_policy(
        &mut timeout,
        ToolId::from_name("ghSearchCode").expect("known tool"),
        &query,
    );
    let hint = timeout["data"]["hints"][0].as_str().expect("hint");
    assert!(hint.contains("Retry once"), "{timeout}");
    assert!(!hint.contains("Broaden keywords"), "{timeout}");

    let mut permission = json!({
        "index": 0,
        "status": "error",
        "data": {"error":"forbidden","errorCode":"permission"}
    });
    apply_hint_policy(
        &mut permission,
        ToolId::from_name("ghSearchCode").expect("known tool"),
        &query,
    );
    assert!(
        permission["data"]["hints"][0]
            .as_str()
            .is_some_and(|hint| hint.contains("token scopes")),
        "{permission}"
    );

    let mut owned = json!({
        "index": 0,
        "status": "error",
        "data": {
            "error":"missing",
            "errorCode":"notFound",
            "hints":["Inspect the repository tree."]
        }
    });
    apply_hint_policy(
        &mut owned,
        ToolId::from_name("ghSearchCode").expect("known tool"),
        &query,
    );
    assert_eq!(
        owned["data"]["hints"],
        json!(["Inspect the repository tree."])
    );

    let mut missing_path = json!({
        "index": 0,
        "status": "error",
        "data": {"error":"missing","errorCode":"fileAccessFailed"}
    });
    apply_hint_policy(
        &mut missing_path,
        ToolId::from_name("localFetch").expect("known tool"),
        &json!({"path":"/repo/missing.rs"}),
    );
    assert!(
        missing_path["data"]["hints"][0]
            .as_str()
            .is_some_and(|hint| hint.contains("structureSearch operation:\"files\"")),
        "{missing_path}"
    );
}

/// A bare empty row (no hint, no lead) gets its tool's own fallback
/// tip, shaped by the query; a recovery the tool already stated stays the
/// row's one hint.
#[test]
fn a_bare_empty_row_gets_its_tools_fallback_hint() {
    for tool in ToolId::ALL {
        let query = json!({});
        let mut row = json!({"index":0,"status":"empty","data":{"files":[]}});
        apply_hint_policy(&mut row, tool, &query);
        let hint = row["data"]["hints"][0].as_str().unwrap_or_default();
        assert_eq!(
            hint,
            concise(tool.output().fallback_hint(&query)),
            "{tool}: {row}"
        );
    }
    let mut stated = json!({"index":0,"status":"empty",
        "data":{"hints":["Own recovery."],"files":[]}});
    apply_hint_policy(&mut stated, ToolId::LocalSearch, &json!({}));
    assert_eq!(stated["data"]["hints"], json!(["Own recovery."]));
    let mut led = json!({"index":0,"status":"empty",
        "data":{"next":{"restart":{"tool":"localSearch","query":{"queries":[{"path":"."}]}}}}});
    apply_hint_policy(&mut led, ToolId::LocalSearch, &json!({}));
    assert!(led["data"].get("hints").is_none(), "{led}");
    // An empty row whose category the tool answers for keeps only its facts.
    let mut answered = json!({"index":0,"status":"empty",
        "data":{"category":"noDiagnostics","reason":"No diagnostics."}});
    apply_hint_policy(
        &mut answered,
        ToolId::LspSearch,
        &json!({"operation":"diagnostic"}),
    );
    assert!(answered["data"].get("hints").is_none(), "{answered}");
    // A successful row is never given one.
    let mut found = json!({"index":0,"data":{"files":[{"path":"a"}]}});
    apply_hint_policy(&mut found, ToolId::LocalSearch, &json!({}));
    assert!(found["data"].get("hints").is_none(), "{found}");
}

/// A later row of a batch gets the recovery its own query earns: the
/// fallback reads the query it is given, whatever the row's `index`.
#[test]
fn a_later_row_recovers_with_its_own_query() {
    let query = json!({"owner":"o","keywords":["a"],"matchIn":"path"});
    let mut row = json!({"index": 2, "status": "empty", "data": {}});
    ensure_recovery(&mut row, ToolId::GhSearchCode, &query);
    assert_eq!(
        row["data"]["hints"],
        json!([ToolId::GhSearchCode.output().fallback_hint(&query)]),
        "{row}"
    );
}

/// An empty row whose only call is a retry (GitHub's incomplete results)
/// stays empty, so the call exits 1; a next page still makes it a success.
#[test]
fn an_empty_row_offering_only_a_retry_stays_empty() {
    let call = json!({"tool":"ghSearchRepo","query":{"queries":[{"keywords":["mcp"]}]}});
    let query = json!({"keywords":["mcp"]});
    let retry = json!({"repositories":[],"isPartial":true,
        "partialReasons":["providerIncompleteResults"],"next":{"retry":call}});
    let row = result_row(ToolId::GhSearchRepo, 0, &query, retry, Some("empty"));
    assert_eq!(row["status"], "empty", "{row}");
    let paged = json!({"repositories":[],"next":{"nextPage":call}});
    let row = result_row(ToolId::GhSearchRepo, 0, &query, paged, Some("empty"));
    assert_ne!(row["status"], "empty", "{row}");
}
