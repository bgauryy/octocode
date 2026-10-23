//! How often a search reads the response store.
use super::*;

/// Counts the store reads a search makes.
#[derive(Default)]
struct CountingCache {
    inner: PartitionedCache,
    gets: std::sync::atomic::AtomicUsize,
}

impl ConditionalCache for CountingCache {
    fn get<'a>(
        &'a self,
        partition: &'a Partition,
        key: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<CachedContent>> + Send + 'a>>
    {
        self.gets.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.inner.get(partition, key)
    }
    fn put<'a>(
        &'a self,
        partition: &'a Partition,
        key: String,
        value: CachedContent,
        retention: Retention,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'a>> {
        self.inner.put(partition, key, value, retention)
    }
}

/// A search its caller just found uncached is sent without reading the
/// store again, and stored for the next one; two identical searches in
/// flight together still send once (the second reads what the first stored).
#[tokio::test]
async fn a_search_known_to_miss_reads_the_store_once() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({
                    "total_count": 0, "incomplete_results": false, "items": []
                }))
                .set_delay(Duration::from_millis(100)),
        )
        .expect(1)
        .mount(&server)
        .await;
    let (mut transport, _) = executor_transport(&server, GitHubBudget::relaxed(), short_retry());
    let cache = Arc::new(CountingCache::default());
    transport.cache = cache.clone();
    let context = RequestContext::new(
        RequestBudget::with_timeout(Duration::from_secs(5), 1 << 20),
        None,
    );
    let request = code_search_request();
    let gets = || cache.gets.load(std::sync::atomic::Ordering::SeqCst);
    assert!(
        transport
            .cached_code_search(&request, &context)
            .await
            .expect("lookup")
            .is_none()
    );
    let (first, second) = tokio::join!(
        transport.search_code_uncached(&request, &context),
        transport.search_code_uncached(&request, &context),
    );
    first.expect("first search");
    second.expect("second search");
    // The lookup, then the waiter's read of what the sender stored.
    assert_eq!(gets(), 2);
    assert!(
        transport
            .cached_code_search(&request, &context)
            .await
            .expect("lookup")
            .is_some()
    );
}
