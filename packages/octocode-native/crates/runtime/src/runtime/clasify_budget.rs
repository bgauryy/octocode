//! Evidence budget for one clasify resource: what counts as a character of
//! evidence, which split candidates fit, how an oversized file page shrinks,
//! and where a deferred search page resumes. Every capture path (file pages,
//! search snippets, hydrated chunks, list items) applies these rules, so
//! `maxChars` bounds the evidence submitted for classification.
use super::{CONTROL_FIELDS, CapturedPage, default_search_page_size, file_evidence, is_file_read};
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
        Some(data) if payload.get("base").is_some() => tool_data_chars(data),
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
            "The captured page is {chars} characters, above maxChars {cap}; no classification was run."
        ),
        "Select a smaller complete section, reduce the read page size, or raise maxChars within its limit.",
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
                "Run its next.read, or narrow the page or raise maxChars to classify it.",
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
    let tool = ToolId::from_name(source.get("tool")?.as_str()?)?;
    if !matches!(tool, ToolId::LocalSearch | ToolId::GhSearchCode)
        || receipt.is_some_and(crate::runtime::clasify_context::has_inner_page)
    {
        return None;
    }
    let mut resumed = source.clone();
    let query = resumed.get_mut("query")?.as_object_mut()?;
    let page = query.get("page").and_then(Value::as_u64).unwrap_or(1);
    let size = query
        .get("pageSize")
        .and_then(Value::as_u64)
        .unwrap_or_else(|| default_search_page_size(source))
        .max(1);
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
    Some(resumed)
}

/// Re-reads one oversized file page may take to fit the resource cap.
pub(super) const MAX_SHRINK_ATTEMPTS: usize = 4;

/// The same file page re-read with fewer chunk units, in proportion to the
/// cap. It keeps the page's start (`pagination.offset`), unit, line range, and
/// version (`snapshot`). `None` unless the page is chunk-addressed and larger
/// than one unit.
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
    let pagination = data
        .get("pagination")
        .or_else(|| data.pointer("/files/0/pagination"))?;
    let size = pagination.get("chunkSize")?.as_u64()?;
    let offset = pagination.get("offset")?.as_u64()?;
    let unit = pagination.get("chunkType")?.as_str()?;
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
                .find_map(|call| call.pointer("/query/snapshot"))
        })
        .cloned();
    let mut read = source.clone();
    let query = read.get_mut("query")?.as_object_mut()?;
    query.remove("fullContent");
    query.insert("chunkType".into(), json!(unit));
    query.insert("offset".into(), json!(offset));
    query.insert("chunkSize".into(), json!(smaller));
    if let Some(snapshot) = snapshot {
        query.entry("snapshot").or_insert(snapshot);
    }
    Some(read)
}

/// A continuation of a shrunk page reads its following pages at the size the
/// walk had before the oversized page.
pub(super) fn restore_chunk(mut next: Value, size: Option<&Value>) -> Value {
    if let (Some(size), Some(query)) = (size, next.get_mut("query").and_then(Value::as_object_mut))
        && query.contains_key("chunkSize")
    {
        query.insert("chunkSize".into(), size.clone());
    }
    next
}

/// What one `capture_pages` call may spend. `left` is this call's share;
/// `cap` is the resource's whole `maxChars`, which a fresh call (the
/// continuation) gets. A page within `cap` but over `left` is deferred; a
/// page over `cap` must shrink or fail. `defer` defers even a first page
/// over `left` (a later prefilter window after the shared budget is spent).
#[derive(Clone, Copy)]
pub(super) struct CaptureBudget {
    pub(super) left: usize,
    pub(super) cap: usize,
    pub(super) defer: bool,
}

impl CaptureBudget {
    pub(super) const fn whole(cap: usize) -> Self {
        Self {
            left: cap,
            cap,
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
        let source = json!({"tool":"localSearch","query":{"path":"/r","searchText":"x","page":2,"pageSize":3}});
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

    #[test]
    fn an_oversized_page_shrinks_at_its_own_start_and_version() {
        let source = json!({"tool":"localFetch","query":{"path":"/r/a.txt","fullContent":true}});
        let state = json!({"results":[{"data":{
            "pagination":{"chunkType":"lines","offset":16,"chunkSize":8,"hasMore":true},
            "next":{"continue":{"tool":"localFetch","query":{"snapshot":"v1"}}}
        }}]});
        let smaller = shrunk_page(&source, &state, 2_233, 2_000).expect("smaller page");
        assert_eq!(
            smaller["query"],
            json!({"path":"/r/a.txt","chunkType":"lines","offset":16,"chunkSize":7,"snapshot":"v1"})
        );
        let single = json!({"results":[{"data":{
            "pagination":{"chunkType":"lines","offset":3,"chunkSize":1}
        }}]});
        assert!(shrunk_page(&source, &single, 500, 100).is_none());
        let search = json!({"tool":"localSearch","query":{"path":"/r"}});
        assert!(shrunk_page(&search, &state, 2_233, 2_000).is_none());
        let restored = restore_chunk(smaller, Some(&json!(8)));
        assert_eq!(restored["query"]["chunkSize"], 8);
    }
}
