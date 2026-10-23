//! Pages of list answers and the paste-ready `next` commands that continue them.
use super::*;

pub(super) struct Page {
    pub(super) rows: Vec<Value>,
    pub(super) total: usize,
}

pub(super) const LIST_PREVIEW: usize = 100;

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
            "--list needs summary.<field> or baseline.<field>",
        ));
    };
    if !matches!(section, "summary" | "baseline") {
        return Err(GraphOutput::error(
            ErrorCode::InvalidInput,
            "--list names a summary or baseline array",
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
    Ok(Some(Page { rows, total }))
}

pub(super) fn preview_extra_lists(
    options: &QueryOptions,
    id: &str,
    extra: &mut Map<String, Value>,
) {
    let mut continuations = Map::new();
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
            let mut next = options.clone();
            next.list = Some(list.clone());
            rows.truncate(LIST_PREVIEW);
            // The preview listed the first rows; the list resumes after them.
            continuations.insert(list, json!(next_command(&next, id, LIST_PREVIEW)));
        }
    }
    if !continuations.is_empty() {
        extra.insert("nextLists".into(), Value::Object(continuations));
    }
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
    // The summary and its evidence lists go out once, with the first page
    // of the results; later pages and `--list` pages carry their rows only.
    if offset == 0 && options.list.is_none() {
        preview_extra_lists(options, &graph.id, &mut extra);
    } else {
        for section in ["summary", "baseline"] {
            extra.remove(section);
        }
    }
    let limit = options.limit.unwrap_or(DEFAULT_LIMIT);
    let total = page.total;
    let rows = page
        .rows
        .into_iter()
        .skip(offset)
        .take(limit)
        .collect::<Vec<_>>();
    let shown = rows.len();
    let mut out = Map::new();
    out.insert("graph".into(), graph.summary());
    out.insert("op".into(), json!(options.op));
    out.extend(extra);
    out.insert("total".into(), json!(total));
    if offset > 0 {
        out.insert("offset".into(), json!(offset));
    }
    out.insert("results".into(), Value::Array(rows));

    if offset + shown < total {
        out.insert("truncated".into(), json!(true));
        out.insert(
            "next".into(),
            json!(next_command(options, &graph.id, offset + shown)),
        );
    }
    // Nothing to list, or a page past the last row.
    if shown == 0 {
        GraphOutput::empty(Value::Object(out))
    } else {
        GraphOutput::ok(Value::Object(out))
    }
}

pub(super) fn next_command(options: &QueryOptions, id: &str, offset: usize) -> String {
    format!("{} --offset {offset}", command(options, id))
}

/// A paste-ready `octocode graph query` command for `options`, pinned to
/// snapshot `id` (and the workspace holding it).
pub(super) fn command(options: &QueryOptions, id: &str) -> String {
    let mut parts = vec!["octocode graph query".to_owned(), quote(&options.op)];
    parts.extend(options.target.iter().map(|t| quote(t)));
    parts.extend(options.to.iter().map(|t| quote(t)));
    parts.push(snapshot_flags(options, id));
    if !options.edges.is_empty() {
        parts.push(format!("--edge {}", quote(&options.edges.join(","))));
    }
    for (flag, value) in [
        ("--kind", options.kind.clone()),
        ("--direction", options.direction.clone()),
        ("--depth", options.depth.map(|d| d.to_string())),
        ("--confidence", options.confidence.clone()),
        ("--list", options.list.clone()),
        ("--limit", options.limit.map(|l| l.to_string())),
        (
            "--detector",
            (!options.detectors.is_empty()).then(|| options.detectors.join(",")),
        ),
        (
            "--entry",
            (!options.entries.is_empty()).then(|| options.entries.join(",")),
        ),
        ("--baseline", options.baseline.clone()),
        ("--min-score", options.min_score.map(|m| m.to_string())),
        (
            "--changed",
            (!options.changed.is_empty()).then(|| options.changed.join(",")),
        ),
        ("--since", options.since.clone()),
        ("--min-tier", options.min_tier.map(|t| t.to_string())),
    ] {
        if let Some(value) = value {
            parts.push(format!("{flag} {}", quote(&value)));
        }
    }
    parts.join(" ")
}

/// `--graph <id>` plus the `--workspace` holding it: what pins a command to
/// the snapshot an answer came from (a later ingest moves `latest`).
pub(super) fn snapshot_flags(options: &QueryOptions, id: &str) -> String {
    match &options.workspace {
        Some(workspace) => format!(
            "--graph {} --workspace {}",
            quote(id),
            quote(&workspace.to_string_lossy())
        ),
        None => format!("--graph {}", quote(id)),
    }
}
