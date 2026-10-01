//! Search → clasify → read handoff. A wide file-search page for a semantic
//! search (a `goal` plus a plain multi-word phrase, never an exact literal,
//! identifier, path, quoted string, alternation, or regex) suggests one
//! relevance + sufficiency matrix over the search resource. Every candidate
//! remains reachable through clasify paging; the host reads relevant snippets
//! only when they are insufficient. File count alone never triggers it: a literal
//! search's hits are already the answer. A local content search is screened on
//! hydrated hit-cluster windows (`fileChunks`): a one-line hit of the searched
//! phrase carries only that phrase, so snippet judgments cannot rank the
//! candidates. Clasify stays the only semantic tool:
//! this only shapes its request. Availability is not decided here; the
//! cross-tool `next` filter drops the handoff when clasify is disabled.
use crate::tools::id::ToolId;
use serde_json::{Map, Value, json};

/// Files a semantic search page must list before screening beats reading the
/// top hit directly. The one handoff threshold; core's clasify gate text names
/// the same number.
const WIDE_RESULT_FILES: usize = 8;
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
    let search = crate::tools::clasify::is_candidate_search_tool(tool);
    if !search && !crate::tools::clasify::is_file_read_tool(tool) {
        return;
    }
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
        if !search {
            let offer = large_read_handoff(tool, query, data);
            if let Some(offer) = offer
                && data
                    .get("next")
                    .is_none_or(|next| next.get(ToolId::Clasify.as_str()).is_none())
                && let Some(next) = data
                    .entry("next")
                    .or_insert_with(|| json!({}))
                    .as_object_mut()
            {
                next.insert(ToolId::Clasify.as_str().into(), offer);
            }
            continue;
        }
        if let Some(request) = request(tool, query, data) {
            let next = data.entry("next").or_insert_with(|| json!({}));
            if let Some(next) = next.as_object_mut() {
                next.insert(
                    ToolId::Clasify.as_str().into(),
                    json!({"tool":ToolId::Clasify.as_str(),"confidence":"medium","query":request}),
                );
            }
        }
    }
}

/// Lines a file must have before a paged read without `matchString` offers a
/// clasify locate instead of the next page.
const LARGE_READ_LINES: u64 = 2_000;

/// `next.clasify` for one file-read row (`localFetch` / `ghGetFileContent`):
/// a read of a file of at least [`LARGE_READ_LINES`] lines that stopped
/// before its end (more pages, or a partial row) and has no `matchString`.
/// The read's goal becomes the locate target; the matrix uses the unified
/// input shape and reads the whole file. Reads that select a range, a match,
/// or a transformed view are already targeted and get nothing. The caller
/// (the file-read tool's `next` builder) inserts the returned continuation;
/// the cross-tool `next` filter drops it when clasify is unavailable.
fn large_read_handoff(tool: &str, query: &Value, data: &Map<String, Value>) -> Option<Value> {
    if !crate::tools::clasify::is_file_read_tool(tool) {
        return None;
    }
    if ["matchString", "startLine", "endLine"]
        .iter()
        .any(|key| query.get(*key).is_some())
        || query
            .get("minify")
            .and_then(Value::as_str)
            .is_some_and(|minify| minify != "none")
    {
        return None;
    }
    let total = data
        .get("totalLines")
        .or_else(|| {
            data.get("pagination")
                .and_then(|page| page.get("totalLines"))
        })
        .and_then(Value::as_u64)?;
    let paged = data.get("pagination").and_then(|page| page.get("hasMore"))
        == Some(&Value::Bool(true))
        || data.get("isPartial") == Some(&Value::Bool(true));
    if total < LARGE_READ_LINES || !paged {
        return None;
    }
    let goal_chars =
        crate::contracts::query_schema_number(ToolId::Clasify, None, "goal", "maxLength")
            .and_then(|length| usize::try_from(length).ok())
            .unwrap_or(usize::MAX);
    let goal: String = query
        .get("goal")?
        .as_str()?
        .chars()
        .take(goal_chars)
        .collect();
    if goal.trim().is_empty() {
        return None;
    }
    let mut read = Map::new();
    for key in ["path", "owner", "repo", "branch"] {
        if let Some(value) = query.get(key) {
            read.insert(key.into(), value.clone());
        }
    }
    read.get("path")?;
    Some(json!({
        "tool": ToolId::Clasify.as_str(),
        "confidence": "medium",
        "query": {
            "goal": goal,
            "reasoning": "Locate the deciding lines of this large file before reading more pages.",
            "resources": [{"id": "file", "tool": tool, "query": Value::Object(read)}],
            "questions": [{"id": "target", "type": "locate", "ask": goal}],
        },
    }))
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
    if tool == ToolId::GhSearchCode.as_str() {
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

fn request(tool: &str, query: &Value, data: &Map<String, Value>) -> Option<Value> {
    if query.get("invertMatch") == Some(&Value::Bool(true))
        || query
            .get("resultView")
            .and_then(Value::as_str)
            .is_some_and(|view| NON_HIT_VIEWS.contains(&view))
    {
        return None;
    }
    // The search brief becomes the matrix goal, cut to clasify's goal bound.
    let goal_chars =
        crate::contracts::query_schema_number(ToolId::Clasify, None, "goal", "maxLength")
            .and_then(|length| usize::try_from(length).ok())
            .unwrap_or(usize::MAX);
    let goal: String = query
        .get("goal")?
        .as_str()?
        .chars()
        .take(goal_chars)
        .collect();
    let files = data.get("files")?.as_array()?;
    if files.len() < WIDE_RESULT_FILES || !semantic_search(tool, query) {
        return None;
    }
    let reasoning = query
        .get("reasoning")
        .and_then(Value::as_str)
        .unwrap_or("Screen search candidates before reading.");
    let mut search = query.clone();
    search["reasoning"] = json!(reasoning);
    let metadata_only = query.get("resultView").and_then(Value::as_str) == Some("files")
        || query.get("concise") == Some(&Value::Bool(true))
        || query.get("match").and_then(Value::as_str) == Some("path");
    let relevance = if metadata_only {
        json!({
            "id":"relevant", "type":"noul",
            "instructions": format!(
                "Does this candidate's path or metadata suggest it likely contains evidence for: {goal}? This is a routing judgment; missing file bodies are not negative evidence."
            ),
        })
    } else {
        json!({"id":"relevant", "questionType":"contribution", "target":goal})
    };
    let mut context = json!({"tool": tool, "query": search});
    // Local reads are cheap and unmetered; GitHub hydration spends API budget
    // without changing the ranking (measured), so it screens snippets.
    if tool == ToolId::LocalSearch.as_str() && !metadata_only {
        context["candidateEvidence"] =
            json!(crate::tools::clasify::CandidateEvidence::FileChunks.to_string());
    }
    Some(json!({
        "goal": goal,
        "reasoning": reasoning,
        "resources": [{
            "id": "candidates",
            "context": context,
        }],
        "questions": [
            relevance,
            {"id": "sufficient", "questionType": "sufficient", "target": goal},
        ],
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_paged_read_of_a_large_file_without_match_string_offers_locate() {
        let query =
            json!({"path":"src/server.c","goal":"find the housekeeping timer","reasoning":"r"});
        let paged = json!({"totalLines":8615,"pagination":{"chunkType":"bytes","offset":0,"chunkSize":20000,"hasMore":true}});
        let offer =
            large_read_handoff("localFetch", &query, paged.as_object().unwrap()).expect("offer");
        assert_eq!(
            offer["query"],
            json!({
                "goal":"find the housekeeping timer",
                "reasoning":"Locate the deciding lines of this large file before reading more pages.",
                "resources":[{"id":"file","tool":"localFetch","query":{"path":"src/server.c"}}],
                "questions":[{"id":"target","type":"locate","ask":"find the housekeeping timer"}]
            })
        );
        let prepared = crate::contracts::prepare_many_and_validate(
            "clasify",
            offer["query"].clone(),
            crate::contracts::PrepareOptions::default(),
        )
        .expect("the offer validates");
        let mut nested = prepared[0].clone();
        crate::tools::clasify::aliases::canonicalize(&mut nested);
        assert_eq!(
            nested["resources"][0]["context"]["query"]["fullContent"],
            true
        );
        let gh = json!({"owner":"o","repo":"r","path":"a.go","branch":"main","goal":"g","reasoning":"r"});
        let partial = json!({"totalLines":2000,"isPartial":true});
        assert!(
            large_read_handoff("ghGetFileContent", &gh, partial.as_object().unwrap()).is_some()
        );
        // Small, complete, targeted, transformed, or non-file reads get nothing.
        for (tool, query, data) in [
            (
                "localFetch",
                query.clone(),
                json!({"totalLines":1999,"pagination":{"hasMore":true}}),
            ),
            (
                "localFetch",
                query.clone(),
                json!({"totalLines":8615,"pagination":{"hasMore":false}}),
            ),
            (
                "localFetch",
                json!({"path":"a","goal":"g","matchString":"cron"}),
                paged.clone(),
            ),
            (
                "localFetch",
                json!({"path":"a","goal":"g","startLine":1,"endLine":50}),
                paged.clone(),
            ),
            (
                "localFetch",
                json!({"path":"a","goal":"g","minify":"symbols"}),
                paged.clone(),
            ),
            ("localSearch", query.clone(), paged.clone()),
        ] {
            assert!(
                large_read_handoff(tool, &query, data.as_object().unwrap()).is_none(),
                "{query}"
            );
        }
    }

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
    fn wide_local_search_screens_all_candidates_without_hydrating_top_files() {
        let mut out = local_rows(9);
        let search = json!({
            "goal":"find retry", "reasoning":"Find production retry policy, including exceptions.",
            "path":"/repo", "searchText":"retry delay", "page":2,
            "pageSize":9, "include":["src/**"], "snapshot":"search-snapshot"
        });
        run(&mut out, "localSearch", std::slice::from_ref(&search));
        let next = handoff(&out).expect("wide semantic page hands off");
        assert_eq!(next["tool"], "clasify");
        let query = &next["query"];
        assert_eq!(query["resources"].as_array().map(Vec::len), Some(1));
        assert_eq!(query["resources"][0]["context"]["tool"], "localSearch");
        assert_eq!(query["resources"][0]["context"]["query"], search);
        assert_eq!(
            query["resources"][0]["context"]["candidateEvidence"], "fileChunks",
            "one-line snippets of the searched phrase cannot rank candidates"
        );
        assert_eq!(query["reasoning"], search["reasoning"]);
        assert_eq!(query["questions"][0]["questionType"], "contribution");
        assert_eq!(query["questions"][1]["questionType"], "sufficient");
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
    fn metadata_candidates_use_a_routing_question_instead_of_content_support() {
        for (tool, query) in [
            (
                "localSearch",
                json!({"goal":"find retry", "searchText":"retry delay", "resultView":"files"}),
            ),
            (
                "ghSearchCode",
                json!({"goal":"find retry", "keywords":["retry", "delay"], "concise":true}),
            ),
            (
                "ghSearchCode",
                json!({"goal":"find retry", "keywords":["retry", "delay"], "match":"path"}),
            ),
        ] {
            let mut out = local_rows(9);
            run(&mut out, tool, &[query]);
            let query = &handoff(&out).expect("metadata handoff")["query"];
            assert!(
                query["resources"][0]["context"]
                    .get("candidateEvidence")
                    .is_none(),
                "paths alone have no hit windows to hydrate"
            );
            let questions = &query["questions"];
            assert_eq!(questions[0]["type"], "noul");
            assert!(
                questions[0]["instructions"]
                    .as_str()
                    .expect("instructions")
                    .contains("routing judgment")
            );
            assert_eq!(questions[1]["questionType"], "sufficient");
        }
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
    fn github_search_handoff_preserves_scope_for_both_result_shapes() {
        let objects: Vec<Value> = (0..8)
            .map(|n| json!({"owner":"o","repo":"r","path":format!("a{n}.rs")}))
            .collect();
        let strings: Vec<Value> = (0..8).map(|n| json!(format!("o/r:b{n}.rs"))).collect();
        let search = json!({
            "goal":"find retry policy", "reasoning":"Screen candidates before reading.",
            "owner":"o", "repo":"r", "keywords":["retry", "backoff"]
        });
        for files in [objects, strings] {
            let mut out = json!({"results":[{"index":0,"data":{"files":files}}]});
            run(&mut out, "ghSearchCode", std::slice::from_ref(&search));
            let context = &handoff(&out).expect("handoff")["query"]["resources"][0]["context"];
            assert_eq!(context["tool"], "ghSearchCode");
            assert_eq!(context["query"], search);
            assert!(context.get("candidateEvidence").is_none());
        }
        let mut out = local_rows(9);
        run(&mut out, "ghSearchRepo", &[search]);
        assert!(handoff(&out).is_none());
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
