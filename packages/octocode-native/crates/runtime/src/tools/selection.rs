//! Bounded classification of sanitized evidence before it reaches the host.
//! The plan records disjoint selected/excluded partitions; unjudged evidence is
//! unknown. Replays need source identity and cardinality, not provider calls.
use crate::execution::{ExecutionContext, SelectionLimits};
use crate::id::ToolId;
use octocode_contract_types::ErrorCode;
use octocode_contract_types::{Selection, SelectionFailure, SelectionStatus, SelectionUnit};
use serde_json::{Value, json};

mod admission;
mod judge;
mod plan;
mod provider_page;
mod run;
#[cfg(test)]
mod tests;

use plan::in_runs;
pub(crate) use provider_page::apply_sanitized_rows;
use run::select_scoped;
pub(crate) use run::{needs_candidate_states, select};

#[derive(Clone)]
pub(crate) struct Runtime {
    pub limits: SelectionLimits,
    pub available: bool,
    pub judge: std::sync::Arc<dyn SelectionJudge>,
    pub handle: tokio::runtime::Handle,
    pub context: ExecutionContext,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Decision {
    Selected,
    /// Judged related but not direct evidence: hidden, and not undecided.
    Related,
    Excluded,
    Unknown,
}

/// Shared evaluation is injected; readers never own provider implementations.
pub(crate) trait SelectionJudge: Send + Sync {
    fn identity(&self) -> Value;
    fn project(&self, state: Value) -> Value {
        state
    }
    fn select(&self, call: JudgeCall<'_>) -> Judgments;
}

/// One decision per judged candidate ordinal, or why none was judged.
pub(crate) type Judgments = Result<Vec<(usize, Decision)>, JudgeFailure>;

/// Why a judge decided no candidate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum JudgeFailure {
    /// Judging cannot run here (no credential, no usable policy): the receipt
    /// is `unavailable` with this reason and the evidence stays unfiltered.
    Unavailable(String),
    /// Every admitted candidate failed at the provider with this typed error.
    /// The row fails with it; an unknown answer is never invented for it.
    Provider(ProviderError),
}

/// A provider's typed error as a classified row reports it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ProviderError {
    pub code: ErrorCode,
    pub message: String,
    pub hints: Vec<String>,
}

impl From<&str> for JudgeFailure {
    fn from(reason: &str) -> Self {
        Self::Unavailable(reason.to_owned())
    }
}

pub(crate) struct JudgeCall<'a> {
    pub failures: &'a mut Vec<SelectionFailure>,
    pub usage: &'a mut crate::tools::judgment::stats::ClassificationUsage,
    pub candidates: &'a [Candidate],
    pub admitted: &'a [usize],
    pub predicates: &'a [String],
    pub(crate) research_context: Option<&'a str>,
    pub(crate) long_context: bool,
    pub limits: SelectionLimits,
    pub handle: &'a tokio::runtime::Handle,
    pub context: &'a ExecutionContext,
}

/// One complete sanitized unit. Missing states are unknown, not irrelevant.
pub(crate) struct Candidate {
    pub state: Option<Value>,
    evidence_chars: usize,
}

impl Candidate {
    /// Admission charges serialized sanitized state, including JSON escaping.
    pub(crate) fn new(state: Option<Value>) -> Self {
        let evidence_chars = state.as_ref().map_or(0, serialized_chars);
        Self {
            state,
            evidence_chars,
        }
    }

    fn fits_budget(&self, limits: SelectionLimits) -> bool {
        self.state.is_some() && limits.max_candidates > 0 && self.evidence_chars <= limits.max_chars
    }
}

/// Unicode scalars of `value` serialized as JSON, counted while writing
/// instead of building the string.
pub(crate) fn serialized_chars(value: &Value) -> usize {
    struct Chars(usize);
    impl std::io::Write for Chars {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            // Every scalar has exactly one non-continuation byte.
            self.0 += bytes.iter().filter(|&&byte| byte & 0xC0 != 0x80).count();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut chars = Chars(0);
    serde_json::to_writer(&mut chars, value).map_or(0, |()| chars.0)
}

pub(crate) struct Outcome {
    pub selected: Vec<usize>,
    /// Judged related but not direct evidence: hidden, not unknown.
    pub related: Vec<usize>,
    /// Judged irrelevant; every other unselected ordinal is unknown.
    pub excluded: Vec<usize>,
    /// The unknown ordinals no judge saw: eligible candidates past the
    /// candidate budget. The rest of the unknown ones were judged unsure,
    /// or had no state to judge.
    pub unjudged: Vec<usize>,
    pub receipt: Selection,
    pub token: String,
    /// The ordinals this candidate page judged from (empty: all of them);
    /// the others were judged by an earlier candidate page.
    pool: Vec<[u32; 2]>,
    /// The token that opens the next candidate page over the unjudged
    /// ordinals, when the budget left any.
    next_candidates: Option<String>,
}

impl Outcome {
    /// Whether the provider could not answer for this view, on this page or
    /// on the page whose plan this one replays (its receipt is
    /// `unavailable` for a [transient](crate::providers::classification::is_transient)
    /// code): nothing was judged, and every candidate is shown as found.
    pub(crate) fn provider_unavailable(&self) -> bool {
        self.receipt.status == SelectionStatus::Unavailable
            && self
                .receipt
                .reason
                .as_deref()
                .and_then(crate::id::error_codes::parse)
                .is_some_and(crate::providers::classification::is_transient)
    }

    /// The ordinals of `total` candidates the view shows, ascending: the
    /// selected ones, or every one this page considers when the provider
    /// was unavailable.
    pub(crate) fn shown(&self, total: usize) -> Vec<usize> {
        if self.provider_unavailable() {
            (0..total)
                .filter(|&ordinal| self.considers(ordinal))
                .collect()
        } else {
            self.selected.clone()
        }
    }

    /// Whether the view leaves evidence out, so its unfiltered lead reaches
    /// more than the view shows.
    pub(crate) fn leaves_out(&self) -> bool {
        self.receipt.selected_items < self.receipt.total_items && !self.provider_unavailable()
    }

    /// Whether this candidate page judged `ordinal` (or left it unjudged);
    /// an ordinal outside its pool was judged by an earlier candidate page.
    pub(crate) fn considers(&self, ordinal: usize) -> bool {
        u32::try_from(ordinal).is_ok_and(|ordinal| in_runs(&self.pool, ordinal))
    }

    /// The `selection` token of `next.nextCandidatePage`: the same call
    /// judging the ordinals this page's budget left unjudged. `None` when
    /// none were left, or when the token would exceed what `tool`'s
    /// `selection` field accepts (the unjudged rest then stays unknown).
    pub(crate) fn next_candidates(&self, tool: ToolId) -> Option<&str> {
        let max = crate::contracts::query_schema_number(tool, None, "selection", "maxLength")
            .and_then(|n| usize::try_from(n).ok())?;
        self.next_candidates
            .as_deref()
            .filter(|token| token.len() <= max)
    }
}

pub(crate) enum Failure {
    Stale,
    Cancelled,
    /// The provider failed every judged candidate: the row reports this
    /// error, its `unavailable` receipt and an unfiltered lead, unless the
    /// failure is transient ([`unless_transient`]).
    Provider(Box<ProviderFailure>),
}

pub(crate) struct ProviderFailure {
    pub error: ProviderError,
    /// The finished `unavailable` outcome: nothing judged, its plan
    /// recording the provider's code, so a later page replays it without
    /// another provider call.
    pub outcome: Outcome,
}

/// A provider that cannot answer now never withholds evidence the reader
/// already has: [`select`]'s failure for a
/// [transient](crate::providers::classification::is_transient) code
/// becomes its finished `unavailable` outcome, whose view shows every
/// candidate ([`Outcome::shown`]), with the error the row warns with
/// ([`unclassified_warning`]). A refused credential, quota, content or size
/// error stays the failure: the caller must fix it, so it is never masked as
/// a page.
pub(crate) fn unless_transient(
    selected: Result<Outcome, Failure>,
) -> Result<(Outcome, Option<ProviderError>), Failure> {
    match selected {
        Err(Failure::Provider(failure))
            if crate::providers::classification::is_transient(failure.error.code) =>
        {
            let ProviderFailure { error, outcome } = *failure;
            Ok((outcome, Some(error)))
        }
        selected => selected.map(|outcome| (outcome, None)),
    }
}

/// The warning of a view shown unfiltered because the provider could not
/// answer: `subject` names the view ("this search"), `shown` what is shown
/// ("its hits are"); the provider's code, message and hints follow.
pub(crate) fn unclassified_warning(error: &ProviderError, subject: &str, shown: &str) -> String {
    let mut note = format!(
        "classification could not classify {subject} ({}: {}); {shown} shown unfiltered.",
        error.code, error.message
    );
    for hint in &error.hints {
        note.push(' ');
        note.push_str(hint);
    }
    note
}

impl ProviderFailure {
    /// The failed row's error with its receipt and `unfiltered` lead: the
    /// evidence stays reachable without classification.
    pub(crate) fn tool_error(self, unfiltered: Value) -> crate::response::result::ToolError {
        let ProviderError {
            code,
            message,
            hints,
        } = self.error;
        crate::response::result::ToolError {
            hints,
            calls: Some(Box::new(json!({ "unfiltered": unfiltered }))),
            selection: Some(Box::new(self.outcome.receipt)),
            ..crate::response::result::ToolError::new(code, message)
        }
    }

    /// [`Self::tool_error`] as a row's `data`.
    pub(crate) fn row(self, unfiltered: Value) -> Value {
        let error = self.tool_error(unfiltered);
        let mut data = json!({"errorCode":error.code, "error":error.message});
        if !error.hints.is_empty() {
            data["hints"] = json!(error.hints);
        }
        data["selection"] = serde_json::to_value(error.selection).unwrap_or(Value::Null);
        if let Some(calls) = error.calls {
            crate::response::result::attach_calls(&mut data, *calls);
        }
        data
    }
}

/// The one unfiltered recovery row: the classified read as an ordinary read
/// from its first window, without classification or private progress.
/// `keep` names cursors that are source identity here (a provider `page`).
pub(crate) fn unfiltered_row(query: &Value, keep: &[&str]) -> Value {
    let mut row = query.clone();
    if let Some(fields) = row.as_object_mut() {
        fields.remove("classification");
        for field in progress_fields() {
            if !keep.contains(&field.as_str()) {
                fields.remove(field);
            }
        }
    }
    row
}

/// [`unfiltered_row`] as the `next.unfiltered` lead of `tool`.
pub(crate) fn unfiltered(tool: ToolId, query: &Value, keep: &[&str]) -> Value {
    crate::response::result::Continuation::continued(tool, unfiltered_row(query, keep)).build()
}

fn progress_fields() -> &'static std::collections::BTreeSet<String> {
    static FIELDS: std::sync::OnceLock<std::collections::BTreeSet<String>> =
        std::sync::OnceLock::new();
    FIELDS.get_or_init(|| {
        ToolId::ALL
            .iter()
            .flat_map(|&tool| crate::contracts::state_fields(tool).iter().cloned())
            .collect()
    })
}

/// The receipt and warning helpers every classified reader shares; the
/// plan, selection and provider-page tests are in `selection/tests.rs`.
#[cfg(test)]
mod shared_tests {
    use super::*;
    use octocode_contract_types::SelectionScope;

    fn outcome(unjudged: Vec<[u32; 2]>) -> Outcome {
        let mut plan = plan::Plan::new(
            "binding".into(),
            3,
            SelectionStatus::Partial,
            SelectionScope::SelectedView,
            SelectionUnit::Chunks,
            None,
        );
        plan.selected = vec![0];
        plan.unjudged = unjudged;
        plan.finish()
    }

    #[test]
    fn related_items_are_decided_hidden_and_apart_from_unknown_ones() {
        let mut plan = plan::Plan::new(
            "binding".into(),
            3,
            SelectionStatus::Partial,
            SelectionScope::SelectedView,
            SelectionUnit::Chunks,
            None,
        );
        plan.selected = vec![0];
        plan.related = vec![2];
        let outcome = plan.finish();
        let receipt = &outcome.receipt;
        assert_eq!(receipt.related_items.map(u64::from), Some(1));
        assert_eq!((receipt.selected_items, receipt.unknown_items), (1, 1));
        assert_eq!(outcome.related, vec![2]);
        assert_eq!(outcome.shown(3), vec![0]);
        assert!(outcome.leaves_out());
        // A page with no related ordinal omits the count.
        assert_eq!(
            super::shared_tests::outcome(vec![]).receipt.related_items,
            None
        );
    }
}
