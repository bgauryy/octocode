//! B2: every tool's empty and error rows, through each tool's real path
//! (local files, a mocked GitHub and registry, a mocked judge), meet one row
//! contract: the output validates, an error row names its `errorCode` and
//! `error`, every empty or error row offers a recovery (a runnable lead or
//! hint text), `retryable` is never `false` (N7), and the exit class says the
//! call did not succeed.
#![allow(clippy::panic, clippy::unwrap_used)]

use crate::support::{MOCK_PROVIDER_TIMEOUT_MS, Workspace};
use octocode_native::runtime::{ExitClass, ToolOutcome, ToolRuntime};
use serde_json::{Value, json};
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

const NEVER: &str = "zzqqxxNeverMatchOctocode";

/// One probe: tool, row, and the row status it must produce.
struct Probe {
    tool: &'static str,
    row: Value,
    status: &'static str,
}

fn probe(tool: &'static str, status: &'static str, row: Value) -> Probe {
    Probe { tool, row, status }
}

/// A recovery the caller can act on: a `{tool, query}` lead or hint text.
fn has_recovery(data: &Value) -> bool {
    let leads = |container: Option<&Value>| {
        container
            .and_then(Value::as_object)
            .is_some_and(|map| map.values().any(|v| v.get("tool").is_some()))
    };
    let text = data
        .pointer("/hints/text")
        .and_then(Value::as_array)
        .is_some_and(|text| !text.is_empty())
        || data
            .get("hints")
            .and_then(Value::as_array)
            .is_some_and(|text| !text.is_empty());
    text || leads(data.get("hints")) || leads(data.get("next"))
}

async fn run(runtime: &ToolRuntime, probe: &Probe) -> ToolOutcome {
    let mut row = probe.row.clone();
    row["mainGoal"] = json!("B2 empty/error row contract.");
    runtime
        .execute(
            format!("b2-{}-{}", probe.tool, probe.status),
            probe.tool.into(),
            json!({"queries":[row]}),
        )
        .await
        .unwrap_or_else(|error| panic!("{} {}: {error:?}", probe.tool, probe.status))
}

fn check(probe: &Probe, outcome: &ToolOutcome) -> Vec<String> {
    let label = format!("{} {}", probe.tool, probe.status);
    let out = &outcome.structured_content;
    let mut failures = Vec::new();
    if let Err(error) = octocode_native::contracts::validate_output(probe.tool, out) {
        failures.push(format!("{label}: output contract: {error:?}"));
    }
    if out.to_string().contains("\"retryable\":false") {
        failures.push(format!("{label}: retryable:false emitted: {out}"));
    }
    let exit = outcome.exit_class();
    let exit_ok = match probe.status {
        "empty" => exit == ExitClass::Empty,
        _ => matches!(exit, ExitClass::Failed(_) | ExitClass::InvalidInput),
    };
    if !exit_ok {
        failures.push(format!("{label}: exit class {exit:?}: {out}"));
    }
    if probe.tool == "clasify" {
        return failures;
    }
    let status = out
        .pointer("/results/0/status")
        .and_then(Value::as_str)
        .unwrap_or("ok");
    if status != probe.status {
        failures.push(format!("{label}: status {status}: {out}"));
        return failures;
    }
    let data = out.pointer("/results/0/data").unwrap_or(&Value::Null);
    if probe.status == "error" {
        for field in ["errorCode", "error"] {
            if data
                .get(field)
                .and_then(Value::as_str)
                .is_none_or(str::is_empty)
            {
                failures.push(format!("{label}: error row without {field}: {out}"));
            }
        }
    }
    if !has_recovery(data) {
        failures.push(format!("{label}: no recovery lead or text: {out}"));
    }
    failures
}

async fn github_and_registry() -> MockServer {
    let server = MockServer::start().await;
    let empty_search = json!({"total_count":0,"incomplete_results":false,"items":[]});
    for search in ["code", "repositories", "issues", "commits"] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/search/{search}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(empty_search.clone()))
            .mount(&server)
            .await;
    }
    // The empty-search probes name a repo that exists (a zero-hit search
    // first confirms the repo); error probes name `a/missing`.
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "name":"b","full_name":"a/b","default_branch":"main","private":false,
            "owner":{"login":"a"}
        })))
        .mount(&server)
        .await;
    Mock::given(path_regex(".*"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({"message":"Not Found"})))
        .mount(&server)
        .await;
    server
}

#[tokio::test]
async fn every_tool_empty_and_error_row_meets_the_row_contract() {
    let server = github_and_registry().await;
    let workspace = Workspace::new();
    workspace.write("src/a.rs", "fn alpha() -> u32 {\n    1\n}\n");
    let runtime = workspace.runtime(&[
        ("GITHUB_API_URL", format!("{}/api/v3", server.uri())),
        ("OCTOCODE_ALLOW_PRIVATE_REGISTRY", "true".into()),
        ("OCTOCODE_BETA", "true".into()),
        ("OCTOCODE_CLASSIFICATION_API", "b2-secret".into()),
        ("OCTOCODE_CLASSIFICATION_API_HOST", server.uri()),
        ("REQUEST_TIMEOUT", MOCK_PROVIDER_TIMEOUT_MS.into()),
    ]);
    let gh = json!({"owner":"a","repo":"b"});
    let missing = json!({"owner":"a","repo":"missing"});
    let with = |base: &Value, extra: Value| {
        let mut row = base.clone();
        for (key, value) in extra.as_object().unwrap() {
            row[key] = value.clone();
        }
        row
    };
    let probes = [
        probe(
            "localSearch",
            "empty",
            json!({"path":"src","matchString":NEVER}),
        ),
        probe(
            "localSearch",
            "error",
            json!({"path":"missing","matchString":"x"}),
        ),
        probe("localFetch", "error", json!({"path":"missing.rs"})),
        probe(
            "structureSearch",
            "empty",
            json!({"path":"src","operation":"files","nameRegex":NEVER}),
        ),
        probe("structureSearch", "error", json!({"path":"missing"})),
        probe(
            "astSearch",
            "empty",
            json!({"operation":"symbols","path":"src","symbolName":NEVER}),
        ),
        probe(
            "astSearch",
            "error",
            json!({"operation":"symbols","path":"missing","symbolName":"x"}),
        ),
        probe(
            "lspSearch",
            "error",
            json!({"path":"missing.rs","symbolName":"x","lineHint":1,"operation":"definition"}),
        ),
        probe(
            "astTopology",
            "error",
            json!({"operation":"cycles","path":"missing"}),
        ),
        probe(
            "astRewrite",
            "empty",
            json!({"path":"src","language":"rust","pattern":format!("{NEVER}($A)"),"rewrite":"x($A)"}),
        ),
        probe(
            "astRewrite",
            "error",
            json!({"path":"missing","language":"rust","pattern":"f($A)","rewrite":"g($A)"}),
        ),
        probe("ghSearchRepo", "empty", json!({"keywords":[NEVER]})),
        probe(
            "ghSearchCode",
            "empty",
            with(&gh, json!({"keywords":[NEVER]})),
        ),
        probe("ghStructure", "error", missing.clone()),
        probe(
            "ghGetFileContent",
            "error",
            with(&missing, json!({"path":"missing.md"})),
        ),
        probe(
            "ghSearchHistory",
            "empty",
            with(&gh, json!({"operation":"pullRequests","keywords":[NEVER]})),
        ),
        probe(
            "ghGetHistoryItem",
            "error",
            with(
                &missing,
                json!({"operation":"pullRequest","number":999_999}),
            ),
        ),
        probe(
            "artifactSearch",
            "error",
            json!({"type":"npm","packageName":NEVER.to_lowercase(),"registry":server.uri()}),
        ),
        probe(
            "clasify",
            "error",
            json!({"resources":[{"id":"r","tool":"localFetch","query":{"path":"missing.rs"}}],
                "questions":[{"id":"q","type":"yesno","ask":"Is it there?"}]}),
        ),
    ];
    let mut failures = Vec::new();
    for probe in &probes {
        let outcome = run(&runtime, probe).await;
        failures.extend(check(probe, &outcome));
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    runtime.close().await;
}

/// B2 + LS6/LF4: one missing path gets one recovery form on every local tool
/// (today lspSearch/localFetch give text while structureSearch gives a
/// `viewTree` lead). Ignored until the local lane's shared lead builder lands.
#[tokio::test]
#[ignore = "B2: enable when LS6/LF4 unify the pathNotFound recovery (local lane)"]
async fn a_missing_path_gets_one_recovery_form_on_every_local_tool() {
    let workspace = Workspace::new();
    workspace.write("src/a.rs", "fn alpha() {}\n");
    let runtime = workspace.runtime(&[("OCTOCODE_BETA", "true".into())]);
    let mut forms = std::collections::BTreeMap::new();
    for (tool, row) in [
        (
            "localSearch",
            json!({"path":"src/missing","matchString":"x"}),
        ),
        ("localFetch", json!({"path":"src/missing.rs"})),
        ("structureSearch", json!({"path":"src/missing"})),
        (
            "lspSearch",
            json!({"path":"src/missing.rs","symbolName":"x","lineHint":1,"operation":"definition"}),
        ),
    ] {
        let outcome = run(
            &runtime,
            &Probe {
                tool,
                row,
                status: "error",
            },
        )
        .await;
        let data = outcome
            .structured_content
            .pointer("/results/0/data")
            .cloned()
            .unwrap_or_default();
        assert_eq!(data["errorCode"], "pathNotFound", "{tool}: {data}");
        let lead = data
            .get("hints")
            .and_then(Value::as_object)
            .is_some_and(|hints| hints.values().any(|hint| hint.get("tool").is_some()));
        forms.insert(tool, lead);
    }
    let kinds = forms.values().collect::<std::collections::BTreeSet<_>>();
    assert_eq!(kinds.len(), 1, "{forms:?}");
    runtime.close().await;
}
