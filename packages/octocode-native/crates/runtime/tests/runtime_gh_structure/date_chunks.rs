//! Tree dates are read in aligned chunks of the listing: one GraphQL
//! request per 100 listed paths, whatever the page size, and a page whose
//! chunks its cursor already owns sends nothing.
use super::*;

/// Entries in the listing and per page: pages straddle chunk boundaries.
const ENTRIES: usize = 250;
const LIMIT: usize = 60;
/// Paths one date request covers (`octocode_github::MAX_PATHS_PER_REQUEST`).
const CHUNK: usize = 100;

/// Requests the server has received so far.
async fn received(server: &MockServer) -> Vec<wiremock::Request> {
    server.received_requests().await.unwrap_or_default()
}

/// The paths one GraphQL dates request asked for, in alias order.
fn asked_paths(body: &serde_json::Value) -> Vec<String> {
    let variables = body["variables"].as_object().expect("variables");
    (0..)
        .map_while(|index| variables.get(&format!("p{index}")))
        .map(|path| path.as_str().expect("path").to_owned())
        .collect()
}

/// A GraphQL answer that carries errors is a failure, not evidence: the
/// page's `retry` asks GitHub again instead of replaying it.
#[tokio::test]
async fn a_failed_date_chunk_is_asked_again_by_its_retry() {
    let server = listing_server(&[("one.rs".into(), "file")]).await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": null,
            "errors": [{"message": "Something unexpected"}]
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = github(&workspace, &server);
    let outcome = call(
        &runtime,
        "ghStructure",
        json!({"operation":"tree","owner": "a", "repo": "b"}),
    )
    .await
    .expect("listing");
    let retry = row_data(&outcome)["next"]["retry"]["input"].clone();
    let asked = graphql_requests(&server).await.len();
    call(&runtime, "ghStructure", retry).await.expect("retry");
    assert!(
        graphql_requests(&server).await.len() > asked,
        "the retry asks GitHub for the dates again"
    );
    runtime.close().await;
}

/// A walk at a page size that does not divide the chunk dates every path
/// exactly once, in `ceil(entries / 100)` aligned requests in total: a page
/// reads only the chunks no earlier page of the walk read, and every date
/// is the one the provider gave for that path. Re-reading a page whose
/// chunks its cursor owns sends no request at all and answers the same.
#[tokio::test]
async fn a_walk_dates_every_path_once_in_aligned_chunks() {
    let names = (0..ENTRIES)
        .map(|n| (format!("f{n:03}.rs"), "file"))
        .collect::<Vec<_>>();
    let server = listing_server(&names).await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(DatesEcho)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = github(&workspace, &server);
    let mut pending = Some(json!({"queries":[{
        "operation":"tree","owner":"a","repo":"b","limit": LIMIT
    }]}));
    let mut dated = serde_json::Map::new();
    let mut inputs = Vec::new();
    let mut outputs = Vec::new();
    while let Some(input) = pending.take() {
        let outcome = call(&runtime, "ghStructure", input.clone())
            .await
            .expect("dated listing");
        let data = row_data(&outcome).clone();
        for group in data["files"].as_array().expect("listing groups") {
            for (name, date) in updated(group) {
                assert!(
                    dated.insert(name.clone(), date).is_none(),
                    "date repeated: {name}"
                );
            }
        }
        pending = data.pointer("/next/nextPage/input").cloned();
        inputs.push(input);
        outputs.push(data);
        assert!(inputs.len() <= ENTRIES.div_ceil(LIMIT), "bounded walk");
    }
    assert_eq!(inputs.len(), ENTRIES.div_ceil(LIMIT));
    // Every path is dated with the provider's date for it.
    assert_eq!(dated.len(), ENTRIES);
    for (name, _) in &names {
        let day = name.len() % 28 + 1;
        assert_eq!(dated[name], format!("2025-03-{day:02}"), "{name}");
    }
    // One request per aligned chunk, each path asked once.
    let requests = graphql_requests(&server).await;
    assert_eq!(requests.len(), ENTRIES.div_ceil(CHUNK), "{requests:?}");
    let mut asked = requests.iter().flat_map(asked_paths).collect::<Vec<_>>();
    asked.sort();
    let listed = names
        .iter()
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>();
    assert_eq!(asked, listed, "each path asked exactly once");
    for request in &requests {
        let paths = asked_paths(request);
        let first = listed.iter().position(|name| *name == paths[0]).unwrap();
        assert_eq!(first % CHUNK, 0, "aligned chunk: {paths:?}");
    }
    // Page 3 (entries 120..180) lies in chunk 1, which page 2 read: its
    // cursor owns it, so re-reading it sends nothing.
    let before = received(&server).await.len();
    let again = call(&runtime, "ghStructure", inputs[2].clone())
        .await
        .expect("re-read");
    let sent = received(&server).await[before..]
        .iter()
        .map(|request| request.url.path().to_owned())
        .collect::<Vec<_>>();
    assert!(sent.is_empty(), "re-reading page 3 sent {sent:?}");
    let mut reread = row_data(&again).clone();
    let mut first = outputs[2].clone();
    for value in [&mut reread, &mut first] {
        value.as_object_mut().map(|object| object.remove("next"));
    }
    assert_eq!(reread, first);
    runtime.close().await;
}

/// `pagination` counts the calls that read a dated walk: every cursor call
/// advances `page`, `totalPages` is the number of calls the walk needs, and
/// `hasMore` ends with the last of them, whatever `limit` asks for (a dated
/// page lists at most one date request's entries).
#[tokio::test]
async fn pagination_counts_every_call_of_a_dated_walk() {
    let names = (0..ENTRIES)
        .map(|n| (format!("f{n:03}.rs"), "file"))
        .collect::<Vec<_>>();
    let server = listing_server(&names).await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(DatesEcho)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = github(&workspace, &server);
    for limit in [None, Some(LIMIT), Some(CHUNK + 50)] {
        let mut query = json!({"operation":"tree","owner":"a","repo":"b"});
        if let Some(limit) = limit {
            query["limit"] = json!(limit);
        }
        let per_page = limit.unwrap_or(usize::MAX).min(CHUNK);
        let calls = ENTRIES.div_ceil(per_page);
        let mut pending = Some(json!({"queries":[query]}));
        let mut seen = Vec::new();
        while let Some(input) = pending.take() {
            let outcome = call(&runtime, "ghStructure", input)
                .await
                .expect("dated listing");
            let data = row_data(&outcome);
            seen.push((
                data["pagination"]["page"].clone(),
                data["pagination"]["totalPages"].clone(),
                data["pagination"]["hasMore"].clone(),
            ));
            pending = data.pointer("/next/nextPage/input").cloned();
            assert!(seen.len() <= calls, "bounded walk: {seen:?}");
        }
        let expected = (1..=calls)
            .map(|page| (json!(page), json!(calls), json!(page < calls)))
            .collect::<Vec<_>>();
        assert_eq!(seen, expected, "limit {limit:?}");
    }
    runtime.close().await;
}
