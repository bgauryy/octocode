//! The selection that fills a plan: bind the query and source, replay a
//! saved plan or judge the admitted candidates of its pool, and record the
//! decisions and failures.
use super::plan::{Plan, VERSION, decode};
use super::{
    Candidate, Decision, Failure, JudgeCall, JudgeFailure, Outcome, ProviderFailure, Runtime,
};
use crate::judgment::stats::ClassificationUsage;
use octocode_contract_types::{SelectionFailure, SelectionScope, SelectionStatus, SelectionUnit};
use serde_json::{Value, json};
use std::collections::HashSet;

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
pub fn needs_candidate_states(
    runtime: Option<&Runtime>,
    prior: Option<&str>,
    unsupported: Option<&'static str>,
    total: usize,
) -> bool {
    prior.is_none_or(|token| decode(token).is_some_and(|plan| plan.opens))
        && skip_reason(runtime, unsupported, total).is_none()
}

/// Bind assessment semantics, including optional research context. Pagination
/// cursors are excluded; canonical source identity owns path spelling; a
/// field stated at its contract default binds as its omission
/// ([`octocode_runtime_base::response::result::canonical_row`]), as a continuation drops it.
pub fn binding(
    tool: octocode_runtime_base::id::ToolId,
    query: &Value,
    source: &str,
    runtime: Option<&Runtime>,
) -> String {
    let mut selected = query.clone();
    octocode_runtime_base::response::result::canonical_row(tool, &mut selected);
    if let Some(map) = selected.as_object_mut() {
        for field in super::progress_fields() {
            map.remove(field);
        }
        map.remove("path");
    }
    octocode_runtime_base::digest::json_sha256(&octocode_engine::digest::canonical_json(
        json!({
            "version":VERSION, "source":source, "query":selected,
            "available":runtime.is_some_and(|runtime| runtime.available),
            "provider":runtime.map(|runtime| runtime.judge.identity()),
        }),
        octocode_engine::digest::Nulls::Drop,
    ))
}

pub fn select(
    runtime: Option<&Runtime>,
    tool: octocode_runtime_base::id::ToolId,
    query: &Value,
    source: &str,
    prior: Option<&str>,
    candidates: &[Candidate],
    unsupported: Option<&'static str>,
    unit: SelectionUnit,
) -> Result<Outcome, Failure> {
    select_scoped(
        runtime,
        tool,
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
    tool: octocode_runtime_base::id::ToolId,
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
        outcome.long_context_notice = usage.long_context_notice();
        outcome
    };
    let mut usage = ClassificationUsage::default();
    let bound = binding(tool, query, source, runtime);
    // A page is saved under the token that opened it, or under its binding
    // when a fresh call opened it: the same query, source and judge is the
    // same judgment, and a judgment bills every time it is made.
    let opening = prior.map_or_else(|| format!("fresh:{bound}"), str::to_owned);
    let saved =
        runtime.map(|runtime| (opening.as_str(), &*runtime.saved, runtime.limits.max_chars));
    // A saved plan replays; a token opening a candidate page judges its pool.
    let (mut pool, mut carried) = (Vec::new(), super::plan::Tally::default());
    if let Some(token) = prior {
        let plan = decode(token)
            .filter(|plan| plan.valid(&bound, candidates.len(), scope, unit))
            .ok_or(Failure::Stale)?;
        if !plan.opens {
            return Ok(finish(plan, &usage));
        }
        // The cursor was read before: its page is as it was judged then.
        if let Some(page) = saved.and_then(|(token, saved, max_chars)| saved.get(token, max_chars))
            && page
                .plan
                .valid(&plan.binding, candidates.len(), scope, unit)
        {
            return Ok(finish(page.plan, &page.usage));
        }
        (pool, carried) = (plan.pool, plan.carried);
    } else if let Some(page) =
        saved.and_then(|(opening, saved, max_chars)| saved.get(opening, max_chars))
        && page.plan.valid(&bound, candidates.len(), scope, unit)
    {
        return Ok(finish(page.plan, &page.usage));
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
    plan.carried = carried;
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
    let context = &query["classification"];
    let predicates = context["predicates"]
        .as_array()
        .filter(|v| !v.is_empty())
        .ok_or(Failure::Stale)?
        .iter()
        .map(|classifier| classifier.as_str().map(str::to_owned))
        .collect::<Option<Vec<_>>>()
        .ok_or(Failure::Stale)?;
    let Some(runtime) = runtime else {
        return Ok(finish(plan, &usage));
    };
    let question_chars = runtime.judge.question_chars(&predicates);
    let charge = |candidate: &Candidate| candidate.state_chars().saturating_add(question_chars);
    let (admitted, chars, omitted_reason) = admit(
        &plan,
        candidates,
        runtime.limits.max_chars,
        pool_size,
        &charge,
    );
    plan.mark_unjudged(candidates, &admitted);
    if admitted.is_empty() {
        // Nothing in the pool had a state to judge.
        plan.unanswered = octocode_engine::num::saturating_u32(pool_size);
        plan.reason = Some(omitted_reason.into());
        return Ok(finish(plan, &usage));
    }
    let mut failures = Vec::new();
    let outcomes = runtime.judge.select(JudgeCall {
        failures: &mut failures,
        usage: &mut usage,
        candidates,
        admitted: &admitted,
        predicates: &predicates,
        research_context: context["context"].as_str(),
        long_context: crate::judgment::LongContext::from_field(context["longContext"].as_bool()),
        handle: &runtime.handle,
        context: &runtime.context,
    });
    if runtime.context.check().is_err() {
        return Err(Failure::Cancelled);
    }
    let mut keep = false;
    match outcomes {
        Ok(outcomes) => {
            record(&mut plan, candidates.len(), &admitted, outcomes, failures);
            plan.settle(admitted.len(), omitted_reason);
            keep = !plan.failures.iter().any(|failure| {
                octocode_runtime_base::providers::classification::is_transient(failure.error_code)
            });
            // The rest of the walk, at the rate this page measured.
            let rest = plan
                .unjudged_ordinals()
                .map(|ordinal| charge(&candidates[ordinal as usize]))
                .sum::<usize>();
            usage.estimate_rest(chars, rest);
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
    let outcome = finish(plan, &usage);
    // A page the provider answered for good is what a replay of its cursor
    // reads; one that left candidates to a retry judges again.
    if let Some((token, saved, max_chars)) = saved.filter(|_| keep) {
        saved.put(token, max_chars, &outcome.plan, &usage, &runtime.context);
    }
    Ok(outcome)
}

/// A page charges states and questions in source order until its
/// checkpoint: the first candidate always runs, and the next page takes the
/// rest, so nothing is lost to the budget. Returns the admitted ordinals,
/// their characters and why the pool's rest was left out.
fn admit(
    plan: &Plan,
    candidates: &[Candidate],
    max_chars: usize,
    pool_size: usize,
    charge: &dyn Fn(&Candidate) -> usize,
) -> (Vec<usize>, usize, &'static str) {
    let mut admitted = Vec::new();
    let mut chars = 0usize;
    for (ordinal, candidate) in candidates.iter().enumerate() {
        if candidate.state.is_none() || !plan.in_pool(ordinal) {
            continue;
        }
        if !admitted.is_empty() && chars.saturating_add(charge(candidate)) > max_chars {
            break;
        }
        chars = chars.saturating_add(charge(candidate));
        admitted.push(ordinal);
    }
    let eligible = candidates
        .iter()
        .enumerate()
        .filter(|(ordinal, candidate)| candidate.state.is_some() && plan.in_pool(*ordinal))
        .count();
    let reason = match (eligible < pool_size, admitted.len() < eligible) {
        (true, true) => "candidateEligibilityAndBudget",
        (true, false) => "candidateIneligible",
        _ => "candidateBudget",
    };
    (admitted, chars, reason)
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
    // A provider that could not answer decided nothing: a later page judges
    // these again, and they are reachable as they are until then.
    let retried = plan
        .failures
        .iter()
        .filter(|failure| {
            octocode_runtime_base::providers::classification::is_transient(failure.error_code)
        })
        .filter_map(|failure| u32::try_from(failure.candidate).ok())
        .collect::<Vec<_>>();
    plan.retry_later(retried.into_iter());
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
    let sanitized = octocode_runtime_base::security::ContentSecurity::new()
        .sanitize_text(text, None)
        .content
        .into_owned()
        .try_into()
        .expect("sanitized nonempty text");
    sanitized
}
