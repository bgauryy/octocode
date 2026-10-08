//! One resource's capture walk: read, split into candidate or item pages,
//! follow in-document continuations, and stop at the budget or page limit.
use super::evidence::{
    candidate_state, fallback_context, file_read_template, is_file_read, provider_state,
};
use super::hydrate::{
    bounded_search_source, file_chunks, host_read, hydrate_candidates, is_candidate_search,
    item_page, mark_followed, positioned_search_candidates,
};
use super::prefilter::{capture_prefiltered, prefilter_windows};
use super::*;
use crate::tools::clasify::resource::{ResourceSource, tool_of};

/// A resource's `maxChars`; the contract default is `maxResourceChars`.
pub(super) fn max_chars(resource: &Value) -> usize {
    resource
        .get("maxChars")
        .and_then(Value::as_u64)
        .map_or(MAX_RESOURCE_CHARS, |chars| chars as usize)
}

pub(super) fn capture_resource(
    resource: &Value,
    dispatcher: &DomainDispatcher,
    execution: &ExecutionContext,
    reads: &ReadLimiter,
    candidate_limit: usize,
) -> Result<Capture, ExecutionError> {
    if let Some(plan) = prefilter_windows(resource, dispatcher, execution, reads)? {
        let (pages, resume) = capture_prefiltered(
            resource,
            &plan,
            dispatcher,
            execution,
            reads,
            candidate_limit,
        )?;
        return Ok((pages, resume, None));
    }
    let captured = capture_pages(
        resource,
        dispatcher,
        execution,
        reads,
        candidate_limit,
        CaptureBudget::whole(max_chars(resource)),
    )?;
    Ok((captured.pages, captured.remaining, captured.uncaptured))
}

/// One capture walk's result.
pub(super) struct PagesCaptured {
    pub(super) pages: Vec<CapturedPage>,
    /// The resource continuation past the captured pages.
    pub(super) remaining: Option<Value>,
    /// Evidence characters captured.
    pub(super) chars: usize,
    /// Search candidates the read reported past the captured page.
    pub(super) uncaptured: Option<u64>,
}

/// Capture one resource's pages within its budget.
pub(super) fn capture_pages(
    resource: &Value,
    dispatcher: &DomainDispatcher,
    execution: &ExecutionContext,
    reads: &ReadLimiter,
    candidate_limit: usize,
    budget: CaptureBudget,
) -> Result<PagesCaptured, ExecutionError> {
    let hydrated = file_chunks(resource);
    let bounded = if hydrated && is_candidate_search(resource) {
        candidate_limit.min(MAX_HYDRATED_CANDIDATES)
    } else {
        candidate_limit
    };
    let source = match bounded_search_source(resource, bounded) {
        Ok(source) => source,
        Err(error) => {
            let context = fallback_context(resource);
            return Ok(PagesCaptured {
                pages: vec![CapturedPage::Failed { error, context }],
                remaining: None,
                chars: 0,
                uncaptured: None,
            });
        }
    };
    let mut walk = PageWalk {
        reader: Reader {
            dispatcher,
            execution,
            reads,
        },
        requested: resource,
        hydrated,
        page_budget: candidate_limit,
        candidate_limit: bounded,
        budget,
        pages_within: tool_of(&source).is_some_and(clasify::pages_within_resource),
        source,
        pages: Vec::new(),
        captured_chars: 0,
        seen: HashSet::new(),
        remaining: None,
        walk_chunk: None,
        range_rest: None,
        uncaptured: None,
    };
    walk.run()?;
    let pages = if walk.pages_within {
        coalesce_pages(walk.pages)
    } else {
        walk.pages
    };
    Ok(PagesCaptured {
        pages,
        remaining: walk.remaining,
        chars: walk.captured_chars,
        uncaptured: walk.uncaptured,
    })
}

/// Search candidates past a search page, from its `pagination` totals.
fn candidates_past_page(state: &Value) -> Option<u64> {
    let pagination = state.pointer("/results/0/data/pagination")?;
    let count = |field: &str| pagination.get(field).and_then(Value::as_u64);
    let seen = count("currentPage")?.saturating_mul(count("pageSize")?);
    Some(count("totalItems")?.saturating_sub(seen))
}

/// The bounds every delegated read of one resource shares.
pub(super) struct Reader<'a> {
    dispatcher: &'a DomainDispatcher,
    execution: &'a ExecutionContext,
    reads: &'a ReadLimiter,
}

impl Reader<'_> {
    fn resolve(
        &self,
        source: &Value,
    ) -> Result<
        Result<(Value, Option<Value>), crate::tools::clasify::context::ContextFailure>,
        ExecutionError,
    > {
        resolve_limited(source, self.dispatcher, self.execution, self.reads)
    }
}

/// What the walk does after one read.
pub(super) enum Step {
    Done,
    Read(Value),
}

/// One resource's capture walk: reads `source`, then its continuations,
/// until the resource, the budget, or the page limit ends it.
pub(super) struct PageWalk<'a> {
    reader: Reader<'a>,
    /// The resource as requested (its `candidateEvidence` rides along).
    requested: &'a Value,
    hydrated: bool,
    /// Pages this resource may judge (one per candidate, or per hit cluster
    /// of a hydrated local candidate) before the matrix cell budget binds.
    page_budget: usize,
    candidate_limit: usize,
    budget: CaptureBudget,
    pages_within: bool,
    source: Value,
    pages: Vec<CapturedPage>,
    captured_chars: usize,
    seen: HashSet<String>,
    remaining: Option<Value>,
    /// The page size a shrunk page's continuation returns to.
    walk_chunk: Option<Value>,
    /// A `ranges` read split to fit: the read as requested (a deferred page
    /// replays it whole) and the read of the lines after the judged head.
    range_rest: Option<(Value, Option<Value>)>,
    /// Search candidates the read reported past its captured page.
    uncaptured: Option<u64>,
}

impl PageWalk<'_> {
    fn run(&mut self) -> Result<(), ExecutionError> {
        loop {
            self.reader.execution.check()?;
            if !self.seen.insert(self.source.to_string()) {
                self.pages.push(CapturedPage::Failed {
                    error: ClassificationError::new(
                        "classificationContextContinuationLoop",
                        "Context continuation repeated without advancing.",
                        "Run the ordinary context tool and inspect its executable continuation.",
                    ),
                    context: fallback_context(&self.source),
                });
                return Ok(());
            }
            let step = match self.reader.resolve(&self.source)? {
                Ok((state, receipt)) => self.page(state, receipt)?,
                Err(failure) => {
                    let context = failure
                        .receipt
                        .unwrap_or_else(|| fallback_context(&self.source));
                    match crate::tools::clasify::context::exact_continuation(&context) {
                        Some(next) => Step::Read(next),
                        None => {
                            self.pages.push(CapturedPage::Failed {
                                error: failure.error,
                                context,
                            });
                            Step::Done
                        }
                    }
                }
            };
            match step {
                Step::Done => return Ok(()),
                Step::Read(next) => self.source = next,
            }
        }
    }

    fn remaining_chars(&self) -> usize {
        self.budget.left.saturating_sub(self.captured_chars)
    }

    /// Keep what fits the budget and fail the rest with the spent budget.
    fn keep(&mut self, candidates: Vec<Candidate>) -> Vec<Candidate> {
        let left = self.remaining_chars();
        let (kept, chars, deferred) = budget_candidates(candidates, left, self.budget.cap);
        self.captured_chars = self.captured_chars.saturating_add(chars);
        self.pages.extend(kept);
        deferred
    }

    fn spend(&mut self, deferred: Vec<Candidate>, left: usize) {
        self.pages.extend(
            deferred
                .into_iter()
                .map(|candidate| budget_spent(candidate, left)),
        );
    }

    /// One successful read: search candidates, list items, or one page.
    fn page(&mut self, state: Value, receipt: Option<Value>) -> Result<Step, ExecutionError> {
        if let Some(candidates) = positioned_search_candidates(&self.source, &state) {
            self.uncaptured = candidates_past_page(&state);
            self.search_page(candidates, receipt)?;
            return Ok(Step::Done);
        }
        // The read is parsed once per page: whether it is an outline and how
        // its list splits depend on its typed query.
        let read = ResourceSource::of(&self.source);
        let (state, outline_next) = if read
            .as_ref()
            .is_some_and(|read| items::is_symbol_outline(read, &state))
        {
            whole_outline_files(
                &self.source,
                state,
                receipt.as_ref(),
                self.reader.dispatcher,
                self.reader.execution,
                self.reader.reads,
            )?
        } else {
            (state, None)
        };
        // Split only when every candidate fits the cell budget; a larger list
        // falls through and is judged as one page.
        if let Some(items) = read
            .as_ref()
            .and_then(|read| items::split(read, &state))
            .filter(|items| items.len() <= self.candidate_limit.max(1))
        {
            self.remaining = outline_next.unwrap_or_else(|| {
                receipt
                    .as_ref()
                    .and_then(crate::tools::clasify::context::continuation)
            });
            self.list_page(&state, items);
            return Ok(Step::Done);
        }
        self.whole_page(state, receipt, outline_next)
    }

    /// A search page: its candidates as pages (snippets, or hydrated file
    /// chunks), and the continuation past the last candidate kept.
    fn search_page(
        &mut self,
        candidates: Vec<(usize, Value)>,
        receipt: Option<Value>,
    ) -> Result<(), ExecutionError> {
        let left = self.remaining_chars();
        let mut next = if self.hydrated && tool_of(&self.source) == Some(ToolId::LocalSearch) {
            crate::tools::clasify::context::continuation_named(
                &receipt
                    .clone()
                    .unwrap_or_else(|| fallback_context(&self.source)),
                "nextPage",
            )
        } else {
            receipt
                .as_ref()
                .and_then(crate::tools::clasify::context::continuation)
        };
        if !self.hydrated {
            let candidates = candidates
                .into_iter()
                .map(|(position, candidate)| self.snippet(position, candidate))
                .collect();
            let deferred = self.keep(candidates);
            self.remaining = match deferred.first().and_then(|first| first.position) {
                None => next,
                Some(position) => match resume_search(&self.source, receipt.as_ref(), position) {
                    Some(resume) => Some(resume),
                    None => {
                        self.spend(deferred, left);
                        next
                    }
                },
            };
            return Ok(());
        }
        if let (Some(next), Some(evidence)) = (
            next.as_mut().and_then(Value::as_object_mut),
            self.requested.get("candidateEvidence"),
        ) {
            next.insert("candidateEvidence".into(), evidence.clone());
        }
        let hydrated_pages = hydrate_candidates(
            &self.source,
            candidates
                .into_iter()
                .map(|(_, candidate)| candidate)
                .collect(),
            left,
            self.page_budget,
            self.reader.dispatcher,
            self.reader.execution,
            self.reader.reads,
        )?;
        let hydrated_pages = hydrated_pages
            .into_iter()
            .map(|page| Candidate {
                chars: match &page {
                    CapturedPage::Ready { state, .. } => evidence_chars(state),
                    CapturedPage::Failed { .. } => 0,
                },
                page,
                position: None,
            })
            .collect();
        let deferred = self.keep(hydrated_pages);
        self.spend(deferred, left);
        self.remaining = next;
        Ok(())
    }

    /// One search snippet as a candidate page with its host read.
    fn snippet(&self, position: usize, candidate: Value) -> Candidate {
        let mut context =
            crate::tools::clasify::context::candidate_receipt(&self.source, &candidate);
        if let Some(read) = host_read(&self.source, &candidate) {
            crate::tools::clasify::context::attach_read(&mut context, read);
        }
        let state = candidate_state(&self.source, candidate);
        Candidate {
            chars: candidate_chars(&state),
            page: CapturedPage::Ready { state, context },
            position: Some(position),
        }
    }

    /// A list page: one page per item, within the budget. Items the budget
    /// defers are judged by the continuation, which resumes at the first of
    /// them when the list pages by rows; otherwise each is reported with its
    /// read.
    fn list_page(&mut self, state: &Value, items: Vec<items::Item>) {
        let left = self.remaining_chars();
        let (positions, rows) = list_rows(state, &items);
        let items = items
            .into_iter()
            .zip(positions)
            .map(|(item, position)| {
                let page = item_page(&self.source, item);
                Candidate {
                    chars: match &page {
                        CapturedPage::Ready { state, .. } => candidate_chars(state),
                        CapturedPage::Failed { .. } => 0,
                    },
                    page,
                    position,
                }
            })
            .collect();
        let deferred = self.keep(items);
        let resume = deferred
            .first()
            .and_then(|first| first.position)
            .and_then(|position| resume_list(&self.source, position, rows));
        match resume {
            Some(resume) => self.remaining = Some(resume),
            None => {
                self.spend(deferred, left);
                // A page a resume put off the original grid steps back to it.
                if self.remaining.is_some()
                    && let Some(next) = after_resumed_list(&self.source, rows)
                {
                    self.remaining = Some(next);
                }
            }
        }
    }

    /// A page over the whole cap would fail on every replay: read the same
    /// start in smaller chunks until it fits. Returns the page and its size.
    fn shrink(
        &mut self,
        mut state: Value,
        mut receipt: Option<Value>,
    ) -> Result<(Value, Option<Value>, usize), ExecutionError> {
        let cap = self.budget.cap;
        let mut chars = assessed_payload_chars(&self.source, &state);
        let mut attempts = 0;
        while chars > cap && attempts < MAX_SHRINK_ATTEMPTS {
            let Some(smaller) = shrunk_page(&self.source, &state, chars, cap) else {
                break;
            };
            attempts += 1;
            let Ok((shrunk_state, shrunk_receipt)) = self.reader.resolve(&smaller)? else {
                break;
            };
            if self.walk_chunk.is_none() {
                self.walk_chunk = state.pointer("/results/0/data/pagination/length").cloned();
            }
            self.seen.insert(smaller.to_string());
            self.source = smaller;
            state = shrunk_state;
            receipt = shrunk_receipt;
            chars = assessed_payload_chars(&self.source, &state);
        }
        // A `ranges` read has no chunk window: judge its first lines that
        // fit and continue with the rest of the range.
        self.range_rest = None;
        let Some(spans) = (chars > cap)
            .then(|| split_ranges(&self.source, &state))
            .flatten()
        else {
            return Ok((state, receipt, chars));
        };
        let origin = self.source.clone();
        let mut lines = span_lines(&spans);
        let mut attempts = 0;
        while chars > cap && lines > 1 && attempts < MAX_SHRINK_ATTEMPTS {
            attempts += 1;
            let scaled = u128::from(lines).saturating_mul(cap as u128) / chars.max(1) as u128;
            let keep = u64::try_from(scaled)
                .unwrap_or(u64::MAX)
                .clamp(1, lines - 1);
            let (head, rest) = ranges_read(&origin, &spans, keep);
            let Ok((head_state, head_receipt)) = self.reader.resolve(&head)? else {
                break;
            };
            // A head the tool pages itself would carry a second cursor.
            if head_receipt
                .as_ref()
                .and_then(crate::tools::clasify::context::continuation)
                .is_some()
            {
                break;
            }
            self.seen.insert(head.to_string());
            self.source = head;
            self.range_rest = Some((origin.clone(), rest));
            state = head_state;
            receipt = head_receipt;
            lines = keep;
            chars = assessed_payload_chars(&self.source, &state);
        }
        Ok((state, receipt, chars))
    }

    /// One whole page (a file chunk, an item, a history page), then the
    /// resource continuation: followed within one document, else returned.
    fn whole_page(
        &mut self,
        state: Value,
        receipt: Option<Value>,
        outline_next: Option<Option<Value>>,
    ) -> Result<Step, ExecutionError> {
        let (state, receipt, state_chars) = self.shrink(state, receipt)?;
        let left = self.remaining_chars();
        if state_chars <= self.budget.cap
            && state_chars > left
            && (self.budget.defer || !self.pages.is_empty())
        {
            // A split range replays as requested; the next call splits it.
            self.remaining = Some(match self.range_rest.take() {
                Some((origin, _)) => origin,
                None => self.source.clone(),
            });
            return Ok(Step::Done);
        }
        let mut context = receipt.unwrap_or_else(|| fallback_context(&self.source));
        if context.get("read").is_none()
            && let Some(template) = file_read_template(&self.source, &context)
        {
            context["fileRead"] = template;
        }
        // Never classify an arbitrary prefix with the full page's source
        // receipt. The caller can choose a smaller complete section; no
        // continuation replays a page that cannot fit.
        if state_chars > left {
            self.pages.push(CapturedPage::Failed {
                error: too_large(state_chars, self.budget.cap),
                context,
            });
            // The lines after an unsplittable head stay reachable.
            if let Some((_, rest)) = self.range_rest.take() {
                self.remaining = rest;
            }
            return Ok(Step::Done);
        }
        self.captured_chars = self.captured_chars.saturating_add(state_chars);
        // An empty file has nothing to judge: a provider "no" would be a
        // confident false negative and still cost a request.
        if state_chars == 0 && is_file_read(&self.source) && self.pages.is_empty() {
            return Ok(self.empty_file(context));
        }
        let state = provider_state(&self.source, state);
        // An outline page that read ahead for its last file resumes after
        // those rows, not at its own next page.
        if let Some(next) = outline_next {
            self.pages.push(CapturedPage::Ready { state, context });
            self.remaining = next;
            return Ok(Step::Done);
        }
        Ok(self.follow(state, context))
    }

    /// An oversized whole-file read (ghGetFileContent `fullContentLimit`)
    /// returns no body plus an exact page continuation: page through it.
    /// Otherwise the empty file fails without a provider request.
    fn empty_file(&mut self, context: Value) -> Step {
        if let Some(next) = crate::tools::clasify::context::continuation(&context)
            && !self.seen.contains(&next.to_string())
        {
            self.captured_chars = 0;
            return Step::Read(next);
        }
        self.pages.push(CapturedPage::Failed {
            error: ClassificationError::new(
                "classificationContextEmpty",
                "The resource has no content to judge.",
                "Drop empty files from the matrix; absence of content is not a verdict.",
            ),
            context,
        });
        Step::Done
    }

    /// Keep a judged page and decide where the walk goes next.
    fn follow(&mut self, state: Value, mut context: Value) -> Step {
        // A split range continues with the lines after its judged head.
        let rest = self.range_rest.take().and_then(|(_, rest)| rest);
        let Some(next) = crate::tools::clasify::context::continuation(&context).or(rest) else {
            self.pages.push(CapturedPage::Ready { state, context });
            return Step::Done;
        };
        // A continuation that repeats a read already captured does not
        // advance (e.g. an item whose optional content menu echoes the same
        // query): the resource is fully captured, not looping.
        if self.seen.contains(&next.to_string()) {
            mark_followed(&mut context);
            if let Some(receipt) = context.as_object_mut() {
                receipt.insert("coverage".into(), json!("bounded"));
            }
            self.pages.push(CapturedPage::Ready { state, context });
            return Step::Done;
        }
        let next = restore_chunk(next, self.walk_chunk.as_ref());
        if !self.pages_within
            || self.captured_chars >= self.budget.left
            || self.pages.len() + 1 >= 100
        {
            self.pages.push(CapturedPage::Ready { state, context });
            self.remaining = Some(next);
            return Step::Done;
        }
        mark_followed(&mut context);
        self.pages.push(CapturedPage::Ready { state, context });
        Step::Read(next)
    }
}

/// Extra outline pages one call may read to finish its last file.
pub(super) const MAX_OUTLINE_FOLLOW_PAGES: usize = 20;

/// Page a symbols outline by file: a file's rows are judged together, on the
/// page that holds its first row. That page reads the following pages for
/// the rest of its last file and resumes after them; the resumed page skips
/// the rows of the file an earlier page started. Returns the page state and,
/// when following pages were read, the continuation that replaces the page's
/// own (`Some(None)`: the outline is exhausted).
pub(super) fn whole_outline_files(
    source: &Value,
    mut state: Value,
    receipt: Option<&Value>,
    dispatcher: &DomainDispatcher,
    execution: &ExecutionContext,
    reads: &ReadLimiter,
) -> Result<(Value, Option<Option<Value>>), ExecutionError> {
    let page = source
        .pointer("/query/page")
        .and_then(Value::as_u64)
        .unwrap_or(1);
    if page > 1 {
        let mut previous = source.clone();
        previous["query"]["page"] = json!(page - 1);
        if let Ok((prior, _)) = resolve_limited(&previous, dispatcher, execution, reads)?
            && items::last_outline_path(&prior).is_some()
            && items::last_outline_path(&prior) == items::first_outline_path(&state)
        {
            items::drop_first_outline_file(&mut state);
        }
    }
    let mut next = receipt.and_then(crate::tools::clasify::context::continuation);
    let mut followed = false;
    for _ in 0..MAX_OUTLINE_FOLLOW_PAGES {
        let Some(candidate) = next.clone() else {
            break;
        };
        let Ok((following, following_receipt)) =
            resolve_limited(&candidate, dispatcher, execution, reads)?
        else {
            break;
        };
        match items::extend_last_outline_file(&mut state, &following) {
            // The last file ends inside that page: resume there.
            Some(true) => {
                followed = true;
                break;
            }
            // That page held only the last file: resume after it.
            Some(false) => {
                followed = true;
                next = following_receipt
                    .as_ref()
                    .and_then(crate::tools::clasify::context::continuation);
            }
            // The next page starts a new file: the page's own continuation.
            None => break,
        }
    }
    Ok((state, followed.then_some(next)))
}

pub(super) fn page_context(page: &CapturedPage) -> &Value {
    match page {
        CapturedPage::Ready { context, .. } | CapturedPage::Failed { context, .. } => context,
    }
}

pub(super) fn page_context_mut(page: &mut CapturedPage) -> &mut Value {
    match page {
        CapturedPage::Ready { context, .. } | CapturedPage::Failed { context, .. } => context,
    }
}

/// Join runs of adjacent captured file pages into larger provider states so
/// one judgment covers a contiguous scope; failed pages break a run.
pub(super) fn coalesce_pages(pages: Vec<CapturedPage>) -> Vec<CapturedPage> {
    let mut output = Vec::with_capacity(pages.len());
    let mut run = Vec::new();
    let flush = |run: &mut Vec<(Value, Value)>, output: &mut Vec<CapturedPage>| {
        output.extend(
            output::coalesce(std::mem::take(run), output::COALESCE_BYTES)
                .into_iter()
                .map(|(state, context)| CapturedPage::Ready { state, context }),
        );
    };
    for page in pages {
        match page {
            CapturedPage::Ready { state, context } => run.push((state, context)),
            failed @ CapturedPage::Failed { .. } => {
                flush(&mut run, &mut output);
                output.push(failed);
            }
        }
    }
    flush(&mut run, &mut output);
    output
}

/// Each list candidate's row on its page (the one row its narrowed state
/// keeps, matched in page order) and the page's row count. A candidate that
/// narrows no single row has no position.
fn list_rows(state: &Value, items: &[items::Item]) -> (Vec<Option<usize>>, usize) {
    let page = state.pointer("/results/0/data").and_then(Value::as_object);
    let mut rows = 0;
    let mut after = 0;
    let positions = items
        .iter()
        .map(|item| {
            let narrowed = item.state.pointer("/results/0/data")?.as_object()?;
            // The narrowed list: a one-row array whose page array holds that
            // row; the longest such page array when several qualify.
            let (all, position) = narrowed
                .iter()
                .filter_map(|(key, value)| {
                    let [row] = value.as_array()?.as_slice() else {
                        return None;
                    };
                    let all = page?.get(key)?.as_array()?;
                    let found = all
                        .get(after..)?
                        .iter()
                        .position(|candidate| candidate == row)?;
                    Some((all, after + found))
                })
                .max_by_key(|(all, _)| all.len())?;
            rows = all.len();
            after = position + 1;
            Some(position)
        })
        .collect();
    (positions, rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_max_chars_matches_every_contract_default() {
        fn walk(value: &Value, found: &mut Vec<u64>) {
            match value {
                Value::Object(fields) => {
                    for (key, field) in fields {
                        if key == "maxChars"
                            && let Some(default) = field.get("default").and_then(Value::as_u64)
                        {
                            found.push(default);
                        }
                        walk(field, found);
                    }
                }
                Value::Array(items) => items.iter().for_each(|item| walk(item, found)),
                _ => {}
            }
        }
        let contract = crate::contracts::parsed_contract().expect("contract");
        let clasify = contract["tools"]
            .as_array()
            .and_then(|tools| tools.iter().find(|tool| tool["name"] == "clasify"))
            .expect("clasify contract");
        let mut defaults = Vec::new();
        walk(clasify, &mut defaults);
        assert!(!defaults.is_empty(), "clasify maxChars default not found");
        assert!(
            defaults
                .iter()
                .all(|default| *default == MAX_RESOURCE_CHARS as u64)
        );
    }

    #[test]
    fn adjacent_ready_pages_coalesce_and_failed_pages_split_runs() {
        let ready = |start: u64| CapturedPage::Ready {
            state: json!({"content":"x"}),
            context: json!({"coverage":"bounded","scope":{"startLine":start,"endLine":start + 99,"totalLines":400}}),
        };
        let pages = coalesce_pages(vec![
            ready(1),
            ready(101),
            CapturedPage::Failed {
                error: ClassificationError::default(),
                context: json!({"coverage":"partial"}),
            },
            ready(201),
        ]);
        assert_eq!(pages.len(), 3);
        let CapturedPage::Ready { state, context } = &pages[0] else {
            panic!("first run is ready");
        };
        assert!(state.is_array());
        assert_eq!(
            context["scope"],
            json!({"startLine":1,"endLine":200,"totalLines":400})
        );
        assert!(matches!(pages[1], CapturedPage::Failed { .. }));
    }
}
