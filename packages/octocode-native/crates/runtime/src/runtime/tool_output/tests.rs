//! Every tool's recovery through the shared error hints
//! (`octocode_runtime_base::response::rows`), with the tool's own output hooks.
use super::ToolOutputOf;
use octocode_contract_types::ErrorCode;
use octocode_runtime_base::id::ToolId;
use octocode_runtime_base::response::rows::{CANCELLED_HINT, attach_error_hint, note_error_hint};
use octocode_test_support::json_fixture;
use serde_json::{Value, json};

#[test]
fn shared_failures_have_recovery_across_every_tool() {
    for tool in ToolId::ALL {
        for code in [
            ErrorCode::Authentication,
            ErrorCode::RateLimited,
            ErrorCode::ExecutionFailed,
            ErrorCode::OutputContractViolation,
            ErrorCode::ToolUnavailable,
            ErrorCode::SourceMismatch,
        ] {
            let mut data = json!({"errorCode":code,"error":"original cause"});
            attach_error_hint(tool.output(), &mut data, &Value::Null);
            assert_eq!(data["error"], "original cause");
            assert!(
                data["hints"][0]
                    .as_str()
                    .is_some_and(|hint| !hint.is_empty()),
                "{tool}: {data}"
            );
            let once = data.clone();
            attach_error_hint(tool.output(), &mut data, &Value::Null);
            assert_eq!(data, once, "recovery is not repeated");
        }
    }
}

#[test]
fn producer_guidance_and_explicit_no_hint_are_preserved() {
    let mut data = json_fixture(
        r#"{
            "errorCode": "server",
            "hints": ["provider-specific cause and remedy"]
        }"#,
    );
    let original = data.clone();
    attach_error_hint(ToolId::GhSearchCode.output(), &mut data, &Value::Null);
    assert_eq!(data, original);
    let mut data = json_fixture(
        r#"{
            "errorCode": "languageMismatch",
            "error": "the message already names both repairs"
        }"#,
    );
    let original = data.clone();
    attach_error_hint(ToolId::AstSearch.output(), &mut data, &Value::Null);
    assert_eq!(data, original);
}

#[test]
fn transient_guidance_respects_final_failures() {
    let mut data = json_fixture(
        r#"{
            "errorCode": "transport",
            "error": "connection failed",
            "retryable": false
        }"#,
    );
    attach_error_hint(ToolId::ArtifactSearch.output(), &mut data, &Value::Null);
    assert_eq!(data["retryable"], false);
    assert!(
        data["hints"][0]
            .as_str()
            .expect("guidance")
            .contains("only if retryable is true")
    );
}

/// N9: the shared recovery is a static note, stated once per response: on
/// the first failed row that needs it, as the shaping stage notes it; a
/// later row that needs the same note says nothing more, and a row with its
/// own hints keeps them.
#[test]
fn a_shared_error_hint_is_stated_once_per_response() {
    let failed = || {
        json_fixture(
            r#"{
                "status": "error",
                "data": {"errorCode": "notFound", "error": "gone"}
            }"#,
        )
    };
    let mut structured = json!({"results":[failed(), failed(),
        {"status":"error","data":{"errorCode":"notFound","error":"gone","hints":["own"]}},
        {"status":"error","data":{"errorCode":"cancelled","error":"stopped"}}]});
    for row in structured["results"].as_array_mut().expect("rows") {
        note_error_hint(ToolId::GhCloneRepo.output(), &mut row["data"], &Value::Null);
    }
    octocode_runtime_base::response::notices::place(&mut structured, "results");
    let hints = |index: usize| structured["results"][index]["data"].get("hints").cloned();
    assert_eq!(
        hints(0),
        Some(json!([
            octocode_runtime_base::response::output::NOT_FOUND_HINT
        ])),
        "{structured}"
    );
    assert_eq!(hints(1), None, "said once: {structured}");
    assert_eq!(hints(2), Some(json_fixture(r#"["own"]"#)), "{structured}");
    assert_eq!(
        hints(3),
        Some(json!([CANCELLED_HINT])),
        "another note: {structured}"
    );
    assert!(!structured.to_string().contains("notices"), "{structured}");
}

/// The tools whose lookups are not paths: each states its own `notFound`
/// recovery ([`octocode_runtime_base::response::output::ToolOutput::error_hint`]).
const OWN_NOT_FOUND: [ToolId; 2] = [ToolId::ArtifactSearch, ToolId::SecurityCheck];

/// One `notFound` rule on every tool that states no recovery of its own. A
/// row without hints of its own carries the shared hint, leads or not: a
/// lead is a call (a parent listing, a search, the nearest nodes), the hint
/// says what to do with its answer. The hint names no registry or version
/// and does not restate the instructions' caution that a miss is no absence
/// elsewhere.
#[test]
fn a_not_found_hint_fits_the_resource_and_repeats_no_caution() {
    let lead = json_fixture(
        r#"{
            "kind": "related",
            "tool": "structureSearch",
            "input": {"queries": [{"path": "."}]}
        }"#,
    );
    for tool in ToolId::ALL
        .into_iter()
        .filter(|tool| !OWN_NOT_FOUND.contains(tool))
    {
        for related in [None, Some(json!([lead.clone()]))] {
            let mut data = json_fixture(r#"{"errorCode": "notFound", "error": "gone"}"#);
            if let Some(related) = &related {
                data["related"] = related.clone();
            }
            attach_error_hint(tool.output(), &mut data, &Value::Null);
            assert_eq!(
                data["hints"],
                json!([octocode_runtime_base::response::output::NOT_FOUND_HINT]),
                "{tool}: {data}"
            );
        }
    }
    let hint = octocode_runtime_base::response::output::NOT_FOUND_HINT.to_lowercase();
    for named in ["registry", "version", "revision", "nowhere else"] {
        assert!(!hint.contains(named), "names {named}: {hint}");
    }
    // A row that states its own recovery keeps it alone.
    let mut own = json_fixture(r#"{"errorCode": "notFound", "error": "gone", "hints": ["own"]}"#);
    attach_error_hint(ToolId::GraphQuery.output(), &mut own, &Value::Null);
    assert_eq!(own["hints"], json_fixture(r#"["own"]"#));
}

/// C3eK C-R8: an expired or stale cursor's message states the recovery
/// (rerun the original request without cursor; a stale row's next.restart);
/// no hint repeats it or names a restart call the row does not carry.
#[test]
fn an_expired_or_stale_cursor_gets_no_hint_that_repeats_its_message() {
    for tool in ToolId::ALL {
        for code in ["responseSnapshotExpired", "staleSnapshot", "sourceMismatch"] {
            let mut data =
                json!({"errorCode":code,"error":"Rerun the original request without cursor."});
            attach_error_hint(tool.output(), &mut data, &Value::Null);
            let hints = data["hints"].to_string().to_lowercase();
            assert!(!hints.contains("restart call"), "{tool} {code}: {data}");
            if code != "sourceMismatch" {
                assert!(data.get("hints").is_none(), "{tool} {code}: {data}");
            }
        }
    }
}

/// C3eK C-R9: the rate-limit hint names no wait value the row may not show
/// (a registry 429 carries no `rateLimit`); a row that has one states it there.
#[test]
fn the_rate_limit_hint_names_no_value_the_row_may_lack() {
    for tool in ToolId::ALL {
        let mut data = json_fixture(
            r#"{
                "errorCode": "rateLimited",
                "error": "npm rate limit reached (HTTP 429). Retry later."
            }"#,
        );
        attach_error_hint(tool.output(), &mut data, &Value::Null);
        let hint = data["hints"][0].as_str().expect("a rate-limit recovery");
        assert!(!hint.contains("Retry-After"), "{tool}: {hint}");
    }
}

/// A package or advisory the registry or database does not know is a name
/// to check there: the tool's own recovery reaches the served row ahead of
/// the shared path advice, once per response like every shared hint, and a
/// row with its own hints keeps them alone.
#[test]
fn a_tools_own_not_found_hint_reaches_the_row_ahead_of_the_shared_one() {
    for (tool, names) in [
        (ToolId::ArtifactSearch, "packageName and ecosystem"),
        (ToolId::SecurityCheck, "GHSA, CVE or OSV id"),
    ] {
        let failed =
            || json!({"status": "error", "data": {"errorCode": "notFound", "error": "gone"}});
        let mut structured = json!({"results": [failed(), failed(),
            {"status": "error", "data": {"errorCode": "notFound", "error": "gone", "hints": ["own"]}}]});
        for row in structured["results"].as_array_mut().expect("rows") {
            note_error_hint(tool.output(), &mut row["data"], &Value::Null);
        }
        octocode_runtime_base::response::notices::place(&mut structured, "results");
        let hints = |index: usize| structured["results"][index]["data"].get("hints").cloned();
        let first = hints(0).unwrap_or_default();
        let stated = first[0].as_str().unwrap_or_default();
        assert!(stated.contains(names), "{tool}: {structured}");
        assert_ne!(
            stated,
            octocode_runtime_base::response::output::NOT_FOUND_HINT,
            "{tool}"
        );
        assert_eq!(
            first.as_array().map(Vec::len),
            Some(1),
            "{tool}: {structured}"
        );
        assert_eq!(hints(1), None, "said once: {structured}");
        assert_eq!(hints(2), Some(json_fixture(r#"["own"]"#)), "{structured}");
    }
}

/// A tool that states no `notFound` recovery still gets the shared one.
#[test]
fn a_tool_with_no_not_found_hint_still_gets_the_shared_one() {
    for tool in [ToolId::GhCloneRepo, ToolId::Clasify, ToolId::LocalFetch] {
        assert_eq!(
            tool.output().error_hint(ErrorCode::NotFound, &Value::Null),
            None
        );
        let mut data = json_fixture(r#"{"errorCode": "notFound", "error": "gone"}"#);
        attach_error_hint(tool.output(), &mut data, &Value::Null);
        assert_eq!(
            data["hints"],
            json!([octocode_runtime_base::response::output::NOT_FOUND_HINT]),
            "{tool}: {data}"
        );
    }
}
