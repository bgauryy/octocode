//! GitHub images captured fresh per request, and the provider's model and long-context contract.

use super::{png, provider, requests, row, runtime};
use crate::runtime_clasify::clasify;
use crate::support::{Run as _, Workspace};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use octocode_test_support::json_fixture;
use serde_json::json;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// An image asset passes the same name rule as any other GitHub source: the
/// row is refused before any fetch, and an ordinary image in the call is judged.
#[tokio::test]
async fn a_credential_named_github_image_is_withheld_before_any_fetch() {
    use crate::runtime_clasify::github_withheld::{MARKER, contents_reads, files_holding, github};
    let server = provider().await;
    let github = github().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/octo/demo/contents/photo.png"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"type":"file",
            "encoding":"base64","content":STANDARD.encode(png()),"size":png().len(),
            "sha":"2".repeat(40),"path":"photo.png"})),
        )
        .with_priority(1)
        .mount(&github)
        .await;
    let workspace = Workspace::new();
    let image = |file: &str| {
        let mut query = row("unused");
        query["assets"]["image_a"]["source"] = json!({"tool":"ghGetFileContent",
            "query":{"owner":"octo","repo":"demo","path":file}});
        query
    };
    for allow in ["withhold", "allow"] {
        let runtime = runtime(
            &workspace,
            &server,
            &[
                ("GITHUB_API_URL", format!("{}/api/v3", github.uri())),
                ("OCTOCODE_CREDENTIAL_FILES", allow.to_owned()),
            ],
        );
        for file in [".Env.Prod", "KEY.PEM", "a/.ssh/config", "server.pem"] {
            let out = clasify(&runtime, vec![image(file), image("photo.png")]).await;
            assert_eq!(
                out["results"][0]["data"]["errorCode"], "pathPolicyDenied",
                "{file}: {out}"
            );
            assert!(
                out["results"][1]["data"].get("answers").is_some(),
                "{file}: {out}"
            );
        }
        runtime.close().await;
    }
    let reads = contents_reads(&github).await;
    assert!(reads.iter().all(|read| read.contains("photo")), "{reads:?}");
    assert!(
        requests(&server)
            .await
            .iter()
            .all(|request| !String::from_utf8_lossy(&request.body).contains(MARKER))
    );
    assert_eq!(files_holding(&workspace.home, MARKER), Vec::<String>::new());
}

/// A capture follows a repository symlink to its target and judges the target's
/// name: an image reached through a link to a credential-named file is refused
/// as the file is, before any of its bytes are asked for, and one reached
/// through a link to an ordinary image is judged.
#[tokio::test]
async fn a_github_image_behind_a_link_is_judged_by_its_target() {
    use crate::runtime_clasify::github_withheld::github;
    let server = provider().await;
    let github = github().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    // Answered as GitHub answers it: a contents read of a link to a file
    // is the target's bytes under the link's name, so the target is known
    // only from the tree and the link's own blob.
    octocode_test_support::mock_http::GitRepository::new()
        .file_bytes("photo.png", &png())
        .file_bytes("server.pem", &png())
        .link("ok-link.png", "photo.png")
        .link("bad-link.png", "server.pem")
        .mount(&github, "octo", "demo", sha)
        .await;
    let workspace = Workspace::new();
    let image = |file: &str| {
        let mut query = row("unused");
        query["assets"]["image_a"]["source"] = json!({"tool":"ghGetFileContent",
            "query":{"owner":"octo","repo":"demo","path":file,"ref":sha}});
        query
    };
    let runtime = runtime(
        &workspace,
        &server,
        &[("GITHUB_API_URL", format!("{}/api/v3", github.uri()))],
    );
    let out = clasify(&runtime, vec![image("bad-link.png"), image("ok-link.png")]).await;
    assert_eq!(
        out["results"][0]["data"]["errorCode"], "pathPolicyDenied",
        "{out}"
    );
    assert!(out["results"][1]["data"].get("answers").is_some(), "{out}");
    let asked = github.received_requests().await.unwrap_or_default();
    for never in ["server.pem", "bad-link.png", "ok-link.png"] {
        assert!(
            asked
                .iter()
                .all(|request| !request.url.path().ends_with(&format!("/contents/{never}"))),
            "{never}"
        );
    }
    runtime.close().await;
}

#[tokio::test]
async fn github_image_resolves_the_commit_and_pins_its_capture() {
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
    octocode_test_support::mock_http::list_in_trees(&github, "a", "b", "image.png").await;
    Mock::given(method("GET")).and(path("/api/v3/repos/a/b/contents/image.png")).and(query_param("ref", sha))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"type":"file","encoding":"base64","content":STANDARD.encode(png()),"size":png().len(),"sha":sha,"path":"image.png"})))
        // A commit's bytes never change: two clasify captures and the ranged read share one fetch.
        .expect(1).mount(&github).await;
    let workspace = Workspace::new();
    let runtime = runtime(
        &workspace,
        &server,
        &[("GITHUB_API_URL", format!("{}/api/v3", github.uri()))],
    );
    let mut query = row("unused");
    query["assets"]["image_a"]["source"] = json_fixture(
        r#"{
            "tool": "ghGetFileContent",
            "query": {
                "owner": "a",
                "repo": "b",
                "path": "image.png",
                "ref": "main"
            }
        }"#,
    );
    let out = clasify(&runtime, vec![query.clone()]).await;
    assert_eq!(
        out["results"][0]["data"]["assets"]["image_a"]["source"]["query"]["ref"], sha,
        "{out}"
    );
    let again = clasify(&runtime, vec![query.clone()]).await;
    assert!(again["results"][0]["data"].get("replayed").is_none());
    assert_eq!(
        again["results"][0]["data"]["assets"]["image_a"]["sourceDigest"],
        out["results"][0]["data"]["assets"]["image_a"]["sourceDigest"],
        "the same commit and path capture the same bytes"
    );
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
        text["results"][0]["data"]["errorCode"], "binaryFileUnsupported",
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
        other["results"][0]["data"]["assets"]["image_a"]["sourceDigest"],
        out["results"][0]["data"]["assets"]["image_a"]["sourceDigest"]
    );
    assert_eq!(
        requests(&server).await.len(),
        3,
        "every evaluation reaches the provider"
    );
    runtime.close().await;
}

#[tokio::test]
async fn models_and_long_context_follow_provider_contract() {
    let server = provider().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json_fixture(
            r#"{
                "models": [
                    {
                        "name": "jev-preview",
                        "description": "vision",
                        "release_date": "2026-09-10T18:39:06.057655+00:00"
                    }
                ]
            }"#,
        )))
        .expect(1)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = runtime(&workspace, &server, &[]);
    let out = clasify(
        &runtime,
        vec![
            json_fixture(r#"{"operation": "models"}"#),
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
