use super::*;

#[test]
fn known_totals_bound_each_wave_by_request_concurrency() {
    let batches = BATCH_CONCURRENCY + 2;
    let total = batches * MAX_PER_PAGE;
    for filtered in [false, true] {
        let window = WindowSpec {
            max_batches: MAX_COLLECTION_BATCHES,
            ..spec(total, filtered, Some(total))
        };
        assert_eq!(window.wave_end(1), BATCH_CONCURRENCY);
        assert_eq!(window.wave_end(BATCH_CONCURRENCY + 1), batches);
    }
}

/// The first matching file covers page one. A match beyond that wave must
/// remain reachable by the emitted next page, without refetching held evidence.
#[tokio::test]
async fn a_filtered_page_stops_after_one_wave_and_continues_to_later_matches() {
    let batches = BATCH_CONCURRENCY + 2;
    let total = batches * MAX_PER_PAGE;
    let server = MockServer::builder().start().await;
    for page in 1..=batches {
        Mock::given(method("GET"))
            .and(path("/api/v3/files"))
            .and(query_param("page", page.to_string()))
            .respond_with(batch_answer(&server, page, total))
            .expect(1)
            .mount(&server)
            .await;
    }
    let provider = one_shot_provider(&server);
    let source = Source::new(&provider.transport, &Passthrough);
    let context = default_context();
    let last_name = format!("f{}", total - 1);
    let keep = |item: &Value| item["filename"] == "f0" || item["filename"] == last_name.as_str();
    let window = |page| WindowSpec {
        page,
        max_batches: MAX_COLLECTION_BATCHES,
        ..spec(1, true, Some(total))
    };
    let first = load_window(&source, &["files"], window(1), keep, &context)
        .await
        .expect("first window");
    assert!(
        server.received_requests().await.expect("requests").len() <= BATCH_CONCURRENCY,
        "a covered filtered page must not fetch the remaining collection"
    );
    assert!(!first.state.exhausted && !first.state.capped);
    let matching = first.items.into_iter().filter(keep).collect::<Vec<_>>();
    let (mut delivered, page) = first.state.paginate(matching, Some(1), Some(1));
    assert_eq!(delivered[0]["filename"], "f0");
    assert_eq!(page["hasMore"], true);
    let next = usize::try_from(page["nextPage"].as_u64().expect("next page")).expect("page");

    let resumed = default_context().resuming(std::sync::Arc::new(context.evidence()));
    let last = load_window(&source, &["files"], window(next), keep, &resumed)
        .await
        .expect("continued window");
    assert!(last.state.exhausted && !last.state.capped);
    let matching = last.items.into_iter().filter(keep).collect::<Vec<_>>();
    let (remaining, page) = last.state.paginate(matching, Some(next), Some(1));
    delivered.extend(remaining);
    assert_eq!(
        delivered,
        vec![json!({"filename": "f0"}), json!({"filename": last_name})]
    );
    assert_eq!(page["hasMore"], false);
    assert_eq!(page["countScope"], "complete");
    assert_eq!(page["total"], 2);
    assert_eq!(
        server.received_requests().await.expect("requests").len(),
        batches
    );
}
