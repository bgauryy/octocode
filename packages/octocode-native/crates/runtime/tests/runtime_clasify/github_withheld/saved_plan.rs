//! A sections plan is saved with its section text for 30 minutes under a key
//! the rule did not change, so a binary with the rule can open one saved before
//! it. Such a plan sends nothing more, and its copy is dropped.

use super::{MARKER, files_holding, github, provider, source};
use crate::runtime_clasify::{noul, sent};
use crate::runtime_clasify_sections::separate_functions;
use crate::support::{Run as _, Workspace, clasify_runtime, github_api};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use octocode_native::runtime::ToolRuntime;
use serde_json::{Value, json};
use wiremock::matchers::{method, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// `docs/big.txt`, large enough for several assessment batches.
async fn big_file(github: &MockServer) {
    let body = separate_functions(70) + MARKER;
    Mock::given(method("GET"))
        .and(path_regex("/contents/docs%2Fbig\\.txt$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "type":"file","encoding":"base64","content":STANDARD.encode(&body),
            "size":body.len(),"sha":"2".repeat(40),"path":"docs/big.txt"
        })))
        .with_priority(1)
        .mount(github)
        .await;
}

async fn first_page(runtime: &ToolRuntime, resources: Value) -> Value {
    runtime
        .run(
            "plan".into(),
            "clasify".into(),
            json!({"queries":[{"operation":"sections","maxRequests":crate::runtime_clasify::budget::EVERY_PAGE,"resources":resources,
                "questions":{"q":noul("Does this section say to restart?")}}]}),
        )
        .await
        .expect("first batch")
        .structured_content
        .into_value()
}

/// What a binary without the rule saved for `docs/big.pem`: the same plan under
/// a credential name of the same length, so every stored offset still holds.
fn as_if_saved_before_the_rule(workspace: &Workspace) {
    let dir = workspace.home.join("tmp/section-assessments");
    let mut rewritten = 0;
    for entry in std::fs::read_dir(dir).expect("saved plans").flatten() {
        let bytes = std::fs::read(entry.path()).expect("plan");
        let text = String::from_utf8_lossy(&bytes);
        if entry.path().extension().is_some_and(|ext| ext == "entry")
            && text.contains("docs/big.txt")
        {
            std::fs::write(entry.path(), text.replace("docs/big.txt", "docs/big.pem"))
                .expect("rewrite");
            rewritten += 1;
        }
    }
    assert!(rewritten > 0, "the plan must be on disk");
}

#[tokio::test]
async fn a_plan_saved_before_the_rule_sends_nothing_more_and_is_dropped() {
    let (provider, github) = (provider().await, github().await);
    big_file(&github).await;
    let workspace = Workspace::new();
    let settings = github_api(&github);
    let runtime = clasify_runtime(&workspace, &provider, &settings);
    let first = first_page(&runtime, json!({"big":source("docs/big.txt")})).await;
    let resume = first["results"][0]["data"]["next"]["nextPage"]["input"].clone();
    assert!(!resume.is_null(), "the plan must take several batches");
    runtime.close().await;
    let judged = sent(&provider).await.len();

    as_if_saved_before_the_rule(&workspace);
    let runtime = clasify_runtime(&workspace, &provider, &settings);
    let out = runtime
        .run("resume".into(), "clasify".into(), resume)
        .await
        .expect("resume")
        .structured_content;
    assert_eq!(
        sent(&provider).await.len(),
        judged,
        "a retained credential-named section was sent: {out}"
    );
    assert_eq!(
        out["results"][0]["data"]["errorCode"], "pathPolicyDenied",
        "{out}"
    );
    runtime.close().await;
    let saved = workspace.home.join("tmp/section-assessments");
    assert_eq!(
        files_holding(&saved, MARKER),
        Vec::<String>::new(),
        "the plan's copy of the file is dropped"
    );
}

/// A plan of this binary holds no text of a refused resource: its row stays,
/// and the sibling's next batches are judged.
#[tokio::test]
async fn a_refused_resource_does_not_stop_its_siblings_next_batch() {
    let (provider, github) = (provider().await, github().await);
    big_file(&github).await;
    let workspace = Workspace::new();
    let runtime = clasify_runtime(&workspace, &provider, &github_api(&github));
    let first = first_page(
        &runtime,
        json!({"bad":source(".env"),"big":source("docs/big.txt")}),
    )
    .await;
    let data = &first["results"][0]["data"];
    assert_eq!(
        data["resources"]["bad"]["error"]["errorCode"], "pathPolicyDenied",
        "{first}"
    );
    let judged = sent(&provider).await.len();
    let out = runtime
        .run(
            "resume".into(),
            "clasify".into(),
            data["next"]["nextPage"]["input"].clone(),
        )
        .await
        .expect("resume")
        .structured_content;
    assert!(
        out["results"][0]["data"].get("errorCode").is_none(),
        "{out}"
    );
    assert!(sent(&provider).await.len() > judged, "{out}");
    runtime.close().await;
}
