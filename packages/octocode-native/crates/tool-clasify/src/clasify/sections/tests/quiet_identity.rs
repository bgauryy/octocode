//! `next.quiet` replays a saved plan only after the plan's GitHub
//! identity is confirmed. An issued cursor is already refused by the engine
//! when the identity changed; this pins the plan's own check, which holds for
//! any assessment token that reaches [`run`].
use super::*;
use octocode_runtime_base::config::ConfigOutput;
use octocode_runtime_base::execution::availability::ToolAvailability;
use octocode_runtime_base::execution::{ExecutionError, RuntimeLimits};
use octocode_runtime_base::id::ToolId;
use octocode_runtime_base::policy::path::PathPolicy;
use octocode_runtime_base::response::outcome::DomainResult;
use octocode_runtime_base::security::{CheckedQuery, ContentSecurity};
use octocode_tools_shared::judgment::Evaluator;
use std::path::{Path, PathBuf};

/// A reader whose live GitHub identity is `identity`. A quiet replay reads
/// nothing, so every other service is a test bug.
struct Identity {
    identity: &'static str,
    evaluator: Evaluator,
    tokio: tokio::runtime::Runtime,
}

impl Identity {
    fn new(identity: &'static str) -> Self {
        Self {
            identity,
            evaluator: Evaluator::new(
                None,
                "http://127.0.0.1:9".into(),
                "jev-1.13.0".into(),
                &octocode_runtime_base::providers::classification::jev::JEV,
                Some(Duration::from_secs(5)),
                0,
                1,
            ),
            tokio: tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .enable_all()
                .build()
                .expect("tokio"),
        }
    }
}

impl SourceReader for Identity {
    fn paths(&self) -> &PathPolicy {
        unreachable!("a quiet replay reads no path")
    }
    fn read_paths(&self) -> &PathPolicy {
        unreachable!("a quiet replay reads no path")
    }
    fn security(&self) -> &ContentSecurity {
        unreachable!("a quiet replay scans nothing")
    }
    fn config(&self) -> &ConfigOutput {
        unreachable!("a quiet replay reads no config")
    }
    fn limits(&self) -> &RuntimeLimits {
        unreachable!("a quiet replay reads no limits")
    }
    fn home(&self) -> &Path {
        unreachable!("a quiet replay reads no home")
    }
    fn handle(&self) -> &tokio::runtime::Handle {
        self.tokio.handle()
    }
    fn availability(&self) -> &ToolAvailability {
        unreachable!("a quiet replay checks no availability")
    }
    fn evaluator(&self) -> &Evaluator {
        &self.evaluator
    }
    fn output(
        &self,
        _tool: ToolId,
    ) -> &'static dyn octocode_runtime_base::response::output::ToolOutput {
        unreachable!("this fake finishes no tool output")
    }
    fn execute(
        &self,
        _: ToolId,
        _: &CheckedQuery,
        _: &ExecutionContext,
    ) -> Result<DomainResult, ExecutionError> {
        unreachable!("a quiet replay runs no row")
    }
    fn retained_github_identity(
        &self,
        _: &ExecutionContext,
    ) -> Result<Option<String>, ClassificationError> {
        Ok(Some(self.identity.into()))
    }
    fn validate_retained_github_identity(
        &self,
        expected: Option<&str>,
        _: &ExecutionContext,
    ) -> Result<(), ClassificationError> {
        if expected.is_none_or(|expected| expected == self.identity) {
            Ok(())
        } else {
            Err(ClassificationError::new(
                ErrorCode::ClassificationContextUnavailable,
                "The saved source access identity changed.",
                "Check source access.",
            ))
        }
    }
    fn text_capture_limit(&self) -> usize {
        unreachable!("a quiet replay captures nothing")
    }
    fn capture_local_text(
        &self,
        _: &Value,
        _: &ExecutionContext,
    ) -> Result<(String, PathBuf, String), ClassificationError> {
        unreachable!("a quiet replay captures nothing")
    }
    fn capture_github_content(
        &self,
        _: &Value,
        _: &ExecutionContext,
        _: usize,
    ) -> Result<octocode_github::ContentResponse, ClassificationError> {
        unreachable!("a quiet replay captures nothing")
    }
}

#[test]
fn next_quiet_confirms_the_plans_github_identity_before_it_serves() {
    let mut session = answered_session(&[0.1, 0.9]);
    session.github_identity = Some("planner".into());
    let sessions = Sessions::new(CacheConfig::derived(), None, "policy".into());
    let runtime = Runtime {
        sessions: &sessions,
        access: [1; 32],
        can_resume: true,
    };
    let context = test_context(Duration::from_secs(5));
    let last = terminal_saved(
        &mut session,
        "terminal",
        ClassificationUsage::default(),
        runtime,
        &context,
    )
    .output;
    assert_eq!(shown_ids(&last), ["s1"], "s0 is held back: {last:#}");
    assert!(last["next"].get("quiet").is_some(), "{last:#}");
    let input = json!({"operation":"sections","assessment":quiet_token(&session.id)});
    let row = super::super::super::Row {
        context: None,
        model: None,
        keys: vec!["a".into()],
        questions: vec![],
        input: super::super::super::Input::Sections(input.clone()),
        assets: None,
        long_context: None,
        details: true,
        per_item: false,
        source_progress: None,
        max_requests: None,
        per_hit: false,
        retry_units: None,
        query: Value::Null,
    };
    let budget = Budget::new(SelectionLimits { max_chars: 1_000 });
    let replay = |identity| {
        let reader = Identity::new(identity);
        let gate = reader.evaluator().gate();
        run(&row, &input, &reader, &context, runtime, &budget, &gate)
    };
    match replay("planner") {
        Ok(outcome) => assert_eq!(shown_ids(&outcome.output), ["s0"], "{:#}", outcome.output),
        Err(error) => panic!("the planning identity replays: {error:?}"),
    }
    match replay("someone-else") {
        Ok(outcome) => panic!("a changed identity was served: {:#}", outcome.output),
        Err(error) => assert!(error.message.contains("identity changed"), "{error:?}"),
    }
}
