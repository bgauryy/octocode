//! `classification` on ghSearchCode: the page's other rows are judged, and a
//! credential-named row is read by no one, sent to no one and disclosed.

use super::{
    MARKER, NORMAL, SHA, assert_nothing_left, contents_reads, github, provider, runtime_for,
};
use crate::runtime_clasify::sent;
use crate::support::{self, Run as _, Workspace};
use octocode_test_support::json_fixture;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// One code-search page: three credential-named files whose index fragments
/// hold the marker, and one ordinary file.
async fn search_page(github: &MockServer) {
    let item = |file: &str, fragment: &str, blob: &str| {
        json!({"name":file,"path":file,"sha":blob,
            "html_url":format!("https://github.com/octo/demo/blob/{SHA}/{file}"),
            "repository":{"full_name":"octo/demo","html_url":"https://x","url":"https://x"},
            "text_matches":[{"fragment":fragment,"matches":[{"text":"needle","indices":[0,6]}]}]})
    };
    let fragment = format!("needle {MARKER}");
    let items = vec![
        item(".Env.Prod", &fragment, "3"),
        item("a/.ssh/config", &fragment, "4"),
        item("KEY.PEM", &fragment, "5"),
        item("src/app.rs", NORMAL, "6"),
    ];
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("etag", "\"page\"")
                .set_body_json(json!({
                    "total_count":items.len(),"incomplete_results":false,"items":items
                })),
        )
        .mount(github)
        .await;
}

/// Repo-scoped, owner-wide and at a ref: the other rows are judged, and the
/// withheld ones are read by no one, sent to no one, and disclosed as unjudged.
#[tokio::test]
async fn a_classified_code_search_judges_the_other_rows_and_discloses_the_withheld() {
    let scoped = json_fixture(
        r#"{
            "owner": "octo",
            "repo": "demo",
            "keywords": ["needle"],
            "classification": {"predicates": ["Does it name the needle?"]}
        }"#,
    );
    let mut owner_wide = scoped.clone();
    owner_wide.as_object_mut().unwrap().remove("repo");
    let mut at_ref = scoped.clone();
    at_ref["ref"] = json!("dev");
    for (name, query) in [
        ("scoped", scoped),
        ("owner-wide", owner_wide),
        ("at a ref", at_ref),
    ] {
        let (provider, github) = (provider().await, github().await);
        search_page(&github).await;
        let workspace = Workspace::new();
        let runtime = runtime_for(&workspace, &provider, &github, false);
        let row = support::run(&runtime, "ghSearchCode", query).await;
        let data = row.data();
        assert_ne!(row.status(), "error", "{name}: {data}");
        assert_eq!(data["selection"]["unknownItems"], 3, "{name}: {data}");
        assert_eq!(data["selection"]["totalItems"], 4, "{name}: {data}");
        let warnings = data["warnings"].to_string();
        for file in [".Env.Prod", "a/.ssh/config", "KEY.PEM"] {
            assert!(warnings.contains(file), "{name}: {file}: {data}");
        }
        assert!(data["next"].get("unfiltered").is_some(), "{name}: {data}");
        assert!(
            !data.to_string().contains(MARKER),
            "{name}: the classified row shows no withheld text: {data}"
        );
        assert!(
            !data["related"]
                .to_string()
                .contains("\"path\":\".Env.Prod\""),
            "{name}: no lead judges a withheld file: {data}"
        );
        let reads = contents_reads(&github).await;
        assert!(
            reads.iter().all(|read| read.contains("app")),
            "{name}: a withheld file was read: {reads:?}"
        );
        let sent = sent(&provider).await;
        if name != "owner-wide" {
            assert!(
                sent.iter()
                    .any(|request| request.to_string().contains(NORMAL)),
                "{name}: the ordinary file is judged: {sent:?}"
            );
        }
        assert_nothing_left(&sent, &workspace, name);
        runtime.close().await;
    }
}

/// Every candidate withheld: nothing is sent at all, and the row still answers.
#[tokio::test]
async fn a_search_page_of_only_withheld_files_sends_nothing() {
    let (provider, github) = (provider().await, github().await);
    let fragment = format!("needle {MARKER}");
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count":1,"incomplete_results":false,"items":[
                {"name":".env","path":".env","sha":"3",
                 "html_url":format!("https://github.com/octo/demo/blob/{SHA}/.env"),
                 "repository":{"full_name":"octo/demo","html_url":"https://x","url":"https://x"},
                 "text_matches":[{"fragment":fragment,"matches":[{"text":"needle","indices":[0,6]}]}]}
            ]
        })))
        .mount(&github)
        .await;
    let workspace = Workspace::new();
    let runtime = runtime_for(&workspace, &provider, &github, false);
    let row = support::run(
        &runtime,
        "ghSearchCode",
        json_fixture(
            r#"{
                "owner": "octo",
                "repo": "demo",
                "keywords": ["needle"],
                "classification": {"predicates": ["Does it name the needle?"]}
            }"#,
        ),
    )
    .await;
    let data = row.data();
    assert_ne!(row.status(), "error", "{data}");
    assert_eq!(data["selection"]["unknownItems"], 1, "{data}");
    assert!(data["warnings"].to_string().contains(".env"), "{data}");
    assert!(sent(&provider).await.is_empty());
    assert_eq!(contents_reads(&github).await, Vec::<String>::new());

    // The disclosed route: the unfiltered search lists the row by path only,
    // since the path policy withholds the file's text from a plain search too.
    let unfiltered = runtime
        .run(
            "unfiltered".into(),
            "ghSearchCode".into(),
            data["next"]["unfiltered"]["input"].clone(),
        )
        .await
        .expect("unfiltered search")
        .structured_content;
    let listed = &unfiltered["results"][0]["data"];
    assert_eq!(listed["files"][0]["path"], ".env", "{unfiltered}");
    assert_eq!(listed["files"][0]["lineResolved"], false, "{unfiltered}");
    assert!(!listed.to_string().contains(MARKER), "{unfiltered}");
    assert_eq!(contents_reads(&github).await, Vec::<String>::new());
    assert!(sent(&provider).await.is_empty());
    runtime.close().await;
}
