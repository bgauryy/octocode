//! GitHub images through the shared capture cache, and the provider's model and long-context contract.

use super::{png, provider, requests, row, runtime};
use crate::runtime_clasify::clasify;
use crate::support::{Run as _, Workspace};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::json;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn github_image_reuses_shared_capture_cache_and_resolved_commit() {
    let server = provider().await;
    let github = MockServer::builder().start().await;
    let sha = "1111111111111111111111111111111111111111";
    let other_sha = "2222222222222222222222222222222222222222";
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/main"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"sha":sha})))
        .mount(&github)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/other"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"sha":other_sha})))
        .mount(&github)
        .await;
    let mut other_image = png();
    other_image.push(0);
    Mock::given(method("GET")).and(path("/api/v3/repos/a/b/contents/image.png")).and(query_param("ref", other_sha))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"type":"file","encoding":"base64","content":STANDARD.encode(&other_image),"size":other_image.len(),"sha":other_sha,"path":"image.png"})))
        .expect(1).mount(&github).await;
    Mock::given(method("GET")).and(path("/api/v3/repos/a/b/contents/image.png")).and(query_param("ref", sha))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"type":"file","encoding":"base64","content":STANDARD.encode(png()),"size":png().len(),"sha":sha,"path":"image.png"})))
        .expect(1).mount(&github).await;
    let workspace = Workspace::new();
    let runtime = runtime(
        &workspace,
        &server,
        &[("GITHUB_API_URL", format!("{}/api/v3", github.uri()))],
    );
    let mut query = row("unused");
    query["assets"]["image_a"]["source"] = json!({"tool":"ghGetFileContent","query":{"owner":"a","repo":"b","path":"image.png","ref":"main"}});
    let out = clasify(&runtime, vec![query.clone()]).await;
    assert_eq!(
        out["results"][0]["data"]["assets"]["image_a"]["source"]["query"]["ref"], sha,
        "{out}"
    );
    let again = clasify(&runtime, vec![query.clone()]).await;
    assert_eq!(again["results"][0]["data"]["replayed"], true);
    let text = runtime
        .run(
            "text".into(),
            "ghGetFileContent".into(),
            json!({"queries":[{
                "owner":"a","repo":"b","path":"image.png","ref":sha,"ranges":[{"startLine":1,"endLine":5}]
            }]}),
        )
        .await
        .unwrap()
        .structured_content;
    assert_eq!(
        text["results"][0]["data"]["errorCode"], "invalidInput",
        "{text}"
    );
    assert!(!text.to_string().contains(&STANDARD.encode(png())));
    query["assets"]["image_a"]["source"]["query"]["ref"] = json!("other");
    let other = clasify(&runtime, vec![query]).await;
    assert_eq!(
        other["results"][0]["data"]["assets"]["image_a"]["source"]["query"]["ref"], other_sha,
        "{other}"
    );
    assert_ne!(
        other["results"][0]["data"]["assets"]["image_a"]["snapshot"],
        out["results"][0]["data"]["assets"]["image_a"]["snapshot"]
    );
    assert_eq!(requests(&server).await.len(), 2);
    runtime.close().await;
}

#[tokio::test]
async fn models_and_long_context_follow_provider_contract() {
    let server = provider().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"models":[{"name":"jev-preview","description":"vision","release_date":"2026-09-10T18:39:06.057655+00:00"}]})),
        )
        .expect(1)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = runtime(&workspace, &server, &[]);
    let out = clasify(
        &runtime,
        vec![
            json!({"operation":"models"}),
            json!({"operation":"evaluate","content":"x".repeat(40_000),"longContext":true,
        "questions":{"q":{"type":"noul","instructions":"Is text present?"}}}),
        ],
    )
    .await;
    assert_eq!(
        out["results"][0]["data"]["models"][0]["name"], "jev-preview",
        "{out}"
    );
    assert_eq!(
        out["results"][1]["data"]["answers"]["q"]["noul"], 0.9,
        "{out}"
    );
    let sent = requests(&server).await;
    let posted = sent
        .iter()
        .find(|request| request.method == "POST")
        .unwrap();
    assert_eq!(
        posted
            .headers
            .get("typesafe-danger-zone-long-context")
            .unwrap(),
        "true"
    );
    runtime.close().await;
}
