use super::*;
use crate::tools::gh_shared::test_support::{default_context, one_shot_provider};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

const SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

struct CommitFiles {
    files: Vec<Value>,
    origin: String,
}

impl Respond for CommitFiles {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let parameter = |key| {
            request
                .url
                .query_pairs()
                .find(|(name, _)| name == key)
                .and_then(|(_, value)| value.parse::<usize>().ok())
                .unwrap_or(1)
        };
        let page = parameter("page");
        let per = parameter("per_page");
        let files = self
            .files
            .iter()
            .skip((page - 1) * per)
            .take(per)
            .cloned()
            .collect::<Vec<_>>();
        let mut response = ResponseTemplate::new(200).set_body_json(json!({
            "sha": SHA, "commit": {"message":"change"}, "files": files
        }));
        if page * per < self.files.len() {
            let next = format!(
                "{}{}?per_page={per}&page={}",
                self.origin,
                request.url.path(),
                page + 1
            );
            response = response.insert_header("link", format!("<{next}>; rel=\"next\""));
        }
        response
    }
}

fn file(name: &str, status: &str, additions: usize) -> Value {
    json!({"filename":name,"status":status,"additions":additions,"deletions":0,"patch":"@@ +1 @@\n+x"})
}

async fn provider(files: Vec<Value>) -> (MockServer, octocode_github::GitHubProvider) {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/o/r/commits/{SHA}")))
        .respond_with(CommitFiles {
            files,
            origin: server.uri(),
        })
        .mount(&server)
        .await;
    let provider = one_shot_provider(&server);
    (server, provider)
}

async fn first_page(
    provider: &octocode_github::GitHubProvider,
    scope: Value,
) -> (Value, Vec<(String, Value)>) {
    let query = HistoryItemRequest::from_row(super::super::super::util::merge(
        json!({"operation":"pullRequest","owner":"o","repo":"r","number":1,"sections":["commitFiles"]}), scope,
    )).expect("PR query");
    let mut row = json!({});
    let next = shape_pr_commits(
        &provider.transport,
        &mut row,
        &mut Map::new(),
        vec![json!({"sha":SHA,"commit":{"message":"change"}})],
        WindowState::COMPLETE,
        &query,
        &default_context(),
    )
    .await
    .expect("commit section");
    (row, next)
}

#[tokio::test]
async fn commit_files_scope_covers_late_matches_and_continuations_exactly_once() {
    let mut files = (0..300)
        .map(|n| file(&format!("src/ignored-{n}.rs"), "modified", 10))
        .collect::<Vec<_>>();
    files.push(file("docs/wrong-status.md", "added", 10));
    files.push(file("docs/too-small.md", "modified", 1));
    files.extend((0..101).map(|n| file(&format!("docs/kept-{n}.md"), "modified", 10)));
    let (_server, provider) = provider(files).await;
    let scope = json!({"include":["docs/**"],"fileStatus":["modified"],"minChanges":5});
    let (row, next) = first_page(&provider, scope.clone()).await;
    let expected = (0..100)
        .map(|n| json!(format!("M +10 -0 kept-{n}.md")))
        .collect::<Vec<_>>();
    assert_eq!(
        row["commits"][0]["files"],
        json!([{"docs/": expected}]),
        "{row}"
    );
    let [(_, next)] = next.as_slice() else {
        panic!("one continuation: {next:?}")
    };
    let saved = &next["input"]["queries"][0];
    for key in ["include", "fileStatus", "minChanges"] {
        assert_eq!(saved[key], scope[key], "{next}");
    }
    assert_eq!(saved["page"], 2);
    assert_eq!(saved["pageSize"], 100);
    let query = HistoryItemRequest::from_row(saved).expect("saved scoped commit");
    let tail = super::super::super::commit::commit(&provider, &query, &default_context())
        .await
        .expect("tail")
        .data;
    assert_eq!(
        tail["files"],
        json!(["M +10 -0 docs/kept-100.md"]),
        "{tail}"
    );
    assert_eq!(
        tail["contentPagination"]["files"]["hasMore"], false,
        "{tail}"
    );
}

#[tokio::test]
async fn commit_files_unfiltered_pages_preserve_every_file() {
    let files = (0..101)
        .map(|n| file(&format!("f{n}.rs"), "modified", 1))
        .collect::<Vec<_>>();
    let (_server, provider) = provider(files).await;
    let (row, next) = first_page(&provider, json!({})).await;
    assert_eq!(
        row["commits"][0]["files"].as_array().expect("files").len(),
        100
    );
    assert_eq!(row["commits"][0]["files"][0], "M +1 -0 f0.rs");
    assert_eq!(row["commits"][0]["files"][99], "M +1 -0 f99.rs");
    let query =
        HistoryItemRequest::from_row(&next[0].1["input"]["queries"][0]).expect("saved commit");
    let tail = super::super::super::commit::commit(&provider, &query, &default_context())
        .await
        .expect("tail")
        .data;
    assert_eq!(tail["files"], json!(["M +1 -0 f100.rs"]));
    assert_eq!(
        tail["contentPagination"]["files"]["hasMore"], false,
        "{tail}"
    );
}

#[tokio::test]
async fn commit_files_scope_discloses_provider_boundary_even_without_next_link() {
    let files = (0..3000)
        .map(|n| file(&format!("src/{n}.rs"), "modified", 1))
        .collect::<Vec<_>>();
    let (server, provider) = provider(files).await;
    let (row, next) = first_page(&provider, json!({"include":["docs/**"]})).await;
    assert_eq!(row["commits"][0]["files"], json!([]), "{row}");
    assert!(next.is_empty(), "{next:?}");
    assert_eq!(row["terminalLimit"], true, "{row}");
    assert!(
        row["partialReasons"]
            .as_array()
            .expect("partial reasons")
            .contains(&json!("providerBatchLimit")),
        "{row}"
    );
    assert_eq!(
        row["commits"][0]["contentPagination"]["files"]["countScope"], "partial",
        "{row}"
    );
    assert_eq!(
        server.received_requests().await.expect("requests").len(),
        10
    );
}
