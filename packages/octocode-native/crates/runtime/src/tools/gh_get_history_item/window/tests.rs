//! Provider batch loading and the public page of a window.

use super::*;
use crate::tools::gh_shared::test_support::{default_context, one_shot_provider};
use serde_json::json;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

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

/// Mount provider batch `page` of `/files` (100 items each, `total` in
/// all), answering after `delay` and expecting `reads` requests.
async fn mount_batch(server: &MockServer, page: usize, total: usize, reads: u64) {
    let items = ((page - 1) * MAX_PER_PAGE..(page * MAX_PER_PAGE).min(total))
        .map(|n| json!({"filename": format!("f{n}")}))
        .collect::<Vec<_>>();
    let mut answer = ResponseTemplate::new(200)
        .set_delay(std::time::Duration::from_millis(300))
        .set_body_json(items);
    if page * MAX_PER_PAGE < total {
        answer = answer.insert_header(
            "link",
            format!(
                "<{}/api/v3/files?page={}>; rel=\"next\"",
                server.uri(),
                page + 1
            ),
        );
    }
    Mock::given(method("GET"))
        .and(path("/api/v3/files"))
        .and(query_param("page", page.to_string()))
        .respond_with(answer)
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
/// three 300 ms batches take about one batch's time, not three.
#[tokio::test]
async fn a_page_spanning_batches_reads_them_at_once_in_order() {
    let server = MockServer::builder().start().await;
    for page in 1..=3 {
        mount_batch(&server, page, 1000, 1).await;
    }
    let provider = one_shot_provider(&server);
    let started = std::time::Instant::now();
    let loaded = load_window(
        &provider.transport,
        &["files"],
        spec(250, false, Some(1000)),
        |_| true,
        &default_context(),
    )
    .await
    .expect("window");
    assert!(
        started.elapsed() < std::time::Duration::from_millis(800),
        "{:?}",
        started.elapsed()
    );
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
    mount_batch(&server, 1, 250, 0).await;
    mount_batch(&server, 2, 250, 1).await;
    mount_batch(&server, 3, 250, 1).await;
    let provider = one_shot_provider(&server);
    let first = (0..100)
        .map(|n| json!({"filename": format!("f{n}")}))
        .collect::<Vec<_>>();
    let loaded = load_window(
        &provider.transport,
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
    assert_eq!(page["totalItems"], 140);
    assert_eq!(page["totalPages"], 5);
    assert_eq!(page["countScope"], "complete");
}

#[test]
fn a_page_past_an_exhausted_window_is_empty_and_flagged_not_clamped() {
    let items = (0..10).map(|n| json!(n)).collect::<Vec<_>>();
    let (slice, page) = paginate_window(items, 0, true, Some(4), Some(5));
    assert!(slice.is_empty(), "must not repeat the last page: {slice:?}");
    assert_eq!(page["page"], 4);
    assert_eq!(page["totalPages"], 2);
    assert_eq!(page["hasMore"], false);
    assert_eq!(page["outOfRange"], true);
}
