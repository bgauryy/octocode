//! A host that exits cannot promise a later pinned read from memory storage:
//! it refuses a section plan; the next call can read the known source afresh.
use super::{provider, provider_calls, separate_functions};
use crate::support::{Workspace, clasify_runtime};
use octocode_native::runtime::{Channel, ToolRuntime};
use octocode_test_support::json_fixture;
use serde_json::{Value, json};

fn questions() -> Value {
    json_fixture(
        r#"{
            "q": {"type": "noul", "instructions": "Does this say restart?"}
        }"#,
    )
}

fn source(path: &str) -> Value {
    json!({"tool":"localFetch","query":{"path":path}})
}

async fn exiting_call(runtime: &ToolRuntime, queries: Vec<Value>) -> Value {
    let admission = runtime
        .admit("exiting-clasify".into())
        .unwrap()
        .requiring_persistent_continuations();
    let outcome = runtime
        .execute(
            admission,
            "clasify".into(),
            json!({"queries":queries}),
            Channel::Cli,
        )
        .await
        .expect("row refusals remain valid tool responses");
    crate::support::checks::assert_output_contract("clasify", &outcome.structured_content);
    outcome.structured_content.into_value()
}

fn refused(row: &Value) {
    assert_eq!(row["status"], "error", "{row:#}");
    assert_eq!(
        row["data"]["errorCode"], "classificationContextUnavailable",
        "{row:#}"
    );
    assert!(
        row["data"]["error"]
            .as_str()
            .unwrap()
            .contains("no source was read"),
        "{row:#}"
    );
    assert!(
        row["data"]["hints"]
            .as_array()
            .unwrap()
            .iter()
            .any(|hint| hint.as_str().unwrap().contains("storage.mode persistent")),
        "{row:#}"
    );
    assert!(
        row["data"].get("usage").is_none(),
        "no provider usage: {row:#}"
    );
    assert!(
        !row.to_string().contains("\"cursor\""),
        "no dead handles: {row:#}"
    );
}

#[tokio::test]
async fn exiting_memory_clasify_refuses_section_plans_before_capture_or_billing() {
    let server = provider().await;
    let workspace = Workspace::new();
    workspace.write("simple.rs", "pub fn restart() {}\n");
    workspace.write("many.rs", separate_functions(70));
    let runtime = clasify_runtime(
        &workspace,
        &server,
        &[
            ("OCTOCODE_STORAGE_MODE", "memory".into()),
            ("GITHUB_API_URL", format!("{}/api/v3", server.uri())),
        ],
    );
    let sources = [
        source("simple.rs"),
        source("many.rs"),
        source("never-created.rs"),
        json_fixture(
            r#"{
                "tool": "ghGetFileContent",
                "query": {
                    "owner": "octo",
                    "repo": "demo",
                    "path": "source.rs",
                    "ref": "main"
                }
            }"#,
        ),
    ];
    for source in sources {
        let rows = vec![
            json!({"operation":"sections","maxRequests":crate::runtime_clasify::budget::EVERY_PAGE,"resources":{"source":source},"questions":questions()}),
        ];
        let output = exiting_call(&runtime, rows).await;
        let rows = output["results"].as_array().unwrap();
        assert_eq!(rows.len(), 1);
        rows.iter().for_each(refused);
    }
    assert!(
        server.received_requests().await.unwrap().is_empty(),
        "neither source GETs nor provider POSTs"
    );
    runtime.close().await;
}

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

/// A one-shot memory runtime keeps nothing for a later call, so an answered
/// source evaluation offers no handle into this runtime. The next CLI process
/// reads the requested source at its known path and immutable GitHub ref.
#[tokio::test]
async fn a_one_shot_memory_evaluation_can_be_verified_in_the_next_runtime() {
    use base64::Engine as _;
    let server = provider().await;
    octocode_test_support::mock_http::list_in_trees(&server, "octo", "demo", "source.rs").await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/api/v3/repos/octo/demo/contents/source.rs",
        ))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({
            "type":"file","encoding":"base64","sha":"blob",
            "content":base64::engine::general_purpose::STANDARD.encode("pub fn restart_remote() {}\n")
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    workspace.write("restart.rs", "pub fn restart() {}\n");
    let env = [
        ("OCTOCODE_STORAGE_MODE", "memory".into()),
        ("GITHUB_API_URL", format!("{}/api/v3", server.uri())),
    ];
    let cases = [
        (source("restart.rs"), "pub fn restart() {}"),
        (
            json!({"tool":"ghGetFileContent","query":{"owner":"octo","repo":"demo","path":"source.rs","ref":SHA}}),
            "pub fn restart_remote() {}",
        ),
    ];
    for (source, text) in cases {
        let first = clasify_runtime(&workspace, &server, &env);
        let output = exiting_call(
            &first,
            vec![json!({"operation":"evaluate","source":source,"questions":questions()})],
        )
        .await;
        first.close().await;
        let data = &output["results"][0]["data"];
        assert_eq!(data["answers"]["q"]["noul"], 0.9, "{output:#}");
        assert!(
            data["next"].get("read").is_none(),
            "no dead handle: {data:#}"
        );
        assert!(!output.to_string().contains("\"cursor\""), "{output:#}");
        assert!(
            data.get("related").is_none(),
            "no optional advice: {data:#}"
        );
        let read = json!({"tool":source["tool"],"input":{"queries":[source["query"]]}});
        let reread = &read["input"]["queries"][0];
        if source["tool"] == "ghGetFileContent" {
            assert_eq!(reread["ref"], SHA, "the captured commit: {read:#}");
        } else {
            assert!(reread.get("snapshot").is_none(), "{read:#}");
        }
        assert!(
            !data.to_string().contains("\"sections\""),
            "no section plan this runtime refuses: {data:#}"
        );
        let second = clasify_runtime(&workspace, &server, &env);
        let admission = second
            .admit("next-cli-call".into())
            .unwrap()
            .requiring_persistent_continuations();
        let fetched = second
            .execute(
                admission,
                read["tool"].as_str().unwrap().into(),
                read["input"].clone(),
                Channel::Cli,
            )
            .await
            .expect("the fresh read runs in a new runtime")
            .structured_content
            .into_value();
        assert!(fetched.to_string().contains(text), "{fetched:#}");
        second.close().await;
    }
}

/// A source larger than one response pages, which the next one-shot memory
/// read refuses. The evaluation discloses that requirement:
/// it needs `storage.mode persistent` or one MCP server. A source that fits
/// one response carries no such warning, and its read completes.
#[tokio::test]
async fn a_one_shot_fresh_read_larger_than_one_response_says_it_needs_saved_continuations() {
    let server = provider().await;
    let workspace = Workspace::new();
    workspace.write("small.rs", "pub fn restart() {}\n");
    workspace.write("large.rs", separate_functions(10));
    let env = [("OCTOCODE_STORAGE_MODE", "memory".into())];
    for (path, pages) in [("small.rs", false), ("large.rs", true)] {
        let first = clasify_runtime(&workspace, &server, &env);
        let output = exiting_call(
            &first,
            vec![json!({"operation":"evaluate","source":source(path),"questions":questions()})],
        )
        .await;
        first.close().await;
        let data = &output["results"][0]["data"];
        assert_eq!(data["answers"]["q"]["noul"], 0.9, "{output:#}");
        let read = json!({"input":{"queries":[source(path)["query"]]}});
        let hinted = data["warnings"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .any(|hint| {
                hint.contains("larger than one response")
                    && hint.contains("storage.mode persistent")
                    && hint.contains("MCP")
            });
        assert_eq!(hinted, pages, "{path}: {data:#}");
        let second = clasify_runtime(&workspace, &server, &env);
        let admission = second
            .admit("next-cli-call".into())
            .unwrap()
            .requiring_persistent_continuations();
        let fetched = second
            .execute(
                admission,
                "localFetch".into(),
                read["input"].clone(),
                Channel::Cli,
            )
            .await;
        second.close().await;
        // The hint is exactly when the read cannot complete in this mode.
        assert_eq!(fetched.is_err(), pages, "{path}: {:?}", fetched.err());
    }
}

#[tokio::test]
async fn exiting_memory_clasify_keeps_literal_siblings_and_long_lived_source_reads() {
    let server = provider().await;
    let workspace = Workspace::new();
    workspace.write("restart.rs", "pub fn restart() {}\n");
    let runtime = clasify_runtime(
        &workspace,
        &server,
        &[("OCTOCODE_STORAGE_MODE", "memory".into())],
    );
    let literal =
        json!({"operation":"evaluate","content":"Restart the service.","questions":questions()});
    let evaluation =
        json!({"operation":"evaluate","source":source("restart.rs"),"questions":questions()});
    let sections = json!({"operation":"sections","maxRequests":crate::runtime_clasify::budget::EVERY_PAGE,"resources":{"source":source("restart.rs")},"questions":questions()});
    let output = exiting_call(&runtime, vec![sections, literal, evaluation.clone()]).await;
    refused(&output["results"][0]);
    for index in [1, 2] {
        assert_eq!(
            output["results"][index]["data"]["answers"]["q"]["noul"], 0.9,
            "{output:#}"
        );
    }
    assert_eq!(
        provider_calls(&server).await,
        2,
        "the literal and the source evaluation are judged; the plan is not"
    );

    // Request lifetime, not channel naming or storage mode alone, owns retention.
    for (index, channel) in [Channel::Cli, Channel::Mcp].into_iter().enumerate() {
        let outcome = runtime
            .execute(
                runtime.admit("living-clasify".into()).unwrap(),
                "clasify".into(),
                json!({"queries":[evaluation.clone()]}),
                channel,
            )
            .await
            .unwrap();
        crate::support::checks::assert_output_contract("clasify", &outcome.structured_content);
        let output = outcome.structured_content.into_value();
        assert_eq!(
            output["results"][0]["data"]["answers"]["q"]["noul"], 0.9,
            "{output:#}"
        );
        let next = &output["results"][0]["data"]["next"]["read"];
        assert_eq!(next.get("kind"), None, "{next:#}");
        assert!(next["input"]["cursor"].is_string(), "{next:#}");
        let read = runtime
            .execute(
                runtime.admit("read-capture".into()).unwrap(),
                "localFetch".into(),
                next["input"].clone(),
                channel,
            )
            .await
            .unwrap()
            .structured_content
            .into_value();
        assert!(read.to_string().contains("pub fn restart() {}"), "{read:#}");
        assert_eq!(
            provider_calls(&server).await,
            index + 3,
            "following the exact read never bills again"
        );
    }
    runtime.close().await;
}

/// Every read of a section plan restarts its idle lifetime: reading a
/// response page of a batch keeps the plan its rows continue, so the batch's
/// next page still answers past 30 minutes from the batch (CLS-C29).
#[tokio::test]
async fn reading_a_response_page_keeps_the_plan_its_batch_continues() {
    let server = provider().await;
    let workspace = Workspace::new();
    workspace.write("many.rs", separate_functions(70));
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let plans = workspace.home.join("tmp/section-assessments");
    let age = |by: std::time::Duration| {
        for entry in std::fs::read_dir(&plans).unwrap() {
            let path = entry.unwrap().path();
            let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
            std::fs::File::options()
                .write(true)
                .open(&path)
                .unwrap()
                .set_modified(modified - by)
                .unwrap();
        }
    };
    let row = json!({"operation":"sections","maxRequests":crate::runtime_clasify::budget::EVERY_PAGE,"resources":{"file":source("many.rs")},"questions":questions()});
    let mut page = crate::support::Run::run(
        &runtime,
        "plan-first".into(),
        "clasify".into(),
        json!({"queries":[row],"responseChars":2_500}),
    )
    .await
    .expect("first page")
    .structured_content
    .into_value();
    age(std::time::Duration::from_secs(25 * 60));
    // Each saved page is read 10 minutes after the last read: the plan lives
    // only if every read restarts its lifetime. The first call that judges
    // again is the batch's next page.
    let mut pages = 0;
    loop {
        let next = crate::support::next_input(&page)
            .unwrap_or_else(|| panic!("the batch continues: {page:#}"));
        if pages > 0 {
            age(std::time::Duration::from_secs(10 * 60));
        }
        let calls = provider_calls(&server).await;
        page = crate::support::Run::run(&runtime, format!("plan-{pages}"), "clasify".into(), next)
            .await
            .expect("next page")
            .structured_content
            .into_value();
        if page["replayed"] == true {
            pages += 1;
            continue;
        }
        assert_ne!(
            page["results"][0]["data"]["errorCode"], "classificationContextUnavailable",
            "the plan lives 30 minutes from its last read: {page:#}"
        );
        assert!(
            provider_calls(&server).await > calls,
            "the next batch is judged"
        );
        break;
    }
    assert!(pages > 1, "the batch pages over saved pages: {page:#}");
    runtime.close().await;
}
