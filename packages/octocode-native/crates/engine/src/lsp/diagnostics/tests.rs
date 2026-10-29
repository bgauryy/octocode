use super::*;
use tokio::time::{Duration, sleep};

#[tokio::test(start_paused = true)]
async fn phases_attribute_virtual_time_and_outcomes_without_query_data() {
    let trace = Trace::default();
    trace
        .run(async {
            Phase::Readiness
                .measure(async {
                    sleep(Duration::from_millis(700)).await;
                    Ok::<_, ()>(())
                })
                .await
                .unwrap();
            let result = Phase::Request
                .measure(async {
                    sleep(Duration::from_millis(30)).await;
                    Err::<(), _>("secret source text")
                })
                .await;
            assert!(result.is_err());
        })
        .await;
    let summary = trace.summary();
    assert!(
        summary.contains("readiness: 700ms, completed=1"),
        "{summary}"
    );
    assert!(
        summary.contains("request: 30ms, completed=0, failed=1"),
        "{summary}"
    );
    assert!(!summary.contains("secret"));
}

#[tokio::test(start_paused = true)]
async fn dropped_work_records_cancellation_and_carries_into_supervised_tasks() {
    let trace = Trace::default();
    let mut task = None;
    trace
        .run(async {
            task = Some(tokio::spawn(Trace::carried(
                Phase::Initialize.measure(async { std::future::pending::<Result<(), ()>>().await }),
            )));
        })
        .await;
    let task = task.expect("supervised task started in the trace scope");
    tokio::task::yield_now().await;
    assert!(trace.summary().contains("active=1"));
    tokio::time::advance(Duration::from_millis(40)).await;
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    let summary = trace.summary();
    assert!(
        summary.contains("initialize: 40ms, completed=0, failed=0, cancelled=1, active=0"),
        "{summary}"
    );
}

#[tokio::test(start_paused = true)]
async fn concurrent_scopes_and_untraced_work_do_not_leak() {
    let a = Trace::default();
    let b = Trace::default();
    let (left, right) = tokio::join!(
        a.run(Phase::Readiness.measure(async {
            sleep(Duration::from_millis(20)).await;
            Ok::<_, ()>(())
        })),
        b.run(Phase::Request.measure(async {
            sleep(Duration::from_millis(80)).await;
            Ok::<_, ()>(())
        }))
    );
    left.unwrap();
    right.unwrap();
    Phase::Spawn
        .measure(async { Ok::<_, ()>(()) })
        .await
        .unwrap();
    assert!(a.summary().contains("readiness: 20ms"));
    assert!(!a.summary().contains("request:"));
    assert!(b.summary().contains("request: 80ms"));
    assert!(!b.summary().contains("readiness:"));
    assert!(!a.summary().contains("spawn:"));
    assert!(!b.summary().contains("spawn:"));
}

#[tokio::test]
async fn the_server_line_names_its_state_and_only_the_last_server_noted() {
    let trace = Trace::default();
    assert!(!trace.summary().contains("server:"));
    trace
        .run(async {
            Trace::note_server(|| ServerNote {
                source: "bundled",
                readiness: None,
                operations: vec!["definition"],
            });
            Trace::note_server(|| ServerNote {
                source: "user",
                readiness: Some("progressIdle".into()),
                operations: vec!["hover", "diagnostic"],
            });
        })
        .await;
    let summary = trace.summary();
    assert!(
        summary.ends_with(
            "; server: source=user, readiness=progressIdle, operations=[hover,diagnostic]"
        ),
        "{summary}"
    );
    // Outside a traced call a note is never built.
    Trace::note_server(|| unreachable!("no traced call"));
}
