//! A repeat asks GitHub the least that keeps it correct: a validator round
//! trip for a mutable resource, nothing for a commit-pinned one, and the bare
//! window (disclosed) only where GitHub offers neither.
use super::*;
use crate::transport::SEARCH_REUSE_WINDOW;
use std::collections::HashMap;
use std::sync::Mutex;
use wiremock::Request;

mod compressed;
mod directives;
mod flight;
mod header_identity;
mod known_misses;
mod proven_commits;
mod repository_proof;
mod vary;
mod what_is_kept;

/// One process's view of a store that outlives requests.
#[derive(Default)]
struct Memory(Mutex<HashMap<(String, String), SavedAnswer>>);

impl SavedAnswers for Memory {
    fn load(&self, scope: &str, request: &str) -> Option<SavedAnswer> {
        let map = self.0.lock().unwrap();
        map.get(&(scope.to_owned(), request.to_owned())).cloned()
    }
    fn save(&self, scope: &str, request: &str, saved: SavedAnswer) {
        let mut map = self.0.lock().unwrap();
        map.insert((scope.to_owned(), request.to_owned()), saved);
    }
    fn forget(&self, scope: &str, request: &str) {
        let mut map = self.0.lock().unwrap();
        map.remove(&(scope.to_owned(), request.to_owned()));
    }
}

impl Memory {
    /// Every saved answer's age moves back by `by`, as a later session sees it.
    fn age_all(&self, by: Duration) {
        let by = u64::try_from(by.as_millis()).unwrap();
        for saved in self.0.lock().unwrap().values_mut() {
            saved.observed_ms = saved.observed_ms.saturating_sub(by);
        }
    }

    fn len(&self) -> usize {
        self.0.lock().unwrap().len()
    }
}

fn keeping(server: &MockServer) -> (GitHubTransport, GitHubEndpoint, Arc<Memory>) {
    let endpoint = mock_endpoint(server);
    let mut transport = anonymous_transport(&endpoint);
    let memory = Arc::new(Memory::default());
    transport.set_saved_answers(Some(memory.clone()));
    (transport, endpoint, memory)
}

fn request(token: &str) -> RequestContext {
    with_token(token, Duration::from_secs(5), 1024 * 1024)
}

async fn requests(server: &MockServer) -> Vec<Request> {
    server.received_requests().await.unwrap()
}

/// `body` under an ETag, or 304 when the request presents that ETag.
fn validated(body: &'static str, etag: &'static str) -> impl Fn(&Request) -> ResponseTemplate {
    move |request: &Request| {
        let presented = request
            .headers
            .get("if-none-match")
            .and_then(|value| value.to_str().ok());
        if presented == Some(etag) {
            ResponseTemplate::new(304).insert_header("etag", etag)
        } else {
            ResponseTemplate::new(200)
                .insert_header("etag", etag)
                .set_body_string(body)
        }
    }
}

#[tokio::test]
async fn a_mutable_read_is_confirmed_by_its_validator_and_answers_the_same_page() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b"))
        .respond_with(validated("{\"name\":\"b\"}", "\"v1\""))
        .mount(&server)
        .await;
    let (transport, endpoint, _) = keeping(&server);
    let spec = || RequestSpec::get(endpoint.rest(&["repos", "a", "b"]).unwrap());
    let first = transport.execute(spec(), &request("t")).await.unwrap();
    assert_eq!(first.body.as_ref(), b"{\"name\":\"b\"}");
    let context = request("t");
    let second = transport.execute(spec(), &context).await.unwrap();
    assert_eq!(
        second.status, 200,
        "a confirmed answer reads as the 200 it was"
    );
    assert_eq!(second.body, first.body);
    let seen = requests(&server).await;
    assert_eq!(seen.len(), 2);
    assert!(!seen[0].headers.contains_key("if-none-match"));
    assert_eq!(seen[1].headers["if-none-match"], "\"v1\"");
    assert_eq!(
        context.reused_since(),
        None,
        "GitHub confirmed it: nothing to disclose"
    );
    assert_eq!(
        context.evidence().answers.len(),
        1,
        "a continuation owns it"
    );
}

#[tokio::test]
async fn a_read_within_the_origins_own_max_age_is_not_disclosed() {
    let server = MockServer::start().await;
    Mock::given(path("/api/v3/repos/a/b"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("cache-control", "private, max-age=60")
                .insert_header("etag", "\"v1\"")
                .set_body_string("{}"),
        )
        .expect(1)
        .mount(&server)
        .await;
    let (transport, endpoint, _) = keeping(&server);
    let spec = || RequestSpec::get(endpoint.rest(&["repos", "a", "b"]).unwrap());
    transport.execute(spec(), &request("t")).await.unwrap();
    let context = request("t");
    transport.execute(spec(), &context).await.unwrap();
    assert_eq!(
        context.reused_since(),
        None,
        "the origin stated how long it stands"
    );
}

#[tokio::test]
async fn a_changed_resource_replaces_the_saved_answer() {
    let server = MockServer::start().await;
    let (transport, endpoint, _) = keeping(&server);
    let spec = || RequestSpec::get(endpoint.rest(&["repos", "a", "b"]).unwrap());
    Mock::given(path("/api/v3/repos/a/b"))
        .respond_with(validated("old", "\"v1\""))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(path("/api/v3/repos/a/b"))
        .respond_with(validated("new", "\"v2\""))
        .mount(&server)
        .await;
    let first = transport.execute(spec(), &request("t")).await.unwrap();
    assert_eq!(first.body.as_ref(), b"old");
    let changed = transport.execute(spec(), &request("t")).await.unwrap();
    assert_eq!(changed.body.as_ref(), b"new");
    let third = transport.execute(spec(), &request("t")).await.unwrap();
    assert_eq!(third.body.as_ref(), b"new");
    assert_eq!(
        requests(&server).await[2].headers["if-none-match"],
        "\"v2\"",
        "the newest validator asks"
    );
}

#[tokio::test]
async fn a_commit_pinned_read_sends_nothing_however_old_the_answer_is() {
    let server = MockServer::start().await;
    let sha = "a".repeat(40);
    Mock::given(path(format!("/api/v3/repos/a/b/git/blobs/{sha}")))
        .respond_with(ResponseTemplate::new(200).set_body_string("immutable"))
        .expect(1)
        .mount(&server)
        .await;
    let (transport, endpoint, memory) = keeping(&server);
    let spec = || {
        RequestSpec::get(
            endpoint
                .rest(&["repos", "a", "b", "git", "blobs", &sha])
                .unwrap(),
        )
    };
    let first = transport.execute(spec(), &request("t")).await.unwrap();
    memory.age_all(Duration::from_secs(365 * 24 * 3600));
    let context = request("t");
    let again = transport.execute(spec(), &context).await.unwrap();
    assert_eq!(again.body, first.body);
    assert_eq!(
        context.reused_since(),
        None,
        "a SHA is exact: nothing to disclose"
    );
    assert_eq!(context.evidence().answers.len(), 1);
}

/// Another credential's answers are neither served nor even validated
/// against: an ETag of one token's view must not reach GitHub under another.
#[tokio::test]
async fn one_credentials_answers_never_reach_another() {
    let server = MockServer::start().await;
    let sha = "b".repeat(40);
    Mock::given(path(format!("/api/v3/repos/a/b/git/blobs/{sha}")))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("etag", "\"p\"")
                .set_body_string("private"),
        )
        .mount(&server)
        .await;
    let (transport, endpoint, _) = keeping(&server);
    let spec = || {
        RequestSpec::get(
            endpoint
                .rest(&["repos", "a", "b", "git", "blobs", &sha])
                .unwrap(),
        )
    };
    transport.execute(spec(), &request("owner")).await.unwrap();
    transport
        .execute(spec(), &request("stranger"))
        .await
        .unwrap();
    let anonymous = RequestContext::with_timeout(Duration::from_secs(5), 1024);
    transport.execute(spec(), &anonymous).await.unwrap();
    let seen = requests(&server).await;
    assert_eq!(seen.len(), 3, "each credential asked for itself");
    assert!(
        seen.iter()
            .all(|request| !request.headers.contains_key("if-none-match"))
    );
    transport.execute(spec(), &request("owner")).await.unwrap();
    assert_eq!(
        requests(&server).await.len(),
        3,
        "the owner's own answer stands"
    );
}

/// GitHub search sends no validator: a repeat inside the window spends no
/// search quota and is disclosed; past it the search runs again.
#[tokio::test]
async fn a_search_without_a_validator_reuses_within_its_window_and_says_so() {
    let server = MockServer::start().await;
    Mock::given(path("/api/v3/search/repositories"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            serde_json::json!({"total_count":1,"incomplete_results":false,"items":[]}),
        ))
        .mount(&server)
        .await;
    let (transport, endpoint, memory) = keeping(&server);
    let spec = || RequestSpec::get(endpoint.rest(&["search", "repositories"]).unwrap());
    let fresh = request("t");
    transport.execute(spec(), &fresh).await.unwrap();
    assert_eq!(fresh.reused_since(), None);
    let before = crate::budget::now_ms();
    let reused = request("t");
    transport.execute(spec(), &reused).await.unwrap();
    assert_eq!(
        requests(&server).await.len(),
        1,
        "the repeat spends no search"
    );
    let observed = reused.reused_since().expect("a reuse is disclosed");
    assert!(observed <= before);
    assert_eq!(
        reused
            .evidence()
            .notes
            .get("reusedSince")
            .map(String::as_str),
        Some(observed.to_string().as_str()),
        "a continuation of this search discloses the same time"
    );
    memory.age_all(SEARCH_REUSE_WINDOW);
    let again = request("t");
    transport.execute(spec(), &again).await.unwrap();
    assert_eq!(
        requests(&server).await.len(),
        2,
        "past the window it searches again"
    );
    assert_eq!(again.reused_since(), None);
}

/// A rate limit that outlasts the call answers with the last answer GitHub
/// gave, past any window, and says when it was observed; without one it fails.
#[tokio::test]
async fn a_rate_limited_repeat_serves_the_saved_answer_and_discloses_it() {
    let server = MockServer::start().await;
    let (transport, endpoint, memory) = keeping(&server);
    Mock::given(path("/api/v3/search/repositories"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            serde_json::json!({"total_count":1,"incomplete_results":false,"items":[]}),
        ))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    let reset = (epoch_secs() + 3600).to_string();
    Mock::given(path("/api/v3/search/repositories"))
        .respond_with(
            ResponseTemplate::new(403)
                .insert_header("x-ratelimit-remaining", "0")
                .insert_header("x-ratelimit-reset", reset.as_str())
                .set_body_json(serde_json::json!({"message":"API rate limit exceeded"})),
        )
        .mount(&server)
        .await;
    let spec = || RequestSpec::get(endpoint.rest(&["search", "repositories"]).unwrap());
    let first = transport.execute(spec(), &request("t")).await.unwrap();
    memory.age_all(SEARCH_REUSE_WINDOW * 100);
    let limited = request("t");
    let answer = transport.execute(spec(), &limited).await.unwrap();
    assert_eq!(answer.body, first.body);
    assert!(
        limited.reused_since().is_some(),
        "the stale answer is disclosed"
    );
    assert_eq!(
        memory.len(),
        1,
        "a rate limit says nothing about the resource"
    );
    let other = transport.execute(spec(), &request("no-saved-answer")).await;
    assert_eq!(other.unwrap_err().kind, ProviderErrorKind::RateLimited);
}

/// A kept answer past this call's body limit does not slip under it.
#[tokio::test]
async fn a_saved_body_past_the_calls_limit_answers_nothing() {
    let server = MockServer::start().await;
    let sha = "e".repeat(40);
    Mock::given(path(format!("/api/v3/repos/a/b/git/blobs/{sha}")))
        .respond_with(ResponseTemplate::new(200).set_body_string("0123456789"))
        .mount(&server)
        .await;
    let (transport, endpoint, _) = keeping(&server);
    let spec = || {
        RequestSpec::get(
            endpoint
                .rest(&["repos", "a", "b", "git", "blobs", &sha])
                .unwrap(),
        )
    };
    transport.execute(spec(), &request("t")).await.unwrap();
    let small = with_token("t", Duration::from_secs(5), 4);
    let refused = transport.execute(spec(), &small).await.unwrap_err();
    assert_eq!(refused.kind, ProviderErrorKind::ResponseTooLarge);
}

/// RV-GH R2-L1 (M1's `record_page` arm): a file read the ordinary way is
/// saved; the same commit-pinned read kept by its caller is then answered
/// from that saved answer, unasked, and that reused body is not the
/// request's evidence either: the caller's copy is its only store.
#[tokio::test]
async fn a_saved_answer_reused_by_a_read_its_caller_keeps_is_not_evidence() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/one.rs"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("etag", "\"v1\"")
                .set_body_json(serde_json::json!({
                    "type": "file", "encoding": "base64", "content": STANDARD.encode("body\n")
                })),
        )
        .mount(&server)
        .await;
    let (transport, _, memory) = keeping(&server);
    let provider = GitHubProvider::new(transport);
    let file = ContentRequest {
        owner: "a".into(),
        repo: "b".into(),
        path: "one.rs".into(),
        reference: Some(sha.into()),
    };
    provider
        .get_file_bytes(&file, &request("reuse"))
        .await
        .expect("an ordinary read");
    assert_eq!(memory.len(), 1, "the ordinary read is saved");
    let kept = request("reuse");
    let read = provider
        .get_file_bytes_kept_by_caller(&file, &kept)
        .await
        .expect("a read its caller keeps");
    assert_eq!(read.bytes, &b"body\n"[..]);
    assert_eq!(
        requests(&server).await.len(),
        1,
        "the kept read was answered by the saved answer, unasked"
    );
    let bodies: usize = kept
        .evidence()
        .answers
        .iter()
        .filter(|answer| answer.request.contains("/contents/"))
        .map(|answer| answer.body.len())
        .sum();
    assert_eq!(bodies, 0, "{:?}", kept.evidence().answers);
}

/// A contents body the read cannot decode is no answer: it is neither kept
/// nor recorded as evidence, so the retry the error's hint offers asks
/// GitHub again and succeeds once GitHub sends a good body, even for a
/// commit-pinned read a kept answer would serve unasked.
#[tokio::test]
async fn an_undecodable_contents_body_is_not_kept_so_a_retry_asks_again() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/one.rs"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "type": "file", "encoding": "base64", "content": "@@ not base64 @@"
        })))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/one.rs"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "type": "file", "encoding": "base64", "content": STANDARD.encode("body\n")
        })))
        .mount(&server)
        .await;
    let (transport, _, memory) = keeping(&server);
    let provider = GitHubProvider::new(transport);
    let file = ContentRequest {
        owner: "a".into(),
        repo: "b".into(),
        path: "one.rs".into(),
        reference: Some(sha.into()),
    };
    let first = request("decode");
    let error = provider
        .get_file_bytes(&file, &first)
        .await
        .expect_err("an undecodable body");
    assert_eq!(error.kind, ProviderErrorKind::Decode, "{error:?}");
    assert_eq!(memory.len(), 0, "the undecodable answer is not kept");
    assert!(
        first
            .evidence()
            .answers
            .iter()
            .all(|answer| !answer.request.contains("/contents/")),
        "{:?}",
        first.evidence().answers
    );
    let read = provider
        .get_file_bytes(&file, &request("decode"))
        .await
        .expect("the retry reads the good body");
    assert_eq!(read.bytes, &b"body\n"[..]);
    assert_eq!(requests(&server).await.len(), 2, "the retry asked GitHub");
    assert_eq!(memory.len(), 1, "the good answer is kept");
}
