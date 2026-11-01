//! The selection that fills a plan: bind the query and source, replay a
//! saved plan or judge the admitted candidates of its pool, and record the
//! decisions and failures.
use super::plan::{Plan, VERSION, decode};
use super::{
    Candidate, Decision, Failure, JudgeCall, JudgeFailure, Outcome, ProviderFailure, Runtime,
    admission,
};
use crate::execution::SelectionLimits;
use crate::tools::judgment::stats::ClassificationUsage;
use octocode_contract_types::{SelectionFailure, SelectionScope, SelectionStatus, SelectionUnit};
use serde_json::{Value, json};
use std::collections::HashSet;
use std::time::{Duration, Instant};

fn skip_reason(
    runtime: Option<&Runtime>,
    unsupported: Option<&'static str>,
    total: usize,
) -> Option<&'static str> {
    unsupported.or_else(|| {
        if !runtime.is_some_and(|runtime| runtime.available) {
            Some("classificationUnavailable")
        } else if total == 0 {
            Some("noCandidates")
        } else {
            None
        }
    })
}

/// Saved plans need cardinality and source binding, not candidate bodies;
/// a token opening a candidate page judges, so it needs them.
pub(crate) fn needs_candidate_states(
    runtime: Option<&Runtime>,
    prior: Option<&str>,
    unsupported: Option<&'static str>,
    total: usize,
) -> bool {
    prior.is_none_or(|token| decode(token).is_some_and(|plan| plan.opens))
        && skip_reason(runtime, unsupported, total).is_none()
}

/// Bind assessment semantics, including optional research context. Pagination
/// cursors are excluded; canonical source identity owns path spelling.
pub(super) fn binding(
    query: &Value,
    source: &str,
    limits: SelectionLimits,
    runtime: Option<&Runtime>,
) -> String {
    let mut selected = query.clone();
    if let Some(map) = selected.as_object_mut() {
        for field in super::progress_fields() {
            map.remove(field);
        }
        map.remove("path");
    }
    crate::digest::json_sha256(&octocode_engine::digest::canonical_json(
        json!({
            "version":VERSION, "source":source, "query":selected,
            "settings":{
                "available":runtime.is_some_and(|runtime| runtime.available),
                "maxCandidates":limits.max_candidates, "maxChars":limits.max_chars,
                "timeoutMs":limits.timeout.as_millis(),
            },
            "provider":runtime.map(|runtime| runtime.judge.identity()),
        }),
        octocode_engine::digest::Nulls::Drop,
    ))
}

pub(crate) fn select(
    runtime: Option<&Runtime>,
    query: &Value,
    source: &str,
    prior: Option<&str>,
    candidates: &[Candidate],
    unsupported: Option<&'static str>,
    unit: SelectionUnit,
) -> Result<Outcome, Failure> {
    select_scoped(
        runtime,
        query,
        source,
        prior,
        candidates,
        unsupported,
        SelectionScope::SelectedView,
        unit,
    )
}

pub(super) fn select_scoped(
    runtime: Option<&Runtime>,
    query: &Value,
    source: &str,
    prior: Option<&str>,
    candidates: &[Candidate],
    unsupported: Option<&'static str>,
    scope: SelectionScope,
    unit: SelectionUnit,
) -> Result<Outcome, Failure> {
    let finish = |plan: Plan, usage: &ClassificationUsage| {
        let mut outcome = plan.finish();
        outcome.receipt.usage = Some(usage.output());
        outcome
    };
    let mut usage = ClassificationUsage::default();
    let limits = runtime.map_or(
        SelectionLimits {
            max_candidates: 0,
            max_chars: 0,
            timeout: Duration::ZERO,
        },
        |r| r.limits,
    );
    let bound = binding(query, source, limits, runtime);
    // A saved plan replays; a token opening a candidate page judges its pool.
    let mut pool = Vec::new();
    if let Some(token) = prior {
        let plan = decode(token)
            .filter(|plan| plan.valid(&bound, candidates.len(), scope, unit))
            .ok_or(Failure::Stale)?;
        if !plan.opens {
            return Ok(finish(plan, &usage));
        }
        pool = plan.pool;
    }
    let mut plan = Plan::new(
        bound,
        candidates.len(),
        SelectionStatus::Partial,
        scope,
        unit,
        None,
    );
    plan.pool = pool;
    let pool_size = plan.pool_size();
    if let Some(reason) = skip_reason(runtime, unsupported, candidates.len()) {
        plan.status = if candidates.is_empty() && reason == "noCandidates" {
            SelectionStatus::Complete
        } else {
            SelectionStatus::Unavailable
        };
        plan.reason = Some(reason.into());
        return Ok(finish(plan, &usage));
    }
    let mut admitted = Vec::new();
    let mut chars = 0usize;
    for ordinal in admission::order(candidates, query, unit)
        .into_iter()
        .filter(|&ordinal| plan.in_pool(ordinal))
    {
        let candidate = &candidates[ordinal];
        if admitted.len() == limits.max_candidates {
            break;
        }
        if candidate.fits_budget(limits)
            && candidate.evidence_chars <= limits.max_chars.saturating_sub(chars)
        {
            chars += candidate.evidence_chars;
            admitted.push(ordinal);
        }
    }
    let eligible = candidates
        .iter()
        .enumerate()
        .filter(|(ordinal, candidate)| candidate.state.is_some() && plan.in_pool(*ordinal))
        .count();
    let oversized = candidates.iter().enumerate().any(|(ordinal, candidate)| {
        plan.in_pool(ordinal)
            && candidate.state.is_some()
            && candidate.evidence_chars > limits.max_chars
    });
    let omitted_reason = if oversized {
        "candidateStateTooLarge"
    } else {
        match (eligible < pool_size, admitted.len() < eligible) {
            (true, true) => "candidateEligibilityAndBudget",
            (true, false) => "candidateIneligible",
            _ => "candidateBudget",
        }
    };
    plan.mark_unjudged(candidates, &admitted, limits);
    if admitted.is_empty() {
        plan.reason = Some(omitted_reason.into());
        return Ok(finish(plan, &usage));
    }
    let Some(runtime) = runtime else {
        return Ok(finish(plan, &usage));
    };
    let context = &query["classification"];
    let predicates = context["predicates"]
        .as_array()
        .filter(|v| !v.is_empty())
        .ok_or(Failure::Stale)?
        .iter()
        .map(|classifier| classifier.as_str().map(str::to_owned))
        .collect::<Option<Vec<_>>>()
        .ok_or(Failure::Stale)?;
    let mut failures = Vec::new();
    let outcomes = runtime.judge.select(JudgeCall {
        failures: &mut failures,
        usage: &mut usage,
        candidates,
        admitted: &admitted,
        predicates: &predicates,
        research_context: context["context"].as_str(),
        long_context: context["longContext"].as_bool().unwrap_or(false),
        limits: runtime.limits,
        handle: &runtime.handle,
        context: &runtime.context,
    });
    if runtime.context.cancellation.is_cancelled() || Instant::now() >= runtime.context.deadline {
        return Err(Failure::Cancelled);
    }
    match outcomes {
        Ok(outcomes) => {
            record(&mut plan, candidates.len(), &admitted, outcomes, failures);
            plan.settle(admitted.len(), omitted_reason);
        }
        Err(JudgeFailure::Unavailable(reason)) => {
            plan.status = SelectionStatus::Unavailable;
            plan.reason = Some(reason);
            plan.unjudged.clear();
        }
        Err(JudgeFailure::Provider(error)) => {
            plan.status = SelectionStatus::Unavailable;
            plan.unjudged.clear();
            plan.reason = Some(error.code.to_string());
            return Err(Failure::Provider(Box::new(ProviderFailure {
                error,
                outcome: finish(plan, &usage),
            })));
        }
    }
    Ok(finish(plan, &usage))
}

/// Records the judge's decisions on admitted ordinals (a repeated decision
/// leaves its ordinal unknown) and the first failure of each admitted
/// ordinal still unknown.
fn record(
    plan: &mut Plan,
    total: usize,
    admitted: &[usize],
    outcomes: Vec<(usize, Decision)>,
    failures: Vec<SelectionFailure>,
) {
    let admitted_set = admitted.iter().copied().collect::<HashSet<_>>();
    let mut seen = HashSet::new();
    let mut decisions = vec![Decision::Unknown; total];
    for (ordinal, decision) in outcomes {
        if !admitted_set.contains(&ordinal) {
            continue;
        }
        if !seen.insert(ordinal) {
            decisions[ordinal] = Decision::Unknown;
            continue;
        }
        decisions[ordinal] = decision;
    }
    // Pool ordinals are `u32` runs, so every decided ordinal has a `u32` id.
    for (ordinal, decision) in (0..=u32::MAX).zip(&decisions) {
        match decision {
            Decision::Selected => plan.selected.push(ordinal),
            Decision::Related => plan.related.push(ordinal),
            Decision::Excluded => plan.excluded.push(ordinal),
            Decision::Unknown => {}
        }
    }
    plan.failures = failures
        .into_iter()
        .filter(|failure| {
            usize::try_from(failure.candidate).is_ok_and(|ordinal| {
                admitted_set.contains(&ordinal) && decisions[ordinal] == Decision::Unknown
            })
        })
        .map(|mut failure| {
            failure.error = sanitized(&failure.error);
            if let Some(hints) = &mut failure.hints {
                for hint in &mut hints.0 {
                    *hint = sanitized(hint);
                }
            }
            failure
        })
        .collect();
    plan.failures.sort_by_key(|failure| failure.candidate);
    plan.failures.dedup_by_key(|failure| failure.candidate);
}

/// The replay token is opaque to output sanitization, so operational
/// messages are sanitized before it is encoded.
fn sanitized<T>(text: &str) -> T
where
    T: TryFrom<String>,
    T::Error: std::fmt::Debug,
{
    #[expect(
        clippy::expect_used,
        reason = "sanitization preserves nonempty text or substitutes a nonempty redaction marker"
    )]
    let sanitized = crate::security::ContentSecurity
        .sanitize_text(text, None)
        .content
        .try_into()
        .expect("sanitized nonempty text");
    sanitized
}
