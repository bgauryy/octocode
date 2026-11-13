//! Provider batch loading and the public page of a window.

use super::*;
use crate::security::scan::Passthrough;
use crate::tools::gh_shared::test_support::{default_context, one_shot_provider};
use serde_json::json;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

mod waves;

#[test]
fn commit_file_limit_uses_raw_extent_and_preserves_existing_caps() {
    for (skipped, count, exhausted, capped, expected) in [
        (0, 2999, true, false, false),
        (0, 3000, true, false, true),
        (2700, 300, true, false, true),
        (2700, 299, true, false, false),
        (0, 3000, false, false, false),
        (0, 1, false, true, true),
    ] {
        let mut loaded = Loaded {
            items: vec![Value::Null; count],
            state: WindowState {
                skipped,
                exhausted,
                capped,
            },
            first: Value::Null,
        };
        loaded.mark_commit_file_limit();
        assert_eq!(
            loaded.state.capped, expected,
            "skipped={skipped}, count={count}, exhausted={exhausted}, capped={capped}"
        );
        assert_eq!(loaded.state.skipped, skipped);
        assert_eq!(loaded.state.exhausted, exhausted);
        assert_eq!(loaded.items.len(), count);
    }
}

/// How long a batch takes to answer. Requests that arrive within one such delay
/// of each other were sent together; a later one waited for an answer.
const BATCH_DELAY: std::time::Duration = std::time::Duration::from_millis(300);

/// When each batch request reached the server.
#[derive(Clone, Default)]
struct Arrivals(std::sync::Arc<std::sync::Mutex<Vec<std::time::Instant>>>);

impl Arrivals {
    /// The groups of requests sent together: the count of rounds the load
    /// needed, whatever the time its answers took.
    fn waves(&self) -> usize {
        let mut times = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        times.sort();
        let mut waves = 0;
        let mut started: Option<std::time::Instant> = None;
        for time in times {
            if started.is_none_or(|first| time.duration_since(first) >= BATCH_DELAY) {
                waves += 1;
                started = Some(time);
            }
        }
        waves
    }
}

/// One batch's answer: records its arrival, then answers after [`BATCH_DELAY`].
struct Batch {
    answer: ResponseTemplate,
    arrivals: Arrivals,
}

impl wiremock::Respond for Batch {
    fn respond(&self, _: &wiremock::Request) -> ResponseTemplate {
        self.arrivals
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(std::time::Instant::now());
        self.answer.clone().set_delay(BATCH_DELAY)
    }
}

/// The answer of provider batch `page` of `/files` (100 items each, `total` in
/// all), with the `Link` header that names its successor and the last page.
fn batch_answer(server: &MockServer, page: usize, total: usize) -> ResponseTemplate {
    let items = ((page - 1) * MAX_PER_PAGE..(page * MAX_PER_PAGE).min(total))
        .map(|n| json!({"filename": format!("f{n}")}))
        .collect::<Vec<_>>();
    let mut answer = ResponseTemplate::new(200).set_body_json(items);
    if page * MAX_PER_PAGE < total {
        answer = answer.insert_header(
            "link",
            format!(
                "<{uri}/api/v3/files?page={}>; rel=\"next\", <{uri}/api/v3/files?page={}>; rel=\"last\"",
                page + 1,
                total.div_ceil(MAX_PER_PAGE),
                uri = server.uri()
            ),
        );
    }
    answer
}

/// Mount provider batch `page` of `/files`, expecting `reads` requests.
async fn mount_batch(
    server: &MockServer,
    arrivals: &Arrivals,
    page: usize,
    total: usize,
    reads: u64,
) {
    Mock::given(method("GET"))
        .and(path("/api/v3/files"))
        .and(query_param("page", page.to_string()))
        .respond_with(Batch {
            answer: batch_answer(server, page, total),
            arrivals: arrivals.clone(),
        })
        .expect(reads)
        .mount(server)
        .await;
}

fn spec(page_size: usize, filtered: bool, total: Option<usize>) -> WindowSpec {
    WindowSpec {
        batch_size: MAX_PER_PAGE,
        max_batches: MAX_FILE_BATCHES,
        page: 1,
        page_size,
        filtered,
        provider_total: total,
        prefetched: None,
    }
}

/// An unfiltered page whose files span several provider batches, with
/// the provider total known, reads those batches at once and in order:
/// the three batches go out in one wave, not three.
#[tokio::test]
async fn a_page_spanning_batches_reads_them_at_once_in_order() {
    let server = MockServer::builder().start().await;
    let arrivals = Arrivals::default();
    for page in 1..=3 {
        mount_batch(&server, &arrivals, page, 1000, 1).await;
    }
    let provider = one_shot_provider(&server);
    let loaded = load_window(
        &Source::new(&provider.transport, &Passthrough),
        &["files"],
        spec(250, false, Some(1000)),
        |_| true,
        &default_context(),
    )
    .await
    .expect("window");
    assert_eq!(arrivals.waves(), 1, "the three batches go out together");
    assert_eq!(loaded.items.len(), 300);
    let names = loaded
        .items
        .iter()
        .map(|item| item["filename"].as_str().unwrap_or_default().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(names, (0..300).map(|n| format!("f{n}")).collect::<Vec<_>>());
    assert!(!loaded.state.exhausted);
}

/// A batch the read already holds (fetched beside the PR) is used as
/// is: a filtered scan of 250 files reads only batches 2 and 3, and
/// every file lands once, in provider order.
#[tokio::test]
async fn a_prefetched_batch_is_never_read_again() {
    let server = MockServer::builder().start().await;
    let arrivals = Arrivals::default();
    mount_batch(&server, &arrivals, 1, 250, 0).await;
    mount_batch(&server, &arrivals, 2, 250, 1).await;
    mount_batch(&server, &arrivals, 3, 250, 1).await;
    let provider = one_shot_provider(&server);
    let first = (0..100)
        .map(|n| json!({"filename": format!("f{n}")}))
        .collect::<Vec<_>>();
    let loaded = load_window(
        &Source::new(&provider.transport, &Passthrough),
        &["files"],
        WindowSpec {
            prefetched: Some((1, (json!(first), true))),
            ..spec(100, true, Some(250))
        },
        |_| true,
        &default_context(),
    )
    .await
    .expect("window");
    assert_eq!(loaded.items.len(), 250);
    assert_eq!(loaded.items[0]["filename"], "f0");
    assert_eq!(loaded.items[249]["filename"], "f249");
    assert!(loaded.state.exhausted && !loaded.state.capped);
}

#[test]
fn window_pages_span_provider_batches_without_provider_cursors() {
    // Page 5 × 30 = items 120..150: an unfiltered load starts at provider
    // batch 2 (items 100..), so 100 items are skipped, never fetched.
    let items = (100..140).map(|n| json!(n)).collect::<Vec<_>>();
    let (slice, page) = paginate_window(items.clone(), 100, false, Some(5), Some(30));
    assert_eq!(slice.first(), Some(&json!(120)));
    assert_eq!(slice.len(), 20);
    assert_eq!(page["hasMore"], true);
    assert_eq!(page["nextPage"], 6);
    assert_eq!(page["countScope"], "loaded");
    let (slice, page) = paginate_window(items, 100, true, Some(5), Some(30));
    assert_eq!(slice.len(), 20);
    assert_eq!(page["hasMore"], false);
    assert_eq!(page["total"], 140);
    assert_eq!(page["countScope"], "complete");
}

#[test]
fn a_page_past_an_exhausted_window_is_empty_and_flagged_not_clamped() {
    let items = (0..10).map(|n| json!(n)).collect::<Vec<_>>();
    let (slice, page) = paginate_window(items, 0, true, Some(4), Some(5));
    assert!(slice.is_empty(), "must not repeat the last page: {slice:?}");
    assert_eq!(
        (&page["offset"], &page["length"], &page["total"]),
        (&json!(10), &json!(0), &json!(10))
    );
    assert_eq!(page["hasMore"], false);
    assert_eq!(page["outOfRange"], true);
}

/// Comments, reviews and discussion have no provider cap (GitHub documents
/// only `per_page` max 100 for them, unlike the 3000-file and 250-commit
/// lists): a filtered scan past item 3000 still finds its match and is
/// neither capped nor terminal.
#[tokio::test]
async fn a_collection_scan_reads_past_three_thousand_items() {
    let total = 30 * MAX_PER_PAGE + 50;
    let server = MockServer::builder().start().await;
    let arrivals = Arrivals::default();
    for page in 1..=total.div_ceil(MAX_PER_PAGE) {
        mount_batch(&server, &arrivals, page, total, 1).await;
    }
    let provider = one_shot_provider(&server);
    let wanted = format!("f{}", total - 1);
    let loaded = load_window(
        &Source::new(&provider.transport, &Passthrough),
        &["files"],
        WindowSpec {
            max_batches: MAX_COLLECTION_BATCHES,
            ..spec(10, true, Some(total))
        },
        |item| item["filename"] == wanted.as_str(),
        &default_context(),
    )
    .await
    .expect("window");
    assert!(!loaded.state.capped, "no provider cap applies");
    assert!(
        loaded
            .items
            .iter()
            .any(|item| item["filename"] == wanted.as_str())
    );
    let mut page = json!({});
    mark_capped(&mut page, loaded.state.capped);
    assert!(page.get("terminalLimit").is_none(), "{page}");
}

/// A filtered scan with no count known up front (a commit's files) reads its
/// first batch, learns the batch count from the `Link` header's last page,
/// and loads the rest at once: the other four go out in one wave: two waves, not five.
#[tokio::test]
async fn a_scan_without_a_known_total_loads_the_linked_batches_at_once() {
    let total = 5 * MAX_PER_PAGE;
    let server = MockServer::builder().start().await;
    let arrivals = Arrivals::default();
    for page in 1..=5 {
        mount_batch(&server, &arrivals, page, total, 1).await;
    }
    let provider = one_shot_provider(&server);
    let loaded = load_window(
        &Source::new(&provider.transport, &Passthrough),
        &["files"],
        spec(10, true, None),
        |_| false,
        &default_context(),
    )
    .await
    .expect("window");
    assert_eq!(
        arrivals.waves(),
        2,
        "the first batch, then the other four together"
    );
    let names = loaded
        .items
        .iter()
        .map(|item| item["filename"].as_str().unwrap_or_default().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        (0..total).map(|n| format!("f{n}")).collect::<Vec<_>>()
    );
    assert!(loaded.state.exhausted && !loaded.state.capped);
}

/// A batch that fails inside a concurrent wave fails the whole load: the
/// batches that answered are not returned as a partial inventory.
#[tokio::test]
async fn a_batch_that_fails_inside_a_wave_fails_the_load_without_a_partial_inventory() {
    let total = 5 * MAX_PER_PAGE;
    let server = MockServer::builder().start().await;
    let arrivals = Arrivals::default();
    mount_batch(&server, &arrivals, 1, total, 1).await;
    for page in [2, 3, 5] {
        Mock::given(method("GET"))
            .and(path("/api/v3/files"))
            .and(query_param("page", page.to_string()))
            .respond_with(Batch {
                answer: batch_answer(&server, page, total),
                arrivals: arrivals.clone(),
            })
            .mount(&server)
            .await;
    }
    Mock::given(method("GET"))
        .and(path("/api/v3/files"))
        .and(query_param("page", "4"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;
    let provider = one_shot_provider(&server);
    let failed = load_window(
        &Source::new(&provider.transport, &Passthrough),
        &["files"],
        spec(10, true, None),
        |_| false,
        &default_context(),
    )
    .await;
    assert!(
        failed.is_err(),
        "a failed batch is an error, not a shorter inventory"
    );
    assert_eq!(arrivals.waves(), 2, "the failure happened inside the wave");
}
