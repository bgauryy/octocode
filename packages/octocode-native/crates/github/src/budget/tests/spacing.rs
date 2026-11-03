//! Spacing from fixed intervals and from the quota GitHub reports (or the
//! assumed code-search quota): burst while covered, spread only the excess,
//! refuse only past the deadline.
use super::*;

/// A spacing rejection names the group's own bucket.
#[tokio::test]
async fn spacing_rejection_names_its_own_resource() {
    let (budget, state) = limited(ExecutorConfig {
        graphql_spacing: Duration::from_secs(5),
        ..ExecutorConfig::relaxed()
    });
    let ahead = Instant::now() + Duration::from_secs(30);
    drop(
        admit(&state, &budget, Group::Graphql, ahead)
            .await
            .expect("first start"),
    );
    let error = admit(
        &state,
        &budget,
        Group::Graphql,
        Instant::now() + Duration::from_millis(100),
    )
    .await
    .err()
    .expect("spacing exceeds the deadline");
    assert_eq!(error.kind, ProviderErrorKind::RateLimited);
    assert_eq!(
        error.rate_limit.and_then(|rate| rate.resource).as_deref(),
        Some("graphql")
    );
}

#[tokio::test]
async fn search_starts_within_quota_burst() {
    let (budget, state) = limited(quota_paced());
    state.report_quota(Group::Search.bucket(), Duration::from_millis(150));
    let deadline = Instant::now() + Duration::from_secs(5);
    let started = Instant::now();
    for _ in 0..3 {
        let _admission = admit(&state, &budget, Group::Search, deadline)
            .await
            .expect("admitted");
    }
    assert!(
        started.elapsed() < Duration::from_millis(290),
        "covered searches must not wait for two reported-spacing intervals"
    );
}

/// Without a quota-paced config, a reported search quota spaces nothing.
#[tokio::test]
async fn an_unpaced_config_ignores_the_reported_quota() {
    let (budget, state) = limited(ExecutorConfig::relaxed());
    state.report_quota(Group::Search.bucket(), Duration::from_secs(5));
    let started = Instant::now();
    for _ in 0..3 {
        drop(
            admit(
                &state,
                &budget,
                Group::Search,
                Instant::now() + Duration::from_secs(5),
            )
            .await
            .expect("admitted"),
        );
    }
    assert!(started.elapsed() < Duration::from_secs(1));
}

/// A row of `searches` code searches planned against the current facts, with
/// `deadline_secs` to start them all.
fn batch_refusal(
    state: &Arc<KeyState>,
    budget: &GitHubBudget,
    searches: usize,
    deadline_secs: u64,
) -> Option<ProviderError> {
    state.code_search_batch_refusal(
        searches,
        budget.config(),
        Instant::now() + Duration::from_secs(deadline_secs),
    )
}

/// A row whose planned searches all fit the reported quota starts them now,
/// however short its deadline: nothing is spread.
#[test]
fn a_quota_covered_batch_is_not_refused() {
    let (budget, state) = limited(quota_paced());
    report_remaining(&state, 10, 60);
    let refusal = batch_refusal(&state, &budget, 10, 2);
    assert!(
        refusal.is_none(),
        "ten searches covered by ten remaining must fit: {:?}",
        refusal.map(|error| error.message)
    );
}

/// Only the seven searches beyond the three remaining are spread until the
/// reset, so the row fits a deadline just past the reset and not one before it.
#[test]
fn only_searches_beyond_the_reported_quota_are_spread() {
    let (budget, state) = limited(quota_paced());
    report_remaining(&state, 3, 20);
    assert!(batch_refusal(&state, &budget, 10, 25).is_none());
    let refusal = batch_refusal(&state, &budget, 10, 15)
        .expect("the excess is spread until the reset, past this deadline");
    assert!(
        refusal
            .message
            .contains("spacing cannot fit this row's 10 searches"),
        "{}",
        refusal.message
    );
    let retry = refusal.rate_limit.expect("rate limit");
    assert!(retry.retry_after_seconds.is_some_and(|secs| secs >= 15));
    assert_eq!(retry.resource.as_deref(), Some("code_search"));
}

/// A row refused for spacing says when to retry and reserves nothing: the
/// same row fits once the deadline reaches past the spread.
#[test]
fn a_refused_batch_reserves_no_slot() {
    let (budget, state) = limited(quota_paced());
    report_remaining(&state, 1, 4);
    assert!(batch_refusal(&state, &budget, 3, 1).is_some());
    assert!(state.facts().last_start_ms.is_empty());
    assert!(batch_refusal(&state, &budget, 3, 10).is_none());
}

/// Before any answer reports a quota, code search plans against the
/// documented 10 a minute: ten searches burst, the excess is spread until
/// the assumed window ends.
#[test]
fn an_unreported_code_search_quota_is_assumed_to_be_ten_a_minute() {
    let (budget, state) = limited(quota_paced());
    assert!(state.facts().buckets.is_empty());
    assert!(batch_refusal(&state, &budget, 10, 2).is_none());
    assert!(batch_refusal(&state, &budget, 12, 5).is_some());
    assert!(batch_refusal(&state, &budget, 12, 120).is_none());
    assert_eq!(state.code_search_room(budget.config()), 10);
}

/// The first answer replaces the assumption: a quota of three, not ten,
/// now covers the row, and the room follows what GitHub reported.
#[test]
fn the_first_reported_quota_replaces_the_assumed_one() {
    let (budget, state) = limited(quota_paced());
    assert!(batch_refusal(&state, &budget, 5, 5).is_none());
    report_remaining(&state, 3, 60);
    assert!(batch_refusal(&state, &budget, 5, 5).is_some());
    assert_eq!(state.code_search_room(budget.config()), 3);
    report_remaining(&state, 500, 60);
    assert_eq!(state.code_search_room(budget.config()), 500);
    assert!(batch_refusal(&state, &budget, 50, 2).is_none());
}

/// A reported window that has ended no longer describes the quota: the
/// assumption stands in again until the next answer.
#[test]
fn an_ended_quota_window_falls_back_to_the_assumption() {
    let (budget, state) = limited(quota_paced());
    state.facts().buckets.insert(
        Group::CodeSearch.bucket().into(),
        key_state::Bucket {
            remaining: 0,
            reset: now_ms() / 1000 - 1,
        },
    );
    assert_eq!(state.code_search_room(budget.config()), 10);
}

/// An unpaced config plans against no quota at all.
#[test]
fn an_unpaced_config_plans_against_no_quota() {
    let (budget, state) = limited(ExecutorConfig::relaxed());
    report_remaining(&state, 1, 60);
    assert!(batch_refusal(&state, &budget, 50, 1).is_none());
    assert_eq!(state.code_search_room(budget.config()), usize::MAX);
}
