//! Pages of list answers and the graphQuery calls that continue them.
use super::*;

pub(super) struct Page<T = Value> {
    pub(super) rows: Vec<T>,
    pub(super) total: usize,
    /// Absolute result offset of the first materialized row.
    pub(super) start: usize,
}

pub(super) const LIST_PREVIEW: usize = 100;

/// Answer facts that do not change from page to page: they go out once,
/// with the first page of the results.
pub(super) const FIRST_PAGE_ONLY: [&str; 4] =
    ["summary", "baseline", "coverage", "argumentCoverage"];

/// Whether `options` asks the first page of an answer's results.
pub(super) fn first_page(options: &QueryOptions) -> bool {
    options.offset.unwrap_or(0) == 0 && options.list.is_none()
}

pub(super) fn select_list(
    options: &QueryOptions,
    extra: &mut Map<String, Value>,
) -> Result<Option<Page>, GraphOutput> {
    let Some(list) = options.list.as_deref() else {
        return Ok(None);
    };
    let Some((section, field)) = list.split_once('.') else {
        return Err(GraphOutput::error(
            ErrorCode::InvalidInput,
            "list needs summary.<field> or baseline.<field>",
        ));
    };
    if !matches!(section, "summary" | "baseline") {
        return Err(GraphOutput::error(
            ErrorCode::InvalidInput,
            "list names a summary or baseline array",
        ));
    }
    let Some(Value::Array(rows)) = extra
        .get_mut(section)
        .and_then(Value::as_object_mut)
        .and_then(|object| object.remove(field))
    else {
        return Err(GraphOutput::error(
            ErrorCode::InvalidInput,
            format!("unknown evidence list {list:?}"),
        ));
    };
    let total = rows.len();
    extra.insert("list".into(), json!(list));
    Ok(Some(Page {
        rows,
        total,
        start: 0,
    }))
}

/// Cuts each summary or baseline list longer than [`LIST_PREVIEW`] to its
/// preview; returns the channel that lists the rest of each.
pub(super) fn preview_extra_lists(extra: &mut Map<String, Value>) -> Vec<(Channel, Resume)> {
    let mut continuations = Vec::new();
    for section in ["summary", "baseline"] {
        let Some(object) = extra.get_mut(section).and_then(Value::as_object_mut) else {
            continue;
        };
        for (field, value) in object {
            let Some(rows) = value.as_array_mut() else {
                continue;
            };
            if rows.len() <= LIST_PREVIEW {
                continue;
            }
            let list = format!("{section}.{field}");
            rows.truncate(LIST_PREVIEW);
            // The preview listed the first rows; the list resumes after them.
            let resume = Resume::Row {
                offset: LIST_PREVIEW,
                list: Some(list.clone()),
                cycle: None,
            };
            continuations.push((Channel::List(list), resume));
        }
    }
    continuations
}

pub(super) fn run_page(
    options: &QueryOptions,
    graph: &Graph,
    mut extra: Map<String, Value>,
    mut page: Page,
) -> GraphOutput {
    if options.list.as_deref() != Some("cycleNodes") {
        match select_list(options, &mut extra) {
            Ok(Some(selected)) => page = selected,
            Ok(None) => {}
            Err(error) => return error,
        }
    }
    let offset = options.offset.unwrap_or(0);
    // The summary, its evidence lists and coverage go out once, with the
    // first page of the results; later pages and `list` pages carry rows only.
    let lists = if first_page(options) {
        preview_extra_lists(&mut extra)
    } else {
        for section in FIRST_PAGE_ONLY {
            extra.remove(section);
        }
        Vec::new()
    };
    let limit = options.limit.unwrap_or_else(default_limit);
    let total = page.total;
    let rows = page
        .rows
        .into_iter()
        .skip(offset.saturating_sub(page.start))
        .take(limit)
        .collect::<Vec<_>>();
    let shown = rows.len();
    let mut out = Map::new();
    out.insert("graph".into(), graph.summary());
    out.insert("operation".into(), json!(options.op));
    out.extend(extra);
    out.insert("rows".into(), Value::Array(rows));
    // Nothing to list, or a page past the last row.
    let mut output = if shown == 0 {
        GraphOutput::empty(Value::Object(out))
    } else {
        GraphOutput::ok(Value::Object(out))
    };
    output.next = lists;
    settle(&mut output, options, total);
    output
}

/// States the page of a list answer's `rows`: its `pagination`, and the
/// channels that continue the rows it shows (the rows after them, each cycle
/// cut at its preview). Evidence-list channels stay.
pub(super) fn settle(output: &mut GraphOutput, options: &QueryOptions, total: usize) {
    let offset = options.offset.unwrap_or(0);
    let rows = output.value["rows"].as_array().map(Vec::as_slice).unwrap_or_default();
    let shown = rows.len();
    let cycles = rows
        .iter()
        .enumerate()
        .filter(|(_, row)| row["nodesTruncated"] == true)
        .map(|(index, row)| {
            let nodes = row["nodes"].as_array().map(Vec::as_slice).unwrap_or_default();
            // The preview shows the first members: the listing reads on after them.
            let resume = Resume::Row {
                offset: nodes.len(),
                list: Some("cycleNodes".into()),
                cycle: nodes.first().and_then(Value::as_str).map(str::to_owned),
            };
            (Channel::Cycle(index), resume)
        })
        .collect::<Vec<_>>();
    let has_more = offset + shown < total;
    output.next.retain(|(channel, _)| matches!(channel, Channel::List(_)));
    if has_more {
        let resume = Resume::Row {
            offset: offset + shown,
            list: None,
            cycle: None,
        };
        output.next.push((Channel::NextPage, resume));
    }
    output.next.extend(cycles);
    output.value["pagination"] = pagination(options, total, has_more);
}

/// The shared `pagination` object: a page's rows vary with the response
/// budget, so `page` counts the calls made and no page count is stated. Row
/// position stays in the cursor.
pub(super) fn pagination(options: &QueryOptions, total: usize, has_more: bool) -> Value {
    crate::response::pages::PageFacts::open(options.page.unwrap_or(1), None, has_more)
        .with_items(total)
        .to_value()
}

/// The request `resume` continues `options` with, in contract field names:
/// the room a page reserves for each continuation it states.
pub(super) fn resume_row(options: &QueryOptions, id: &str, resume: &Resume) -> Map<String, Value> {
    let (mut row, offset) = match resume {
        Resume::Row { offset, list, cycle } => {
            let mut row = query_row(options, id);
            if let Some(list) = list {
                row.insert("list".into(), json!(list));
            }
            if let Some(cycle) = cycle {
                row.insert("cycle".into(), json!(cycle));
            }
            (row, *offset)
        }
        Resume::Query(next) => (query_row(next, id), next.offset.unwrap_or(0)),
    };
    row.insert("offset".into(), json!(offset));
    row
}

/// One tool call as text: the tool name, then its query row as JSON, for a
/// call named inside a prose note.
pub(super) fn call(tool: &str, row: Map<String, Value>) -> String {
    format!("{tool} {}", Value::Object(row))
}

/// The graphQuery row `options` asks, in contract field names, pinned to
/// snapshot `id` (and the `path` that selected it).
pub(crate) fn query_row(options: &QueryOptions, id: &str) -> Map<String, Value> {
    let mut row = Map::new();
    row.insert("operation".into(), json!(options.op));
    pin(&mut row, options, id);
    let sources = options.sources().collect::<Vec<_>>();
    match sources.as_slice() {
        [] => {}
        [one] => {
            row.insert("target".into(), json!(one));
        }
        several => {
            row.insert("target".into(), json!(several));
        }
    }
    let lists = [
        ("edges", &options.edges),
        ("reasons", &options.reasons),
        ("detectors", &options.detectors),
        ("entries", &options.entries),
        ("changed", &options.changed),
    ];
    for (field, values) in lists {
        if !values.is_empty() {
            row.insert(field.into(), json!(values));
        }
    }
    let baseline = options
        .baseline
        .as_deref()
        .map(|base| snapshot_id(base).to_owned());
    let fields = [
        ("to", options.to.as_ref().map(|to| json!(to))),
        ("kind", options.kind.as_ref().map(|kind| json!(kind))),
        ("view", options.view.as_ref().map(|view| json!(view))),
        ("direction", options.direction.as_ref().map(|d| json!(d))),
        ("depth", options.depth.map(|depth| json!(depth))),
        ("confidence", options.confidence.as_ref().map(|c| json!(c))),
        ("list", options.list.as_ref().map(|list| json!(list))),
        ("limit", options.limit.map(|limit| json!(limit))),
        ("baseline", baseline.map(|base| json!(base))),
        ("minScore", options.min_score.map(|score| json!(score))),
        ("since", options.since.as_ref().map(|since| json!(since))),
        ("minTier", options.min_tier.map(|tier| json!(tier))),
        (
            "includeProvenance",
            options.include_provenance.then_some(json!(true)),
        ),
    ];
    for (field, value) in fields {
        if let Some(value) = value {
            row.insert(field.into(), value);
        }
    }
    row
}

/// Pins `row` to snapshot `id` and the `path` that selected it.
pub(super) fn pin(row: &mut Map<String, Value>, options: &QueryOptions, id: &str) {
    if let Some(path) = &options.path {
        row.insert("path".into(), json!(path));
    }
    row.insert("graph".into(), json!(id));
}

/// The snapshot id a snapshot reference names: an id, or a snapshot
/// directory whose name is its id.
fn snapshot_id(reference: &str) -> &str {
    let path = Path::new(reference);
    if path.is_absolute() {
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(reference)
    } else {
        reference
    }
}
