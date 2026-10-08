//! Default clasify output: the verbose receipt (`debug:true`) reduced to the
//! decision data. A resource states its file path (and a GitHub file's ref)
//! and line total once, pages become `lines` + answers keyed by question,
//! answers become the bare
//! label / P(yes) / level / exists unless the distribution is uncertain, and
//! `best` rows become `{resourceId?, path?, lines, exists, probability}` with
//! one exact read of the top window in `next.read`. Runner-up windows stay
//! under `debug`.
use crate::tools::id::ToolId;
use serde_json::{Map, Value, json};

/// Choice/score answers whose top probability is at least this keep only the
/// label or level; below it the full distribution stays.
const CERTAIN: f64 = 0.9;

/// What the compact form needs from the matrix that produced the output.
pub(super) struct Matrix<'a> {
    /// The only resource id when the matrix has exactly one resource.
    pub(super) single_resource: Option<&'a str>,
    /// Locate question ids in question order.
    pub(super) locate_ids: &'a [&'a str],
    /// Contract default `maxChars`, omitted from continuations.
    pub(super) default_max_chars: u64,
}

/// Compact one verbose query result in place.
pub(super) fn compact_query_result(output: &mut Value, matrix: &Matrix<'_>) {
    let listed = best_windows(output);
    let mut paths = Map::new();
    if let Some(resources) = output.get_mut("resources").and_then(Value::as_array_mut) {
        for resource in resources.iter_mut() {
            compact_resource(resource, &listed);
            if let (Some(id), Some(path)) = (
                resource.get("id").and_then(Value::as_str),
                resource.get("path"),
            ) {
                paths.insert(id.to_owned(), path.clone());
            }
        }
    }
    let top_read = matrix.locate_ids.iter().find_map(|id| {
        output
            .pointer(&format!("/best/{}/0/next/read", escape(id)))
            .cloned()
    });
    if let Some(best) = output.get_mut("best").and_then(Value::as_object_mut) {
        for rows in best.values_mut() {
            if let Some(rows) = rows.as_array_mut() {
                for row in rows {
                    *row = compact_row(row, matrix.single_resource, &paths);
                }
            }
        }
    }
    if let Some(read) = top_read {
        output["next"]["read"] = read;
    }
    if let Some(next) = output.pointer_mut("/next/clasify") {
        unify_continuation(next, matrix, &paths);
    }
}

/// `(resourceId, startLine, endLine)` of every verbose `best` row: the
/// located windows the compact output already lists.
fn best_windows(output: &Value) -> Vec<(Value, Value, Value)> {
    output
        .get("best")
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(|best| best.values())
        .filter_map(Value::as_array)
        .flatten()
        .map(|row| {
            (
                row["resourceId"].clone(),
                row["startLine"].clone(),
                row["endLine"].clone(),
            )
        })
        .collect()
}

fn escape(token: &str) -> String {
    token.replace('~', "~0").replace('/', "~1")
}

/// `{resourceId, path?, exists, startLine, endLine, probability, next?}` →
/// `{resourceId?, path?, line, endLine, exists, probability}` (the span names
/// localFetch reads take); the only resource's id and a path the resource
/// already states are omitted.
pub(super) fn compact_row(row: &Value, single: Option<&str>, paths: &Map<String, Value>) -> Value {
    let resource = row.get("resourceId").and_then(Value::as_str);
    let mut out = Map::new();
    if resource.is_some() && resource != single {
        out.insert("resourceId".into(), json!(resource));
    }
    if let Some(path) = row.get("path")
        && resource.and_then(|id| paths.get(id)) != Some(path)
    {
        out.insert("path".into(), path.clone());
    }
    out.insert("line".into(), row["startLine"].clone());
    out.insert("endLine".into(), row["endLine"].clone());
    out.insert("exists".into(), row["exists"].clone());
    out.insert("probability".into(), row["probability"].clone());
    Value::Object(out)
}

/// A verbose `best` row in its published shape: `startLine` becomes `line`
/// beside `endLine`; the resource id stays.
fn verbose_row(row: &Value) -> Value {
    let Some(fields) = row.as_object() else {
        return row.clone();
    };
    let mut out = Map::new();
    for (key, value) in fields {
        match key.as_str() {
            "startLine" => {
                out.insert("line".into(), value.clone());
            }
            _ => {
                out.insert(key.clone(), value.clone());
            }
        }
    }
    Value::Object(out)
}

/// Publish one `debug:true` query result in place: verbose `best` rows with
/// `line`/`endLine`, and next.clasify in the public input shape.
pub(super) fn publish_verbose(output: &mut Value, matrix: &Matrix<'_>) {
    if let Some(best) = output.get_mut("best").and_then(Value::as_object_mut) {
        for rows in best.values_mut() {
            if let Some(rows) = rows.as_array_mut() {
                for row in rows {
                    *row = verbose_row(row);
                }
            }
        }
    }
    if let Some(next) = output.pointer_mut("/next/clasify") {
        unify_continuation(next, matrix, &Map::new());
    }
}

/// A carry row back to the ranking row `rank_locate` merges. `None` when it
/// names no resource and the matrix has several.
pub(super) fn ranking_row(row: &Value, single: Option<&str>) -> Option<Value> {
    let resource = row
        .get("resourceId")
        .and_then(Value::as_str)
        .or(single)?
        .to_owned();
    let mut out = json!({
        "resourceId": resource,
        "exists": row.get("exists")?,
        "startLine": row.get("line")?,
        "endLine": row.get("endLine")?,
        "probability": row.get("probability")?,
    });
    if let Some(path) = row.get("path") {
        out["path"] = path.clone();
    }
    Some(out)
}

fn compact_resource(resource: &mut Value, listed: &[(Value, Value, Value)]) {
    let resource_id = resource.get("id").cloned().unwrap_or(Value::Null);
    let Some(fields) = resource.as_object_mut() else {
        return;
    };
    if fields.get("coverage").and_then(Value::as_str) == Some("complete") {
        fields.remove("coverage");
    }
    let Some(pages) = fields.get_mut("pages").and_then(Value::as_array_mut) else {
        return;
    };
    // One file read on every page (a local `{path}` or a GitHub
    // `{ref, path}` source) is stated once, on the resource.
    let shared_source = shared(pages, |page| {
        page.get("source")
            .and_then(Value::as_object)
            .filter(|source| {
                source.contains_key("path")
                    && source.keys().all(|key| key == "path" || key == "ref")
            })
            .map(|source| Value::Object(source.clone()))
    });
    let shared_path = shared_source
        .as_ref()
        .and_then(|source| source.get("path"))
        .cloned();
    let shared_ref = shared_source
        .as_ref()
        .and_then(|source| source.get("ref"))
        .cloned();
    let shared_total = shared(pages, |page| {
        page.get("scope")
            .filter(|scope| scope.get("startLine").is_some())
            .and_then(|scope| scope.get("totalLines"))
            .cloned()
    });
    for page in pages.iter_mut() {
        let window_listed = |read: &Value| {
            crate::tools::clasify::output::read_range(&read["query"]).is_some_and(|(start, end)| {
                listed.contains(&(resource_id.clone(), json!(start), json!(end)))
            })
        };
        compact_page(
            page,
            shared_source.as_ref(),
            shared_total.is_some(),
            &window_listed,
        );
    }
    for page in pages.iter_mut() {
        collapse_cell_errors(page);
    }
    // One error repeated across pages (e.g. the provider refused every
    // request) is stated once, on the resource: when every page failed with
    // it, or when nothing was judged and it is the most repeated page error
    // (other errors, such as a spent page budget, stay on their pages), or
    // when every page repeating it keeps a read (e.g. hit windows a spent
    // budget left unjudged beside judged pages). A page left with only its
    // line span is not listed; a page keeps its read, the host's fallback
    // for an unjudged window.
    if let Some(error) = shared(pages, |page| page.get("error").cloned())
        .or_else(|| unjudged_repeated_error(pages))
        .or_else(|| repeated_read_error(pages))
    {
        for page in pages.iter_mut() {
            if let Some(page) = page.as_object_mut()
                && page.get("error") == Some(&error)
            {
                page.remove("error");
            }
        }
        pages.retain(|page| {
            page.as_object()
                .is_some_and(|page| page.keys().any(|key| key != "line" && key != "endLine"))
        });
        fields.insert("error".into(), error);
    }
    if fields
        .get("pages")
        .and_then(Value::as_array)
        .is_some_and(Vec::is_empty)
    {
        fields.remove("pages");
    }
    let Some(pages) = fields.get_mut("pages").and_then(Value::as_array_mut) else {
        if let Some(path) = shared_path {
            fields.insert("path".into(), path);
        }
        if let Some(reference) = shared_ref {
            fields.insert("ref".into(), reference);
        }
        if let Some(total) = shared_total {
            fields.insert("totalLines".into(), total);
        }
        return;
    };
    let single_bare = match pages.as_slice() {
        [page] => page
            .as_object()
            .filter(|page| {
                page.keys()
                    .all(|key| matches!(key.as_str(), "answers" | "error" | "limitations"))
                    && !(page.contains_key("answers") && page.contains_key("error"))
            })
            .cloned(),
        _ => None,
    };
    if let Some(path) = shared_path {
        fields.insert("path".into(), path);
    }
    if let Some(reference) = shared_ref {
        fields.insert("ref".into(), reference);
    }
    if let Some(total) = shared_total {
        fields.insert("totalLines".into(), total);
    }
    if let Some(page) = single_bare {
        fields.remove("pages");
        fields.extend(page);
    }
}

/// Every cell of a page failed with one identical error (e.g. the provider
/// refused the request): the page states it once instead of per question.
fn collapse_cell_errors(page: &mut Value) {
    let Some(fields) = page.as_object_mut() else {
        return;
    };
    let error = fields
        .get("answers")
        .and_then(Value::as_object)
        .and_then(|answers| {
            let mut errors = answers.values().map(|answer| {
                answer
                    .as_object()
                    .filter(|answer| answer.len() == 1)
                    .and_then(|answer| answer.get("error"))
            });
            let first = errors.next()??;
            errors
                .all(|error| error == Some(first))
                .then(|| first.clone())
        });
    if let Some(error) = error {
        fields.remove("answers");
        fields.insert("error".into(), error);
    }
}

/// The error most pages of a resource that judged nothing repeat, when at
/// least two pages carry it (ties keep the first seen).
fn unjudged_repeated_error(pages: &[Value]) -> Option<Value> {
    if pages.iter().any(|page| page.get("answers").is_some()) {
        return None;
    }
    let mut counts: Vec<(&Value, usize)> = Vec::new();
    for error in pages.iter().filter_map(|page| page.get("error")) {
        match counts.iter_mut().find(|(seen, _)| *seen == error) {
            Some((_, count)) => *count += 1,
            None => counts.push((error, 1)),
        }
    }
    let (error, count) =
        counts
            .into_iter()
            .fold(None::<(&Value, usize)>, |best, entry| match best {
                Some((_, top)) if top >= entry.1 => best,
                _ => Some(entry),
            })?;
    (count >= 2).then(|| error.clone())
}

/// The error most pages repeat (at least two, ties keep the first seen),
/// when every page carrying it keeps its read: stated once on the resource,
/// each such page still lists what stays unjudged.
fn repeated_read_error(pages: &[Value]) -> Option<Value> {
    let mut counts: Vec<(&Value, usize)> = Vec::new();
    for error in pages.iter().filter_map(|page| page.get("error")) {
        match counts.iter_mut().find(|(seen, _)| *seen == error) {
            Some((_, count)) => *count += 1,
            None => counts.push((error, 1)),
        }
    }
    // A page without a read would keep only its error: that error stays on
    // every page carrying it.
    let readless = |error: &Value| {
        pages
            .iter()
            .any(|page| page.get("error") == Some(error) && page.pointer("/next/read").is_none())
    };
    counts
        .into_iter()
        .filter(|(error, count)| *count >= 2 && !readless(error))
        .fold(None::<(&Value, usize)>, |best, entry| match best {
            Some((_, top)) if top >= entry.1 => best,
            _ => Some(entry),
        })
        .map(|(error, _)| error.clone())
}

/// The value every page yields, when every page yields the same one.
fn shared(pages: &[Value], value: impl Fn(&Value) -> Option<Value>) -> Option<Value> {
    let mut values = pages.iter().map(value);
    let first = values.next()??;
    values
        .all(|value| value.as_ref() == Some(&first))
        .then_some(first)
}

fn compact_page(
    page: &mut Value,
    hoisted_source: Option<&Value>,
    total_hoisted: bool,
    window_listed: &dyn Fn(&Value) -> bool,
) {
    let Some(fields) = page.as_object_mut() else {
        return;
    };
    if hoisted_source.is_some_and(|source| fields.get("source") == Some(source)) {
        // The resource states it.
        fields.remove("source");
    } else if let Some(path) = fields
        .get("source")
        .and_then(Value::as_object)
        .filter(|source| source.len() == 1)
        .and_then(|source| source.get("path"))
        .cloned()
    {
        // A source that names only its file is that file's path.
        fields.remove("source");
        fields.insert("path".into(), path);
    }
    if total_hoisted
        && let Some(scope) = fields.get("scope").and_then(Value::as_object)
        && let (Some(start), Some(end)) = (scope.get("startLine"), scope.get("endLine"))
        && scope.len() == 3
    {
        // The span names localFetch `ranges` take (X1): `line`, `endLine`.
        let (start, end) = (start.clone(), end.clone());
        fields.remove("scope");
        fields.shift_insert(0, "endLine".into(), end);
        fields.shift_insert(0, "line".into(), start);
    }
    // A page that judged its whole file states the span as `line`/`endLine`;
    // `scope` (with `totalLines`) stays only on a page that judged part of it.
    if let Some(scope) = fields.get("scope").and_then(Value::as_object)
        && scope.len() == 3
        && scope.get("startLine") == Some(&json!(1))
        && scope.get("endLine").is_some()
        && scope.get("endLine") == scope.get("totalLines")
    {
        let end = scope["endLine"].clone();
        fields.remove("scope");
        fields.shift_insert(0, "endLine".into(), end);
        fields.shift_insert(0, "line".into(), json!(1));
    }
    // A list page that names its own file implies a read that is only
    // localFetch of that path and span: `{path, ranges:["line-endLine"]}`.
    // A one-file resource keeps its page reads (the GATE flow runs them).
    let path = fields.get("path").cloned();
    for key in ["next", "hints"] {
        if path
            .as_ref()
            .is_some_and(|path| implied_local_read(fields, key, path))
        {
            fields.remove(key);
        }
    }
    let Some(answers) = fields.get_mut("answers").and_then(Value::as_object_mut) else {
        return;
    };
    let located = !answers.is_empty()
        && answers
            .values()
            .all(|answer| answer.get("exists").is_some());
    for answer in answers.values_mut() {
        *answer = bare_verdict(answer);
    }
    // A located page's read is its top window: `best` (and the top
    // next.read) already carry a listed one; any other stays reachable here.
    if located
        && fields
            .get("next")
            .and_then(|next| next.get("read"))
            .is_none_or(window_listed)
    {
        fields.remove("next");
    }
}

/// Whether the page's only hint is `localFetch {path, ranges:["line-endLine"]}`
/// for its own path and span, so the page itself names that read.
/// `key` is where the page holds its read: `next` while compacting, `hints`
/// once published.
fn implied_local_read(fields: &Map<String, Value>, key: &str, path: &Value) -> bool {
    let (Some(line), Some(end)) = (
        fields.get("line").and_then(Value::as_u64),
        fields.get("endLine").and_then(Value::as_u64),
    ) else {
        return false;
    };
    let Some(hints) = fields.get(key).and_then(Value::as_object) else {
        return false;
    };
    let Some(read) = hints.get("read").filter(|_| hints.len() == 1) else {
        return false;
    };
    // `confidence` and `why` are read metadata, not part of the call.
    if read.get("tool").and_then(Value::as_str) != Some(ToolId::LocalFetch.as_str())
        || read.as_object().is_none_or(|read| {
            read.keys()
                .any(|key| !matches!(key.as_str(), "tool" | "query" | "confidence" | "why"))
        })
    {
        return false;
    }
    let row = match read.pointer("/query/queries").and_then(Value::as_array) {
        Some(rows) if rows.len() == 1 => &rows[0],
        Some(_) => return false,
        None => &read["query"],
    };
    row.as_object().is_some_and(|row| row.len() == 2)
        && row.get("path") == Some(path)
        && row.get("ranges") == Some(&json!([format!("{line}-{end}")]))
}

/// The bare verdict: locate exists, P(yes), or a certain label/level.
pub(super) fn bare_verdict(answer: &Value) -> Value {
    if let Some(exists) = answer.get("exists") {
        return exists.clone();
    }
    if let Some(yes) = answer.get("yesno") {
        return yes.clone();
    }
    for key in ["choice", "score"] {
        if let Some(verdict) = answer.get(key) {
            let top = answer
                .get("probabilities")
                .and_then(Value::as_object)
                .into_iter()
                .flat_map(|probabilities| probabilities.values())
                .filter_map(Value::as_f64)
                .fold(0.0_f64, f64::max);
            return if top >= CERTAIN {
                verdict.clone()
            } else {
                answer.clone()
            };
        }
    }
    answer.clone()
}

/// A continued resource without the fields its matrix implies: the default
/// capture cap, and `fileChunks` on a search that `locate` reads.
fn omit_implied(resource: &mut Value, locate: bool, default_max_chars: u64) {
    let implied_chunks = locate
        && crate::tools::clasify::resource::tool_of(resource)
            .is_some_and(crate::tools::clasify::is_candidate_search_tool)
        && resource["candidateEvidence"] == "fileChunks";
    let Some(fields) = resource.as_object_mut() else {
        return;
    };
    if implied_chunks {
        fields.remove("candidateEvidence");
    }
    if fields.get("maxChars").and_then(Value::as_u64) == Some(default_max_chars) {
        fields.remove("maxChars");
    }
}

/// next.clasify in the public input shape (`{queries:[matrix]}`) with
/// compact carry rows.
fn unify_continuation(next: &mut Value, matrix: &Matrix<'_>, paths: &Map<String, Value>) {
    unify_matrix(next, matrix, paths);
    *next = json!({"queries":[next.take()]});
}

fn unify_matrix(next: &mut Value, matrix: &Matrix<'_>, paths: &Map<String, Value>) {
    let locate = !matrix.locate_ids.is_empty();
    let brief = [
        ("mainGoal", next.get("mainGoal").cloned()),
        ("reasoning", next.get("reasoning").cloned()),
    ];
    if let Some(resources) = next.get_mut("resources").and_then(Value::as_array_mut) {
        for resource in resources.iter_mut() {
            omit_implied(resource, locate, matrix.default_max_chars);
            // A read inherits the matrix brief on replay; repeating it is noise.
            if let Some(query) = resource.get_mut("query").and_then(Value::as_object_mut) {
                for (field, value) in &brief {
                    if value.is_some() && query.get(*field) == value.as_ref() {
                        query.remove(*field);
                    }
                }
            }
        }
    }
    if let Some(carry) = next.get_mut("carry").and_then(Value::as_object_mut) {
        for rows in carry.values_mut() {
            if let Some(rows) = rows.as_array_mut() {
                for row in rows {
                    *row = compact_row(row, matrix.single_resource, paths);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A whole-file page states `line`/`endLine`, and a read that is only its
    /// own path and span is implied; any other read stays.
    #[test]
    fn whole_file_pages_drop_their_implied_read() {
        let page = |read: Value| {
            json!({"answers":{"rel":0.9},"scope":{"startLine":1,"endLine":40,"totalLines":40},
                "source":{"path":"src/a.rs"},"hints":{"read":read}})
        };
        let mut implied = page(
            json!({"tool":"localFetch","query":{"queries":[{"ranges":["1-40"],"path":"src/a.rs"}]}}),
        );
        compact_page(&mut implied, None, false, &|_| false);
        assert_eq!(
            implied,
            json!({"line":1,"endLine":40,"answers":{"rel":0.9},"path":"src/a.rs"})
        );
        let mut pinned = page(json!({"tool":"localFetch","query":{"queries":[
            {"ranges":["1-40"],"path":"src/a.rs","snapshot":"abc"}]}}));
        compact_page(&mut pinned, None, false, &|_| false);
        assert!(pinned.pointer("/hints/read").is_some(), "{pinned}");
        let mut partial = json!({"answers":{"rel":0.9},"scope":{"startLine":5,"endLine":40,"totalLines":90},
            "source":{"path":"src/a.rs"},"hints":{"read":{"tool":"localFetch","query":{"queries":[{"ranges":["5-40"],"path":"src/a.rs"}]}}}});
        compact_page(&mut partial, None, false, &|_| false);
        assert!(
            partial.get("scope").is_some() && partial.get("hints").is_some(),
            "{partial}"
        );
    }

    fn read(start: u64, end: u64) -> Value {
        json!({"tool":"localFetch","query":{"path":"src/server.c","ranges":[format!("{start}-{end}")]},"confidence":"exact"})
    }

    fn verbose_locate() -> Value {
        let page = |start: u64, end: u64, exists: f64, window: (u64, u64, f64)| {
            json!({"source":{"path":"src/server.c"},
                "scope":{"startLine":start,"endLine":end,"totalLines":8615},
                "next":{"read":read(window.0, window.1)},
                "answers":{"t":{"exists":exists,"matches":[
                    {"lines":[window.0,window.1],"probability":window.2}]}}})
        };
        json!({
            "id":"matrix-1",
            "best":{"t":[
                {"resourceId":"f","exists":0.96,"startLine":1551,"endLine":1558,"probability":0.68,
                 "path":"src/server.c","next":{"read":read(1551,1558)}},
                {"resourceId":"f","exists":0.94,"startLine":1854,"endLine":1861,"probability":0.76,
                 "path":"src/server.c","next":{"read":read(1854,1861)}},
                {"resourceId":"f","exists":0.89,"startLine":879,"endLine":886,"probability":0.53,
                 "path":"src/server.c","next":{"read":read(879,886)}}
            ]},
            "resources":[{"id":"f","coverage":"partial","pages":[
                page(845, 1206, 0.89, (879, 886, 0.53)),
                page(1481, 1851, 0.96, (1551, 1558, 0.68))
            ]}],
            "next":{"clasify":{
                "id":"matrix-1","mainGoal":"g","reasoning":"r",
                "resources":[{"id":"f","tool":"localFetch","query":{"path":"src/server.c","ranges":["2608-8615"],"mainGoal":"g","reasoning":"own"},"prefilter":["cron"],"maxChars":80000}],
                "questions":[{"id":"t","type":"locate","ask":"timer"}],
                "carry":{"t":[{"resourceId":"f","exists":0.96,"startLine":1551,"endLine":1558,"probability":0.68,"path":"src/server.c"}]}
            }}
        })
    }

    #[test]
    fn locate_output_states_the_path_once_and_reads_only_the_top_window() {
        let mut output = verbose_locate();
        let before = output.to_string().len();
        compact_query_result(
            &mut output,
            &Matrix {
                single_resource: Some("f"),
                locate_ids: &["t"],
                default_max_chars: 80_000,
            },
        );
        assert_eq!(
            output,
            json!({
                "id":"matrix-1",
                "best":{"t":[
                    {"line":1551,"endLine":1558,"exists":0.96,"probability":0.68},
                    {"line":1854,"endLine":1861,"exists":0.94,"probability":0.76},
                    {"line":879,"endLine":886,"exists":0.89,"probability":0.53}
                ]},
                "resources":[{"id":"f","coverage":"partial","path":"src/server.c","totalLines":8615,"pages":[
                    {"line":845,"endLine":1206,"answers":{"t":0.89}},
                    {"line":1481,"endLine":1851,"answers":{"t":0.96}}
                ]}],
                "next":{
                    "clasify":{"queries":[{
                        "id":"matrix-1","mainGoal":"g","reasoning":"r",
                        "resources":[{"id":"f","tool":"localFetch","query":{"path":"src/server.c","ranges":["2608-8615"],"reasoning":"own"},"prefilter":["cron"]}],
                        "questions":[{"id":"t","type":"locate","ask":"timer"}],
                        "carry":{"t":[{"line":1551,"endLine":1558,"exists":0.96,"probability":0.68}]}
                    }]},
                    "read":read(1551,1558)
                }
            })
        );
        // The resource, the resume read, and the top-window read name the file.
        assert_eq!(output.to_string().matches("src/server.c").count(), 3);
        assert!(output.to_string().len() * 2 < before);
    }

    /// P5: pages of one GitHub file share one `source {ref, path}`: the
    /// resource states it once (`path`, `ref`) and the pages drop it. A page
    /// whose source differs keeps its own.
    #[test]
    fn a_shared_github_source_is_stated_once_on_the_resource() {
        let page = |line: u64, source: Value| {
            json!({"source":source,"scope":{"startLine":line,"endLine":line + 9,"totalLines":900},
                   "answers":{"a":{"yesno":0.4}}})
        };
        let source = json!({"ref":"v5.9.3","path":"microsoft/TypeScript/src/compiler/checker.ts"});
        let matrix = Matrix {
            single_resource: Some("f"),
            locate_ids: &[],
            default_max_chars: 80_000,
        };
        let mut output = json!({"id":"m","resources":[{"id":"f","coverage":"complete","pages":[
            page(1, source.clone()), page(11, source.clone()), page(21, source.clone())
        ]}]});
        compact_query_result(&mut output, &matrix);
        let resource = &output["resources"][0];
        assert_eq!(resource["path"], source["path"], "{output}");
        assert_eq!(resource["ref"], "v5.9.3", "{output}");
        assert_eq!(output.to_string().matches("v5.9.3").count(), 1, "{output}");
        assert!(
            resource["pages"]
                .as_array()
                .expect("pages")
                .iter()
                .all(|page| page.get("source").is_none()),
            "{output}"
        );

        let other = json!({"ref":"main","path":"microsoft/TypeScript/src/compiler/checker.ts"});
        let mut mixed = json!({"id":"m","resources":[{"id":"f","pages":[
            page(1, source.clone()), page(11, other.clone())
        ]}]});
        compact_query_result(&mut mixed, &matrix);
        let resource = &mixed["resources"][0];
        assert!(resource.get("ref").is_none(), "{mixed}");
        assert_eq!(resource["pages"][0]["source"], source, "{mixed}");
        assert_eq!(resource["pages"][1]["source"], other, "{mixed}");
    }

    /// `best` keeps the top windows only; a located page whose window it
    /// does not list keeps that window's read, so no located window drops
    /// out of the default output.
    #[test]
    fn a_located_window_outside_best_keeps_its_page_read() {
        let mut output = verbose_locate();
        output["best"]["t"].as_array_mut().expect("best").pop();
        compact_query_result(
            &mut output,
            &Matrix {
                single_resource: Some("f"),
                locate_ids: &["t"],
                default_max_chars: 80_000,
            },
        );
        let pages = &output["resources"][0]["pages"];
        assert_eq!(pages[0]["next"]["read"], read(879, 886), "{output}");
        assert!(pages[1].get("next").is_none(), "{output}");
    }

    #[test]
    fn rows_name_the_resource_only_when_the_matrix_has_several() {
        let paths = Map::from_iter([("f".to_owned(), json!("a.c"))]);
        let row = json!({"resourceId":"f","path":"a.c","exists":0.9,"startLine":3,"endLine":9,"probability":0.5});
        assert_eq!(
            compact_row(&row, Some("f"), &paths),
            json!({"line":3,"endLine":9,"exists":0.9,"probability":0.5})
        );
        let public = json!({"resourceId":"f","path":"a.c","line":3,"endLine":9,"exists":0.9,"probability":0.5});
        assert_eq!(compact_row(&row, None, &Map::new()), public);
        assert_eq!(
            ranking_row(
                &json!({"line":3,"endLine":9,"exists":0.9,"probability":0.5}),
                Some("f")
            ),
            Some(
                json!({"resourceId":"f","exists":0.9,"startLine":3,"endLine":9,"probability":0.5})
            )
        );
        assert_eq!(
            ranking_row(
                &json!({"line":3,"endLine":9,"exists":0.9,"probability":0.5}),
                None
            ),
            None
        );
        assert_eq!(ranking_row(&public, None), Some(row.clone()));
    }

    #[test]
    fn judge_answers_keep_distributions_only_when_uncertain() {
        let mut output = json!({"id":"m","resources":[
            {"id":"a","coverage":"complete","pages":[{"answers":{"q":{"choice":"runtime","confidence":1.0,
                "probabilities":{"test":0.0,"runtime":1.0}}}}]},
            {"id":"b","coverage":"complete","pages":[{"answers":{"q":{"choice":"docs","confidence":0.3,
                "probabilities":{"docs":0.6,"test":0.4}}}}]},
            {"id":"c","coverage":"complete","pages":[{"answers":{"q":{"yesno":0.12}}}]},
            {"id":"d","coverage":"error","pages":[{"error":{"errorCode":"x","error":"y"}}]}
        ]});
        compact_query_result(
            &mut output,
            &Matrix {
                single_resource: None,
                locate_ids: &[],
                default_max_chars: 80_000,
            },
        );
        assert_eq!(
            output["resources"],
            json!([
                {"id":"a","answers":{"q":"runtime"}},
                {"id":"b","answers":{"q":{"choice":"docs","confidence":0.3,"probabilities":{"docs":0.6,"test":0.4}}}},
                {"id":"c","answers":{"q":0.12}},
                {"id":"d","coverage":"error","error":{"errorCode":"x","error":"y"}}
            ])
        );
    }

    /// D3 (CL5 fc1.json: 64 copies, 12.2 KB of 38 KB): pages a spent budget
    /// left unjudged beside judged pages state their shared error once, on
    /// the resource, and keep their reads; another error stays on its page.
    #[test]
    fn a_repeated_unjudged_error_is_stated_once_beside_judged_pages() {
        let spent = json!({"errorCode":"classificationBudgetSpent","error":"not judged","hints":{"text":["Run hints.read."]}});
        let other = json!({"errorCode":"classificationContextEmpty","error":"empty"});
        let unjudged = |path: &str, ranges: Value| {
            json!({"source":{"path":path},"error":spent,
                "next":{"read":{"tool":"localFetch","query":{"path":path,"ranges":ranges}}}})
        };
        let mut output = json!({"id":"m","resources":[{"id":"s","coverage":"partial","pages":[
            {"source":{"path":"a.rs"},"answers":{"r":{"yesno":0.94}}},
            unjudged("a.rs", json!(["16-22","108-114"])),
            {"source":{"path":"b.rs"},"answers":{"r":{"yesno":0.1}}},
            unjudged("b.rs", json!(["40-46"])),
            {"source":{"path":"c.rs"},"error":other,
                "next":{"read":{"tool":"localFetch","query":{"path":"c.rs","ranges":["1-9"]}}}}
        ]}]});
        compact_query_result(
            &mut output,
            &Matrix {
                single_resource: Some("s"),
                locate_ids: &[],
                default_max_chars: 80_000,
            },
        );
        let resource = &output["resources"][0];
        assert_eq!(resource["error"], spent, "{resource}");
        assert_eq!(resource["coverage"], "partial");
        assert_eq!(
            output
                .to_string()
                .matches("classificationBudgetSpent")
                .count(),
            1
        );
        let pages = resource["pages"].as_array().expect("pages");
        assert_eq!(pages.len(), 5, "{resource}");
        for at in [1, 3] {
            assert!(pages[at].get("error").is_none(), "{}", pages[at]);
            assert!(pages[at].pointer("/next/read").is_some(), "{}", pages[at]);
        }
        assert_eq!(pages[4]["error"], other);
        assert_eq!(pages[0]["answers"]["r"], 0.94);
        // A repeated error on a page with no read stays on its pages.
        let mut readless = json!({"id":"m","resources":[{"id":"s","coverage":"partial","pages":[
            {"source":{"path":"a.rs"},"answers":{"r":{"yesno":0.94}}},
            {"source":{"path":"b.rs"},"error":spent},
            unjudged("c.rs", json!(["1-9"]))
        ]}]});
        compact_query_result(
            &mut readless,
            &Matrix {
                single_resource: Some("s"),
                locate_ids: &[],
                default_max_chars: 80_000,
            },
        );
        assert!(
            readless["resources"][0].get("error").is_none(),
            "{readless}"
        );
    }

    #[test]
    fn identical_cell_errors_collapse_to_one_error() {
        let error = json!({"errorCode":"x","error":"y","hints":{"text":["z"]}});
        let failed = json!({"error":error});
        let mut output = json!({"id":"m","resources":[
            {"id":"a","coverage":"error","pages":[{"answers":{"q":failed,"r":failed}}]},
            {"id":"b","coverage":"error","pages":[
                {"line":1,"endLine":9,"answers":{"q":failed,"r":failed}},
                {"line":10,"endLine":19,"answers":{"q":failed,"r":failed}}
            ]},
            {"id":"c","coverage":"partial","pages":[{"answers":{"q":{"yesno":0.5},"r":failed}}]},
            {"id":"d","coverage":"error","pages":[{"answers":{"q":failed,"r":{"error":{"errorCode":"x","error":"other"}}}}]}
        ]});
        compact_query_result(
            &mut output,
            &Matrix {
                single_resource: None,
                locate_ids: &[],
                default_max_chars: 80_000,
            },
        );
        assert_eq!(
            output["resources"],
            json!([
                {"id":"a","coverage":"error","error":error},
                {"id":"b","coverage":"error","error":error},
                {"id":"c","coverage":"partial","answers":{"q":0.5,"r":failed}},
                {"id":"d","coverage":"error","answers":{"q":failed,"r":{"error":{"errorCode":"x","error":"other"}}}}
            ])
        );
    }
}
