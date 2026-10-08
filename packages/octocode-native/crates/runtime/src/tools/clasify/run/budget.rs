//! Evidence budget for one clasify resource: what counts as a character of
//! evidence, which split candidates fit, how an oversized file page shrinks,
//! and where a deferred search page resumes. Every capture path (file pages,
//! search snippets, hydrated chunks, list items) applies these rules, so
//! `maxChars` bounds the evidence submitted for classification.
use super::evidence::{file_evidence, is_file_read};
use super::hydrate::default_search_page_size;
use super::{CONTROL_FIELDS, CapturedPage, MAX_PAGE_CHARS};
use crate::tools::clasify::resource::tool_of;
use crate::tools::clasify::transport::ClassificationError;
use crate::tools::id::ToolId;
use serde_json::{Value, json};

/// Characters of keys and scalar values; JSON punctuation is not evidence.
fn logical_chars(value: &Value) -> usize {
    match value {
        Value::Null => 0,
        Value::Bool(value) => value.to_string().chars().count(),
        Value::Number(value) => value.to_string().chars().count(),
        Value::String(value) => value.chars().count(),
        Value::Array(values) => values.iter().map(logical_chars).sum(),
        Value::Object(values) => values
            .iter()
            .map(|(key, value)| key.chars().count().saturating_add(logical_chars(value)))
            .sum(),
    }
}

/// One row's evidence characters, without its control fields.
fn tool_data_chars(data: &Value) -> usize {
    let Some(fields) = data.as_object() else {
        return logical_chars(data);
    };
    fields
        .iter()
        .filter(|(key, _)| !CONTROL_FIELDS.contains(&key.as_str()))
        .map(|(key, value)| key.chars().count().saturating_add(logical_chars(value)))
        .sum()
}

/// Source characters of file evidence (`content` only).
pub(super) fn evidence_chars(evidence: &Value) -> usize {
    match evidence {
        Value::Array(entries) => entries.iter().map(evidence_chars).sum(),
        entry => entry["content"]
            .as_str()
            .map_or(0, |content| content.chars().count()),
    }
}

/// Count the sanitized resource payload rather than its transport envelope.
/// JSON punctuation, escaping, row wrappers, and executable continuations are
/// control-plane overhead and must not reduce the caller's `maxChars` budget.
pub(super) fn assessed_payload_chars(source: &Value, state: &Value) -> usize {
    if source.get("value").is_some() {
        return logical_chars(state);
    }
    if let Some(evidence) = is_file_read(source).then(|| file_evidence(state)).flatten() {
        return evidence_chars(&evidence);
    }
    state
        .get("results")
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter_map(|row| row.get("data"))
                .map(tool_data_chars)
                .sum()
        })
        .unwrap_or_else(|| logical_chars(state))
}

/// Evidence characters of one split candidate's provider payload, counted
/// like a whole tool page (`assessed_payload_chars`).
pub(super) fn candidate_chars(payload: &Value) -> usize {
    match payload.get("data") {
        Some(data) if payload.get("root").is_some() => tool_data_chars(data),
        _ => tool_data_chars(payload),
    }
}

/// One page split from a search or list page, with the evidence characters it
/// adds and, for a search candidate, its file row on the page.
pub(super) struct Candidate {
    pub(super) page: CapturedPage,
    pub(super) chars: usize,
    pub(super) position: Option<usize>,
}

/// A page or candidate above the whole `maxChars`: no continuation replays it.
pub(super) fn too_large(chars: usize, cap: usize) -> ClassificationError {
    ClassificationError::new(
        "classificationContextTooLarge",
        format!(
            "The captured page is {chars} characters, above the {cap}-character page limit; no classification was run."
        ),
        "Select a smaller complete section or reduce the read page size.",
    )
}

/// Keep split candidates in page order while their evidence fits `remaining`.
/// A candidate larger than the whole `cap` can never be judged: it fails with
/// its read. The first candidate that only overflows `remaining` is deferred
/// with every later one. Returns the kept pages, their characters, and the
/// deferred candidates.
pub(super) fn budget_candidates(
    candidates: Vec<Candidate>,
    remaining: usize,
    cap: usize,
) -> (Vec<CapturedPage>, usize, Vec<Candidate>) {
    let mut kept = Vec::new();
    let mut used = 0usize;
    let mut candidates = candidates.into_iter();
    while let Some(candidate) = candidates.next() {
        match candidate.page {
            CapturedPage::Ready { context, .. } if candidate.chars > cap => {
                kept.push(CapturedPage::Failed {
                    error: too_large(candidate.chars, cap),
                    context,
                });
            }
            CapturedPage::Ready { .. } if used.saturating_add(candidate.chars) > remaining => {
                let mut deferred = vec![candidate];
                deferred.extend(candidates);
                return (kept, used, deferred);
            }
            page => {
                if matches!(page, CapturedPage::Ready { .. }) {
                    used = used.saturating_add(candidate.chars);
                }
                kept.push(page);
            }
        }
    }
    (kept, used, Vec::new())
}

/// A deferred candidate no continuation can reach: reported with its read,
/// never dropped.
pub(super) fn budget_spent(candidate: Candidate, remaining: usize) -> CapturedPage {
    match candidate.page {
        CapturedPage::Ready { context, .. } => CapturedPage::Failed {
            error: ClassificationError::new(
                "classificationBudgetSpent",
                format!(
                    "Earlier candidates used this call's {remaining} characters; this {}-character candidate was not classified.",
                    candidate.chars
                ),
                "Run its hints.read, or narrow the page or raise maxChars to classify it.",
            ),
            context,
        },
        failed => failed,
    }
}

/// The search page that starts at file row `position` of `source`'s page, so
/// a replay resumes at the first deferred candidate. A smaller page size that
/// divides the new offset keeps every later page aligned. `None` while the
/// page has an unvisited inner page, which a file-row resume would drop.
pub(super) fn resume_search(
    source: &Value,
    receipt: Option<&Value>,
    position: usize,
) -> Option<Value> {
    let tool = crate::tools::clasify::resource::tool_of(source)?;
    if !matches!(tool, ToolId::LocalSearch | ToolId::GhSearchCode)
        || receipt.is_some_and(crate::tools::clasify::context::has_inner_page)
    {
        return None;
    }
    let mut resumed = source.clone();
    let query = resumed.get_mut("query")?.as_object_mut()?;
    let page = query.get("page").and_then(Value::as_u64).unwrap_or(1);
    // A later localSearch page without `pageSize` is cut by the response
    // budget, not at a file offset a resume could name.
    if tool == ToolId::LocalSearch && page > 1 && !query.contains_key("pageSize") {
        return None;
    }
    let size = query
        .get("pageSize")
        .and_then(Value::as_u64)
        .unwrap_or_else(|| default_search_page_size(source))
        .max(1);
    start_page_at(query, page, size, position)?;
    Some(resumed)
}

/// The list page that starts at row `position` of `source`'s page, so a
/// replay resumes at the first deferred candidate: a paged list whose
/// candidates are its rows (repositories, history items, discovered
/// packages). `rows` is the page's row count, the size a first page without
/// `pageSize` starts from. `None` when the page's offset is unknown (a later
/// page without `pageSize`) or the tool groups rows into candidates.
pub(super) fn resume_list(source: &Value, position: usize, rows: usize) -> Option<Value> {
    let (page, size) = list_page_of(source, rows)?;
    let offset = page
        .saturating_sub(1)
        .saturating_mul(size)
        .saturating_add(u64::try_from(position).ok()?);
    list_page_at(source, offset, size)
}

/// The page after `source`'s list page when a resume left it off the
/// original grid (`resumePageSize`): it steps back to that size at the next
/// aligned row. `None` when the page is on its grid (its own continuation
/// is exact).
pub(super) fn after_resumed_list(source: &Value, rows: usize) -> Option<Value> {
    source.get("resumePageSize")?;
    let (page, size) = list_page_of(source, rows)?;
    list_page_at(source, page.saturating_mul(size), size)
}

/// `(page, pageSize)` of a row-per-candidate paged list read.
fn list_page_of(source: &Value, rows: usize) -> Option<(u64, u64)> {
    let read = crate::tools::clasify::resource::ResourceSource::of(source)?;
    let one_row_each = matches!(
        tool_of(source)?,
        ToolId::GhSearchRepo | ToolId::GhSearchHistory | ToolId::ArtifactSearch
    );
    if !(one_row_each && read.is_paged_list()) {
        return None;
    }
    let query = source.get("query")?.as_object()?;
    let page = query.get("page").and_then(Value::as_u64).unwrap_or(1);
    let size = match query.get("pageSize").and_then(Value::as_u64) {
        Some(size) => size,
        // A first page starts at offset 0 under any page size.
        None if page <= 1 => u64::try_from(rows).ok()?,
        None => return None,
    }
    .max(1);
    Some((page, size))
}

/// `source` re-paged to start at row `offset`. The walk's page size is the
/// original one (`resumePageSize`, else `size`); a page that starts off its
/// grid takes the largest size that divides the offset and stops at the next
/// aligned row, where the original size returns (and `resumePageSize` is
/// dropped), so every row is read once and the walk regains its page.
fn list_page_at(source: &Value, offset: u64, size: u64) -> Option<Value> {
    let original = source
        .get("resumePageSize")
        .and_then(Value::as_u64)
        .unwrap_or(size)
        .max(1);
    let mut limit = original - offset % original;
    while !offset.is_multiple_of(limit) {
        limit -= 1;
    }
    let mut resumed = source.clone();
    let fields = resumed.as_object_mut()?;
    if limit == original {
        fields.remove("resumePageSize");
    } else {
        fields.insert("resumePageSize".into(), json!(original));
    }
    let query = fields.get_mut("query")?.as_object_mut()?;
    query.insert("pageSize".into(), json!(limit));
    query.insert("page".into(), json!(offset / limit + 1));
    Some(resumed)
}

/// Re-page `query` (now page `page` of `size`) to start at its row
/// `position`: the largest page size up to `size` that divides the new
/// offset keeps every later page aligned.
fn start_page_at(
    query: &mut serde_json::Map<String, Value>,
    page: u64,
    size: u64,
    position: usize,
) -> Option<()> {
    let offset = page
        .saturating_sub(1)
        .saturating_mul(size)
        .saturating_add(u64::try_from(position).ok()?);
    let mut limit = size;
    while offset % limit != 0 {
        limit -= 1;
    }
    query.insert("pageSize".into(), json!(limit));
    query.insert("page".into(), json!(offset / limit + 1));
    Some(())
}

/// Re-reads one oversized file page may take to fit the resource cap.
pub(super) const MAX_SHRINK_ATTEMPTS: usize = 4;

/// Query fields that select less than the whole file in the source view; a
/// flat page read with any of them is not re-read from line one.
const PARTIAL_READ_FIELDS: [&str; 7] = [
    "matchString",
    "ranges",
    "block",
    "unit",
    "offset",
    "length",
    "contextBytes",
];

/// `(unit, offset, size)` of a page's chunk window. A flat whole-file page
/// (no pagination, no range or match selector, source view) is the window of
/// all its lines from the first.
fn chunk_window<'a>(source: &Value, data: &'a Value) -> Option<(&'a str, u64, u64)> {
    if let Some(pagination) = data.get("pagination") {
        return Some((
            pagination.get("unit")?.as_str()?,
            pagination.get("offset")?.as_u64()?,
            pagination.get("length")?.as_u64()?,
        ));
    }
    let query = source.get("query")?.as_object()?;
    let source_view = query
        .get("minify")
        .is_none_or(|minify| minify.as_str() == Some("none"));
    if !source_view
        || PARTIAL_READ_FIELDS
            .iter()
            .any(|field| query.contains_key(*field))
    {
        return None;
    }
    Some(("lines", 0, data.get("totalLines")?.as_u64()?))
}

/// The same file page re-read with fewer chunk units, in proportion to the
/// cap. It keeps the page's start (`pagination.offset`), unit, line range, and
/// version (`snapshot`); a flat whole-file page is re-read as line chunks from
/// its first line. `None` unless the page is chunk-addressed (or whole) and
/// larger than one unit.
pub(super) fn shrunk_page(
    source: &Value,
    state: &Value,
    chars: usize,
    cap: usize,
) -> Option<Value> {
    if !is_file_read(source) {
        return None;
    }
    let data = state.pointer("/results/0/data")?;
    let (unit, offset, size) = chunk_window(source, data)?;
    if size <= 1 || chars == 0 {
        return None;
    }
    let scaled = u128::from(size).saturating_mul(cap as u128) / chars as u128;
    let smaller = u64::try_from(scaled).unwrap_or(u64::MAX).clamp(1, size - 1);
    let snapshot = data
        .get("next")
        .and_then(Value::as_object)
        .and_then(|next| {
            next.values()
                .find_map(|call| call.pointer("/query/queries/0/snapshot"))
        })
        .cloned();
    let mut read = source.clone();
    let query = read.get_mut("query")?.as_object_mut()?;
    query.remove("fullContent");
    query.insert("unit".into(), json!(unit));
    query.insert("offset".into(), json!(offset));
    query.insert("length".into(), json!(smaller));
    if let Some(snapshot) = snapshot {
        query.entry("snapshot").or_insert(snapshot);
    }
    Some(read)
}

/// Line spans of a `ranges` file read that a smaller window can split: a
/// plain source view (no match, block, chunk, or byte selector), clamped to
/// the file's `totalLines`. `None` for any other read.
pub(super) fn split_ranges(source: &Value, state: &Value) -> Option<Vec<(u64, u64)>> {
    if !is_file_read(source) {
        return None;
    }
    let query = source.get("query")?.as_object()?;
    let source_view = query
        .get("minify")
        .is_none_or(|minify| minify.as_str() == Some("none"));
    let data = state.pointer("/results/0/data")?;
    if !source_view
        || data.get("pagination").is_some()
        || PARTIAL_READ_FIELDS
            .iter()
            .any(|field| *field != "ranges" && query.contains_key(*field))
    {
        return None;
    }
    let total = data.get("totalLines").and_then(Value::as_u64);
    let spans = query
        .get("ranges")?
        .as_array()?
        .iter()
        .map(|range| {
            let (start, end) = range.as_str()?.split_once('-')?;
            let (start, end) = (start.parse::<u64>().ok()?, end.parse::<u64>().ok()?);
            Some((start, total.map_or(end, |total| end.min(total))))
        })
        .collect::<Option<Vec<_>>>()?
        .into_iter()
        .filter(|(start, end)| start <= end)
        .collect::<Vec<_>>();
    (!spans.is_empty()).then_some(spans)
}

/// Lines `spans` cover.
pub(super) fn span_lines(spans: &[(u64, u64)]) -> u64 {
    spans.iter().map(|(start, end)| end - start + 1).sum()
}

/// `source` reading the first `keep` lines of `spans` (`head`) and the
/// read of every line after them (`None` when nothing is left).
pub(super) fn ranges_read(
    source: &Value,
    spans: &[(u64, u64)],
    keep: u64,
) -> (Value, Option<Value>) {
    let mut head = Vec::new();
    let mut tail = Vec::new();
    let mut left = keep;
    for &(start, end) in spans {
        let lines = end - start + 1;
        if left >= lines {
            head.push((start, end));
            left -= lines;
        } else if left > 0 {
            head.push((start, start + left - 1));
            tail.push((start + left, end));
            left = 0;
        } else {
            tail.push((start, end));
        }
    }
    let read = |spans: &[(u64, u64)]| {
        let mut read = source.clone();
        read["query"]["ranges"] = json!(
            spans
                .iter()
                .map(|(start, end)| format!("{start}-{end}"))
                .collect::<Vec<_>>()
        );
        read
    };
    (read(&head), (!tail.is_empty()).then(|| read(&tail)))
}

/// A continuation of a shrunk page reads its following pages at the size the
/// walk had before the oversized page.
pub(super) fn restore_chunk(mut next: Value, size: Option<&Value>) -> Value {
    if let (Some(size), Some(query)) = (size, next.get_mut("query").and_then(Value::as_object_mut))
        && query.contains_key("length")
    {
        query.insert("length".into(), size.clone());
    }
    next
}

/// What one `capture_pages` call may spend. `left` is this call's share;
/// `cap` is the largest page: one provider request (`maxPageChars`), or less
/// when `maxChars` is smaller. A page
/// within `cap` but over `left` is deferred; a page over `cap` must shrink or
/// fail. `defer` defers even a first page
/// over `left` (a later prefilter window after the shared budget is spent).
#[derive(Clone, Copy)]
pub(super) struct CaptureBudget {
    pub(super) left: usize,
    pub(super) cap: usize,
    pub(super) defer: bool,
}

impl CaptureBudget {
    pub(super) fn whole(max_chars: usize) -> Self {
        Self {
            left: max_chars,
            cap: max_chars.min(MAX_PAGE_CHARS),
            defer: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supplied_value_budget_counts_object_keys_and_fields_named_next() {
        let state = json!({"next":{"this-key-is-evidence":"retained"}});
        let source = json!({"value":state});
        // Keys and values count like tool evidence; JSON punctuation does not.
        assert_eq!(
            assessed_payload_chars(&source, &state),
            "next".len() + "this-key-is-evidence".len() + "retained".len()
        );
    }

    #[test]
    fn tool_budget_excludes_only_the_canonical_data_continuation() {
        let source = json!({"tool":"localFetch","query":{"path":"/tmp/source"}});
        let first = json!({"results":[{"data":{
            "content":{"next":"evidence"},
            "next":{"continue":{"tool":"localFetch","query":{"path":"short"}}}
        }}]});
        let second = json!({"results":[{"data":{
            "content":{"next":"evidence"},
            "next":{"continue":{"tool":"localFetch","query":{"path":"a".repeat(10_000)}}}
        }}]});
        assert_eq!(
            assessed_payload_chars(&source, &first),
            assessed_payload_chars(&source, &second),
            "continuation metadata must not consume the evidence budget"
        );
        assert!(
            assessed_payload_chars(&source, &first) >= "contentnextevidence".chars().count(),
            "nested evidence named next must still consume the budget"
        );
    }

    #[test]
    fn budget_keeps_page_order_fails_what_never_fits_and_defers_the_rest() {
        let candidate = |chars: usize| Candidate {
            page: CapturedPage::Ready {
                state: json!({}),
                context: json!({"read":{"tool":"localFetch","query":{"path":format!("{chars}")}}}),
            },
            chars,
            position: Some(chars),
        };
        let (kept, used, deferred) = budget_candidates(
            vec![candidate(40), candidate(500), candidate(50), candidate(30)],
            100,
            100,
        );
        assert_eq!(used, 90);
        assert_eq!(kept.len(), 3);
        let CapturedPage::Failed { error, context } = &kept[1] else {
            panic!("a candidate over the cap fails");
        };
        assert_eq!(error.code, "classificationContextTooLarge");
        assert!(context["read"].is_object(), "it keeps its read");
        assert_eq!(deferred.len(), 1);
        assert_eq!(deferred[0].position, Some(30));
        let spent = budget_spent(deferred.into_iter().next().unwrap(), 10);
        assert!(matches!(&spent, CapturedPage::Failed { error, context }
                if error.code == "classificationBudgetSpent" && context["read"].is_object()));
    }

    #[test]
    fn deferred_search_candidates_resume_at_their_file_row() {
        let source = json!({"tool":"localSearch","query":{"path":"/r","matchString":"x","page":2,"pageSize":3}});
        // Row 1 of page 2 is overall row 4: a page size of 2 starts there.
        let resumed = resume_search(&source, None, 1).expect("resume");
        assert_eq!(resumed["query"]["pageSize"], 2);
        assert_eq!(resumed["query"]["page"], 3);
        let first = resume_search(&source, None, 0).expect("resume");
        assert_eq!(first["query"]["pageSize"], 3);
        assert_eq!(first["query"]["page"], 2);
        let inner = json!({"next":{"nextMatchPage":{"tool":"localSearch","query":{}}}});
        assert!(resume_search(&source, Some(&inner), 1).is_none());
        let list = json!({"tool":"astSearch","query":{"operation":"match","page":1,"pageSize":3}});
        assert!(resume_search(&list, None, 1).is_none());
    }

    /// P3: a list whose candidates are its rows resumes at the first
    /// deferred row; a list that groups rows (AST matches per file) or a
    /// later page of unknown size does not.
    #[test]
    fn deferred_list_candidates_resume_at_their_row() {
        let repos = json!({"tool":"ghSearchRepo","query":{"keywords":["x"],"page":2,"pageSize":5}});
        // Row 2 of page 2 is overall row 7: a page size of 1 starts there,
        // and the walk remembers its 5-row grid.
        let resumed = resume_list(&repos, 2, 5).expect("resume");
        assert_eq!(resumed["query"]["pageSize"], 1);
        assert_eq!(resumed["query"]["page"], 8);
        assert_eq!(resumed["resumePageSize"], 5);
        // Off the grid, the next pages step back to it: rows 8-9, then 10-14.
        let page = |value: &Value| {
            (
                value["query"]["page"].clone(),
                value["query"]["pageSize"].clone(),
            )
        };
        let next = after_resumed_list(&resumed, 1).expect("step");
        assert_eq!(page(&next), (json!(5), json!(2)));
        assert_eq!(next["resumePageSize"], 5);
        let aligned = after_resumed_list(&next, 2).expect("step");
        assert_eq!(page(&aligned), (json!(3), json!(5)));
        assert!(aligned.get("resumePageSize").is_none(), "{aligned}");
        // On its grid, a page's own continuation is exact.
        assert!(after_resumed_list(&aligned, 5).is_none());
        let even = resume_list(&repos, 0, 5).expect("resume");
        assert_eq!(
            (
                even["query"]["page"].clone(),
                even["query"]["pageSize"].clone()
            ),
            (json!(2), json!(5))
        );
        // A first page without pageSize starts at offset 0 under any size.
        let first = json!({"tool":"ghSearchHistory","query":{"operation":"pullRequest","owner":"o","repo":"r"}});
        let resumed = resume_list(&first, 4, 10).expect("resume");
        assert_eq!(resumed["query"]["pageSize"], 4);
        assert_eq!(resumed["query"]["page"], 2);
        assert_eq!(resumed["resumePageSize"], 10);
        let later = json!({"tool":"ghSearchRepo","query":{"keywords":["x"],"page":3}});
        assert!(resume_list(&later, 1, 5).is_none());
        let ast = json!({"tool":"astSearch","query":{"operation":"match","path":"/r","pattern":"f()","page":1,"pageSize":3}});
        assert!(resume_list(&ast, 1, 3).is_none());
        let lookup =
            json!({"tool":"artifactSearch","query":{"ecosystem":"npm","packageName":"ajv"}});
        assert!(resume_list(&lookup, 1, 3).is_none());
    }

    /// A `ranges` read splits at a line count into the head it judges now
    /// and the read of every later line; other selectors never split.
    #[test]
    fn a_ranges_read_splits_into_a_head_and_the_rest() {
        let source =
            json!({"tool":"localFetch","query":{"path":"/r/a.txt","ranges":["3-10","20-40"]}});
        let state = json!({"results":[{"data":{"totalLines":30,"content":"x"}}]});
        let spans = split_ranges(&source, &state).expect("splits");
        assert_eq!(spans, vec![(3, 10), (20, 30)], "clamped to the file");
        assert_eq!(span_lines(&spans), 19);
        let (head, tail) = ranges_read(&source, &spans, 10);
        assert_eq!(head["query"]["ranges"], json!(["3-10", "20-21"]));
        assert_eq!(tail.expect("rest")["query"]["ranges"], json!(["22-30"]));
        let (whole, rest) = ranges_read(&source, &spans, 19);
        assert_eq!(whole["query"]["ranges"], json!(["3-10", "20-30"]));
        assert!(rest.is_none());
        let matched = json!({"tool":"localFetch","query":{"path":"/r/a.txt","ranges":["3-10"],"matchString":"x"}});
        assert!(split_ranges(&matched, &state).is_none());
        let paged = json!({"results":[{"data":{"totalLines":30,"pagination":{"unit":"lines"}}}]});
        assert!(split_ranges(&source, &paged).is_none());
    }

    #[test]
    fn an_oversized_page_shrinks_at_its_own_start_and_version() {
        let source = json!({"tool":"localFetch","query":{"path":"/r/a.txt","fullContent":true}});
        let state = json!({"results":[{"data":{
            "pagination":{"unit":"lines","offset":16,"length":8,"hasMore":true},
            "next":{"continue":{"tool":"localFetch","query":{"queries":[{"snapshot":"v1"}]}}}
        }}]});
        let smaller = shrunk_page(&source, &state, 2_233, 2_000).expect("smaller page");
        assert_eq!(
            smaller["query"],
            json!({"path":"/r/a.txt","unit":"lines","offset":16,"length":7,"snapshot":"v1"})
        );
        let single = json!({"results":[{"data":{
            "pagination":{"unit":"lines","offset":3,"length":1}
        }}]});
        assert!(shrunk_page(&source, &single, 500, 100).is_none());
        let search = json!({"tool":"localSearch","query":{"path":"/r"}});
        assert!(shrunk_page(&search, &state, 2_233, 2_000).is_none());
        // A flat whole-file page re-reads its first lines as chunks.
        let whole = json!({"results":[{"data":{"content":"1\tx","totalLines":1_144}}]});
        let first = shrunk_page(&source, &whole, 43_477, 6_000).expect("whole file shrinks");
        assert_eq!(
            first["query"],
            json!({"path":"/r/a.txt","unit":"lines","offset":0,"length":157})
        );
        // A flat page selected by match or range is not the whole file.
        let matched = json!({"tool":"localFetch","query":{"path":"/r/a.txt","matchString":"x"}});
        assert!(shrunk_page(&matched, &whole, 43_477, 6_000).is_none());
        let outline = json!({"tool":"localFetch","query":{"path":"/r/a.txt","minify":"symbols"}});
        assert!(shrunk_page(&outline, &whole, 43_477, 6_000).is_none());
        let restored = restore_chunk(smaller, Some(&json!(8)));
        assert_eq!(restored["query"]["length"], 8);
    }
}
