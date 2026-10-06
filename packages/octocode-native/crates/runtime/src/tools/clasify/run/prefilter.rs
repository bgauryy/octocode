//! `prefilter` on file reads: probe the literal terms, then capture only
//! hit windows and resume the rest through `next.clasify`.
use super::capture::{PagesCaptured, capture_pages, max_chars, page_context, page_context_mut};
use super::hydrate::densest_run;
use super::*;
use crate::tools::clasify::resource::tool_of;

/// Judge prefilter windows in file order under one shared `maxChars` budget.
/// Hits the windows do not reach (more hit clusters than windows, an
/// unfinished probe, or a spent budget) get a limitation and a `next.clasify`
/// continuation: the same prefiltered read from the first unjudged line.
pub(super) fn capture_prefiltered(
    resource: &Value,
    plan: &PrefilterPlan,
    dispatcher: &DomainDispatcher,
    execution: &ExecutionContext,
    reads: &ReadLimiter,
    candidate_limit: usize,
) -> Result<(Vec<CapturedPage>, Option<Value>), ExecutionError> {
    let budget = max_chars(resource);
    let mut used = 0usize;
    let mut pages = Vec::new();
    let mut resume = plan.resume;
    for &(start, end) in &plan.windows {
        let left = budget.saturating_sub(used);
        if left == 0 {
            resume = Some(start);
            break;
        }
        let window = prefilter_window(resource, start, end);
        let PagesCaptured {
            pages: window_pages,
            remaining: next,
            chars,
            ..
        } = capture_pages(
            &window,
            dispatcher,
            execution,
            reads,
            candidate_limit,
            CaptureBudget {
                left,
                cap: budget,
                defer: !pages.is_empty(),
            },
        )?;
        used = used.saturating_add(chars);
        let covered = window_pages
            .iter()
            .filter_map(|page| page_context(page)["scope"]["endLine"].as_u64())
            .max();
        pages.extend(window_pages);
        if next.is_some() {
            resume = Some(covered.map_or(start, |line| line.saturating_add(1).max(start)));
            break;
        }
    }
    let resume = resume.filter(|from| {
        *from <= plan.range_end && (plan.probe_open || plan.hits.iter().any(|hit| hit >= from))
    });
    if let Some(from) = resume
        && let Some(last) = pages.last_mut()
    {
        crate::tools::clasify::context::append_limitation(
            page_context_mut(last),
            &format!(
                "Prefilter windows stopped before line {from}; later hits are unjudged. next.clasify resumes there."
            ),
        );
    }
    Ok((
        pages,
        resume.map(|from| prefilter_resume(resource, from, plan.range_end)),
    ))
}

/// Lines per prefilter window (contract `prefilterWindowLines`, as a line number).
pub(super) const PREFILTER_WINDOW_LINES: u64 =
    crate::tools::id::clasify_policy::PREFILTER_WINDOW_LINES as u64;
/// Match pages the prefilter probe follows before it stops collecting hits.
pub(super) const PREFILTER_PROBE_PAGES: usize = 20;

/// Windows one prefiltered call judges, and where the walk resumes.
pub(super) struct PrefilterPlan {
    /// Inclusive line windows in file order, non-overlapping.
    windows: Vec<(u64, u64)>,
    /// Every known hit in range, sorted.
    hits: Vec<u64>,
    /// Last line of the walk: the caller's range end, else the file end.
    range_end: u64,
    /// First line after the windows that may still hold unjudged hits.
    resume: Option<u64>,
    /// The probe stopped before its last match page: hits past the known
    /// ones may exist.
    probe_open: bool,
}

/// `prefilter` on a file resource: read the same file for the literal terms,
/// then capture only hit windows as bounded reads instead of the whole file.
/// When the hits fit in `PREFILTER_WINDOWS` windows they are the densest runs;
/// otherwise windows follow file order and `resume` continues after the last
/// one. A caller line range bounds the walk (continuations use it).
/// `None` (no prefilter, not a file read, or no hits) captures the resource
/// as given.
pub(super) fn prefilter_windows(
    resource: &Value,
    dispatcher: &DomainDispatcher,
    execution: &ExecutionContext,
    reads: &ReadLimiter,
) -> Result<Option<PrefilterPlan>, ExecutionError> {
    let Some(probe) = PrefilterProbe::of(resource) else {
        return Ok(None);
    };
    let Some((total, mut hits, probe_open)) = probe.hits(dispatcher, execution, reads)? else {
        return Ok(None);
    };
    let range_end = probe.requested_end.unwrap_or(total).min(total);
    hits.retain(|line| (probe.range_start..=range_end).contains(line));
    hits.sort_unstable();
    hits.dedup();
    if hits.is_empty() {
        return Ok(None);
    }
    let (windows, resume) = plan_windows(&hits, probe.range_start, range_end, probe_open);
    Ok(Some(PrefilterPlan {
        windows,
        hits,
        range_end,
        resume,
        probe_open,
    }))
}

/// The literal-term search over a prefiltered file read.
pub(super) struct PrefilterProbe {
    read: Value,
    tool: ToolId,
    pattern: String,
    range_start: u64,
    requested_end: Option<u64>,
}

impl PrefilterProbe {
    /// `None` without prefilter terms or on a resource that is not a file read.
    fn of(resource: &Value) -> Option<Self> {
        let terms = resource
            .get("prefilter")?
            .as_array()?
            .iter()
            .filter_map(Value::as_str)
            .filter(|term| !term.trim().is_empty())
            .map(regex::escape)
            .collect::<Vec<_>>();
        let tool = tool_of(resource).filter(|tool| clasify::is_file_read_tool(*tool))?;
        if terms.is_empty() {
            return None;
        }
        let mut query = resource["query"].clone();
        let object = query.as_object_mut()?;
        let requested = output::single_range(object);
        for field in [
            "fullContent",
            "ranges",
            "offset",
            "length",
            "unit",
            "minify",
        ] {
            object.remove(field);
        }
        let pattern = terms.join("|");
        object.insert("matchString".into(), json!(pattern));
        object.insert("regex".into(), json!("rust"));
        object.insert("caseMode".into(), json!("insensitive"));
        object.insert("contextLines".into(), json!(0));
        Some(Self {
            read: json!({"tool":tool.as_str(),"query":query}),
            tool,
            pattern,
            range_start: requested.map_or(1, |(start, _)| start).max(1),
            requested_end: requested.map(|(_, end)| end),
        })
    }

    /// The file's line total, every hit line the probe pages reach, and
    /// whether hits past them may exist. `None` when the first page fails.
    fn hits(
        &self,
        dispatcher: &DomainDispatcher,
        execution: &ExecutionContext,
        reads: &ReadLimiter,
    ) -> Result<Option<(u64, Vec<u64>, bool)>, ExecutionError> {
        let mut probe = self.read.clone();
        let mut total = None;
        let mut hits = Vec::new();
        let mut seen = HashSet::new();
        let mut open = false;
        for page in 0..=PREFILTER_PROBE_PAGES {
            if page == PREFILTER_PROBE_PAGES || !seen.insert(probe.to_string()) {
                open = page == PREFILTER_PROBE_PAGES;
                break;
            }
            let read = resolve_limited(&probe, dispatcher, execution, reads)?;
            let Some((lines, receipt)) = read.ok().and_then(|(_, receipt)| {
                let receipt = receipt?;
                Some((receipt["scope"]["totalLines"].as_u64()?, receipt))
            }) else {
                if page == 0 {
                    return Ok(None);
                }
                open = true;
                break;
            };
            total = Some(lines);
            hits.extend(scope_lines(&receipt["scope"]));
            // Follow only match pages of the same probe; any other
            // continuation (e.g. a plain read) would count unmatched lines
            // as hits.
            match crate::tools::clasify::context::continuation(&receipt) {
                Some(next)
                    if tool_of(&next) == Some(self.tool)
                        && next["query"]["matchString"] == self.pattern.as_str() =>
                {
                    probe = next;
                }
                Some(_) => {
                    open = true;
                    break;
                }
                None => break,
            }
        }
        Ok(total.map(|total| (total, hits, open)))
    }
}

/// Every line a match page's scope covers (one span or disjoint ranges).
pub(super) fn scope_lines(scope: &Value) -> Vec<u64> {
    let ranges = scope["lineRanges"]
        .as_array()
        .cloned()
        .unwrap_or_else(|| vec![scope.clone()]);
    ranges
        .iter()
        .filter_map(|range| Some(range["startLine"].as_u64()?..=range["endLine"].as_u64()?))
        .flatten()
        .collect()
}

/// Windows over sorted `hits`, and where the walk resumes. When the hits fit
/// in `PREFILTER_WINDOWS` windows they are the densest runs; otherwise
/// windows follow file order and the walk resumes after the last one.
pub(super) fn plan_windows(
    hits: &[u64],
    range_start: u64,
    range_end: u64,
    probe_open: bool,
) -> (Vec<(u64, u64)>, Option<u64>) {
    let span = PREFILTER_WINDOW_LINES - 1;
    // A window centered on a run of hits: a fixed bucket grid cuts a hit near
    // a boundary away from the lines that introduce it.
    let centered = |first: u64, last: u64| {
        let center = first + (last - first) / 2;
        let end = (center.saturating_sub(PREFILTER_WINDOW_LINES / 2).max(1) + span).min(range_end);
        (end.saturating_sub(span).max(range_start), end)
    };
    let mut windows = Vec::new();
    let mut left = hits.to_vec();
    while windows.len() < PREFILTER_WINDOWS
        && let Some((first, last)) = densest_run(&left, span - 1)
    {
        let (start, end) = centered(first, last);
        windows.push((start, end));
        left.retain(|line| *line < start || *line > end);
    }
    let mut resume = None;
    if !left.is_empty() || probe_open {
        // More hit clusters than windows: densest windows would strand hits
        // on both sides, so judge in file order and resume after the last.
        windows.clear();
        left = hits.to_vec();
        while windows.len() < PREFILTER_WINDOWS
            && let Some(&first) = left.first()
        {
            let last = left
                .iter()
                .copied()
                .take_while(|line| *line < first + span)
                .last()
                .unwrap_or(first);
            let (start, end) = centered(first, last);
            windows.push((start, end));
            left.retain(|line| *line > end);
        }
        resume = windows
            .last()
            .map(|(_, end)| end.saturating_add(1))
            .filter(|from| !left.is_empty() || probe_open && *from <= range_end);
    }
    windows.sort_unstable();
    // Later windows hold only hits outside earlier ones, so trimming an
    // overlap keeps every hit exactly once.
    for index in 1..windows.len() {
        windows[index].0 = windows[index].0.max(windows[index - 1].1 + 1);
    }
    (windows, resume)
}

/// One prefilter window as a bounded read of the same resource.
pub(super) fn prefilter_window(resource: &Value, start: u64, end: u64) -> Value {
    let mut window = resource.clone();
    if let Some(object) = window.as_object_mut() {
        object.remove("prefilter");
    }
    let query = &mut window["query"];
    if let Some(object) = query.as_object_mut() {
        object.remove("fullContent");
    }
    output::set_read_range(query, start, end);
    window
}

/// The read that continues a prefiltered walk at `from`; the continued
/// resource keeps its `prefilter`, which honors the range.
pub(super) fn prefilter_resume(resource: &Value, from: u64, range_end: u64) -> Value {
    let mut query = resource["query"].clone();
    if let Some(object) = query.as_object_mut() {
        object.remove("fullContent");
    }
    output::set_read_range(&mut query, from, range_end);
    json!({"tool":resource["tool"],"query":query})
}
