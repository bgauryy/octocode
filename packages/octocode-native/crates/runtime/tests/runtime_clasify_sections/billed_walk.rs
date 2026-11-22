//! A clasify evaluation over a source whose reply spans three pages: walking
//! the reply's `next` bills each page once, each page but the last says what
//! the next costs, and every source row is judged exactly once.
use super::{provider, provider_calls};
use crate::runtime_clasify::{clasify, sent};
use crate::support::{self, Run as _, Workspace, clasify_runtime};
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const FILES: [&str; 3] = ["o/alpha", "o/beta", "o/gamma"];

/// GitHub's repository search over [`FILES`], cut into the pages the request names.
async fn github_serving_pages() -> MockServer {
    let github = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/repositories"))
        .respond_with(|request: &wiremock::Request| {
            let ask = |name: &str| -> usize {
                request
                    .url
                    .query_pairs()
                    .find(|(key, _)| key == name)
                    .and_then(|(_, value)| value.parse().ok())
                    .unwrap_or(1)
            };
            let (page, per) = (ask("page"), ask("per_page"));
            let items = FILES
                .iter()
                .skip((page - 1) * per)
                .take(per)
                .map(|name| {
                    json!({"full_name":name,"name":name,"html_url":"https://x",
                        "default_branch":"main","archived":false,
                        "description":format!("needle in {name}")})
                })
                .collect::<Vec<_>>();
            ResponseTemplate::new(200).set_body_json(json!({
                "total_count":FILES.len(),"incomplete_results":false,"items":items}))
        })
        .mount(&github)
        .await;
    github
}

fn billed_next_warning(data: &Value) -> Option<String> {
    data["warnings"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .find(|warning| warning.contains("bills again"))
        .map(str::to_owned)
}

/// The number written before `after` in `note` ("1 judge request", "2 of").
fn number_before(note: &str, after: &str) -> u64 {
    let head = &note[..note
        .find(after)
        .unwrap_or_else(|| panic!("{after}: {note}"))];
    head.rsplit(' ')
        .find(|word| !word.is_empty())
        .and_then(|word| word.parse().ok())
        .unwrap_or_else(|| panic!("no number before {after}: {note}"))
}

#[tokio::test]
async fn a_three_page_source_walk_bills_each_page_once_and_judges_each_row_once() {
    let server = provider().await;
    let github = github_serving_pages().await;
    let workspace = Workspace::new();
    let runtime = clasify_runtime(&workspace, &server, &support::github_api(&github));
    let row = json!({"operation":"evaluate",
        "source":{"tool":"ghSearchRepo","query":{"operation":"search","keywords":["needle"],"limit":1}},
        "questions":{"q":{"type":"noul","instructions":"The entry names a restart."}}});
    let mut page = clasify(&runtime, vec![row]).await;
    let (mut pages, mut last_remaining) = (0usize, None::<u64>);
    let mut judged_rows: u64;
    loop {
        pages += 1;
        let data = &page["results"][0]["data"];
        assert!(data.get("errorCode").is_none(), "{page:#}");
        assert_eq!(
            provider_calls(&server).await,
            pages,
            "one billed request per page: {page:#}"
        );
        // Rows judged so far: the source rows the judge has been sent.
        let sent_so_far = sent(&server).await;
        judged_rows = FILES
            .iter()
            .filter(|name| {
                sent_so_far
                    .iter()
                    .any(|body| body.to_string().contains(*name))
            })
            .count() as u64;
        let next = crate::support::next_input(&page);
        let warning = billed_next_warning(data);
        let Some(next) = next else {
            assert!(warning.is_none(), "the last page bills no next: {page:#}");
            break;
        };
        let note = warning.unwrap_or_else(|| panic!("page {pages} states its next: {page:#}"));
        let total = FILES.len() as u64;
        assert_eq!(
            number_before(&note, " judge request"),
            data["usage"]["calls"].as_u64().unwrap(),
            "this page's own count: {note}"
        );
        let remaining = number_before(&note, " of ghSearchRepo's rows remain");
        assert_eq!(remaining, total - judged_rows, "{note}");
        assert!(last_remaining.is_none_or(|last| remaining < last), "{note}");
        last_remaining = Some(remaining);
        page = runtime
            .run(format!("walk-{pages}"), "clasify".into(), next)
            .await
            .expect("next page")
            .structured_content
            .into_value();
    }
    assert_eq!(pages, 3, "the source spans three pages");
    assert_eq!(judged_rows, FILES.len() as u64);
    let bodies = sent(&server).await;
    for file in FILES {
        let judged = bodies
            .iter()
            .filter(|body| body.to_string().contains(file))
            .count();
        assert_eq!(judged, 1, "{file} is judged once");
    }
    runtime.close().await;
}
