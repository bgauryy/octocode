//! Ranked (selected) reads: shaping off the acquisition worker and minification refusal.
use super::*;

struct TimedJudge(std::sync::Arc<std::sync::atomic::AtomicUsize>);

impl crate::tools::selection::SelectionJudge for TimedJudge {
    fn identity(&self) -> Value {
        json!({"model":"timed-test"})
    }

    fn select(
        &self,
        call: crate::tools::selection::JudgeCall<'_>,
    ) -> crate::tools::selection::Judgments {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        call.handle.block_on(async {
            tokio::time::sleep(Duration::from_millis(5)).await;
        });
        Ok(call
            .admitted
            .iter()
            .map(|&index| (index, crate::tools::selection::Decision::Selected))
            .collect())
    }
}

async fn selected_body_is_shaped_off_the_acquisition_worker() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    mount_json(&server, "/api/v3/repos/a/b/contents/large.txt", 200, json!({
        "type":"file", "encoding":"base64", "content":STANDARD.encode("line with useful evidence and a long suffix\n".repeat(300))
    })).await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let query: GhGetFileContentQuery = serde_json::from_value(json!({
        "owner":"a", "repo":"b", "path":"large.txt", "ref":sha,
        "unit":"lines", "length":100, "clasifyContext":{"classifiers":["find useful evidence"]}
    }))
    .expect("ranked query");
    let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let result = tokio::time::timeout(
        Duration::from_secs(10),
        execute(
            &provider,
            &query,
            &fixture_context(Duration::from_secs(10), 1 << 20),
            Some(50_000),
            shaping(TimedJudge(calls.clone())),
        ),
    )
    .await
    .expect("ranked read must not block the reactor")
    .expect("ranked read");
    assert_eq!(
        result.file.content.selection.as_ref().map(|r| &r.status),
        Some(&octocode_contract_types::SelectionStatus::Complete)
    );
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
}

/// Explicit compact views cannot supply raw classification evidence. They
/// fail clearly and offer unfiltered recovery pinned to the fetched commit.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn explicit_minification_refuses_selection_and_offers_pinned_recovery() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    let body: String = (1..=1050)
        .map(|i| {
            if i % 3 == 0 {
                format!("// note {i}\n")
            } else {
                format!("var value_{i} = compute({i}, 'some longer argument text');\n")
            }
        })
        .collect();
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/lib%2Fr.js",
        200,
        serde_json::json!({"type":"file","encoding":"base64","content":STANDARD.encode(&body)}),
    )
    .await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let owned = shaping(TimedJudge(calls));
    let mut pages = Vec::new();
    for classifiers in [
        None,
        Some(serde_json::json!(["where is value 500 computed"])),
    ] {
        let mut row = serde_json::json!({
            "owner":"a","repo":"b","path":"lib/r.js","ref":sha,"minify":"standard","mainGoal":"test","reasoning":"test"
        });
        if let Some(classifiers) = classifiers {
            row["clasifyContext"] = json!({"classifiers":classifiers});
        }
        let query: GhGetFileContentQuery = serde_json::from_value(row).expect("query");
        let result = execute(
            &provider,
            &query,
            &fixture_context(Duration::from_secs(10), 1 << 20),
            Some(50_000),
            owned.clone(),
        )
        .await
        .expect("read");
        pages.push(result.file.clone());
    }
    let (plain, classifiers) = (&pages[0], &pages[1]);
    assert!(plain.content.pagination.is_some(), "{plain:?}");
    assert_eq!(
        classifiers.content.error_code,
        Some(ErrorCode::InvalidInput)
    );
    assert!(classifiers.content.content.is_none());
    let recovery =
        &classifiers.next.as_ref().expect("recovery")["unfiltered"]["query"]["queries"][0];
    assert_eq!(recovery["ref"], sha);
    assert!(recovery.get("clasifyContext").is_none());
}

#[test]
fn selected_gh_body_runs_on_current_and_multithread_runtimes() {
    let current = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("current runtime");
    current.block_on(selected_body_is_shaped_off_the_acquisition_worker());
    let multi = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("multithread runtime");
    multi.block_on(selected_body_is_shaped_off_the_acquisition_worker());
}
