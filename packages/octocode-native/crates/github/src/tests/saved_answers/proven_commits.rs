//! B90: a miss is kept only for a path inside a commit this request
//! proved from the origin; a missing object (tree, blob, commit, compare)
//! is asked again, since it can be pushed later. Only answers GitHub gave
//! in this request vouch for a repository or a commit, never replays.
use super::*;
use crate::transport::route::RENAME_ROUTE_WINDOW;

pub(super) fn not_found() -> ResponseTemplate {
    ResponseTemplate::new(404).set_body_string("{\"message\":\"Not Found\"}")
}

async fn answering_repository(server: &MockServer) {
    Mock::given(path("/api/v3/repos/a/b"))
        .respond_with(validated("{\"full_name\":\"a/b\"}", "\"r1\""))
        .mount(server)
        .await;
}

pub(super) fn read(endpoint: &GitHubEndpoint, rest: &[&str]) -> RequestSpec {
    let mut segments = vec!["repos", "a", "b"];
    segments.extend_from_slice(rest);
    RequestSpec::get(endpoint.rest(&segments).unwrap())
}

pub(super) fn sends_to(seen: &[wiremock::Request], wanted: &str) -> usize {
    seen.iter()
        .filter(|request| request.url.path() == wanted)
        .count()
}

#[tokio::test]
async fn a_missing_object_at_a_sha_is_asked_again() {
    let server = MockServer::start().await;
    answering_repository(&server).await;
    Mock::given(wiremock::matchers::path_regex(
        "^/api/v3/repos/a/b/(git|commits|compare)",
    ))
    .respond_with(not_found())
    .mount(&server)
    .await;
    let (transport, endpoint, memory) = keeping(&server);
    let sha = "a".repeat(40);
    let other = "b".repeat(40);
    let range = format!("{sha}...{other}");
    let objects: [&[&str]; 4] = [
        &["git", "trees", &sha],
        &["git", "blobs", &sha],
        &["commits", &sha],
        &["compare", &range],
    ];
    for _ in 0..2 {
        let context = request("t");
        transport
            .execute(read(&endpoint, &[]), &context)
            .await
            .unwrap();
        for object in objects {
            transport
                .execute(read(&endpoint, object), &context)
                .await
                .unwrap_err();
        }
    }
    let seen = requests(&server).await;
    for object in objects {
        let wanted = format!("/api/v3/repos/a/b/{}", object.join("/"));
        assert_eq!(sends_to(&seen, &wanted), 2, "{wanted} asked each time");
    }
    assert!(
        memory.len() <= 1,
        "only the repository's own answer is kept, no miss"
    );
}

#[tokio::test]
async fn a_contents_miss_where_no_commit_is_found_is_asked_again() {
    let server = MockServer::start().await;
    answering_repository(&server).await;
    Mock::given(path("/api/v3/repos/a/b/contents/gone.md"))
        .respond_with(ResponseTemplate::new(404).set_body_string(
            "{\"message\":\"No commit found for the ref 0000000000000000000000000000000000000000\"}",
        ))
        .mount(&server)
        .await;
    let (transport, endpoint, _) = keeping(&server);
    let sha = "0".repeat(40);
    let file = || {
        let mut spec = read(&endpoint, &["contents", "gone.md"]);
        spec.url.query_pairs_mut().append_pair("ref", &sha);
        spec
    };
    for _ in 0..2 {
        let context = request("t");
        transport
            .execute(read(&endpoint, &[]), &context)
            .await
            .unwrap();
        transport.execute(file(), &context).await.unwrap_err();
    }
    let seen = requests(&server).await;
    assert_eq!(sends_to(&seen, "/api/v3/repos/a/b/contents/gone.md"), 2);
}

/// GitHub answers a tree path at an unknown SHA with a bare 404, as it does
/// a missing path: only a tree or commit GitHub returned for that SHA in
/// this request makes the miss the path's.
#[tokio::test]
async fn a_tree_path_miss_is_kept_only_inside_a_proven_commit() {
    let server = MockServer::start().await;
    answering_repository(&server).await;
    let sha = "c".repeat(40);
    let tree_path = format!("{sha}:docs");
    Mock::given(path(format!("/api/v3/repos/a/b/git/trees/{sha}:docs")))
        .respond_with(not_found())
        .mount(&server)
        .await;
    Mock::given(path(format!("/api/v3/repos/a/b/commits/{sha}")))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .mount(&server)
        .await;
    let (transport, endpoint, _) = keeping(&server);
    let docs = || read(&endpoint, &["git", "trees", &tree_path]);
    let wanted = format!("/api/v3/repos/a/b/git/trees/{sha}:docs");
    // The repository answered, the commit did not: asked each time.
    for _ in 0..2 {
        let context = request("t");
        transport
            .execute(read(&endpoint, &[]), &context)
            .await
            .unwrap();
        transport.execute(docs(), &context).await.unwrap_err();
    }
    assert_eq!(sends_to(&requests(&server).await, &wanted), 2);
    // The commit answered from GitHub: the miss is the path's, kept.
    let context = request("t");
    transport
        .execute(read(&endpoint, &["commits", &sha]), &context)
        .await
        .unwrap();
    let missed = transport.execute(docs(), &context).await.unwrap_err();
    let again = transport.execute(docs(), &request("t")).await.unwrap_err();
    assert_eq!(again, missed);
    assert_eq!(
        sends_to(&requests(&server).await, &wanted),
        3,
        "the repeat sent nothing"
    );
}

/// A replayed answer is what GitHub said before, not now: it proves neither
/// that the repository answers nor that the commit exists.
#[tokio::test]
async fn a_replayed_answer_does_not_vouch_for_its_repository_or_commit() {
    let server = MockServer::start().await;
    let sha = "d".repeat(40);
    Mock::given(path(format!("/api/v3/repos/a/b/commits/{sha}")))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .mount(&server)
        .await;
    Mock::given(path("/api/v3/repos/a/b/contents/gone.md"))
        .respond_with(not_found())
        .mount(&server)
        .await;
    let (transport, endpoint, _) = keeping(&server);
    let commit = || read(&endpoint, &["commits", &sha]);
    let file = || {
        let mut spec = read(&endpoint, &["contents", "gone.md"]);
        spec.url.query_pairs_mut().append_pair("ref", &sha);
        spec
    };
    transport.execute(commit(), &request("t")).await.unwrap();
    let context = request("t");
    transport.execute(commit(), &context).await.unwrap();
    assert!(
        !context.repository_resolves("a", "b"),
        "a pinned replay does not prove the repository answers now"
    );
    transport.execute(file(), &context).await.unwrap_err();
    transport.execute(file(), &request("t")).await.unwrap_err();
    let seen = requests(&server).await;
    assert_eq!(
        sends_to(&seen, &format!("/api/v3/repos/a/b/commits/{sha}")),
        1
    );
    assert_eq!(
        sends_to(&seen, "/api/v3/repos/a/b/contents/gone.md"),
        2,
        "a miss vouched for only by a replay is asked again"
    );
}

async fn renamed_server(server: &MockServer) -> String {
    let id = format!("{}/api/v3/repositories/9", server.uri());
    Mock::given(path("/api/v3/repos/old/b"))
        .respond_with(ResponseTemplate::new(301).insert_header("location", id.as_str()))
        .up_to_n_times(1)
        .mount(server)
        .await;
    Mock::given(path("/api/v3/repositories/9"))
        .respond_with(validated("{\"full_name\":\"new/b\"}", "\"m1\""))
        .up_to_n_times(1)
        .mount(server)
        .await;
    id
}

/// A route this request just learned from GitHub is no reused answer.
#[tokio::test]
async fn a_route_minted_in_this_request_is_not_disclosed_as_reused() {
    let server = MockServer::start().await;
    renamed_server(&server).await;
    Mock::given(path("/api/v3/repositories/9/readme"))
        .respond_with(validated("{}", "\"x1\""))
        .mount(&server)
        .await;
    let (transport, endpoint, _) = keeping(&server);
    let context = request("t");
    let old = |rest: &[&str]| {
        let mut segments = vec!["repos", "old", "b"];
        segments.extend_from_slice(rest);
        RequestSpec::get(endpoint.rest(&segments).unwrap())
    };
    transport.execute(old(&[]), &context).await.unwrap();
    transport.execute(old(&["readme"]), &context).await.unwrap();
    assert!(context.followed_rename("old", "b"));
    assert_eq!(
        context.reused_since(),
        None,
        "GitHub stated the rename in this very request"
    );
    let seen = requests(&server).await;
    assert_eq!(
        seen.last().unwrap().url.path(),
        "/api/v3/repositories/9/readme"
    );
}

/// A route whose repository is gone (the old name reclaimed) answers no
/// path read either: it is forgotten, the rename note cleared, and the
/// given name asked.
#[tokio::test]
async fn a_path_read_under_a_stale_route_falls_back_to_the_given_name() {
    let server = MockServer::start().await;
    renamed_server(&server).await;
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
    transport
        .execute(
            RequestSpec::get(endpoint.rest(&["repos", "old", "b"]).unwrap()),
            &request("t"),
        )
        .await
        .unwrap();
    let file = || {
        let mut url = endpoint
            .rest(&["repos", "old", "b", "contents", "a.md"])
            .unwrap();
        url.query_pairs_mut().append_pair("ref", "main");
        RequestSpec::get(url)
    };
    let context = request("t");
    let page = transport.execute(file(), &context).await.unwrap();
    assert_eq!(page.body.as_ref(), b"reclaimed");
    assert!(
        !context.followed_rename("old", "b"),
        "the old name answered for itself"
    );
    assert_eq!(context.reused_since(), None, "the route did not answer");
    let before = requests(&server).await.len();
    transport.execute(file(), &request("t")).await.unwrap();
    let seen = requests(&server).await;
    assert_eq!(seen.len(), before + 1);
    assert_eq!(
        seen.last().unwrap().url.path(),
        "/api/v3/repos/old/b/contents/a.md",
        "the forgotten route is not taken again"
    );
}

/// GitHub states `max-age` for some reads (`commits/HEAD`): under a kept
/// route a repeat within it sends nothing, as it does without a rename.
#[tokio::test]
async fn a_read_within_its_max_age_under_a_route_sends_nothing() {
    let server = MockServer::start().await;
    let head = format!("{}/api/v3/repositories/9/commits/HEAD", server.uri());
    Mock::given(path("/api/v3/repos/old/b/commits/HEAD"))
        .respond_with(ResponseTemplate::new(301).insert_header("location", head.as_str()))
        .mount(&server)
        .await;
    Mock::given(path("/api/v3/repositories/9/commits/HEAD"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("cache-control", "private, max-age=60, s-maxage=60")
                .set_body_string("abc"),
        )
        .mount(&server)
        .await;
    let (transport, endpoint, _) = keeping(&server);
    let spec = || {
        RequestSpec::get(
            endpoint
                .rest(&["repos", "old", "b", "commits", "HEAD"])
                .unwrap(),
        )
    };
    transport.execute(spec(), &request("t")).await.unwrap();
    assert_eq!(requests(&server).await.len(), 2);
    let context = request("t");
    let again = transport.execute(spec(), &context).await.unwrap();
    assert_eq!(again.body.as_ref(), b"abc");
    assert!(context.followed_rename("old", "b"));
    assert_eq!(requests(&server).await.len(), 2, "nothing was sent");
}

/// `body` under an ETag and `max-age=60`, as GitHub answers repository
/// metadata; 304 when the request presents that ETag.
pub(super) fn fresh_for_a_minute(
    body: &'static str,
    etag: &'static str,
) -> impl Fn(&Request) -> ResponseTemplate {
    move |request: &Request| {
        let presented = request
            .headers
            .get("if-none-match")
            .and_then(|value| value.to_str().ok());
        let answer = if presented == Some(etag) {
            ResponseTemplate::new(304)
        } else {
            ResponseTemplate::new(200).set_body_string(body)
        };
        answer
            .insert_header("etag", etag)
            .insert_header("cache-control", "private, max-age=60, s-maxage=60")
    }
}

pub(super) fn conditional_sends(seen: &[wiremock::Request], wanted: &str) -> usize {
    seen.iter()
        .filter(|request| {
            request.url.path() == wanted && request.headers.contains_key("if-none-match")
        })
        .count()
}

/// Repository metadata replayed inside its `max-age` proves nothing now: a
/// miss held for want of proof asks GitHub once, conditionally (a 304 is
/// free of the rate limit), and is kept on that proof.
#[tokio::test]
async fn a_miss_after_fresh_metadata_is_proven_by_one_conditional_ask_and_kept() {
    let server = MockServer::start().await;
    Mock::given(path("/api/v3/repos/a/b"))
        .respond_with(fresh_for_a_minute("{\"full_name\":\"a/b\"}", "\"r1\""))
        .mount(&server)
        .await;
    Mock::given(path("/api/v3/repos/a/b/contents/gone.md"))
        .respond_with(not_found())
        .mount(&server)
        .await;
    let (transport, endpoint, _) = keeping(&server);
    let sha = "5".repeat(40);
    let file = || {
        let mut spec = read(&endpoint, &["contents", "gone.md"]);
        spec.url.query_pairs_mut().append_pair("ref", &sha);
        spec
    };
    transport
        .execute(read(&endpoint, &[]), &request("t"))
        .await
        .unwrap();
    let first = request("t");
    transport
        .execute(read(&endpoint, &[]), &first)
        .await
        .unwrap();
    let missed = transport.execute(file(), &first).await.unwrap_err();
    let before = requests(&server).await.len();
    let context = request("t");
    transport
        .execute(read(&endpoint, &[]), &context)
        .await
        .unwrap();
    let again = transport.execute(file(), &context).await.unwrap_err();
    assert_eq!(again, missed);
    let seen = requests(&server).await;
    assert_eq!(seen.len(), before, "the repeat sent nothing");
    assert_eq!(
        conditional_sends(&seen, "/api/v3/repos/a/b"),
        1,
        "one conditional ask proved the repository"
    );
    assert_eq!(sends_to(&seen, "/api/v3/repos/a/b/contents/gone.md"), 1);
}

/// A miss GitHub answered behind a rename is kept under the name it was
/// asked by and under the id the route leads to, as a success is.
#[tokio::test]
async fn a_miss_behind_a_rename_is_kept_under_both_names() {
    let server = MockServer::start().await;
    let sha = "6".repeat(40);
    let id = format!("{}/api/v3/repositories/9", server.uri());
    let contents = format!("{id}/contents/gone.md?ref={sha}");
    Mock::given(path("/api/v3/repos/old/b/contents/gone.md"))
        .respond_with(ResponseTemplate::new(301).insert_header("location", contents.as_str()))
        .mount(&server)
        .await;
    Mock::given(path("/api/v3/repositories/9/contents/gone.md"))
        .respond_with(not_found())
        .mount(&server)
        .await;
    Mock::given(path("/api/v3/repositories/9"))
        .respond_with(validated("{\"full_name\":\"new/b\"}", "\"m1\""))
        .mount(&server)
        .await;
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
    let before = requests(&server).await.len();
    // Within the route's window the repeat goes to the id.
    let routed = transport
        .execute(old(&["contents", "gone.md"]), &request("t"))
        .await
        .unwrap_err();
    assert_eq!(routed, missed);
    assert_eq!(
        requests(&server).await.len(),
        before,
        "the routed repeat sent nothing"
    );
    // Past it, under the name asked.
    memory.age_all(RENAME_ROUTE_WINDOW + Duration::from_secs(1));
    transport
        .execute(old(&["contents", "gone.md"]), &request("t"))
        .await
        .unwrap_err();
    assert_eq!(
        requests(&server).await.len(),
        before,
        "the named repeat sent nothing"
    );
}

/// The probe that tells a stale route from a path missing behind a live one
/// is an answer like any other: kept, so the next probe need not ask.
#[tokio::test]
async fn the_stale_route_probe_is_kept() {
    let server = MockServer::start().await;
    let id = format!("{}/api/v3/repositories/9", server.uri());
    Mock::given(path("/api/v3/repos/old/b"))
        .respond_with(ResponseTemplate::new(301).insert_header("location", id.as_str()))
        .mount(&server)
        .await;
    Mock::given(path("/api/v3/repositories/9"))
        .respond_with(fresh_for_a_minute("{\"full_name\":\"new/b\"}", "\"m1\""))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(path("/api/v3/repositories/9"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("cache-control", "max-age=60")
                .set_body_string("{\"full_name\":\"new/b\"}"),
        )
        .mount(&server)
        .await;
    Mock::given(path("/api/v3/repositories/9/contents/a.md"))
        .respond_with(not_found())
        .mount(&server)
        .await;
    let (transport, endpoint, memory) = keeping(&server);
    transport
        .execute(
            RequestSpec::get(endpoint.rest(&["repos", "old", "b"]).unwrap()),
            &request("t"),
        )
        .await
        .unwrap();
    // Let the saved metadata lapse, so the probe has to ask once.
    memory.age_all(Duration::from_secs(61));
    let file = || {
        let mut url = endpoint
            .rest(&["repos", "old", "b", "contents", "a.md"])
            .unwrap();
        url.query_pairs_mut().append_pair("ref", "main");
        RequestSpec::get(url)
    };
    for _ in 0..2 {
        transport.execute(file(), &request("t")).await.unwrap_err();
    }
    let seen = requests(&server).await;
    assert_eq!(
        sends_to(&seen, "/api/v3/repositories/9"),
        2,
        "the first read, then one probe whose answer the second probe reused"
    );
    assert_eq!(sends_to(&seen, "/api/v3/repositories/9/contents/a.md"), 2);
}
