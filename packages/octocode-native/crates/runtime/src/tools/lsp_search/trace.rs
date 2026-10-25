//! Emit opt-in timing summaries outside the evidence result, including on cancellation.

use octocode_engine::lsp::diagnostics::Trace;
use std::future::Future;

struct Report {
    trace: Trace,
    notices: Option<octocode_io::notice::Scope>,
}

impl Drop for Report {
    fn drop(&mut self) {
        octocode_io::notice::emit(self.trace.summary(), self.notices.as_ref());
    }
}

pub(super) async fn run<F: Future>(enabled: bool, work: F) -> F::Output {
    if !enabled {
        return work.await;
    }
    let report = Report {
        trace: Trace::default(),
        notices: octocode_io::notice::Scope::current(),
    };
    report.trace.run(work).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use octocode_engine::lsp::diagnostics::Phase;

    #[tokio::test(start_paused = true)]
    async fn tracing_is_opt_in_repeatable_and_keeps_results_unchanged() {
        let notices = octocode_io::notice::Scope::default();
        notices
            .run(async {
                for enabled in [false, true, true] {
                    let value = run(
                        enabled,
                        Phase::Request.measure(async {
                            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
                            Ok::<_, ()>("unchanged evidence")
                        }),
                    )
                    .await
                    .unwrap();
                    assert_eq!(value, "unchanged evidence");
                    if !enabled {
                        assert!(notices.notices().is_empty());
                    }
                }
            })
            .await;
        let lines = notices.notices();
        assert_eq!(lines.len(), 2, "every traced call, not once per process");
        assert!(lines.iter().all(|line| line.contains("request: 25ms")));
        assert!(lines.iter().all(|line| !line.contains("evidence")));
    }

    #[tokio::test(start_paused = true)]
    async fn cancelled_work_still_reports_its_phase() {
        let notices = octocode_io::notice::Scope::default();
        let result = notices
            .run(tokio::time::timeout(
                std::time::Duration::from_millis(80),
                run(
                    true,
                    Phase::Readiness.measure(std::future::pending::<Result<(), ()>>()),
                ),
            ))
            .await;
        assert!(result.is_err());
        let lines = notices.notices();
        assert_eq!(lines.len(), 1);
        assert!(
            lines[0].contains("readiness: 80ms, completed=0, failed=0, cancelled=1"),
            "{}",
            lines[0]
        );
    }

    #[tokio::test(start_paused = true)]
    async fn dropping_under_another_call_preserves_the_original_notice_scope() {
        for scoped in [true, false] {
            let original = octocode_io::notice::Scope::default();
            let unrelated = octocode_io::notice::Scope::default();
            let mut work = Box::pin(run(
                true,
                Phase::Request.measure(std::future::pending::<Result<(), ()>>()),
            ));
            {
                let _scope = scoped.then(|| original.enter());
                let mut context = std::task::Context::from_waker(std::task::Waker::noop());
                assert!(work.as_mut().poll(&mut context).is_pending());
            }
            tokio::time::advance(std::time::Duration::from_millis(40)).await;
            {
                let _scope = unrelated.enter();
                drop(work);
            }
            assert!(unrelated.notices().is_empty());
            let lines = original.notices();
            assert_eq!(lines.len(), usize::from(scoped));
            if scoped {
                assert!(lines[0].contains("request: 40ms, completed=0, failed=0, cancelled=1"));
            }
        }
    }
}
