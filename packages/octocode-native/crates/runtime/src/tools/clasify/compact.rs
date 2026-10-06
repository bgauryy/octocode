//! Default clasify output: the verbose receipt (`debug:true`) reduced to the
//! decision data. A resource states its file path and line total once, pages
//! become `lines` + answers keyed by question, answers become the bare
//! label / P(yes) / level / exists unless the distribution is uncertain, and
//! `best` rows become `{resourceId?, path?, lines, exists, probability}` with
//! one exact read of the top window in `next.read`. Runner-up windows stay
//! under `debug`.
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
/// `{resourceId?, path?, lines, exists, probability}`; the only resource's
/// id and a path the resource already states are omitted.
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
    out.insert(
        "lines".into(),
        json!([row["startLine"].clone(), row["endLine"].clone()]),
    );
    out.insert("exists".into(), row["exists"].clone());
    out.insert("probability".into(), row["probability"].clone());
    Value::Object(out)
}

/// A verbose `best` row in its published shape: `startLine`/`endLine` become
/// `lines`; the resource id stays.
fn verbose_row(row: &Value) -> Value {
    let Some(fields) = row.as_object() else {
        return row.clone();
    };
    let mut out = Map::new();
    for (key, value) in fields {
        match key.as_str() {
            "startLine" => {
                out.insert("lines".into(), json!([value, row["endLine"]]));
            }
            "endLine" => {}
            _ => {
                out.insert(key.clone(), value.clone());
            }
        }
    }
    Value::Object(out)
}

/// Publish one `debug:true` query result in place: verbose `best` rows with
/// `lines`, and next.clasify in the public input shape.
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
    let lines = row["lines"].as_array()?;
    let mut out = json!({
        "resourceId": resource,
        "exists": row.get("exists")?,
        "startLine": lines.first()?,
        "endLine": lines.get(1)?,
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
    let shared_path = shared(pages, |page| {
        page.get("source")
            .and_then(Value::as_object)
            .filter(|source| source.len() == 1)
            .and_then(|source| source.get("path"))
            .cloned()
    });
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
            shared_path.is_some(),
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
    // (other errors, such as a spent page budget, stay on their pages). A
    // page left with only its line span is not listed; a page keeps its read,
    // the host's fallback for an unjudged window.
    if let Some(error) =
        shared(pages, |page| page.get("error").cloned()).or_else(|| unjudged_repeated_error(pages))
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
                .is_some_and(|page| page.keys().any(|key| key != "lines"))
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
    path_hoisted: bool,
    total_hoisted: bool,
    window_listed: &dyn Fn(&Value) -> bool,
) {
    let Some(fields) = page.as_object_mut() else {
        return;
    };
    // A source that names only its file is that file's path.
    if let Some(path) = fields
        .get("source")
        .and_then(Value::as_object)
        .filter(|source| source.len() == 1)
        .and_then(|source| source.get("path"))
        .cloned()
    {
        fields.remove("source");
        if !path_hoisted {
            fields.insert("path".into(), path);
        }
    }
    if total_hoisted
        && let Some(scope) = fields.get("scope").and_then(Value::as_object)
        && let (Some(start), Some(end)) = (scope.get("startLine"), scope.get("endLine"))
        && scope.len() == 3
    {
        let lines = json!([start, end]);
        fields.remove("scope");
        fields.insert("lines".into(), lines);
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
                    {"lines":[1551,1558],"exists":0.96,"probability":0.68},
                    {"lines":[1854,1861],"exists":0.94,"probability":0.76},
                    {"lines":[879,886],"exists":0.89,"probability":0.53}
                ]},
                "resources":[{"id":"f","coverage":"partial","path":"src/server.c","totalLines":8615,"pages":[
                    {"lines":[845,1206],"answers":{"t":0.89}},
                    {"lines":[1481,1851],"answers":{"t":0.96}}
                ]}],
                "next":{
                    "clasify":{"queries":[{
                        "id":"matrix-1","mainGoal":"g","reasoning":"r",
                        "resources":[{"id":"f","tool":"localFetch","query":{"path":"src/server.c","ranges":["2608-8615"],"reasoning":"own"},"prefilter":["cron"]}],
                        "questions":[{"id":"t","type":"locate","ask":"timer"}],
                        "carry":{"t":[{"lines":[1551,1558],"exists":0.96,"probability":0.68}]}
                    }]},
                    "read":read(1551,1558)
                }
            })
        );
        // The resource, the resume read, and the top-window read name the file.
        assert_eq!(output.to_string().matches("src/server.c").count(), 3);
        assert!(output.to_string().len() * 2 < before);
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
            json!({"lines":[3,9],"exists":0.9,"probability":0.5})
        );
        let public =
            json!({"resourceId":"f","path":"a.c","lines":[3,9],"exists":0.9,"probability":0.5});
        assert_eq!(compact_row(&row, None, &Map::new()), public);
        assert_eq!(
            ranking_row(
                &json!({"lines":[3,9],"exists":0.9,"probability":0.5}),
                Some("f")
            ),
            Some(
                json!({"resourceId":"f","exists":0.9,"startLine":3,"endLine":9,"probability":0.5})
            )
        );
        assert_eq!(
            ranking_row(&json!({"lines":[3,9],"exists":0.9,"probability":0.5}), None),
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

    #[test]
    fn identical_cell_errors_collapse_to_one_error() {
        let error = json!({"errorCode":"x","error":"y","hints":{"text":["z"]}});
        let failed = json!({"error":error});
        let mut output = json!({"id":"m","resources":[
            {"id":"a","coverage":"error","pages":[{"answers":{"q":failed,"r":failed}}]},
            {"id":"b","coverage":"error","pages":[
                {"lines":[1,9],"answers":{"q":failed,"r":failed}},
                {"lines":[10,19],"answers":{"q":failed,"r":failed}}
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
