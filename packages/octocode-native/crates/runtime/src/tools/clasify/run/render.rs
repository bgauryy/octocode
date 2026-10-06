//! Rendering captured and judged pages into the matrix output: resources,
//! the located `best`, literal-target tips, and the `next.clasify` walk.
use super::capture::page_context_mut;
use super::*;

/// The resource a walk resumes: its own fields, reading the continuation's
/// tool and query (and evidence mode, when the continuation names one).
pub(super) fn continued_resource(resource: &Value, continuation: &Value) -> Value {
    let mut pending = resource.clone();
    for field in ["tool", "query", "candidateEvidence"] {
        if let Some(value) = continuation.get(field) {
            pending[field] = value.clone();
        }
    }
    pending
}

/// Rendered resources and what the matrix output and receipts need from them.
pub(super) struct Rendered {
    pub(super) resources: Vec<Value>,
    pub(super) continuations: Vec<Value>,
    pub(super) usage: Vec<Value>,
    pub(super) locate_reads: Vec<LocateRead>,
    pub(super) read_failures: Vec<Option<crate::tools::result::FailureKind>>,
    pub(super) judged: bool,
    /// Resources with content past their captured pages (index, and the
    /// search candidates the read reported past them).
    pub(super) withheld: Vec<(usize, Option<u64>)>,
}

/// Render every captured resource with its page assessments (consumed in
/// resource and page order).
pub(super) fn render_captured(
    captured: Vec<CapturedResource<'_>>,
    assessments: Vec<PageAssessment>,
    questions: &[Value],
    dispatcher: &DomainDispatcher,
    debug: bool,
) -> Result<Rendered, ExecutionError> {
    let question_ids = questions
        .iter()
        .map(|question| &question["id"])
        .collect::<Vec<_>>();
    let mut assessments = assessments.into_iter();
    let mut rendered = Rendered {
        resources: Vec::with_capacity(captured.len()),
        continuations: Vec::new(),
        usage: Vec::new(),
        locate_reads: Vec::new(),
        read_failures: Vec::new(),
        judged: false,
        withheld: Vec::new(),
    };
    for (resource_index, captured_resource) in captured.into_iter().enumerate() {
        let CapturedResource {
            resource,
            pages,
            continuation,
            uncaptured,
        } = captured_resource;
        let mut outcomes = Vec::with_capacity(pages.len());
        let mut page_usage = Vec::with_capacity(pages.len());
        for (page_index, mut page) in pages.into_iter().enumerate() {
            output::host_receipt(page_context_mut(&mut page), &dispatcher.paths);
            let (outcome, usage) = match page {
                CapturedPage::Failed { error, context } => {
                    rendered.read_failures.push(error.failure);
                    let outcome = PageOutcome::Failed {
                        error,
                        receipt: context,
                    };
                    (outcome, None)
                }
                CapturedPage::Ready { context, .. } => {
                    let assessed = assessments.next().ok_or(ExecutionError::WorkerFailed)?;
                    if (assessed.0, assessed.1) != (resource_index, page_index) {
                        return Err(ExecutionError::WorkerFailed);
                    }
                    rendered.judged = true;
                    rendered.record_page(resource, context, assessed.2, assessed.3)
                }
            };
            outcomes.push(outcome);
            page_usage.push(usage);
        }
        let has_continuation = continuation.is_some();
        if has_continuation {
            rendered.withheld.push((resource_index, uncaptured));
        }
        if let Some(mut context) = continuation {
            if let Some(tool) = tool_of(&context)
                && let Some(query) = context.get_mut("query")
            {
                crate::response::continuations::compact_input(tool.as_str(), query);
            }
            rendered
                .continuations
                .push(continued_resource(resource, &context));
        }
        let mut rendered_resource =
            output::resource(&resource["id"], &question_ids, outcomes, has_continuation);
        if debug
            && let Some(pages) = rendered_resource["pages"].as_array_mut()
            && pages.len() == page_usage.len()
        {
            for (page, usage) in pages.iter_mut().zip(page_usage) {
                if let Some(usage) = usage {
                    page["usage"] = usage;
                }
            }
        }
        rendered.resources.push(rendered_resource);
    }
    if assessments.next().is_some() {
        return Err(ExecutionError::WorkerFailed);
    }
    Ok(rendered)
}

impl Rendered {
    /// One judged page: its usage records, its locate read, and its outcome
    /// with the page's usage summary.
    fn record_page(
        &mut self,
        resource: &Value,
        context: Value,
        answers: Vec<Result<Value, ClassificationError>>,
        usage: Option<Value>,
    ) -> (PageOutcome, Option<Value>) {
        let records = match usage {
            Some(usage) => match usage.get("calls").and_then(Value::as_array) {
                Some(calls) => calls.clone(),
                None => vec![usage],
            },
            None => Vec::new(),
        };
        let summary = usage_summary(&records);
        self.usage.extend(records);
        if let Some(read) = output::read_template(&context) {
            self.locate_reads.push(LocateRead {
                resource_id: resource["id"].as_str().unwrap_or_default().to_owned(),
                path: context
                    .pointer("/source/path")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                scope: context["scope"]["startLine"]
                    .as_u64()
                    .zip(context["scope"]["endLine"].as_u64()),
                read: read.clone(),
            });
        }
        let outcome = PageOutcome::Assessed {
            receipt: context,
            answers,
        };
        (outcome, Some(summary))
    }
}

/// The matrix output: rendered resources, the located `best`, literal-target
/// tips, and the walk's next steps.
pub(super) fn matrix_output(
    query: &Value,
    resources: &[Value],
    mut rendered: Vec<Value>,
    continuations: Vec<Value>,
    withheld: &[(usize, Option<u64>)],
    locate_reads: &[LocateRead],
) -> Value {
    let mut output = json!({"id":query["id"]});
    let locate_targets = locate_targets(query);
    let locate_ids = locate_targets.iter().map(|(id, _)| *id).collect::<Vec<_>>();
    // `carry` is the running best from earlier calls of this walk; the merged
    // ranking is file-wide on the final call and travels in next.clasify.
    let best = rank_locate(&rendered, &locate_ids, query.get("carry"));
    let visible = best
        .as_ref()
        .and_then(|best| readable_best(best, !continuations.is_empty()))
        // Public rows gain an exact read; `carry` keeps the copyable rows.
        .map(|visible| with_row_reads(visible, locate_reads));
    drop_redundant_page_reads(&mut rendered, visible.as_ref());
    // A strong window needs no prose: its exact read is `hints.read`, which
    // comes before the walk in `next.clasify`.
    if let Some(visible) = visible {
        output["best"] = visible;
    }
    let (tips, literal) = literal_routes(query, resources);
    if let Some(tips) = tips {
        // Already in its public shape, so the tips keep their place ahead of
        // the walk.
        output[crate::response::channels::HINTS_KEY] = tips;
    }
    // A walk with no answer (e.g. the provider refused every page) has no
    // next step: continuing would skip the failed pages.
    let answered = rendered
        .iter()
        .any(|resource| resource["coverage"] != "error");
    if !answered {
        disclose_withheld(&mut rendered, withheld);
    }
    output["resources"] = Value::Array(rendered);
    if answered && !continuations.is_empty() {
        output["next"] = json!({(ToolId::Clasify.as_str()): walk(query, continuations, best)});
    }
    if let Some(literal) = literal {
        output["next"]["textSearch"] = literal;
    }
    output
}

/// A walk that judged nothing offers no `next.clasify`, so each resource
/// with content past its captured pages says what stays unread (with the
/// search candidate count when the read reported one).
fn disclose_withheld(rendered: &mut [Value], withheld: &[(usize, Option<u64>)]) {
    for &(index, uncaptured) in withheld {
        let Some(resource) = rendered.get_mut(index).and_then(Value::as_object_mut) else {
            continue;
        };
        let unread = match uncaptured {
            Some(count) if count > 0 => format!("{count} more search candidates were not captured"),
            _ => "Content past the captured pages was not read".to_owned(),
        };
        let limitation = json!(format!(
            "{unread}; nothing was judged, so no next.clasify is offered: rerun this matrix once the provider answers."
        ));
        match resource
            .get_mut("limitations")
            .and_then(Value::as_array_mut)
        {
            Some(limitations) => limitations.push(limitation),
            None => {
                resource.insert("limitations".into(), json!([limitation]));
            }
        }
    }
}

/// The matrix's locate questions as `(id, ask)`.
pub(super) fn locate_targets(query: &Value) -> Vec<(&str, &str)> {
    query["questions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|question| clasify::is_locate(question))
        .filter_map(|question| Some((question["id"].as_str()?, question["ask"].as_str()?)))
        .collect()
}

/// Literal-target tips (public `hints` shape) and the exact localSearch for
/// them. Both are local facts, so a call the provider never judged routes too.
pub(super) fn literal_routes(query: &Value, resources: &[Value]) -> (Option<Value>, Option<Value>) {
    let targets = locate_targets(query);
    let tips = targets
        .iter()
        .filter_map(|(_, target)| literal_target_hint(target))
        .collect::<Vec<_>>();
    let tips =
        (!tips.is_empty()).then(|| json!({(crate::response::channels::HINT_TEXT_KEY): tips}));
    (
        tips,
        literal_search(targets.iter().map(|(_, target)| *target), resources),
    )
}

/// next.clasify: the same matrix over the continued resources, with the
/// running ranking in `carry`.
pub(super) fn walk(query: &Value, resources: Vec<Value>, best: Option<Value>) -> Value {
    let mut next = json!({
        "id":query["id"],
        "resources":resources,
        "questions":query["questions"]
    });
    if let Some(best) = best {
        next["carry"] = best;
    }
    copy_brief(&mut next, query);
    // A debug walk stays one, as every other tool's continuation keeps
    // `debug`: the receipt shape does not change mid-walk.
    if query.get("debug").and_then(Value::as_bool) == Some(true) {
        next["debug"] = json!(true);
    }
    next
}

/// A matrix that ran no read and no provider request: every resource fails
/// with `error` once, and the caller reruns the same input.
pub(super) fn unjudged_matrix(
    query: &Value,
    error: &ClassificationError,
    limitation: &str,
) -> DomainResult {
    let resources = query["resources"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|resource| {
            output::resource(
                &resource["id"],
                &[],
                vec![PageOutcome::Failed {
                    error: error.clone(),
                    receipt: json!({"limitations":[limitation]}),
                }],
                false,
            )
        })
        .collect::<Vec<_>>();
    dispatch::value_result(json!({"id":query["id"],"resources":resources}))
}

pub(super) fn shared_kind(
    kinds: &[Option<crate::tools::result::FailureKind>],
) -> Option<crate::tools::result::FailureKind> {
    let first = (*kinds.first()?)?;
    kinds
        .iter()
        .all(|kind| *kind == Some(first))
        .then_some(first)
}

/// Every question locates a bare identifier over local sources: an exact
/// literal search answers it, so the matrix routes to `next.localSearch`
/// without a read or provider request.
pub(super) fn literal_route(query: &Value) -> Option<Value> {
    let identifiers = query["questions"]
        .as_array()
        .filter(|questions| !questions.is_empty())?
        .iter()
        .map(|question| {
            clasify::is_locate(question)
                .then(|| question["ask"].as_str().and_then(bare_identifier))
                .flatten()
        })
        .collect::<Option<Vec<_>>>()?;
    let search = literal_search(identifiers.iter().copied(), query["resources"].as_array()?)?;
    let mut hints = Vec::<String>::new();
    for identifier in identifiers {
        let hint = bare_target_hint(identifier);
        if !hints.contains(&hint) {
            hints.push(hint);
        }
    }
    Some(json!({
        "id":query["id"],
        "hints":hints,
        "resources":[],
        "next":{"textSearch":search}
    }))
}
