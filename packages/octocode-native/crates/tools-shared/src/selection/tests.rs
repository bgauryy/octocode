use super::plan::{Plan, VERSION, decode};
use super::run::binding;
use super::*;
use crate::test_support::selection_fake;
use crate::test_support::selection_runtime::{select_source, selecting_runtime, test_runtime};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use octocode_contract_types::{SelectionScope, SelectionUnit};
use octocode_test_support::json_fixture;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

mod admission;
mod plan_binding;
mod plan_token;

struct FailedJudge;
impl SelectionJudge for FailedJudge {
    fn identity(&self) -> Value {
        json_fixture(r#"{"backend": "test"}"#)
    }
    fn select(&self, _call: JudgeCall<'_>) -> Judgments {
        Err("providerUnavailable".into())
    }
}

/// Only the declared recovery channels are leads: no emitter numbers them,
/// so a numbered name is none.
#[test]
fn recovery_leads_are_the_declared_channels_only() {
    use crate::selection::{RecoveryLead, recovery_lead};
    assert_eq!(recovery_lead("unfiltered"), Some(RecoveryLead::Whole));
    assert_eq!(
        recovery_lead("unfilteredUnknown"),
        Some(RecoveryLead::Saved)
    );
    assert_eq!(
        recovery_lead("nextCandidatePage"),
        Some(RecoveryLead::Saved)
    );
    assert_eq!(recovery_lead("unfilteredUnknown2"), None);
}
