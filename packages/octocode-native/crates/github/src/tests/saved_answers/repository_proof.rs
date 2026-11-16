//! B98: the edges of proving a repository for a held contents miss: a
//! repository that is gone, a saved answer with nothing to confirm it by,
//! a rename route past its window, and a route proved stale in the request
//! that learned it.
use super::proven_commits::{conditional_sends, fresh_for_a_minute, not_found, read, sends_to};
use super::*;
use crate::transport::RENAME_ROUTE_WINDOW;

fn missing_file(endpoint: &GitHubEndpoint, sha: &str) -> RequestSpec {
    let mut spec = read(endpoint, &["contents", "gone.md"]);
    spec.url.query_pairs_mut().append_pair("ref", sha);
    spec
}

async fn miss_at(server: &MockServer) {
    Mock::given(path("/api/v3/repos/a/b/contents/gone.md"))
        .respond_with(not_found())
        .mount(server)
        .await;
}

/// The conditional ask finds the repository gone: that proves nothing about
/// the path, so no miss is kept, and the saved metadata is dropped.
#[tokio::test]
async fn a_repository_404_on_the_conditional_ask_keeps_no_miss_and_drops_the_metadata() {
    let server = MockServer::start().await;
    Mock::given(path("/api/v3/repos/a/b"))
        .respond_with(fresh_for_a_minute("{\"full_name\":\"a/b\"}", "\"r1\""))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(path("/api/v3/repos/a/b"))
        .respond_with(not_found())
        .mount(&server)
        .await;
    miss_at(&server).await;
    let (transport, endpoint, memory) = keeping(&server);
    let sha = "7".repeat(40);
    transport
        .execute(read(&endpoint, &[]), &request("t"))
        .await
        .unwrap();
    let held = request("t");
    transport
        .execute(read(&endpoint, &[]), &held)
        .await
        .unwrap();
    transport
        .execute(missing_file(&endpoint, &sha), &held)
        .await
        .unwrap_err();
    let seen = requests(&server).await;
    assert_eq!(conditional_sends(&seen, "/api/v3/repos/a/b"), 1);
    assert_eq!(memory.len(), 0, "no miss kept, and the metadata is dropped");
    // The next request asks everything again: nothing was proved.
    let next = request("t");
    transport
        .execute(read(&endpoint, &[]), &next)
        .await
        .unwrap_err();
    transport
        .execute(missing_file(&endpoint, &sha), &next)
        .await
        .unwrap_err();
    let seen = requests(&server).await;
    assert_eq!(sends_to(&seen, "/api/v3/repos/a/b/contents/gone.md"), 2);
    assert_eq!(
        conditional_sends(&seen, "/api/v3/repos/a/b"),
        1,
        "the dropped metadata offers no validator"
    );
}

/// A saved repository answer GitHub gave no validator for cannot be
/// confirmed: nothing is sent to prove it, and the miss is not kept.
#[tokio::test]
async fn a_repository_answer_with_no_validator_sends_no_extra_ask() {
    let server = MockServer::start().await;
    Mock::given(path("/api/v3/repos/a/b"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("cache-control", "private, max-age=60")
                .set_body_string("{\"full_name\":\"a/b\"}"),
        )
        .mount(&server)
        .await;
    miss_at(&server).await;
    let (transport, endpoint, _) = keeping(&server);
    let sha = "8".repeat(40);
    transport
        .execute(read(&endpoint, &[]), &request("t"))
        .await
        .unwrap();
    for _ in 0..2 {
        let context = request("t");
        transport
            .execute(read(&endpoint, &[]), &context)
            .await
            .unwrap();
        transport
            .execute(missing_file(&endpoint, &sha), &context)
            .await
            .unwrap_err();
    }
    let seen = requests(&server).await;
    assert_eq!(sends_to(&seen, "/api/v3/repos/a/b"), 1, "no extra ask");
    assert_eq!(conditional_sends(&seen, "/api/v3/repos/a/b"), 0);
    assert_eq!(
        sends_to(&seen, "/api/v3/repos/a/b/contents/gone.md"),
        2,
        "unproven, the miss is asked again"
    );
}

async fn renamed_forever(server: &MockServer, sha: &str) {
    let id = format!("{}/api/v3/repositories/9", server.uri());
    let contents = format!("{id}/contents/gone.md?ref={sha}");
    Mock::given(path("/api/v3/repos/old/b/contents/gone.md"))
        .respond_with(ResponseTemplate::new(301).insert_header("location", contents.as_str()))
        .mount(server)
        .await;
    Mock::given(path("/api/v3/repos/old/b"))
        .respond_with(ResponseTemplate::new(301).insert_header("location", id.as_str()))
        .mount(server)
        .await;
    Mock::given(path("/api/v3/repositories/9/contents/gone.md"))
        .respond_with(not_found())
        .mount(server)
        .await;
    Mock::given(path("/api/v3/repositories/9"))
        .respond_with(validated("{\"full_name\":\"new/b\"}", "\"m1\""))
        .mount(server)
        .await;
}

/// Past `RENAME_ROUTE_WINDOW` a repeat of a kept miss under the old name
/// replays it (nothing sent, no redirect seen by the transport), while the
/// repository read under that name asks again and names the rename.
#[tokio::test]
async fn an_old_name_repeat_past_the_route_window_replays_and_the_repository_read_names_the_rename()
{
    let server = MockServer::start().await;
    let sha = "9".repeat(40);
    renamed_forever(&server, &sha).await;
    let (transport, endpoint, memory) = keeping(&server);
    let old = |rest: &[&str]| {
        let mut segments = vec!["repos", "old", "b"];
        segments.extend_from_slice(rest);
        let mut url = endpoint.rest(&segments).unwrap();
        if !rest.is_empty() {
            url.query_pairs_mut().append_pair("ref", &sha);
        }
        RequestSpec::get(url)
    };
    let first = request("t");
    let missed = transport
        .execute(old(&["contents", "gone.md"]), &first)
        .await
        .unwrap_err();
    transport.execute(old(&[]), &first).await.unwrap();
    memory.age_all(RENAME_ROUTE_WINDOW + Duration::from_secs(1));
    let before = requests(&server).await.len();
    let repeat = request("t");
    let again = transport
        .execute(old(&["contents", "gone.md"]), &repeat)
        .await
        .unwrap_err();
    assert_eq!(again, missed);
    assert_eq!(requests(&server).await.len(), before, "replayed, not asked");
    assert!(
        !repeat.followed_rename("old", "b"),
        "the replay saw no redirect: the transport states no rename here"
    );
    transport.execute(old(&[]), &repeat).await.unwrap();
    assert!(
        repeat.followed_rename("old", "b"),
        "the repository read under the old name asks again and names the rename"
    );
    assert!(requests(&server).await.len() > before);
}

/// A rename GitHub stated earlier in this request is dropped, with its route,
/// when the repository it leads to then answers 404: the repository just
/// became inaccessible, so the rename is no fact to disclose for the answer
/// the given name gives next.
#[tokio::test]
async fn a_route_proved_stale_drops_a_rename_proved_earlier_in_the_request() {
    let server = MockServer::start().await;
    let id = format!("{}/api/v3/repositories/9", server.uri());
    Mock::given(path("/api/v3/repos/old/b"))
        .respond_with(ResponseTemplate::new(301).insert_header("location", id.as_str()))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(path("/api/v3/repositories/9"))
        .respond_with(validated("{\"full_name\":\"new/b\"}", "\"m1\""))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(path("/api/v3/repositories/9"))
        .respond_with(not_found())
        .mount(&server)
        .await;
    Mock::given(path("/api/v3/repositories/9/contents/a.md"))
        .respond_with(not_found())
        .mount(&server)
        .await;
    Mock::given(path("/api/v3/repos/old/b/contents/a.md"))
        .respond_with(ResponseTemplate::new(200).set_body_string("reclaimed"))
        .mount(&server)
        .await;
    let (transport, endpoint, _) = keeping(&server);
    let context = request("t");
    transport
        .execute(
            RequestSpec::get(endpoint.rest(&["repos", "old", "b"]).unwrap()),
            &context,
        )
        .await
        .unwrap();
    assert!(context.followed_rename("old", "b"), "proved by GitHub now");
    let mut url = endpoint
        .rest(&["repos", "old", "b", "contents", "a.md"])
        .unwrap();
    url.query_pairs_mut().append_pair("ref", "main");
    let page = transport
        .execute(RequestSpec::get(url), &context)
        .await
        .unwrap();
    assert_eq!(page.body.as_ref(), b"reclaimed");
    assert!(!context.followed_rename("old", "b"));
    assert!(!page.redirected, "the old name answered for itself");
}
