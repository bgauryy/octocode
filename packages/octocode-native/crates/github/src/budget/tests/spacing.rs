//! Search spacing: ordered starts and the bucket a spacing refusal names.
use super::*;

/// A spacing rejection names the group's own bucket, not the code-search
/// window.
#[tokio::test]
async fn spacing_rejection_names_its_own_resource() {
    let (budget, state) = limited(ExecutorConfig {
        graphql_spacing: Duration::from_secs(5),
        ..ExecutorConfig::relaxed()
    });
    let token = CancellationToken::new();
    let admit = |deadline| {
        state.admit(
            Some(Group::Graphql),
            budget.config(),
            false,
            Duration::from_secs(10),
            deadline,
            &token,
        )
    };
    drop(
        admit(Instant::now() + Duration::from_secs(30))
            .await
            .expect("first start"),
    );
    let error = admit(Instant::now() + Duration::from_millis(100))
        .await
        .err()
        .expect("spacing exceeds the deadline");
    assert_eq!(error.kind, ProviderErrorKind::RateLimited);
    assert_eq!(
        error.rate_limit.and_then(|rate| rate.resource).as_deref(),
        Some("graphql")
    );
    assert!(!error.message.contains("code search"), "{}", error.message);
}

#[tokio::test]
async fn search_spacing_orders_starts() {
    let (budget, state) = limited(ExecutorConfig {
        search_spacing: Duration::from_millis(150),
        ..ExecutorConfig::relaxed()
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    let token = CancellationToken::new();
    let started = Instant::now();
    for _ in 0..3 {
        let _admission = state
            .admit(
                Some(Group::Search),
                budget.config(),
                false,
                Duration::from_secs(10),
                deadline,
                &token,
            )
            .await
            .expect("admitted");
    }
    assert!(started.elapsed() >= Duration::from_millis(290));
}

/// A batch refused by spacing alone names the spacing, not the window.
#[test]
fn a_batch_refused_by_spacing_names_the_spacing() {
    let (budget, state) = limited(ExecutorConfig {
        search_spacing: Duration::from_secs(2),
        ..ExecutorConfig::relaxed()
    });
    let refusal = state
        .code_search_batch_refusal(
            3,
            budget.config(),
            Duration::from_secs(1),
            Instant::now() + Duration::from_secs(30),
        )
        .expect("three starts 2 s apart exceed the cap");
    assert!(
        refusal
            .message
            .contains("spacing cannot fit this row's 3 searches"),
        "{}",
        refusal.message
    );
    assert!(!refusal.message.contains("window"), "{}", refusal.message);
}
