//! Bounded classification of sanitized evidence before it reaches the host.
//! The plan records disjoint selected/excluded partitions; unjudged evidence is
//! unknown. Replays need source identity and cardinality, not provider calls.
use crate::execution::{ExecutionContext, SelectionLimits};
use crate::id::ToolId;
use octocode_contract_types::ErrorCode;
use octocode_contract_types::{Selection, SelectionFailure, SelectionStatus};
use serde_json::{Value, json};

mod judge;
#[cfg(test)]
pub(crate) mod long_window_fake;
pub(crate) use judge::section_question;
mod plan;
mod provider_hits;
mod provider_page;
mod row_units;
pub(crate) use row_units::entry_name;
mod run;
mod saved;
#[cfg(test)]
mod tests;

use plan::in_runs;
pub(crate) use provider_page::apply_sanitized_rows;
use run::select_scoped;
pub(crate) use run::{needs_candidate_states, select};
pub(crate) use saved::SavedPlans;

#[derive(Clone)]
pub(crate) struct Runtime {
    pub limits: SelectionLimits,
    pub available: bool,
    pub judge: std::sync::Arc<dyn SelectionJudge>,
    /// The candidate pages the owning runtime judged, so a replayed cursor
    /// reads its page instead of billing the provider again.
    pub saved: std::sync::Arc<SavedPlans>,
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
    /// Characters one candidate's questions add to its request.
    fn question_chars(&self, _predicates: &[String]) -> usize {
        0
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
    pub(crate) long_context: crate::tools::judgment::LongContext,
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

    /// Serialized characters of the state.
    pub(crate) fn state_chars(&self) -> usize {
        self.evidence_chars
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

/// What an undecided view says of the `shown` units it lists.
pub(crate) fn undecided_note(shown: usize) -> String {
    format!(
        "Lists only the {} the selection's walk left undecided (judged unsure, unjudged or not judgeable), unfiltered, as the unfiltered lead of the page that decided them lists every hit (this view carries no lead of its own): they are not selected evidence, and the selection receipt of the page that decided them counts them.",
        octocode_engine::text::counted(shown, "hit", "hits")
    )
}

pub(crate) struct Outcome {
    pub selected: Vec<usize>,
    /// Judged related but not direct evidence: hidden, not unknown.
    pub related: Vec<usize>,
    /// Judged irrelevant; every other unselected ordinal is unknown.
    pub excluded: Vec<usize>,
    /// The unknown ordinals a next candidate page judges: past the page's
    /// budget, or failed because the provider could not answer. The rest of
    /// the unknown ones were judged unsure, or had no state to judge.
    pub unjudged: Vec<usize>,
    pub receipt: Selection,
    pub token: String,
    /// The ordinals this candidate page judged from (empty: all of them);
    /// the others were judged by an earlier candidate page.
    pool: Vec<[u32; 2]>,
    /// The token that opens the next candidate page over the unjudged
    /// ordinals, when the budget left any.
    next_candidates: Option<String>,
    /// The ordinals of the whole walk no page decided, ascending: the units
    /// `unknownItems` counts ([`plan::Plan::undecided_ordinals`]).
    undecided: Vec<usize>,
    /// The sealed plan this outcome finished, from which the undecided view's
    /// token is built when a reader asks for it.
    plan: plan::Plan,
    /// This outcome is an undecided view: `selected` holds the undecided
    /// ordinals of the walk it replays, nothing else is judged.
    lists_undecided: bool,
    /// What this page's long-context retries billed, when it made any
    /// ([`crate::tools::judgment::stats::ClassificationUsage::long_context_notice`]).
    pub(crate) long_context_notice: Option<String>,
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
    /// judging the ordinals this page's budget or a provider that could not
    /// answer left unjudged. `None` when none were left.
    pub(crate) fn next_candidates(&self) -> Option<&str> {
        self.next_candidates.as_deref()
    }

    /// The `selection` token that replays this page as exactly the units its
    /// walk left undecided, every candidate page's, unbilled:
    /// `next.unfilteredUnknown` of a reader whose pages replay a selection.
    /// `None` when none, or every unit of the walk, was left undecided. Built
    /// on demand: only a reader emitting that lead pays for it.
    pub(crate) fn undecided_view(&self) -> Option<String> {
        (!self.lists_undecided
            && !self.undecided.is_empty()
            && self.undecided.len() < self.plan.total as usize)
            .then(|| self.plan.undecided_view())
    }

    /// Whether this outcome is an undecided view ([`Self::undecided_view`]).
    pub(crate) fn lists_undecided(&self) -> bool {
        self.lists_undecided
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
/// ([`unclassified_notice`]). A refused credential, quota, content or size
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

/// The note of a classified page that left units out, in the receipt's own
/// counts over the whole walk: how many it kept, how many of them this page
/// shows when an earlier candidate page kept some, whether units wait for
/// `next.nextCandidatePage` or none is left to judge, how many it hid as
/// related near misses (complete is not full recall), and, when this page
/// shows none, that an empty classified result is no absence. A page that
/// shows everything it kept, with nothing left to judge, unknown or hidden,
/// has only counts the receipt states: no note.
pub(crate) fn kept_note(outcome: &Outcome, shown: usize) -> Option<String> {
    kept_note_with(outcome, shown, true)
}

/// [`kept_note`] for a reader that may not carry `next.nextCandidatePage`
/// yet: with `candidates_lead` false the note says the units are judged from
/// the read's last page, naming no channel the row lacks.
pub(crate) fn kept_note_with(
    outcome: &Outcome,
    shown: usize,
    candidates_lead: bool,
) -> Option<String> {
    let receipt = &outcome.receipt;
    if outcome.provider_unavailable()
        || receipt.total_items == 0
        || receipt.selected_items >= receipt.total_items
    {
        return None;
    }
    let counted = |count: usize| match receipt.unit {
        octocode_contract_types::SelectionUnit::Hits => {
            octocode_engine::text::counted(count, "hit", "hits")
        }
        octocode_contract_types::SelectionUnit::Chunks => {
            octocode_engine::text::counted(count, "chunk", "chunks")
        }
    };
    let count = |value: i64| usize::try_from(value).unwrap_or_default();
    let pending = outcome.unjudged.len();
    // Counts the receipt already states (`selectedItems`, `totalItems`) and
    // nothing to act on: this page shows all it kept, nothing awaits
    // judgment, no unit stays unknown, no near miss is hidden.
    if count(receipt.selected_items) == shown
        && shown > 0
        && pending == 0
        && receipt.unknown_items == 0
        && receipt.related_items.is_none()
    {
        return None;
    }
    let mut note = format!(
        "classification kept {} of {} that pass the classifiers",
        receipt.selected_items,
        counted(count(receipt.total_items))
    );
    if count(receipt.selected_items) != shown {
        note.push_str(&if shown == 0 {
            " (none on this page)".to_owned()
        } else {
            format!(" ({shown} on this page)")
        });
    }
    if pending > 0 {
        note.push_str(&if candidates_lead {
            format!(
                "; {} not judged yet: next.nextCandidatePage judges them",
                counted(pending)
            )
        } else {
            format!(
                "; {} not judged yet: nextCandidatePage on the last page judges them",
                counted(pending)
            )
        });
    } else if receipt.unknown_items > 0 {
        let unknown = count(receipt.unknown_items);
        note.push_str(&format!(
            "; none is left to judge, and {} {} unknown",
            counted(unknown),
            if unknown == 1 { "stays" } else { "stay" }
        ));
    }
    if let Some(related) = receipt.related_items {
        note.push_str(&format!(
            "; {} judged related (near misses, hidden): next.unfiltered shows them",
            counted(usize::try_from(related.get()).unwrap_or(usize::MAX))
        ));
    }
    note.push('.');
    if shown == 0 {
        note.push_str(
            " An empty classified result is not absence: next.unfiltered lists them all.",
        );
    }
    Some(note)
}

/// The view a reader shows unfiltered when the provider could not answer.
#[derive(Clone, Copy)]
pub(crate) enum Unclassified {
    /// A localSearch page: its hits.
    Search,
    /// A localFetch or ghGetFileContent read: its content.
    Read,
    /// A provider page (ghSearchCode): its files.
    Page,
}

/// The warning of a view shown unfiltered because the provider could not
/// answer: the view, then the provider's code, message and hints.
pub(crate) fn unclassified_notice(error: &ProviderError, view: Unclassified) -> String {
    let ProviderError {
        code,
        message,
        hints,
    } = error;
    let mut note = match view {
        Unclassified::Search => format!(
            "classification could not classify this search ({code}: {message}); its hits are shown unfiltered."
        ),
        Unclassified::Read => format!(
            "classification could not classify this read ({code}: {message}); its content is shown unfiltered."
        ),
        Unclassified::Page => format!(
            "classification could not classify this page ({code}: {message}); its files are shown unfiltered."
        ),
    };
    for hint in hints {
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
        crate::response::result::add_hints(&mut data, error.hints);
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
    crate::response::result::with_changes(query.clone(), &unfiltered_changes(keep))
}

/// The edits that make a classified query its unfiltered recovery
/// ([`unfiltered_row`]): classification and every private cursor not in
/// `keep` dropped. Apply with [`crate::response::result::edited_query`]
/// for a typed query.
pub(crate) fn unfiltered_changes(keep: &[&str]) -> Value {
    let mut changes = serde_json::Map::new();
    changes.insert("classification".into(), Value::Null);
    for field in progress_fields() {
        if !keep.contains(&field.as_str()) {
            changes.insert(field.clone(), Value::Null);
        }
    }
    Value::Object(changes)
}

/// How a classified result's recovery lead is reached.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RecoveryLead {
    /// `next.unfiltered`: the read again without classification, a fresh
    /// query once its private progress is dropped.
    Whole,
    /// `next.unfilteredUnknown` and `next.nextCandidatePage`: saved plan
    /// pages, reached only by a cursor of work the runtime kept.
    Saved,
}

/// The recovery lead a `next` channel name is; `None` for any other channel.
pub(crate) fn recovery_lead(channel: &str) -> Option<RecoveryLead> {
    match channel {
        "unfiltered" => Some(RecoveryLead::Whole),
        "unfilteredUnknown" | "nextCandidatePage" => Some(RecoveryLead::Saved),
        _ => None,
    }
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
mod shared_tests;
