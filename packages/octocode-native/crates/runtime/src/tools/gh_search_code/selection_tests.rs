//! Classification must see captured evidence before host projection, and only
//! selected files may supply ordinary read leads. GitHub is mocked; the judge
//! records the actual sanitized state and makes deterministic decisions.
use super::{GhSearchCodeQuery, execute};
use crate::id::ToolId;
use crate::security::scan::ContentScan;
use crate::tools::gh_shared::test_support::{default_context, one_shot_provider};
use crate::tools::selection::{Decision, JudgeCall, Runtime, SelectionJudge, apply_sanitized_rows};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use octocode_contract_types::ErrorCode;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

type Recorded = Arc<Mutex<Vec<Vec<Value>>>>;

struct RecordingJudge(Recorded);

impl SelectionJudge for RecordingJudge {
    fn identity(&self) -> Value {
        json!({"test":"record-sanitized-github-candidates"})
    }

    fn select(&self, call: JudgeCall<'_>) -> crate::tools::selection::Judgments {
        let states = call
            .admitted
            .iter()
            .map(|&ordinal| {
                call.candidates[ordinal]
                    .state
                    .clone()
                    .expect("admitted candidate state")
            })
            .collect::<Vec<_>>();
        self.0.lock().expect("recording lock").push(states.clone());
        Ok(call
            .admitted
            .iter()
            .copied()
            .zip(states)
            .map(|(ordinal, state)| {
                let decision = if state["evidence"]["path"] == "src/selected.rs" {
                    Decision::Selected
                } else {
                    Decision::Excluded
                };
                (ordinal, decision)
            })
            .collect())
    }
}

struct TestRedaction;

impl ContentScan for TestRedaction {
    fn sanitize(
        &self,
        text: &str,
        _: &std::path::Path,
    ) -> Result<(String, Vec<String>), (ErrorCode, String)> {
        Ok((
            text.replace("PRIVATE_SENTINEL", "[REDACTED_FOR_TEST]"),
            vec![],
        ))
    }
}

fn runtime() -> (Runtime, Recorded) {
    let recorded = Arc::new(Mutex::new(Vec::new()));
    let runtime = crate::tools::test_support::selection_fake::runtime(
        RecordingJudge(recorded.clone()),
        tokio::runtime::Handle::current(),
        Duration::from_secs(5),
    );
    (runtime, recorded)
}

async fn fixture() -> MockServer {
    let server = MockServer::builder().start().await;
    let item = |name: &str| {
        json!({"name":name,"path":format!("src/{name}"),"sha":"1","html_url":"https://x",
            "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"}})
    };
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count":100,"incomplete_results":false,
            "items":[item("excluded.rs"),item("selected.rs")]
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/HEAD"))
        .respond_with(ResponseTemplate::new(200).set_body_string(SHA))
        .mount(&server)
        .await;
    for name in ["excluded.rs", "selected.rs"] {
        let body = format!("wrap_app(); // {name} PRIVATE_SENTINEL\n").repeat(8);
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/a/b/contents/src%2F{name}")))
            .and(wiremock::matchers::query_param("ref", SHA))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "type":"file","encoding":"base64","content":STANDARD.encode(body)
            })))
            .mount(&server)
            .await;
    }
    server
}

fn query() -> Value {
    json!({"owner":"a","repo":"b","keywords":["wrap_app"],"classification":{"predicates":["Does this evidence show the selected behavior?"]}})
}

async fn capture(server: &MockServer, query: &Value, runtime: &Runtime) -> Value {
    let typed: GhSearchCodeQuery = serde_json::from_value(query.clone()).expect("query");
    let output = execute(
        &one_shot_provider(server),
        &typed,
        &default_context(),
        &TestRedaction,
    )
    .await
    .expect("mocked GitHub search");
    let row = crate::response::rows::result_row(0, output.data, output.status);
    crate::security::ContentSecurity::new()
        .seal_output(json!({"results":[row]}), false, || runtime.context.check())
        .expect("shared output sanitization")
        .into_value()
}

fn select(mut structured: Value, query: &Value, runtime: &Runtime) -> (Value, Value) {
    let mut response_query = query.clone();
    apply_sanitized_rows(
        ToolId::GhSearchCode,
        ToolId::GhSearchCode.output(),
        &mut structured,
        &[crate::security::CheckedQuery::checked(query.clone())],
        &[0],
        &mut response_query,
        runtime,
    )
    .expect("select sanitized page");
    (structured, response_query)
}

#[tokio::test]
async fn classification_reads_sanitized_evidence_and_replay_preserves_it() {
    let server = fixture().await;
    let (runtime, recorded) = runtime();
    let picked = query();
    let ordinary_projection = capture(&server, &picked, &runtime).await;
    let (selected, _) = select(ordinary_projection, &picked, &runtime);
    let data = &selected["results"][0]["data"];
    assert_eq!(data["selection"]["selectedItems"], 1);
    assert_eq!(data["selection"]["excludedItems"], 1);
    assert_eq!(data["selection"]["unknownItems"], 0);
    assert_eq!(data["files"].as_array().map(Vec::len), Some(1));
    assert_eq!(data["files"][0]["path"], "src/selected.rs");
    assert!(data["files"][0]["lines"].is_array());
    assert!(data["files"][0]["matches"].is_array());

    let broad = query();
    let (unprojected, _) = select(capture(&server, &broad, &runtime).await, &broad, &runtime);
    assert_eq!(
        unprojected["results"][0]["data"]["selection"],
        data["selection"]
    );
    {
        let calls = recorded.lock().expect("recorded states");
        assert_eq!(
            calls.len(),
            2,
            "both calls must classify, not silently become unknown"
        );
        assert_eq!(
            calls[0], calls[1],
            "repeating the same query preserves judged evidence"
        );
        assert_eq!(calls[0].len(), 2);
        let state = serde_json::to_string(&calls[0]).expect("states");
        assert!(state.contains("[REDACTED_FOR_TEST]"));
        assert!(!state.contains("PRIVATE_SENTINEL"));
    }

    let recovery = data["next"]["unfiltered"]["input"]["queries"][0].clone();
    assert!(recovery.get("classification").is_none());
    assert!(recovery.get("selection").is_none());
    let (recovered, _) = select(
        capture(&server, &recovery, &runtime).await,
        &recovery,
        &runtime,
    );
    let files = recovered["results"][0]["data"]["files"]
        .as_array()
        .expect("recovered files");
    let mut paths = files
        .iter()
        .flat_map(|file| {
            std::iter::once(file["path"].as_str().expect("file path")).chain(
                file["alsoAt"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|path| path.as_str().expect("equivalent path")),
            )
        })
        .collect::<Vec<_>>();
    paths.sort_unstable();
    assert_eq!(paths, ["src/excluded.rs", "src/selected.rs"]);
    assert!(
        files
            .iter()
            .all(|file| file["lines"].is_array() && file["matches"].is_array())
    );
    assert_eq!(recorded.lock().expect("recorded states").len(), 2);
}

#[tokio::test]
async fn complete_hit_rows_and_optional_reads_do_not_bypass_selection() {
    let server = fixture().await;
    let (runtime, recorded) = runtime();
    let query = query();
    let captured = capture(&server, &query, &runtime).await;
    let original = &captured["results"][0]["data"];
    assert_eq!(
        crate::tools::test_support::checks::related_call(original, "ghGetFileContent", |_| true)["input"]
            ["queries"][0]["path"],
        "src/excluded.rs"
    );
    assert!(original["next"].get("readHits").is_none());
    assert!(original["next"].get("readHits2").is_none());
    let (selected, replay_query) = select(captured.clone(), &query, &runtime);
    let data = &selected["results"][0]["data"];
    assert_eq!(data["selection"]["selectedItems"], 1);
    assert_eq!(data["selection"]["excludedItems"], 1);
    assert!(
        crate::tools::test_support::checks::find_related_call(data, "ghGetFileContent", |input| {
            input["queries"][0]["path"] == "src/excluded.rs"
        })
        .is_none(),
        "excluded top hit must not leak through optional reads"
    );
    assert!(data["next"].get("readHits").is_none());
    assert!(data["next"].get("readHits2").is_none());
    assert_eq!(data["files"].as_array().expect("selected files").len(), 1);
    assert_eq!(data["files"][0]["path"], "src/selected.rs");
    assert_eq!(
        data["files"][0]["lines"]
            .as_array()
            .expect("complete hit lines")
            .len(),
        8
    );

    let provider_next = &data["next"]["nextPage"]["input"]["queries"][0];
    assert_eq!(provider_next["page"], 2);
    assert_eq!(provider_next["classification"], query["classification"]);
    assert!(
        provider_next.get("selection").is_none(),
        "a new provider page is a new candidate set"
    );
    assert!(
        replay_query["selection"].as_str().is_some(),
        "response paging owns the saved plan"
    );
    let (replayed, _) = select(captured, &replay_query, &runtime);
    assert_eq!(replayed, selected);
    assert_eq!(
        recorded.lock().expect("recorded states").len(),
        1,
        "response replay must not judge again"
    );

    let recovery = data["next"]["unfiltered"]["input"]["queries"][0].clone();
    let (recovered, _) = select(
        capture(&server, &recovery, &runtime).await,
        &recovery,
        &runtime,
    );
    let recovered = &recovered["results"][0]["data"];
    assert_eq!(recovered["files"].as_array().map(Vec::len), Some(2));
    assert!(recovered["files"][0]["lines"].as_array().is_some());
    assert!(recovered["next"].get("readHits").is_none());
    assert!(recovered["next"].get("readHits2").is_none());
    assert!(
        recovered["files"]
            .as_array()
            .expect("files")
            .iter()
            .all(|file| file["lines"].as_array().expect("complete lines").len() == 8)
    );
    assert_eq!(recorded.lock().expect("recorded states").len(), 1);
}

#[test]
fn selected_equivalent_paths_keep_their_reads_without_admitting_other_repositories() {
    let read = |repo: &str, path: &str| {
        json!({"kind":"continue","tool":"ghGetFileContent","input":{"queries":[{
            "owner":"o","repo":repo,"path":path,"ref":SHA,"ranges":[{"startLine": 6, "endLine": 8}]
        }]}})
    };
    let primary = read("r", "src/main.rs");
    let alias = read("r", "src/copy.rs");
    let mut page = json!({
        "files":[{"owner":"o","repo":"r","path":"src/main.rs","alsoAt":["src/copy.rs"],"lines":["1\twrap_app();"]}],
        "next":{"readHits":alias.clone(),"otherRepo":read("other", "src/copy.rs"),"read":primary.clone(),"excluded":read("r", "src/excluded.rs")}
    });
    super::finish_code_selection(&mut page, &json!({"owner":"o"}), &[]);
    assert_eq!(page["next"]["readHits"], alias);
    assert_eq!(page["next"]["read"], primary);
    assert!(page["next"].get("otherRepo").is_none());
    assert!(page["next"].get("excluded").is_none());
    assert_eq!(page["files"][0]["alsoAt"], json!(["src/copy.rs"]));
    assert!(page["files"][0]["lines"].is_array());
}

#[test]
fn a_hidden_file_with_unlisted_hits_gets_a_judged_read_and_an_excluded_one_none() {
    let read = |path: &str| {
        json!({"kind":"continue","tool":"ghGetFileContent","input":{"queries":[{
            "owner":"o","repo":"r","path":path,"ref":SHA,"ranges":[{"startLine": 9, "endLine": 40}],
            "contextLines":{"before":0,"after":0},"matchString":["Detection"]
        }]}})
    };
    let context = json!({"predicates":["Does this code quit on a NUL byte?"]});
    let mut page = json!({
        "files":[{"owner":"o","repo":"r","path":"src/shown.rs","lines":["1\tquit()"]}],
        "next":{"readHits":read("src/hidden.rs"),"readHits2":read("src/excluded.rs")}
    });
    let hidden = [
        json!({"owner":"o","repo":"r","path":"src/hidden.rs","lines":["3\tDetection"],"matchCount":14}),
    ];
    super::finish_code_selection(
        &mut page,
        &json!({"owner":"o","repo":"r","classification":context}),
        &hidden,
    );
    // Neither capped read survives: only selected files keep plain reads.
    assert!(page["next"].get("readHits").is_none());
    assert!(page["next"].get("readHits2").is_none());
    assert_eq!(
        crate::tools::test_support::checks::related_call(&page, "ghGetFileContent", |input| input
            ["queries"][0]
            .get("classification")
            .is_some())["input"]["queries"],
        json!([{"owner":"o","repo":"r","path":"src/hidden.rs","ref":SHA,
            "matchString":["Detection"],"classification":context}])
    );
    assert_eq!(
        crate::tools::test_support::checks::related_call(&page, "ghGetFileContent", |input| input
            ["queries"][0]
            .get("classification")
            .is_some())["tool"],
        "ghGetFileContent"
    );
}
