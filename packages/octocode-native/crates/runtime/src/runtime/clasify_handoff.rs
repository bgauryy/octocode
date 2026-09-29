//! Search → clasify → read handoff. A wide file-search page suggests one
//! `locate` matrix over its top files, so the host reads decisive windows
//! instead of whole files. Clasify stays the only semantic tool: this only
//! shapes its request. Availability is not decided here; the cross-tool `next`
//! filter drops the handoff when clasify is disabled.
use serde_json::{Map, Value, json};

/// Files a page must list before locating beats reading the top hit directly.
const WIDE_RESULT_FILES: usize = 8;
/// Top-ranked files sent to one locate matrix. Three whole files with two
/// questions stay inside the 25-cell budget once prefiltered.
const LOCATE_FILES: usize = 3;
/// Distinct matched strings a regex search passes on as prefilter literals.
const MAX_PREFILTER_TERMS: usize = 8;
/// Result views whose files are not evidence-bearing hits.
const NON_HIT_VIEWS: [&str; 4] = ["filesWithout", "countLines", "countMatches", "matchOnly"];

/// One entry per output row: the valid query that produced it, or `None` for a
/// row rejected at input. Output rows keep their original input position while
/// `queries` holds only the valid ones, so a plain index would misalign every
/// query after a rejection.
pub(super) fn row_queries<'a>(queries: &'a [Value], rejected: &[usize]) -> Vec<Option<&'a Value>> {
    let mut valid = queries.iter();
    (0..queries.len() + rejected.len())
        .map(|position| {
            if rejected.contains(&position) {
                None
            } else {
                valid.next()
            }
        })
        .collect()
}

pub(super) fn attach(structured: &mut Value, tool: &str, queries: &[Option<&Value>]) {
    if !matches!(tool, "ghSearchCode" | "localSearch") {
        return;
    }
    let base = structured
        .get("base")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let Some(rows) = structured.get_mut("results").and_then(Value::as_array_mut) else {
        return;
    };
    for (position, row) in rows.iter_mut().enumerate() {
        let index = row
            .get("index")
            .and_then(Value::as_u64)
            .and_then(|index| usize::try_from(index).ok())
            .unwrap_or(position);
        let Some(Some(query)) = queries.get(index) else {
            continue;
        };
        let Some(data) = row.get_mut("data").and_then(Value::as_object_mut) else {
            continue;
        };
        if let Some(request) = request(tool, query, data, base.as_deref()) {
            let next = data.entry("next").or_insert_with(|| json!({}));
            if let Some(next) = next.as_object_mut() {
                next.insert(
                    "clasify".into(),
                    json!({"tool":"clasify","confidence":"medium","query":request}),
                );
            }
        }
    }
}

/// Literals every hit contains, used to keep the densest windows of a large
/// file. A literal search passes its text; a regex search passes the distinct
/// strings it matched on this page (a pattern has no single literal).
fn prefilter(tool: &str, query: &Value, data: &Map<String, Value>) -> Vec<String> {
    let plain =
        |text: &str| !text.trim().is_empty() && !text.contains(|c| r"\.^$*+?()[]{}|".contains(c));
    if tool == "ghSearchCode" {
        return query
            .get("keywords")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter(|word| !word.trim().is_empty())
            .take(MAX_PREFILTER_TERMS)
            .map(str::to_owned)
            .collect();
    }
    let Some(text) = query.get("searchText").and_then(Value::as_str) else {
        return Vec::new();
    };
    let mode = query.get("regex").and_then(Value::as_str);
    if mode == Some("literal") || plain(text) {
        return vec![text.to_owned()];
    }
    if mode == Some("pcre2") {
        return Vec::new();
    }
    let insensitive = match query.get("caseMode").and_then(Value::as_str) {
        Some("insensitive") => true,
        Some("sensitive") => false,
        _ => !text.chars().any(char::is_uppercase),
    };
    let Ok(pattern) = regex::RegexBuilder::new(text)
        .case_insensitive(insensitive)
        .build()
    else {
        return Vec::new();
    };
    let mut terms = Vec::<String>::new();
    let values = data
        .get("files")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|file| file.get("matches")?.as_array())
        .flatten()
        .filter_map(|matched| matched.get("value")?.as_str());
    for value in values {
        for found in pattern.find_iter(value) {
            let term = found.as_str().trim();
            if !term.is_empty() && !terms.iter().any(|seen| seen == term) {
                terms.push(term.to_owned());
                if terms.len() == MAX_PREFILTER_TERMS {
                    return terms;
                }
            }
        }
    }
    terms
}

/// A search for one exact identifier (a whole word, or a single token that
/// only code would spell: `snake_case`, `camelCase`, `$x`, `v2`) already names
/// what it wants; its hits are the answer, so a locate pass adds only cost.
fn exact_identifier(query: &Value) -> bool {
    if query.get("wholeWord") == Some(&Value::Bool(true)) {
        return true;
    }
    let Some(text) = query.get("searchText").and_then(Value::as_str) else {
        return false;
    };
    let mut chars = text.chars();
    let identifier = chars
        .next()
        .is_some_and(|first| first.is_alphabetic() || matches!(first, '_' | '$'))
        && chars.all(|c| c.is_alphanumeric() || matches!(c, '_' | '$'));
    identifier
        && (text.contains(['_', '$'])
            || text.chars().any(|c| c.is_ascii_digit())
            || text.chars().skip(1).any(char::is_uppercase))
}

fn request(
    tool: &str,
    query: &Value,
    data: &Map<String, Value>,
    base: Option<&str>,
) -> Option<Value> {
    if query.get("invertMatch") == Some(&Value::Bool(true))
        || query
            .get("resultView")
            .and_then(Value::as_str)
            .is_some_and(|view| NON_HIT_VIEWS.contains(&view))
    {
        return None;
    }
    let goal: String = query.get("goal")?.as_str()?.chars().take(500).collect();
    let files = data.get("files")?.as_array()?;
    if files.len() < WIDE_RESULT_FILES || (tool == "localSearch" && exact_identifier(query)) {
        return None;
    }
    let literals = prefilter(tool, query, data);
    let mut seen = Vec::new();
    let resources: Vec<Value> = files
        .iter()
        .filter_map(|file| context_query(tool, query, file, base))
        .filter(|(key, _)| {
            let fresh = !seen.contains(key);
            seen.push(key.clone());
            fresh
        })
        .take(LOCATE_FILES)
        .enumerate()
        .map(|(n, (_, context))| {
            let mut resource = json!({"id": format!("f{n}"), "context": context});
            if !literals.is_empty() {
                resource["prefilter"] = json!(literals);
            }
            resource
        })
        .collect();
    if resources.is_empty() {
        return None;
    }
    Some(json!({
        "goal": goal,
        "reasoning": format!("{} files matched; locate before reading.", files.len()),
        "resources": resources,
        "questions": [{
            "id": "answer",
            "questionType": "locate",
            "target": format!("The lines that answer: {goal}"),
        }],
    }))
}

/// `(identity, context)` for one hit, or `None` when it has no readable path.
fn context_query(
    tool: &str,
    query: &Value,
    file: &Value,
    base: Option<&str>,
) -> Option<(String, Value)> {
    if tool == "ghSearchCode" {
        let (owner, repo, path) = match file {
            Value::String(row) => {
                let (slug, path) = row.split_once(':')?;
                let (owner, repo) = slug.split_once('/')?;
                (owner.to_owned(), repo.to_owned(), path.to_owned())
            }
            Value::Object(row) => (
                field(row, "owner").or_else(|| field_of(query, "owner"))?,
                field(row, "repo").or_else(|| field_of(query, "repo"))?,
                field(row, "path")?,
            ),
            _ => return None,
        };
        let key = format!("{owner}/{repo}:{path}");
        return Some((
            key,
            json!({"tool":"ghGetFileContent","query":{
                "owner":owner,"repo":repo,"path":path,"fullContent":true
            }}),
        ));
    }
    let relative = match file {
        Value::String(path) => path.as_str(),
        Value::Object(row) => row.get("path")?.as_str()?,
        _ => return None,
    };
    let path = if relative.starts_with('/') {
        relative.to_owned()
    } else {
        format!("{}/{relative}", base?.trim_end_matches('/'))
    };
    Some((
        path.clone(),
        json!({"tool":"localFetch","query":{"path":path,"fullContent":true}}),
    ))
}

fn field(row: &Map<String, Value>, key: &str) -> Option<String> {
    row.get(key)?.as_str().map(str::to_owned)
}

fn field_of(query: &Value, key: &str) -> Option<String> {
    query.get(key)?.as_str().map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(structured: &mut Value, tool: &str, queries: &[Value]) {
        let by_row = row_queries(queries, &[]);
        attach(structured, tool, &by_row);
    }

    fn local_rows(count: usize) -> Value {
        let files: Vec<Value> = (0..count)
            .map(|n| json!({"path": format!("src/f{n}.rs")}))
            .collect();
        json!({"base":"/repo","results":[{"index":0,"data":{"files":files}}]})
    }

    fn handoff(structured: &Value) -> Option<&Value> {
        structured["results"][0]["data"]["next"].get("clasify")
    }

    #[test]
    fn wide_local_search_hands_off_top_files_as_one_locate_matrix() {
        let mut out = local_rows(9);
        run(
            &mut out,
            "localSearch",
            &[json!({"goal":"find retry","path":"/repo","searchText":"retry"})],
        );
        let next = handoff(&out).expect("wide page hands off");
        assert_eq!(next["tool"], "clasify");
        let query = &next["query"];
        assert_eq!(
            query["resources"].as_array().map(Vec::len),
            Some(LOCATE_FILES)
        );
        assert_eq!(query["resources"][0]["context"]["tool"], "localFetch");
        assert_eq!(
            query["resources"][0]["context"]["query"]["path"],
            "/repo/src/f0.rs"
        );
        assert_eq!(query["resources"][0]["prefilter"], json!(["retry"]));
        assert_eq!(query["questions"][0]["questionType"], "locate");
    }

    #[test]
    fn exact_identifier_searches_do_not_hand_off() {
        for query in [
            json!({"goal":"who uses it","searchText":"newElementWith","regex":"literal"}),
            json!({"goal":"who uses it","searchText":"spawn_blocking","resultView":"files"}),
            json!({"goal":"who uses it","searchText":"get object","wholeWord":true}),
        ] {
            let mut wide = local_rows(12);
            run(&mut wide, "localSearch", std::slice::from_ref(&query));
            assert!(handoff(&wide).is_none(), "{query}");
        }
    }

    #[test]
    fn handoff_resources_carry_only_the_read() {
        let mut out = local_rows(9);
        run(
            &mut out,
            "localSearch",
            &[json!({"goal":"how retries back off","searchText":"retry delay"})],
        );
        let query = &handoff(&out).expect("semantic wide page")["query"];
        let context = &query["resources"][0]["context"]["query"];
        assert!(context.get("goal").is_none(), "{context}");
        assert!(context.get("reasoning").is_none(), "{context}");
        assert_eq!(context["fullContent"], true);
    }

    #[test]
    fn regex_searches_prefilter_on_the_strings_they_matched() {
        let files: Vec<Value> = (0..9)
            .map(|n| {
                json!({"path": format!("s{n}.go"), "matches": [
                    {"line": 1, "value": "\tif sampleLimit > 0 {"},
                    {"line": 2, "value": "return errSampleLimit // sample_limit"}
                ]})
            })
            .collect();
        let mut out = json!({"base":"/repo","results":[{"index":0,"data":{"files":files}}]});
        run(
            &mut out,
            "localSearch",
            &[
                json!({"goal":"how is the limit enforced","searchText":"sample.?limit","caseMode":"insensitive"}),
            ],
        );
        let query = &handoff(&out).expect("regex page")["query"];
        assert_eq!(
            query["resources"][0]["prefilter"],
            json!(["sampleLimit", "SampleLimit", "sample_limit"])
        );
    }

    #[test]
    fn prefilter_uses_only_literals_every_hit_contains() {
        let empty = Map::new();
        let regex = prefilter("localSearch", &json!({"searchText":"fn (a|b)\\d+"}), &empty);
        assert!(regex.is_empty());
        let forced_literal = prefilter(
            "localSearch",
            &json!({"searchText":"a.b(c)","regex":"literal"}),
            &empty,
        );
        assert_eq!(forced_literal, vec!["a.b(c)".to_owned()]);
        let words = prefilter(
            "ghSearchCode",
            &json!({"keywords":["sync"," ","hash"]}),
            &empty,
        );
        assert_eq!(words, vec!["sync".to_owned(), "hash".to_owned()]);
    }

    #[test]
    fn narrow_pages_and_non_hit_views_stay_lean() {
        let mut narrow = local_rows(WIDE_RESULT_FILES - 1);
        run(&mut narrow, "localSearch", &[json!({"goal":"g"})]);
        assert!(handoff(&narrow).is_none());
        for query in [
            json!({"goal":"g","resultView":"filesWithout"}),
            json!({"goal":"g","invertMatch":true}),
        ] {
            let mut wide = local_rows(12);
            run(&mut wide, "localSearch", &[query]);
            assert!(handoff(&wide).is_none());
        }
    }

    #[test]
    fn github_hits_in_both_shapes_become_ghgetfilecontent_resources() {
        let objects: Vec<Value> = (0..8)
            .map(|n| json!({"owner":"o","repo":"r","path":format!("a{n}.rs")}))
            .collect();
        let mut full = json!({"results":[{"index":0,"data":{"files":objects}}]});
        run(&mut full, "ghSearchCode", &[json!({"goal":"g"})]);
        let context = &handoff(&full).expect("full")["query"]["resources"][1]["context"];
        assert_eq!(context["tool"], "ghGetFileContent");
        assert_eq!(context["query"]["path"], "a1.rs");

        let strings: Vec<Value> = (0..8).map(|n| json!(format!("o/r:b{n}.rs"))).collect();
        let mut concise = json!({"results":[{"index":0,"data":{"files":strings}}]});
        run(
            &mut concise,
            "ghSearchCode",
            &[json!({"goal":"g","owner":"o","repo":"r"})],
        );
        assert_eq!(
            handoff(&concise).expect("concise")["query"]["resources"][0]["context"]["query"]["repo"],
            "r"
        );
    }

    #[test]
    fn other_tools_and_unresolvable_local_paths_are_skipped() {
        let mut out = local_rows(9);
        run(&mut out, "ghSearchRepo", &[json!({"goal":"g"})]);
        assert!(handoff(&out).is_none());
        let mut no_base = local_rows(9);
        no_base.as_object_mut().map(|map| map.remove("base"));
        run(&mut no_base, "localSearch", &[json!({"goal":"g"})]);
        assert!(handoff(&no_base).is_none());
    }

    #[test]
    fn rows_after_a_rejected_query_keep_their_own_query() {
        // Inputs: 0 ok, 1 rejected, 2 ok, 3 ok. Output rows keep those positions.
        let queries = [
            json!({"goal":"goal-0","searchText":"a"}),
            json!({"goal":"goal-2","searchText":"b"}),
            json!({"goal":"goal-3","searchText":"c"}),
        ];
        let mut out = json!({"base":"/repo","results":[
            {"index":0,"data":{"files": local_rows(9)["results"][0]["data"]["files"].clone()}},
            {"index":1,"status":"error","data":{}},
            {"index":2,"data":{"files": local_rows(9)["results"][0]["data"]["files"].clone()}},
            {"index":3,"data":{"files": local_rows(9)["results"][0]["data"]["files"].clone()}},
        ]});
        let by_row = row_queries(&queries, &[1]);
        assert_eq!(by_row.len(), 4);
        assert!(by_row[1].is_none());
        attach(&mut out, "localSearch", &by_row);
        let goal =
            |row: usize| out["results"][row]["data"]["next"]["clasify"]["query"]["goal"].clone();
        assert_eq!(goal(0), "goal-0");
        assert!(out["results"][1]["data"].get("next").is_none());
        assert_eq!(goal(2), "goal-2");
        assert_eq!(goal(3), "goal-3");
    }

    #[test]
    fn generated_handoffs_satisfy_the_tool_output_contract() {
        let mut local = local_rows(9);
        run(
            &mut local,
            "localSearch",
            &[json!({"goal":"find retry","searchText":"retry"})],
        );
        let files: Vec<Value> = (0..8)
            .map(|n| json!({"owner":"o","repo":"r","path":format!("a{n}.rs")}))
            .collect();
        let mut github = json!({"results":[{"index":0,"data":{"files":files}}]});
        run(
            &mut github,
            "ghSearchCode",
            &[json!({"goal":"find sync","keywords":["sync"]})],
        );
        for (tool, output) in [("localSearch", &local), ("ghSearchCode", &github)] {
            assert!(handoff(output).is_some(), "{tool} produced a handoff");
            if let Err(violation) = crate::contracts::validate_output(tool, output) {
                panic!(
                    "{tool}: {:?}",
                    violation.issues.first().map(|i| (&i.path, &i.message))
                );
            }
        }
    }
}
