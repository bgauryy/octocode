//! Admission rules for clasify rows that the row schema cannot express:
//! a source reader this runtime does not enable, and a plan whose requests
//! pass what the row bounds ([`admit`]).
use serde_json::Value;

/// The reader a row's source needs: its tool, or localFetch for a URL (its
/// capture is read back through localFetch).
fn source_reader(source: &Value) -> &str {
    source
        .get("tool")
        .and_then(Value::as_str)
        .unwrap_or(octocode_runtime_base::id::ToolId::LocalFetch.as_str())
}

/// One issue for a row whose source reader this runtime does not enable.
/// MCP rejects the same input through its availability-scoped schema.
pub fn unavailable_source_issues(
    query: &Value,
    prefix: &[String],
    availability: &octocode_runtime_base::execution::availability::ToolAvailability,
) -> Vec<octocode_runtime_base::contracts::ValidationIssue> {
    let mut sources = Vec::new();
    if let Some(source) = query.get("source") {
        sources.push((source, vec!["source".to_owned()]));
    }
    if let Some(resources) = query["resources"].as_object() {
        for (key, source) in resources {
            sources.push((source, vec!["resources".into(), key.clone()]));
        }
    }
    if let Some(assets) = query["assets"].as_object() {
        for (key, asset) in assets {
            sources.push((
                &asset["source"],
                vec!["assets".into(), key.clone(), "source".into()],
            ));
        }
    }
    sources.into_iter().filter_map(|(source, suffix)| {
    let reader = source_reader(source);
    if availability.is_available(reader) { return None; }
    let reason = availability
        .get(reader)
        .map_or_else(|| "no such tool.".to_owned(), octocode_runtime_base::execution::availability::Availability::reason_text);
    let mut path = prefix.to_vec();
    path.extend(suffix);
    Some(octocode_runtime_base::contracts::ValidationIssue {
        rule_id: "clasify.source-reader-available".into(),
        path,
        message: format!(
            "This source reads through {reader}, which is disabled in this runtime: {reason} Supply the data as content instead."
        ),
        schema: None,
        received: None,
    })
    }).collect()
}

/// A section plan is frozen for its later batches and its pinned reads.
/// Refuse it before capture and billing when the host keeps nothing for a
/// later call. A source evaluation answers in one call and offers a fresh
/// read instead ([`octocode_tools_shared::source_capture::fresh_read`]).
pub(super) fn unretainable_source(row: &super::Row, can_resume: bool) -> Option<super::RowOutcome> {
    use super::{Input, RowOutcome};
    use octocode_contract_types::ErrorCode;
    pub(crate) use octocode_tools_shared::judgment::ClassificationError;

    if can_resume || !matches!(row.input, Input::Sections(_)) {
        return None;
    }
    Some(RowOutcome::Judgment {
        judged: Err(ClassificationError::new(
            ErrorCode::ClassificationContextUnavailable,
            "This runtime keeps nothing for a later call, so it cannot hold a section plan for its next batch or its pinned reads; no source was read and no classification provider was called.",
            "Section plans across CLI calls need storage.mode persistent, or one running MCP server; operation:\"evaluate\" with this source answers here.",
        )),
        captured: None,
        assets: None,
        identity: None,
        price: None,
    })
}

/// A row's requests, planned before any is sent: its first round, every
/// request its refusals, splits and re-asks can add (transient retries
/// aside), and whether that bound holds (`false`: a capped source the last
/// window may refuse, whose sections may follow within its cap); the units
/// it judges and the input tokens its first round costs at most.
#[derive(Clone, Copy, Debug)]
pub(super) struct Plan {
    pub first: u64,
    pub at_most: u64,
    pub bounded: bool,
    pub units: usize,
    pub tokens: u64,
    /// The provider's request window `tokens` was priced in, when they are a
    /// price (`None`: an unpriced plan): what one named request may cost.
    pub window: Option<u64>,
}

impl Plan {
    /// Whether the first round costs no more input tokens than the `named`
    /// requests an unset row may send could: one window each. Only a bounded,
    /// priced plan states a cost to compare.
    fn within_named_spend(&self, named: u64) -> bool {
        self.bounded
            && self
                .window
                .is_some_and(|window| self.tokens <= named.saturating_mul(window))
    }

    /// The window each first-round request was priced at whole: no request
    /// of the model was taken here yet, so nothing bounds the provider's own
    /// prompt below it ([`octocode_tools_shared::judgment::Evaluator::tokens_at_most`]).
    fn whole_windows(&self) -> Option<u64> {
        self.window
            .filter(|&window| self.first > 0 && self.tokens >= self.first.saturating_mul(window))
    }
}

/// How a planned row runs: under its cap (`None`: an unbounded plan, which
/// states no ceiling), at its price.
pub(super) struct Admitted {
    pub cap: Option<octocode_tools_shared::judgment::RequestCap>,
    pub price: super::reply::Price,
}

/// Admit `plan` before anything is sent. With `maxRequests` (`max_requests`), the row runs
/// when its first round fits the cap and is held to it: re-asks, splits and
/// the `retries` it shares take only what the first round leaves, and what
/// the cap leaves unanswered is offered again at its price. Without it, the
/// row runs when its first round sends no more requests than the `named`
/// sources, resources and items it names, or costs no more input tokens than
/// those could ([`Plan::within_named_spend`]), held to its own ceiling:
/// requests its own expansion adds (a tool's rows, a glob's matches, each hit
/// alone) past that spend are a price nobody accepted. Either way a plan past
/// its bound sends nothing and states its price, with its whole ceiling for
/// `next.confirm`.
pub(super) fn admit(
    max_requests: Option<u64>,
    plan: Plan,
    named: u64,
    retries: u64,
) -> Result<Admitted, Box<super::reply::OverBudget>> {
    use octocode_tools_shared::judgment::RequestCap;
    let over = |limit| {
        Box::new(super::reply::OverBudget {
            units: plan.units,
            price: super::reply::Price {
                planned: plan.first,
                ceiling: Some(RequestCap::ceiling(plan.at_most, retries)),
                room: retries,
                tokens: Some(plan.tokens),
            },
            limit,
            room: retries,
            spent: 0,
            whole_windows: plan.whole_windows(),
            learned: 0,
        })
    };
    match max_requests {
        Some(max) if plan.first > max => Err(over(super::reply::Limit::Max(max))),
        Some(max) => Ok(within(max, plan, retries)),
        None if plan.first > named && !plan.within_named_spend(named) => {
            Err(over(super::reply::Limit::Named(named)))
        }
        None => Ok(own_ceiling(plan, retries)),
    }
}

/// A row of `plan` held to `max`: its requests past the first round and the
/// `retries` it shares take only what the cap leaves, so its stated ceiling
/// never passes it. An unbounded plan (a source that may be judged in
/// sections) keeps the whole cap for them.
pub(super) fn within(max: u64, plan: Plan, retries: u64) -> Admitted {
    use octocode_tools_shared::judgment::RequestCap;
    let at_most = if plan.bounded {
        plan.at_most.min(max)
    } else {
        max
    };
    let room = retries.min(max - at_most);
    Admitted {
        cap: Some(RequestCap::held(at_most, room)),
        price: super::reply::Price {
            planned: plan.first,
            ceiling: Some(RequestCap::ceiling(at_most, room)),
            room,
            tokens: Some(plan.tokens),
        },
    }
}

/// An uncapped row of `plan` held to its own ceiling, its stated windows'
/// ([`octocode_tools_shared::judgment::RequestCap::implicit`]); an unbounded
/// one states none and has none.
pub(super) fn own_ceiling(plan: Plan, retries: u64) -> Admitted {
    use octocode_tools_shared::judgment::RequestCap;
    Admitted {
        cap: plan
            .bounded
            .then(|| RequestCap::implicit(plan.at_most, retries)),
        price: super::reply::Price {
            planned: plan.first,
            ceiling: plan
                .bounded
                .then(|| RequestCap::ceiling(plan.at_most, retries)),
            room: retries,
            tokens: plan.bounded.then_some(plan.tokens),
        },
    }
}
