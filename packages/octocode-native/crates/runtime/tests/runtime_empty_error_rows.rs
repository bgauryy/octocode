//! Empty and failed tool executions retain stable rows, typed errors and exit status.

use crate::support::{self, PROVIDER_LIMITS, Run as _, Workspace};
use octocode_native::response::exit::ExitClass;
use octocode_native::response::outcome::ToolOutcome;
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
    failures
}

async fn github_and_registry() -> MockServer {
    let server = MockServer::builder().start().await;
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
    let runtime = workspace.runtime_with(
        &[
            ("GITHUB_API_URL", format!("{}/api/v3", server.uri())),
            ("OCTOCODE_ALLOW_PRIVATE_REGISTRY", "true".into()),
            ("OCTOCODE_BETA", "true".into()),
            ("OCTOCODE_CLASSIFICATION_API", "b2-secret".into()),
            ("OCTOCODE_CLASSIFICATION_API_HOST", server.uri()),
        ],
        PROVIDER_LIMITS,
    );
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
            json!({"path":"src","matchString":[NEVER]}),
        ),
        probe(
            "localSearch",
            "error",
            json!({"path":"missing","matchString":["x"]}),
        ),
        probe("localFetch", "error", json!({"path":"missing.rs"})),
        probe(
            "structureSearch",
            "empty",
            json!({"path":"src","operation":"files","include":[NEVER]}),
        ),
        probe("structureSearch", "error", json!({"path":"missing"})),
        probe(
            "astSearch",
            "empty",
            json!({"operation":"symbols","path":"src","symbolFilter":{"terms":[NEVER],"mode":"exact"}}),
        ),
        probe(
            "astSearch",
            "error",
            json!({"operation":"symbols","path":"missing"}),
        ),
        probe(
            "lspSearch",
            "error",
            json!({"path":"missing.rs","symbolName":"x","line":1,"operation":"definition"}),
        ),
        probe(
            "ghSearchRepo",
            "empty",
            json!({"operation":"search","keywords":[NEVER]}),
        ),
        probe(
            "ghSearchCode",
            "empty",
            with(&gh, json!({"keywords":[NEVER]})),
        ),
        probe(
            "ghStructure",
            "error",
            with(&missing, json!({"operation":"tree"})),
        ),
        probe(
            "ghGetFileContent",
            "error",
            with(&missing, json!({"path":"missing.md"})),
        ),
        probe(
            "ghSearchHistory",
            "empty",
            with(&gh, json!({"operation":"pullRequest","keywords":[NEVER]})),
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
            json!({"operation":"resolve","ecosystem":"npm","packageName":NEVER.to_lowercase(),"registryUrl":server.uri()}),
        ),
        probe(
            "clasify",
            "error",
            json!({"operation":"evaluate","source":{"tool":"localFetch","query":{"path":"missing.rs"}},
                "questions":{"q":{"type":"noul","instructions":"Is it there?"}}}),
        ),
    ];
    let mut failures = Vec::new();
    for probe in &probes {
        let outcome = support::run(&runtime, probe.tool, probe.row.clone()).await;
        failures.extend(check(probe, &outcome));
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    runtime.close().await;
}

/// Every missing-path recovery can execute within the nearest existing scope.
#[tokio::test]
async fn a_missing_path_gets_one_recovery_form_on_every_local_tool() {
    let workspace = Workspace::new();
    workspace.write("src/a.rs", "fn alpha() {}\n");
    let runtime = workspace.runtime(&[("OCTOCODE_BETA", "true".into())]);
    for (tool, row) in [
        (
            "localSearch",
            json!({"path":"src/missing","matchString":["x"]}),
        ),
        ("localFetch", json!({"path":"src/missing.rs"})),
        ("structureSearch", json!({"path":"src/missing"})),
        (
            "lspSearch",
            json!({"path":"src/missing.rs","symbolName":"x","line":1,"operation":"definition"}),
        ),
    ] {
        let outcome = support::run(&runtime, tool, row).await;
        let data = outcome
            .structured_content
            .pointer("/results/0/data")
            .cloned()
            .unwrap_or_default();
        assert_eq!(data["errorCode"], "pathNotFound", "{tool}: {data}");
        let leads = data["related"].as_array().expect("related recovery");
        let lead = leads
            .iter()
            .find(|call| call["tool"] == "structureSearch")
            .expect("parent listing");
        assert_eq!(lead["kind"], "related");
        let listing = runtime
            .run(
                format!("recover-{tool}"),
                "structureSearch".into(),
                lead["input"].clone(),
            )
            .await
            .expect("scoped recovery executes");
        assert!(
            listing.structured_content.to_string().contains("a.rs"),
            "{}",
            listing.structured_content
        );
    }
    runtime.close().await;
}
