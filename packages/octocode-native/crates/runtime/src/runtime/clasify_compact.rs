//! Default clasify output: the verbose receipt (`debug:true`) reduced to the
//! decision data. A resource states its file path and line total once, pages
//! become `lines` + answers keyed by question, answers become the bare
//! label / P(yes) / level / exists unless the distribution is uncertain, and
//! `best` rows become `{r?, path?, lines, exists, p}` with one exact read of
//! the top window in `next.read`. Runner-up windows stay under `debug`.
use crate::tools::clasify::aliases;
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
pub(super) fn compact_query(output: &mut Value, matrix: &Matrix<'_>) {
    let mut paths = Map::new();
    if let Some(resources) = output.get_mut("resources").and_then(Value::as_array_mut) {
        for resource in resources.iter_mut() {
            compact_resource(resource);
            if let (Some(id), Some(path)) = (
                resource.get("resourceId").and_then(Value::as_str),
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

fn escape(token: &str) -> String {
    token.replace('~', "~0").replace('/', "~1")
}

/// `{resourceId, path?, exists, startLine, endLine, probability, next?}` →
/// `{r?, path?, lines, exists, p}`; the resource id and a path the resource
/// already states are omitted.
pub(super) fn compact_row(row: &Value, single: Option<&str>, paths: &Map<String, Value>) -> Value {
    let resource = row.get("resourceId").and_then(Value::as_str);
    let mut out = Map::new();
    if resource.is_some() && resource != single {
        out.insert("r".into(), json!(resource));
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
    out.insert("p".into(), row["probability"].clone());
    Value::Object(out)
}

/// A compact carry/best row back to the ranking row `rank_locate` merges.
/// `None` when it names no resource and the matrix has several.
pub(super) fn ranking_row(row: &Value, single: Option<&str>) -> Option<Value> {
    if row.get("lines").is_none() {
        return Some(row.clone());
    }
    let resource = row.get("r").and_then(Value::as_str).or(single)?.to_owned();
    let lines = row["lines"].as_array()?;
    let mut out = json!({
        "resourceId": resource,
        "exists": row.get("exists")?,
        "startLine": lines.first()?,
        "endLine": lines.get(1)?,
        "probability": row.get("p")?,
    });
    if let Some(path) = row.get("path") {
        out["path"] = path.clone();
    }
    Some(out)
}

fn compact_resource(resource: &mut Value) {
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
        compact_page(page, shared_path.is_some(), shared_total.is_some());
    }
    let single_bare = match pages.as_slice() {
        [page] => page
            .as_object()
            .filter(|page| {
                page.keys()
                    .all(|key| matches!(key.as_str(), "answers" | "error"))
                    && page.len() == 1
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

/// The value every page yields, when every page yields the same one.
fn shared(pages: &[Value], value: impl Fn(&Value) -> Option<Value>) -> Option<Value> {
    let mut values = pages.iter().map(value);
    let first = values.next()??;
    values
        .all(|value| value.as_ref() == Some(&first))
        .then_some(first)
}

fn compact_page(page: &mut Value, path_hoisted: bool, total_hoisted: bool) {
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
        *answer = compact_answer(answer);
    }
    // best and next.read carry the located windows.
    if located {
        fields.remove("next");
    }
}

/// The bare verdict: locate exists, P(yes), or a certain label/level.
pub(super) fn compact_answer(answer: &Value) -> Value {
    if let Some(exists) = answer.get("exists") {
        return exists.clone();
    }
    if let Some(noul) = answer.get("noul") {
        return noul.clone();
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

/// next.clasify in the unified input shape with compact carry rows.
fn unify_continuation(next: &mut Value, matrix: &Matrix<'_>, paths: &Map<String, Value>) {
    let locate = !matrix.locate_ids.is_empty();
    let brief = [
        ("goal", next.get("goal").cloned()),
        ("reasoning", next.get("reasoning").cloned()),
    ];
    if let Some(resources) = next.get_mut("resources").and_then(Value::as_array_mut) {
        for resource in resources.iter_mut() {
            *resource = aliases::unified_resource(resource, locate, matrix.default_max_chars);
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
    if let Some(questions) = next.get_mut("questions").and_then(Value::as_array_mut) {
        for question in questions.iter_mut() {
            *question = aliases::unified_question(question);
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
        json!({"tool":"localFetch","query":{"path":"src/server.c","startLine":start,"endLine":end},"confidence":"exact"})
    }

    fn verbose_locate() -> Value {
        let page = |start: u64, end: u64, exists: f64, window: (u64, u64, f64)| {
            json!({"source":{"path":"src/server.c"},
                "scope":{"startLine":start,"endLine":end,"totalLines":8615},
                "next":{"read":read(window.0, window.1)},
                "answers":{"t":{"exists":exists,"matches":[
                    {"startLine":window.0,"endLine":window.1,"probability":window.2}]}}})
        };
        json!({
            "queryId":"matrix-1",
            "best":{"t":[
                {"resourceId":"f","exists":0.96,"startLine":1551,"endLine":1558,"probability":0.68,
                 "path":"src/server.c","next":{"read":read(1551,1558)}},
                {"resourceId":"f","exists":0.94,"startLine":1854,"endLine":1861,"probability":0.76,
                 "path":"src/server.c","next":{"read":read(1854,1861)}}
            ]},
            "resources":[{"resourceId":"f","coverage":"partial","pages":[
                page(845, 1206, 0.89, (879, 886, 0.53)),
                page(1481, 1851, 0.96, (1551, 1558, 0.68))
            ]}],
            "next":{"clasify":{
                "id":"matrix-1","goal":"g","reasoning":"r",
                "resources":[{"id":"f","context":{"tool":"localFetch","query":{"path":"src/server.c","startLine":2608,"endLine":8615,"goal":"g","reasoning":"own"}},"prefilter":["cron"],"maxChars":80000}],
                "questions":[{"id":"t","questionType":"locate","target":"timer"}],
                "carry":{"t":[{"resourceId":"f","exists":0.96,"startLine":1551,"endLine":1558,"probability":0.68,"path":"src/server.c"}]}
            }}
        })
    }

    #[test]
    fn locate_output_states_the_path_once_and_reads_only_the_top_window() {
        let mut output = verbose_locate();
        let before = output.to_string().len();
        compact_query(
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
                "queryId":"matrix-1",
                "best":{"t":[
                    {"lines":[1551,1558],"exists":0.96,"p":0.68},
                    {"lines":[1854,1861],"exists":0.94,"p":0.76}
                ]},
                "resources":[{"resourceId":"f","coverage":"partial","path":"src/server.c","totalLines":8615,"pages":[
                    {"lines":[845,1206],"answers":{"t":0.89}},
                    {"lines":[1481,1851],"answers":{"t":0.96}}
                ]}],
                "next":{
                    "clasify":{
                        "id":"matrix-1","goal":"g","reasoning":"r",
                        "resources":[{"id":"f","tool":"localFetch","query":{"path":"src/server.c","startLine":2608,"endLine":8615,"reasoning":"own"},"prefilter":["cron"]}],
                        "questions":[{"id":"t","type":"locate","ask":"timer"}],
                        "carry":{"t":[{"lines":[1551,1558],"exists":0.96,"p":0.68}]}
                    },
                    "read":read(1551,1558)
                }
            })
        );
        // The resource, the resume read, and the top-window read name the file.
        assert_eq!(output.to_string().matches("src/server.c").count(), 3);
        assert!(output.to_string().len() * 2 < before);
    }

    #[test]
    fn rows_name_the_resource_only_when_the_matrix_has_several() {
        let paths = Map::from_iter([("f".to_owned(), json!("a.c"))]);
        let row = json!({"resourceId":"f","path":"a.c","exists":0.9,"startLine":3,"endLine":9,"probability":0.5});
        assert_eq!(
            compact_row(&row, Some("f"), &paths),
            json!({"lines":[3,9],"exists":0.9,"p":0.5})
        );
        assert_eq!(
            compact_row(&row, None, &Map::new()),
            json!({"r":"f","path":"a.c","lines":[3,9],"exists":0.9,"p":0.5})
        );
        assert_eq!(
            ranking_row(&json!({"lines":[3,9],"exists":0.9,"p":0.5}), Some("f")),
            Some(
                json!({"resourceId":"f","exists":0.9,"startLine":3,"endLine":9,"probability":0.5})
            )
        );
        assert_eq!(
            ranking_row(&json!({"lines":[3,9],"exists":0.9,"p":0.5}), None),
            None
        );
        assert_eq!(ranking_row(&row, None), Some(row.clone()));
    }

    #[test]
    fn judge_answers_keep_distributions_only_when_uncertain() {
        let mut output = json!({"queryId":"m","resources":[
            {"resourceId":"a","coverage":"complete","pages":[{"answers":{"q":{"choice":"runtime","confidence":1.0,
                "probabilities":{"test":0.0,"runtime":1.0}}}}]},
            {"resourceId":"b","coverage":"complete","pages":[{"answers":{"q":{"choice":"docs","confidence":0.3,
                "probabilities":{"docs":0.6,"test":0.4}}}}]},
            {"resourceId":"c","coverage":"complete","pages":[{"answers":{"q":{"noul":0.12}}}]},
            {"resourceId":"d","coverage":"error","pages":[{"error":{"code":"x","message":"y"}}]}
        ]});
        compact_query(
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
                {"resourceId":"a","answers":{"q":"runtime"}},
                {"resourceId":"b","answers":{"q":{"choice":"docs","confidence":0.3,"probabilities":{"docs":0.6,"test":0.4}}}},
                {"resourceId":"c","answers":{"q":0.12}},
                {"resourceId":"d","coverage":"error","error":{"code":"x","message":"y"}}
            ])
        );
    }
}
