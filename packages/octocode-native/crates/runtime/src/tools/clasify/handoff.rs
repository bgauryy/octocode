//! Search → clasify → read handoff. A wide file-search page for a semantic
//! search (a plain multi-word phrase, never an exact literal, identifier,
//! path, quoted string, alternation, or regex) suggests one relevance +
//! sufficiency matrix over the search resource, asking the caller's
//! `mainGoal` when sent, else the phrase. Every candidate
//! remains reachable through clasify paging; the host reads relevant
//! candidates through their `next.read`, and a sufficient one ends screening
//! (its read still verifies the deciding lines). File count alone never triggers it: a literal
//! search's hits are already the answer. A local content search is screened on
//! hydrated hit-cluster windows (`fileChunks`): a one-line hit of the searched
//! phrase carries only that phrase, so snippet judgments cannot rank the
//! candidates. Clasify stays the only semantic tool:
//! this only shapes its request. Availability is not decided here; the
//! cross-tool `next` filter drops the handoff when clasify is disabled.
use crate::tools::id::ToolId;
use crate::tools::id::query_limits::clasify::MAIN_GOAL_MAX_LENGTH;
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
pub(crate) fn row_queries<'a>(queries: &'a [Value], rejected: &[usize]) -> Vec<Option<&'a Value>> {
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

pub(crate) fn attach(structured: &mut Value, tool: &str, queries: &[Option<&Value>]) {
    let Some(id) = ToolId::from_name(tool) else {
        return;
    };
    let search = crate::tools::clasify::is_candidate_search_tool(id);
    if !search && !crate::tools::clasify::is_file_read_tool(id) {
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
            if let Some((name, lead)) = large_read_lead(tool, query, data)
                && data.get("next").is_none_or(|next| next.get(name).is_none())
                && let Some(next) = data
                    .entry("next")
                    .or_insert_with(|| json!({}))
                    .as_object_mut()
            {
                next.insert(name.into(), lead);
            }
            continue;
        }
        if let Some(request) = request(tool, query, data) {
            let next = data.entry("next").or_insert_with(|| json!({}));
            if let Some(next) = next.as_object_mut() {
                next.insert(
                    ToolId::Clasify.as_str().into(),
                    crate::tools::result::Continuation::new(ToolId::Clasify, request)
                        .confidence("medium")
                        .build(),
                );
            }
        }
    }
}

/// Lines a file must have before a paged read without `matchString` offers a
/// clasify locate instead of the next page (the read returned only its head).
const LARGE_READ_LINES: u64 = crate::tools::local_fetch::LARGE_READ_LINES as u64;

/// The lead for one file-read row (`localFetch` / `ghGetFileContent`): a
/// read of a file of at least [`LARGE_READ_LINES`] lines that stopped before
/// its end (more pages, or a partial row) and has no `matchString`, whose
/// `mainGoal` asks where something is. A goal naming an identifier is a
/// literal lookup: a local read gets a `textSearch` lead (`localSearch`, exact and cheaper
/// than locate), a remote read nothing. Any other locate goal gets
/// `clasify`, which reads the whole file with the goal as its target. Reads
/// that select a range, a match, or a transformed view are already targeted
/// and get nothing. The cross-tool filter drops the lead when its tool is
/// unavailable.
fn large_read_lead(
    tool: &str,
    query: &Value,
    data: &Map<String, Value>,
) -> Option<(&'static str, Value)> {
    if !ToolId::from_name(tool).is_some_and(crate::tools::clasify::is_file_read_tool) {
        return None;
    }
    if ["matchString", "ranges", "block"]
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
    let goal: String = query
        .get("mainGoal")?
        .as_str()?
        .chars()
        .take(MAIN_GOAL_MAX_LENGTH)
        .collect();
    let path = query.get("path")?.as_str()?;
    let literal_lead = |identifier: &str| {
        (tool == ToolId::LocalFetch.as_str())
            .then(|| crate::tools::clasify::locate::literal_file_search(path, identifier))
            .flatten()
            .map(|search| ("textSearch", search))
    };
    if let Some(identifier) = crate::tools::clasify::locate::bare_identifier(&goal) {
        return literal_lead(identifier);
    }
    if goal.trim().is_empty() || generic_read_goal(&goal) || !locate_goal(&goal) {
        return None;
    }
    if let Some(identifier) = crate::tools::clasify::locate::literal_target(&goal) {
        return literal_lead(identifier);
    }
    let mut read = Map::new();
    for key in ["path", "owner", "repo", "ref"] {
        if let Some(value) = query.get(key) {
            read.insert(key.into(), value.clone());
        }
    }
    let resource = json!({"id": "file", "tool": tool, "query": Value::Object(read)});
    // The handoff carries only the brief the caller sent.
    let mut matrix = json!({
        "mainGoal": goal,
        "resources": [resource],
        "questions": [{"id": "target", "type": "locate", "ask": goal}],
    });
    if let Some(reasoning) = query.get("reasoning").filter(|value| value.is_string()) {
        matrix["reasoning"] = reasoning.clone();
    }
    Some((
        ToolId::Clasify.as_str(),
        crate::tools::result::Continuation::new(ToolId::Clasify, matrix)
            .confidence("medium")
            .build(),
    ))
}

/// Words that ask where something is.
const LOCATE_WORDS: &[&str] = &[
    "where", "which", "find", "finds", "locate", "locates", "identify", "pinpoint",
];
/// Openers of a question about the file's behavior.
const QUESTION_OPENERS: &[&str] = &[
    "how", "why", "when", "what", "who", "whether", "does", "do", "is", "are", "can",
];

/// Whether a read goal asks where something is ("where the timer fires",
/// "find the retry floor", "How does X avoid Y?"), not to understand,
/// review, or check the file: a locate over a non-locate goal has no target.
fn locate_goal(goal: &str) -> bool {
    let words: Vec<String> = goal
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect();
    goal.trim_end().ends_with('?')
        || words
            .first()
            .is_some_and(|word| QUESTION_OPENERS.contains(&word.as_str()))
        || words
            .iter()
            .any(|word| LOCATE_WORDS.contains(&word.as_str()))
}

/// Words that describe reading itself, not what to find in the file.
const GENERIC_READ_WORDS: &[&str] = &[
    "a",
    "about",
    "all",
    "an",
    "and",
    "any",
    "are",
    "as",
    "at",
    "body",
    "browse",
    "by",
    "check",
    "code",
    "content",
    "contents",
    "context",
    "detail",
    "details",
    "do",
    "does",
    "entire",
    "explore",
    "file",
    "files",
    "first",
    "flow",
    "flows",
    "for",
    "from",
    "full",
    "get",
    "here",
    "how",
    "i",
    "important",
    "in",
    "inspect",
    "into",
    "is",
    "it",
    "its",
    "key",
    "learn",
    "line",
    "lines",
    "logic",
    "look",
    "lookup",
    "main",
    "me",
    "more",
    "next",
    "of",
    "on",
    "or",
    "overview",
    "page",
    "pages",
    "part",
    "parts",
    "read",
    "relevant",
    "remaining",
    "rest",
    "review",
    "scan",
    "section",
    "sections",
    "see",
    "skim",
    "source",
    "structure",
    "subset",
    "summary",
    "the",
    "these",
    "this",
    "those",
    "to",
    "top",
    "understand",
    "understanding",
    "we",
    "what",
    "where",
    "which",
    "whole",
];

/// Whether a read goal names nothing to locate ("read key sections",
/// "understand this code"): a locate over it judges every window against the
/// act of reading and only adds provider calls.
fn generic_read_goal(goal: &str) -> bool {
    goal.split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|word| !word.is_empty())
        .all(|word| GENERIC_READ_WORDS.contains(&word.to_lowercase().as_str()))
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

/// The searched phrase when it reads as described behavior rather than an
/// exact literal. `localSearch` judges its `matchString` (a `wholeWord`
/// search is an exact identifier); `ghSearchCode` judges its ANDed
/// `keywords` as one phrase.
fn semantic_phrase(tool: &str, query: &Value) -> Option<String> {
    let phrase = if tool == ToolId::GhSearchCode.as_str() {
        query
            .get("keywords")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(" ")
    } else if query.get("wholeWord") == Some(&Value::Bool(true)) {
        return None;
    } else {
        query.get("matchString")?.as_str()?.to_owned()
    };
    (!literal_term(&phrase)).then(|| phrase.trim().to_owned())
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
    let files = data.get("files")?.as_array()?;
    if files.len() < WIDE_RESULT_FILES {
        return None;
    }
    let phrase = semantic_phrase(tool, query)?;
    // The question target is the search brief when the caller sent one,
    // else the searched phrase; cut to clasify's goal bound.
    let brief = query.get("mainGoal").and_then(Value::as_str);
    let goal: String = brief
        .unwrap_or(&phrase)
        .chars()
        .take(MAIN_GOAL_MAX_LENGTH)
        .collect();
    let search = query.clone();
    let metadata_only = query.get("resultView").and_then(Value::as_str) == Some("files")
        || query.get("concise") == Some(&Value::Bool(true))
        || query.get("match").and_then(Value::as_str) == Some("path");
    let relevance = if metadata_only {
        json!({
            "id":"relevant", "type":"yesno",
            "ask": format!(
                "Does this candidate's path or metadata suggest it likely contains evidence for: {goal}? This is a routing judgment; missing file bodies are not negative evidence."
            ),
        })
    } else {
        json!({"id":"relevant", "type":"relevant", "ask":goal})
    };
    let mut resource = json!({"id": "candidates", "tool": tool, "query": search});
    // Local reads are cheap and unmetered; GitHub hydration spends API budget
    // without changing the ranking, so it screens snippets.
    if tool == ToolId::LocalSearch.as_str() && !metadata_only {
        resource["candidateEvidence"] =
            json!(crate::tools::clasify::CandidateEvidence::FileChunks.to_string());
    }
    // The handoff carries only the brief the caller sent.
    let mut matrix = json!({
        "resources": [resource],
        "questions": [
            relevance,
            {"id": "sufficient", "type": "sufficient", "ask": goal},
        ],
    });
    if brief.is_some() {
        matrix["mainGoal"] = json!(goal);
    }
    if let Some(reasoning) = query.get("reasoning").filter(|value| value.is_string()) {
        matrix["reasoning"] = reasoning.clone();
    }
    Some(matrix)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The clasify offer of a large read, if that is the lead it gets.
    fn locate_offer(tool: &str, query: &Value, data: &Value) -> Option<Value> {
        large_read_lead(tool, query, data.as_object().expect("data"))
            .filter(|(name, _)| *name == ToolId::Clasify.as_str())
            .map(|(_, offer)| offer)
    }

    #[test]
    fn a_paged_read_of_a_large_file_without_match_string_offers_locate() {
        let query =
            json!({"path":"src/server.c","mainGoal":"find the housekeeping timer","reasoning":"r"});
        let paged = json!({"totalLines":8615,"pagination":{"unit":"bytes","offset":0,"length":20000,"hasMore":true}});
        let offer = locate_offer("localFetch", &query, &paged).expect("offer");
        assert_eq!(
            offer["query"]["queries"][0],
            json!({
                "mainGoal":"find the housekeeping timer",
                "reasoning":"r",
                "resources":[{"id":"file","tool":"localFetch","query":{"path":"src/server.c"}}],
                "questions":[{"id":"target","type":"locate","ask":"find the housekeeping timer"}]
            })
        );
        // The handoff carries only the brief the caller sent; no mainGoal,
        // no handoff.
        let goal_only = json!({"path":"src/server.c","mainGoal":"find the housekeeping timer"});
        let offer_without_reasoning =
            locate_offer("localFetch", &goal_only, &paged).expect("offer");
        assert!(
            offer_without_reasoning["query"]["queries"][0]
                .get("reasoning")
                .is_none()
        );
        let bare = json!({"path":"src/server.c","reasoning":"r"});
        assert!(large_read_lead("localFetch", &bare, paged.as_object().unwrap()).is_none());
        let prepared = crate::contracts::prepare_many_and_validate(
            "clasify",
            offer["query"].clone(),
            crate::contracts::PrepareOptions::default(),
        )
        .expect("the offer validates");
        let mut normalized = prepared;
        crate::tools::clasify::normalize_rows(&mut normalized);
        assert_eq!(normalized[0]["resources"][0]["query"]["fullContent"], true);
        let gh = json!({"owner":"o","repo":"r","path":"a.go","ref":"main",
            "mainGoal":"where the cache is flushed","reasoning":"r"});
        let partial = json!({"totalLines":2000,"isPartial":true});
        assert!(locate_offer("ghGetFileContent", &gh, &partial).is_some());
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
                json!({"path":"a","mainGoal":"find g","matchString":"cron"}),
                paged.clone(),
            ),
            (
                "localFetch",
                json!({"path":"a","mainGoal":"find g","ranges":["1-50","900-950"]}),
                paged.clone(),
            ),
            (
                "localFetch",
                json!({"path":"a","mainGoal":"find g","minify":"symbols"}),
                paged.clone(),
            ),
            ("localSearch", query.clone(), paged.clone()),
        ] {
            assert!(
                large_read_lead(tool, &query, data.as_object().unwrap()).is_none(),
                "{query}"
            );
        }
    }

    /// A goal that names an identifier is a literal lookup: localSearch finds
    /// it exactly, so the read offers that search instead of a locate. A
    /// remote read has no literal-search lead and gets nothing.
    #[test]
    fn an_identifier_goal_offers_a_literal_search_instead_of_locate() {
        let paged = json!({"totalLines":3137,"pagination":{"hasMore":true}});
        for (goal, literal) in [
            ("find bulk_update", "bulk_update"),
            ("Where bulk_update refuses pk changes", "bulk_update"),
            (
                "why does worker_threads reject 0 in Builder::new or maxThreads",
                "worker_threads",
            ),
            ("MAX_CALL_CAPTURES", "MAX_CALL_CAPTURES"),
            ("where is `newElementWith()` defined", "newElementWith"),
        ] {
            let query = json!({"path":"django/db/models/query.py","mainGoal":goal,"reasoning":"r"});
            let (name, lead) =
                large_read_lead("localFetch", &query, paged.as_object().unwrap()).expect(goal);
            assert_eq!(name, "textSearch", "{goal}");
            assert_eq!(lead["tool"], "localSearch", "{goal}");
            assert_eq!(
                lead["query"]["queries"][0]["path"], "django/db/models/query.py",
                "{lead}"
            );
            assert_eq!(
                lead["query"]["queries"][0]["matchString"], literal,
                "{lead}"
            );
            crate::contracts::prepare_many_and_validate(
                "localSearch",
                lead["query"].clone(),
                crate::contracts::PrepareOptions::default(),
            )
            .expect("the literal search validates");
            let remote = json!({"owner":"o","repo":"r","path":"query.py","mainGoal":goal});
            assert!(
                large_read_lead(
                    "ghGetFileContent",
                    &remote,
                    json!({"totalLines":3137,"isPartial":true})
                        .as_object()
                        .unwrap()
                )
                .is_none(),
                "{goal}"
            );
        }
    }

    #[test]
    fn a_goal_that_locates_nothing_gets_no_large_read_lead() {
        let paged = json!({"totalLines":8615,"pagination":{"hasMore":true}});
        for goal in [
            "read key sections",
            "Understand the flow",
            "read the rest of this file",
            "lookup: subset of the main sections",
            "understand this code",
            "Review file",
            "octocode-local-testing regression check",
            "Understand QuerySet",
            "understand the eviction policy",
            "read key sections on retries",
            "g",
        ] {
            let query = json!({"path":"src/server.c","mainGoal":goal,"reasoning":"r"});
            assert!(
                large_read_lead("localFetch", &query, paged.as_object().unwrap()).is_none(),
                "{goal}"
            );
        }
        // A goal that asks where something is gets a locate.
        for goal in [
            "where the eviction policy runs",
            "find the retry backoff",
            "Locate where objects are batched when saving many rows",
            "How does the scheduler avoid starving remote tasks?",
            "Which branch rejects an empty header",
        ] {
            let query = json!({"path":"src/server.c","mainGoal":goal,"reasoning":"r"});
            assert!(
                locate_offer("localFetch", &query, &paged).is_some(),
                "{goal}"
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
        json!({"root":"/repo","results":[{"index":0,"data":{"files":files}}]})
    }

    fn handoff(structured: &Value) -> Option<&Value> {
        structured["results"][0]["data"]["next"].get("clasify")
    }

    #[test]
    fn wide_local_search_screens_all_candidates_without_hydrating_top_files() {
        let mut out = local_rows(9);
        let search = json!({
            "mainGoal":"find retry", "reasoning":"Find production retry policy, including exceptions.",
            "path":"/repo", "matchString":"retry delay", "page":2,
            "pageSize":9, "include":["src/**"], "snapshot":"search-snapshot"
        });
        run(&mut out, "localSearch", std::slice::from_ref(&search));
        let next = handoff(&out).expect("wide semantic page hands off");
        assert_eq!(next["tool"], "clasify");
        let query = &next["query"]["queries"][0];
        assert_eq!(query["resources"].as_array().map(Vec::len), Some(1));
        assert_eq!(query["resources"][0]["tool"], "localSearch");
        assert_eq!(query["resources"][0]["query"], search);
        assert_eq!(
            query["resources"][0]["candidateEvidence"], "fileChunks",
            "one-line snippets of the searched phrase cannot rank candidates"
        );
        assert_eq!(query["reasoning"], search["reasoning"]);
        assert_eq!(
            query["questions"],
            json!([
                {"id":"relevant","type":"relevant","ask":"find retry"},
                {"id":"sufficient","type":"sufficient","ask":"find retry"}
            ])
        );
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
            json!({"mainGoal":"who uses it","matchString":"newElementWith","regex":"literal"}),
            json!({"mainGoal":"who uses it","matchString":"spawn_blocking","resultView":"files"}),
            json!({"mainGoal":"who uses it","matchString":"get object","wholeWord":true}),
            json!({"mainGoal":"who uses it","matchString":"retry"}),
            json!({"mainGoal":"who uses it","matchString":"\"retry delay\""}),
            json!({"mainGoal":"who uses it","matchString":"retry|backoff"}),
            json!({"mainGoal":"who uses it","matchString":"src/retry.rs"}),
            json!({"mainGoal":"how is the limit enforced","matchString":"sample.?limit"}),
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
                &[json!({"mainGoal":"who uses it","owner":"o","keywords":keywords})],
            );
            assert!(handoff(&wide).is_none(), "{keywords}");
        }
    }

    /// Callers rarely send a brief, so the query shape alone gates the
    /// handoff: the searched phrase becomes the ask, and the matrix carries
    /// no brief the caller did not send.
    #[test]
    fn a_semantic_search_without_a_goal_asks_its_phrase() {
        let mut local = local_rows(12);
        run(
            &mut local,
            "localSearch",
            &[json!({"matchString":"retry delay"})],
        );
        let files: Vec<Value> = (0..8).map(|n| json!({"path":format!("a{n}.rs")})).collect();
        let mut github = json!({"results":[{"index":0,"data":{"files":files}}]});
        run(
            &mut github,
            "ghSearchCode",
            &[json!({"owner":"o","repo":"r","keywords":["retry","backoff"]})],
        );
        for (output, ask) in [(&local, "retry delay"), (&github, "retry backoff")] {
            let query = &handoff(output).expect("query-shape handoff")["query"]["queries"][0];
            assert!(query.get("mainGoal").is_none(), "{query}");
            assert!(query.get("reasoning").is_none(), "{query}");
            assert_eq!(query["questions"][0]["ask"], ask);
            assert_eq!(query["questions"][1]["ask"], ask);
            crate::contracts::prepare_many_and_validate(
                "clasify",
                json!({"queries":[query.clone()]}),
                crate::contracts::PrepareOptions::default(),
            )
            .unwrap_or_else(|error| panic!("{:?}: {query}", error.issues.first()));
        }
    }

    #[test]
    fn metadata_candidates_use_a_routing_question_instead_of_content_support() {
        for (tool, query) in [
            (
                "localSearch",
                json!({"mainGoal":"find retry", "matchString":"retry delay", "resultView":"files"}),
            ),
            (
                "ghSearchCode",
                json!({"mainGoal":"find retry", "keywords":["retry", "delay"], "concise":true}),
            ),
            (
                "ghSearchCode",
                json!({"mainGoal":"find retry", "keywords":["retry", "delay"], "match":"path"}),
            ),
        ] {
            let mut out = local_rows(9);
            run(&mut out, tool, &[query]);
            let query = &handoff(&out).expect("metadata handoff")["query"]["queries"][0];
            assert!(
                query["resources"][0].get("candidateEvidence").is_none(),
                "paths alone have no hit windows to hydrate"
            );
            let questions = &query["questions"];
            assert_eq!(questions[0]["type"], "yesno");
            assert!(
                questions[0]["ask"]
                    .as_str()
                    .expect("ask")
                    .contains("routing judgment")
            );
            assert_eq!(questions[1]["type"], "sufficient");
        }
    }

    /// A wide localSearch output for "retry delay" and an 8-file ghSearchCode
    /// output, each run through the handoff with its query.
    fn retry_outputs(local_goal: &str, github_query: Value) -> (Value, Value) {
        let mut local = local_rows(9);
        run(
            &mut local,
            "localSearch",
            &[json!({"mainGoal":local_goal,"matchString":"retry delay"})],
        );
        let files: Vec<Value> = (0..8)
            .map(|n| json!({"owner":"o","repo":"r","path":format!("a{n}.rs")}))
            .collect();
        let mut github = json!({"results":[{"index":0,"data":{"files":files}}]});
        run(&mut github, "ghSearchCode", &[github_query]);
        (local, github)
    }

    #[test]
    fn emitted_handoffs_are_schema_valid_clasify_queries() {
        let (local, github) = retry_outputs(
            "how retries back off",
            json!({"mainGoal":"how sync retries","owner":"o","keywords":["sync","retry"]}),
        );
        for output in [&local, &github] {
            let query = handoff(output).expect("handoff")["query"]["queries"][0].clone();
            serde_json::from_value::<crate::contracts::tool_types::ClasifyQuery>(query.clone())
                .unwrap_or_else(|error| panic!("generated ClasifyQuery: {error}: {query}"));
            let prepared = crate::contracts::prepare_many_and_validate(
                "clasify",
                json!({"queries":[query.clone()]}),
                crate::contracts::PrepareOptions::default(),
            )
            .unwrap_or_else(|error| {
                panic!(
                    "clasify input contract: {:?}: {query}",
                    error.issues.first().map(|i| (&i.path, &i.message))
                )
            });
            assert_eq!(prepared.len(), 1);
            // Continuations publish only the unified input shape.
            let text = query.to_string();
            for nested in [
                "\"context\"",
                "\"questionType\"",
                "\"target\"",
                "\"instructions\"",
            ] {
                assert!(!text.contains(nested), "{nested} in {query}");
            }
        }
    }

    #[test]
    fn narrow_pages_and_non_hit_views_stay_lean() {
        let mut narrow = local_rows(WIDE_RESULT_FILES - 1);
        run(
            &mut narrow,
            "localSearch",
            &[json!({"mainGoal":"g","matchString":"retry delay"})],
        );
        assert!(handoff(&narrow).is_none());
        let mut exact = local_rows(WIDE_RESULT_FILES);
        run(
            &mut exact,
            "localSearch",
            &[json!({"mainGoal":"g","matchString":"retry delay"})],
        );
        assert!(handoff(&exact).is_some(), "the threshold is inclusive");
        for query in [
            json!({"mainGoal":"g","matchString":"retry delay","resultView":"filesWithout"}),
            json!({"mainGoal":"g","matchString":"retry delay","invertMatch":true}),
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
            "mainGoal":"find retry policy", "reasoning":"Screen candidates before reading.",
            "owner":"o", "repo":"r", "keywords":["retry", "backoff"]
        });
        for files in [objects, strings] {
            let mut out = json!({"results":[{"index":0,"data":{"files":files}}]});
            run(&mut out, "ghSearchCode", std::slice::from_ref(&search));
            let resource = &handoff(&out).expect("handoff")["query"]["queries"][0]["resources"][0];
            assert_eq!(resource["tool"], "ghSearchCode");
            assert_eq!(resource["query"], search);
            assert!(resource.get("candidateEvidence").is_none());
        }
        let mut out = local_rows(9);
        run(&mut out, "ghSearchRepo", &[search]);
        assert!(handoff(&out).is_none());
    }

    #[test]
    fn rows_after_a_rejected_query_keep_their_own_query() {
        // Inputs: 0 ok, 1 rejected, 2 ok, 3 ok. Output rows keep those positions.
        let queries = [
            json!({"mainGoal":"goal-0","matchString":"retry a"}),
            json!({"mainGoal":"goal-2","matchString":"retry b"}),
            json!({"mainGoal":"goal-3","matchString":"retry c"}),
        ];
        let mut out = json!({"root":"/repo","results":[
            {"index":0,"data":{"files": local_rows(9)["results"][0]["data"]["files"].clone()}},
            {"index":1,"status":"error","data":{}},
            {"index":2,"data":{"files": local_rows(9)["results"][0]["data"]["files"].clone()}},
            {"index":3,"data":{"files": local_rows(9)["results"][0]["data"]["files"].clone()}},
        ]});
        let by_row = row_queries(&queries, &[1]);
        assert_eq!(by_row.len(), 4);
        assert!(by_row[1].is_none());
        attach(&mut out, "localSearch", &by_row);
        let goal = |row: usize| {
            out["results"][row]["data"]["next"]["clasify"]["query"]["queries"][0]["mainGoal"]
                .clone()
        };
        assert_eq!(goal(0), "goal-0");
        assert!(out["results"][1]["data"].get("next").is_none());
        assert_eq!(goal(2), "goal-2");
        assert_eq!(goal(3), "goal-3");
    }

    #[test]
    fn generated_handoffs_satisfy_the_tool_output_contract() {
        let (local, github) = retry_outputs(
            "find retry",
            json!({"mainGoal":"find sync","keywords":["sync","retry"]}),
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
