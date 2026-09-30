//! Search → clasify → read handoff. A wide file-search page for a semantic
//! search (a `goal` plus a plain multi-word phrase, never an exact literal,
//! identifier, path, quoted string, alternation, or regex) suggests one
//! `locate` matrix over its top files, so the host reads decisive windows
//! instead of whole files. File count alone never triggers it: a literal
//! search's hits are already the answer. Clasify stays the only semantic tool:
//! this only shapes its request. Availability is not decided here; the
//! cross-tool `next` filter drops the handoff when clasify is disabled.
use serde_json::{Map, Value, json};

/// Files a semantic search page must list before locating beats reading the
/// top hit directly. The one handoff threshold; core's clasify gate text names
/// the same number.
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

/// Whether a search term already names what it wants, so its hits are the
/// answer and a locate pass adds only cost. Only a plain phrase of two or more
/// words (letters, digits, `-`, `'`) with no `camelCase` token is semantic;
/// every single token (`retry`, `spawn_blocking`, `newElementWith`), quoted
/// string, `a|b` alternation, path (`src/x.rs`), member access, and regex is
/// literal.
fn literal_term(term: &str) -> bool {
    let term = term.trim();
    let camel = |word: &str| {
        word.chars()
            .zip(word.chars().skip(1))
            .any(|(a, b)| a.is_lowercase() && b.is_uppercase())
    };
    let plain_phrase = term.split_whitespace().nth(1).is_some()
        && term
            .chars()
            .all(|c| c.is_alphanumeric() || c.is_whitespace() || matches!(c, '-' | '\''))
        && !term.split_whitespace().any(camel);
    let quoted = term.len() > 1
        && ['"', '\'', '`']
            .iter()
            .any(|q| term.starts_with(*q) && term.ends_with(*q));
    !plain_phrase || quoted
}

/// Whether a search reads as described behavior rather than an exact literal.
/// `localSearch` judges its `searchText` (a `wholeWord` search is an exact
/// identifier); `ghSearchCode` judges its ANDed `keywords` as one phrase.
fn semantic_search(tool: &str, query: &Value) -> bool {
    if tool == "ghSearchCode" {
        let words: Vec<&str> = query
            .get("keywords")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        return !literal_term(&words.join(" "));
    }
    query.get("wholeWord") != Some(&Value::Bool(true))
        && query
            .get("searchText")
            .and_then(Value::as_str)
            .is_some_and(|text| !literal_term(text))
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
    if files.len() < WIDE_RESULT_FILES || !semantic_search(tool, query) {
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
            &[json!({"goal":"find retry","path":"/repo","searchText":"retry delay"})],
        );
        let next = handoff(&out).expect("wide semantic page hands off");
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
        assert_eq!(query["resources"][0]["prefilter"], json!(["retry delay"]));
        assert_eq!(query["questions"][0]["questionType"], "locate");
    }

    #[test]
    fn literal_term_separates_exact_literals_from_described_behavior() {
        for literal in [
            "retry",
            "newElementWith",
            "spawn_blocking",
            "$scope",
            "v2",
            "\"retry delay\"",
            "'retry delay'",
            "`retry delay`",
            "foo|bar",
            "retry | backoff",
            "src/runtime/mod.rs",
            "config.json",
            "self.retry",
            "fn (a|b)\\d+",
            "sample.?limit",
            "^use serde",
            "Result<T, E>",
            "std::fs read",
            "useState hook",
            "",
            "   ",
        ] {
            assert!(literal_term(literal), "literal: {literal:?}");
        }
        for semantic in [
            "retry delay",
            "how retries back off",
            "HTTP retry policy",
            "don't retry twice",
            "rate-limit handling",
        ] {
            assert!(!literal_term(semantic), "semantic: {semantic:?}");
        }
    }

    #[test]
    fn literal_searches_do_not_hand_off() {
        for query in [
            json!({"goal":"who uses it","searchText":"newElementWith","regex":"literal"}),
            json!({"goal":"who uses it","searchText":"spawn_blocking","resultView":"files"}),
            json!({"goal":"who uses it","searchText":"get object","wholeWord":true}),
            json!({"goal":"who uses it","searchText":"retry"}),
            json!({"goal":"who uses it","searchText":"\"retry delay\""}),
            json!({"goal":"who uses it","searchText":"retry|backoff"}),
            json!({"goal":"who uses it","searchText":"src/retry.rs"}),
            json!({"goal":"how is the limit enforced","searchText":"sample.?limit"}),
        ] {
            let mut wide = local_rows(12);
            run(&mut wide, "localSearch", std::slice::from_ref(&query));
            assert!(handoff(&wide).is_none(), "{query}");
        }
        let files: Vec<Value> = (0..12)
            .map(|n| json!({"owner":"o","repo":"r","path":format!("a{n}.rs")}))
            .collect();
        for keywords in [
            json!(["sync"]),
            json!(["useState"]),
            json!(["spawn_blocking", "tokio"]),
            json!(["\"retry delay\""]),
            json!(["retry|backoff"]),
            json!([]),
        ] {
            let mut wide = json!({"results":[{"index":0,"data":{"files":files.clone()}}]});
            run(
                &mut wide,
                "ghSearchCode",
                &[json!({"goal":"who uses it","owner":"o","keywords":keywords})],
            );
            assert!(handoff(&wide).is_none(), "{keywords}");
        }
    }

    #[test]
    fn semantic_search_needs_a_goal() {
        let mut wide = local_rows(12);
        run(
            &mut wide,
            "localSearch",
            &[json!({"searchText":"retry delay"})],
        );
        assert!(handoff(&wide).is_none());
    }

    #[test]
    fn emitted_handoffs_are_schema_valid_clasify_queries() {
        let mut local = local_rows(9);
        run(
            &mut local,
            "localSearch",
            &[json!({"goal":"how retries back off","searchText":"retry delay"})],
        );
        let files: Vec<Value> = (0..8)
            .map(|n| json!({"owner":"o","repo":"r","path":format!("a{n}.rs")}))
            .collect();
        let mut github = json!({"results":[{"index":0,"data":{"files":files}}]});
        run(
            &mut github,
            "ghSearchCode",
            &[json!({"goal":"how sync retries","owner":"o","keywords":["sync","retry"]})],
        );
        for output in [&local, &github] {
            let query = handoff(output).expect("handoff")["query"].clone();
            serde_json::from_value::<crate::contracts::tool_types::ClasifyQuery>(query.clone())
                .unwrap_or_else(|error| panic!("generated ClasifyQuery: {error}: {query}"));
            let prepared = crate::contracts::prepare_many_and_validate(
                "clasify",
                query.clone(),
                crate::contracts::PrepareOptions::default(),
            )
            .unwrap_or_else(|error| {
                panic!(
                    "clasify input contract: {:?}: {query}",
                    error.issues.first().map(|i| (&i.path, &i.message))
                )
            });
            assert_eq!(prepared.len(), 1);
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
    fn regex_prefilter_uses_the_strings_they_matched() {
        let files: Vec<Value> = (0..9)
            .map(|n| {
                json!({"path": format!("s{n}.go"), "matches": [
                    {"line": 1, "value": "\tif sampleLimit > 0 {"},
                    {"line": 2, "value": "return errSampleLimit // sample_limit"}
                ]})
            })
            .collect();
        let data = json!({"files":files});
        let terms = prefilter(
            "localSearch",
            &json!({"searchText":"sample.?limit","caseMode":"insensitive"}),
            data.as_object().expect("object"),
        );
        assert_eq!(terms, vec!["sampleLimit", "SampleLimit", "sample_limit"]);
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
        run(
            &mut narrow,
            "localSearch",
            &[json!({"goal":"g","searchText":"retry delay"})],
        );
        assert!(handoff(&narrow).is_none());
        let mut exact = local_rows(WIDE_RESULT_FILES);
        run(
            &mut exact,
            "localSearch",
            &[json!({"goal":"g","searchText":"retry delay"})],
        );
        assert!(handoff(&exact).is_some(), "the threshold is inclusive");
        for query in [
            json!({"goal":"g","searchText":"retry delay","resultView":"filesWithout"}),
            json!({"goal":"g","searchText":"retry delay","invertMatch":true}),
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
        run(
            &mut full,
            "ghSearchCode",
            &[json!({"goal":"g","keywords":["retry","backoff"]})],
        );
        let context = &handoff(&full).expect("full")["query"]["resources"][1]["context"];
        assert_eq!(context["tool"], "ghGetFileContent");
        assert_eq!(context["query"]["path"], "a1.rs");

        let strings: Vec<Value> = (0..8).map(|n| json!(format!("o/r:b{n}.rs"))).collect();
        let mut concise = json!({"results":[{"index":0,"data":{"files":strings}}]});
        run(
            &mut concise,
            "ghSearchCode",
            &[json!({"goal":"g","owner":"o","repo":"r","keywords":["retry backoff"]})],
        );
        assert_eq!(
            handoff(&concise).expect("concise")["query"]["resources"][0]["context"]["query"]["repo"],
            "r"
        );
    }

    #[test]
    fn other_tools_and_unresolvable_local_paths_are_skipped() {
        let mut out = local_rows(9);
        run(
            &mut out,
            "ghSearchRepo",
            &[json!({"goal":"g","keywords":["retry backoff"]})],
        );
        assert!(handoff(&out).is_none());
        let mut no_base = local_rows(9);
        no_base.as_object_mut().map(|map| map.remove("base"));
        run(
            &mut no_base,
            "localSearch",
            &[json!({"goal":"g","searchText":"retry delay"})],
        );
        assert!(handoff(&no_base).is_none());
    }

    #[test]
    fn rows_after_a_rejected_query_keep_their_own_query() {
        // Inputs: 0 ok, 1 rejected, 2 ok, 3 ok. Output rows keep those positions.
        let queries = [
            json!({"goal":"goal-0","searchText":"retry a"}),
            json!({"goal":"goal-2","searchText":"retry b"}),
            json!({"goal":"goal-3","searchText":"retry c"}),
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
            &[json!({"goal":"find retry","searchText":"retry delay"})],
        );
        let files: Vec<Value> = (0..8)
            .map(|n| json!({"owner":"o","repo":"r","path":format!("a{n}.rs")}))
            .collect();
        let mut github = json!({"results":[{"index":0,"data":{"files":files}}]});
        run(
            &mut github,
            "ghSearchCode",
            &[json!({"goal":"find sync","keywords":["sync","retry"]})],
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
