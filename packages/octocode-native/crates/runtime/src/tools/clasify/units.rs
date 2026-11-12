//! Many small things judged one by one in a single row: the rows of a tool's
//! page (`source: {tool, query}` over a search, listing or history tool), or
//! the items of a `content` array (`perItem`). Each unit is asked every
//! question in its own provider request, so an answer is about that unit
//! alone; the row holds the answers keyed by the unit's identity, never the
//! unit's text.
//!
//! A tool's page is run through the path a direct call takes
//! ([`source_capture::read_rows`]): its contract, the input gate, its
//! executor, its own paging and output sanitization. One page of the tool is
//! one page here: `coverage.totalUnits` counts it, and `next.nextPage` is the
//! same row over the tool's next page.
//!
//! What a unit is, for any tool: each element of a top-level array of `data`
//! is a row; a row that lists child rows ([`CHILD_ROWS`]) is their context
//! and each child is the unit (a file's hits, a pull request's comments, a
//! directory's entries); a row without them is the unit itself (a commit, a
//! repository, a package).
use super::{Row, RowOutcome, reply};
use crate::execution::ExecutionContext;
use crate::id::ToolId;
use crate::providers::classification::gate::{GateLease, call_cap};
use crate::tools::judgment::{
    ClassificationError, EvaluationInput, Evaluator, Judged, LongContext, shared::Evaluations,
};
use crate::tools::source_capture::{self, SourceReader};
use futures_util::{StreamExt as _, stream};
use octocode_contract_types::{ClasifyChannel, ErrorCode};
use serde_json::{Map, Value, json};

/// The fields of a row that list its child rows: hits and symbols of a
/// file, lines of a remote file, entries of a directory, changed files and
/// discussion entries of a pull request, issue or commit.
const CHILD_ROWS: [&str; 7] = [
    "matches",
    "symbols",
    "lines",
    "files",
    "comments",
    "reviews",
    "reviewComments",
];

/// The fields that name a row, most specific first.
const NAMES: [&str; 7] = [
    "number",
    "id",
    "path",
    "commitSha",
    "packageName",
    "symbolName",
    "name",
];

/// Below this P(yes) a `noul` answer is a no, as a quiet section's is.
const QUIET_BELOW: f64 = 0.3;

/// One unit: its key in the row, and what the judge sees of it (`None`:
/// withheld, nothing is sent).
struct Unit {
    key: String,
    state: Option<Value>,
}

/// The tool whose rows `source` names, when it is not one text: every tool
/// the row contract admits besides the two file readers.
pub(super) fn rows_tool(source: &Value) -> Option<ToolId> {
    ToolId::from_name(source["tool"].as_str()?)
        .filter(|tool| !matches!(tool, ToolId::LocalFetch | ToolId::GhGetFileContent))
}

fn scalar(value: &Value) -> Option<String> {
    match value {
        Value::String(text) if !text.trim().is_empty() => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

/// A listing entry `<name>[/][ (<fields>)]` as its name.
fn entry_name(entry: &str) -> &str {
    match entry
        .strip_suffix(')')
        .and_then(|fields| fields.rsplit_once(" ("))
    {
        Some((name, _)) => name,
        None => entry,
    }
}

/// What names `row`: its first naming field (an `owner/repo` pair when it
/// has no other), with its `line` when it has one.
fn name(row: &Value) -> Option<String> {
    if let Some(entry) = row.as_str() {
        return Some(entry_name(entry).to_owned());
    }
    let named = NAMES
        .iter()
        .find_map(|field| scalar(&row[*field]))
        .or_else(|| {
            Some(format!(
                "{}/{}",
                row["owner"].as_str()?,
                row["repo"].as_str()?
            ))
        });
    match (named, scalar(&row["line"])) {
        (Some(named), Some(line)) => Some(format!("{named}:{line}")),
        (named, line) => named.or(line),
    }
}

/// The line number a text line starts with (`12\t…`, `12: …`), if any.
fn gutter(line: &str) -> Option<String> {
    let end = line
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(line.len());
    let (digits, rest) = line.split_at_checked(end)?;
    (!digits.is_empty() && rest.starts_with(['\t', ':', ' ', '|'])).then(|| digits.to_owned())
}

/// The path a unit names, which decides whether the judge may see it.
fn path_of<'a>(parent: Option<&'a Value>, unit: &'a Value, key: &'a str) -> Option<&'a str> {
    unit["path"]
        .as_str()
        .or_else(|| parent.and_then(|parent| parent["path"].as_str()))
        .or_else(|| {
            // A directory entry: its key is its path.
            parent
                .filter(|parent| parent.get("dir").is_some())
                .map(|_| key)
        })
}

/// The units of one page of a tool's `data`, in page order.
fn page_units(data: &Value) -> Vec<Unit> {
    let mut units = Vec::new();
    let mut push = |key: String, parent: Option<&Value>, kind: Option<&str>, unit: &Value| {
        let withheld = path_of(parent, unit, &key)
            .is_some_and(crate::policy::discovery::is_sensitive_repository_file);
        let state = (!withheld).then(|| match (parent.and_then(name), kind) {
            (Some(of), Some(kind)) => json!({"of": of, kind: unit}),
            _ => unit.clone(),
        });
        units.push(Unit { key, state });
    };
    let Some(fields) = data.as_object() else {
        return units;
    };
    for (collection, rows) in fields {
        let Some(rows) = rows.as_array().filter(|_| collection != "related") else {
            continue;
        };
        for (at, row) in rows.iter().enumerate().filter(|(_, row)| row.is_object()) {
            let own = name(row);
            let mut children = 0;
            for kind in CHILD_ROWS {
                for (child_at, child) in row[kind].as_array().into_iter().flatten().enumerate() {
                    children += 1;
                    // A string child is a name only in a directory listing;
                    // a text line is named by its line-number gutter.
                    let named = match child.as_str() {
                        Some(line) if row.get("dir").is_none() => gutter(line),
                        _ => name(child),
                    };
                    let key = match (row["dir"].as_str(), &own, named) {
                        (Some(dir), _, Some(entry)) => format!("{dir}/{entry}"),
                        (None, Some(own), Some(child)) => format!("{own}:{child}"),
                        (None, None, Some(child)) => child,
                        (_, Some(own), None) => format!("{own}:{kind}/{child_at}"),
                        (_, None, None) => format!("{collection}/{at}/{kind}/{child_at}"),
                    };
                    push(key, Some(row), Some(kind), child);
                }
            }
            if children == 0 {
                let key = own.unwrap_or_else(|| format!("{collection}/{at}"));
                push(key, None, None, row);
            }
        }
    }
    units
}

/// Each item of a `content` array as one unit, keyed by its index.
fn item_units(items: &[Value]) -> Vec<Unit> {
    items
        .iter()
        .enumerate()
        .map(|(at, item)| Unit {
            key: at.to_string(),
            state: (!blank(item)).then(|| item.clone()),
        })
        .collect()
}

fn blank(item: &Value) -> bool {
    match item {
        Value::Null => true,
        Value::String(text) => text.trim().is_empty(),
        Value::Array(items) => items.is_empty(),
        Value::Object(fields) => fields.is_empty(),
        _ => false,
    }
}

/// Two units of one page may share a name (two hits on one line): the later
/// one takes the first free `<key>#<n>`.
fn distinct(units: &mut [Unit]) {
    let mut seen = std::collections::HashSet::new();
    for unit in units {
        if seen.insert(unit.key.clone()) {
            continue;
        }
        let free = (2..)
            .map(|ordinal| format!("{}#{ordinal}", unit.key))
            .find(|key| !seen.contains(key))
            .unwrap_or_default();
        seen.insert(free.clone());
        unit.key = free;
    }
}

/// Whether a unit's `cells` hold back from a summary: every one answered
/// with a `noul` below [`QUIET_BELOW`].
pub(super) fn is_quiet(cells: &Value) -> bool {
    cells.as_object().is_some_and(|cells| {
        !cells.is_empty()
            && cells.values().all(|cell| {
                cell["status"] == "answered"
                    && cell["answer"]["noul"]
                        .as_f64()
                        .is_some_and(|noul| noul < QUIET_BELOW)
            })
    })
}

fn top_noul(cells: &Value) -> f64 {
    cells
        .as_object()
        .into_iter()
        .flat_map(|cells| cells.values())
        .filter_map(|cell| cell["answer"]["noul"].as_f64())
        .fold(-1.0, f64::max)
}

/// An `evaluate` row whose `source` is tool `tool`'s page of rows.
pub(super) fn judge_source(
    evaluator: &Evaluator,
    evaluations: &Evaluations,
    row: &Row,
    tool: ToolId,
    source: &Value,
    dispatcher: &dyn SourceReader,
    context: &ExecutionContext,
    gate: &GateLease,
    can_resume: bool,
) -> RowOutcome {
    let name = tool.as_str();
    if !dispatcher.availability().is_available(name) {
        return RowOutcome::Units(Err(ClassificationError::new(
            ErrorCode::ClassificationContextUnavailable,
            format!("{name} is not available in this runtime, so its rows cannot be read."),
            "Enable the tool, or supply the rows as content with perItem.",
        )));
    }
    let data = match source_capture::read_rows(tool, &source["query"], dispatcher, context) {
        Ok((data, _)) => data,
        Err(error) => return RowOutcome::Units(Err(error)),
    };
    let units = page_units(&data);
    let mut output = judge(
        evaluator,
        evaluations,
        row,
        units,
        dispatcher,
        context,
        gate,
    );
    // The tool's own next page, as the same row over it.
    let next = &data["next"]["nextPage"];
    match next["input"]["queries"][0].as_object() {
        Some(query) => {
            if let Some(call) = next_page(row, name, query, can_resume) {
                if can_resume {
                    crate::response::result::set_next(
                        &mut output,
                        ClasifyChannel::NextPage.key(),
                        call,
                    );
                } else {
                    crate::response::result::attach_related(&mut output, call);
                }
            }
        }
        None if next.is_object() || data["pagination"]["hasMore"] == true => {
            crate::response::result::add_hints(
                &mut output,
                [format!(
                    "{name} has more rows than this page: page it with its own query fields and pass that query as source.query."
                )],
            );
        }
        None => {}
    }
    RowOutcome::Units(Ok(output))
}

/// The same row over the tool's next page `query`.
fn next_page(row: &Row, tool: &str, query: &Map<String, Value>, can_resume: bool) -> Option<Value> {
    let questions = row
        .keys
        .iter()
        .cloned()
        .zip(row.questions.iter().cloned())
        .collect::<Map<_, _>>();
    let mut call = json!({
        "operation":"evaluate",
        "source":{"tool":tool,"query":query},
        "questions":questions,
    });
    if let Some(context) = &row.context {
        call["context"] = json!(context);
    }
    if let Some(model) = &row.model {
        call["model"] = json!(model);
    }
    if let Some(long_context) = row.long_context {
        call["longContext"] = json!(long_context);
    }
    crate::contracts::validate_built_query(ToolId::Clasify.as_str(), call.clone()).ok()?;
    let continuation = if can_resume {
        crate::response::result::Continuation::continued(ToolId::Clasify, call)
    } else {
        crate::response::result::Continuation::related(ToolId::Clasify, call)
    };
    Some(continuation.build())
}

/// An `evaluate` row whose `content` array is judged item by item.
pub(super) fn judge_items(
    evaluator: &Evaluator,
    evaluations: &Evaluations,
    row: &Row,
    content: &Value,
    dispatcher: &dyn SourceReader,
    context: &ExecutionContext,
    gate: &GateLease,
) -> RowOutcome {
    let Some(items) = content.as_array().filter(|items| !items.is_empty()) else {
        return RowOutcome::Units(Err(ClassificationError::new(
            ErrorCode::InvalidClassificationContext,
            "perItem judges each item of a content array; content is not a non-empty array.",
            "Pass the items as a content array, or drop perItem to judge content whole.",
        )));
    };
    let units = item_units(items);
    RowOutcome::Units(Ok(judge(
        evaluator,
        evaluations,
        row,
        units,
        dispatcher,
        context,
        gate,
    )))
}

/// Ask every question of every unit, as many at once as the provider gate
/// grants one call, and shape the row: the units' answers (highest `noul`
/// first), the coverage and the spend.
fn judge(
    evaluator: &Evaluator,
    evaluations: &Evaluations,
    row: &Row,
    mut units: Vec<Unit>,
    dispatcher: &dyn SourceReader,
    context: &ExecutionContext,
    gate: &GateLease,
) -> Value {
    distinct(&mut units);
    let budget = evaluator.budget(
        crate::execution::cancel::deadline_of(context),
        context.cancellation.clone(),
    );
    let width = call_cap(evaluator.max_concurrency);
    let budget = &budget;
    let judged: Vec<Option<Result<Judged, ClassificationError>>> = dispatcher.handle().block_on(
        stream::iter(units.iter())
            .map(|unit| async move {
                let state = unit.state.as_ref()?;
                let state = super::rows::contextual_state(state, row.context.as_deref());
                let input = EvaluationInput {
                    state: &state,
                    questions: &row.questions,
                    assets: None,
                    long_context: LongContext::from_field(row.long_context),
                    companions: 0,
                };
                let (judged, _) = evaluator
                    .evaluate_once(evaluations, input, row.model.as_deref(), budget, gate)
                    .await;
                Some(judged)
            })
            .buffered(width.max(1))
            .collect(),
    );
    shape(row, units, judged)
}

/// A stop of the call, not a failure of the unit: the same call judges it.
fn stopped(error: &ClassificationError) -> bool {
    matches!(error.code, ErrorCode::Timeout | ErrorCode::Cancelled)
}

fn shape(
    row: &Row,
    units: Vec<Unit>,
    judged: Vec<Option<Result<Judged, ClassificationError>>>,
) -> Value {
    let total = units.len();
    let (mut input_tokens, mut output_tokens, mut calls) = (0_u64, 0_u64, 0_u64);
    let (mut answered, mut quiet, mut failed, mut unassessed, mut withheld) = (0, 0, 0, 0, 0);
    let mut model = None;
    let mut shown: Vec<(String, Value)> = Vec::new();
    for (unit, outcome) in units.into_iter().zip(judged) {
        let cells = match outcome {
            None => {
                withheld += 1;
                continue;
            }
            Some(Ok(judged)) => {
                input_tokens = input_tokens.saturating_add(judged.input_tokens);
                output_tokens = output_tokens.saturating_add(judged.output_tokens);
                calls = calls.saturating_add(judged.provider_calls);
                model.get_or_insert(judged.model);
                answered += 1;
                row.keys
                    .iter()
                    .cloned()
                    .zip(judged.answers.into_iter().map(|mut answer| {
                        if let Some(fields) = answer.as_object_mut() {
                            fields.remove("type");
                            fields.remove("legend");
                        }
                        json!({"status":"answered","answer":answer})
                    }))
                    .collect::<Map<_, _>>()
            }
            Some(Err(error)) => {
                let usage = error.usage();
                input_tokens = input_tokens.saturating_add(usage.input_tokens);
                output_tokens = output_tokens.saturating_add(usage.output_tokens);
                calls = calls.saturating_add(error.provider_calls);
                let cell = if stopped(&error) {
                    unassessed += 1;
                    json!({"status":"unassessed","reason":error.message})
                } else {
                    failed += 1;
                    json!({"status":"error","error":{"errorCode":error.code,"error":error.message}})
                };
                row.keys
                    .iter()
                    .cloned()
                    .map(|key| (key, cell.clone()))
                    .collect::<Map<_, _>>()
            }
        };
        let cells = Value::Object(cells);
        quiet += usize::from(is_quiet(&cells));
        shown.push((unit.key, cells));
    }
    // Highest yes first; units with no noul answer keep page order.
    shown.sort_by(|a, b| top_noul(&b.1).total_cmp(&top_noul(&a.1)));
    let mut coverage = json!({"totalUnits":total,"answered":answered});
    for (field, count) in [
        ("quiet", quiet),
        ("error", failed),
        ("unassessed", unassessed),
        ("withheld", withheld),
    ] {
        if count > 0 {
            coverage[field] = json!(count);
        }
    }
    let mut output = json!({
        "units": shown.into_iter().collect::<Map<_, _>>(),
        "coverage": coverage,
        "usage": reply::evaluation_usage(input_tokens, output_tokens, calls, 0),
    });
    if let Some(model) = model {
        output["model"] = json!(model);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(data: &Value) -> Vec<String> {
        let mut units = page_units(data);
        distinct(&mut units);
        units.into_iter().map(|unit| unit.key).collect()
    }

    /// A search page: each hit of each file, named `path:line`; the file's
    /// other lists (its declarations) are context, not units.
    #[test]
    fn search_hits_are_units_named_path_and_line() {
        let data = json!({"matchCount":3,"files":[
            {"path":"src/a.rs","declarations":[{"symbolName":"f","line":1}],
             "matches":[{"line":49,"value":"x"},{"line":125,"value":"y"},{"line":125,"value":"z"}]},
            {"path":"src/b.rs","matches":[{"line":7,"value":"w"}]},
        ],"related":[{"kind":"related"}]});
        assert_eq!(
            keys(&data),
            [
                "src/a.rs:49",
                "src/a.rs:125",
                "src/a.rs:125#2",
                "src/b.rs:7"
            ]
        );
        let units = page_units(&data);
        assert_eq!(
            units[0].state,
            Some(json!({"of":"src/a.rs","matches":{"line":49,"value":"x"}}))
        );
    }

    /// A page that lists files without hits (a files-only view): each file.
    #[test]
    fn rows_without_child_rows_are_the_units() {
        assert_eq!(
            keys(&json!({"files":[{"path":"a.rs","matchCount":3},{"path":"b.rs"}]})),
            ["a.rs", "b.rs"]
        );
        assert_eq!(
            keys(&json!({"repositories":[{"owner":"o","repo":"r","topics":["x"]}]})),
            ["o/r"]
        );
        assert_eq!(
            keys(&json!({"artifacts":[{"packageName":"zod","version":"4"}]})),
            ["zod"]
        );
        assert_eq!(
            keys(
                &json!({"commits":[{"commitSha":"abc","messageHeadline":"fix"}],
                "pullRequests":[{"number":12,"title":"t"}],"issues":[{"number":7}]})
            ),
            ["abc", "12", "7"]
        );
        assert_eq!(
            keys(
                &json!({"symbols":[{"symbolName":"run","line":4},{"symbolName":"stop","line":9}]})
            ),
            ["run:4", "stop:9"]
        );
    }

    /// A directory listing: each entry under its directory, by path.
    #[test]
    fn listing_entries_are_units_named_by_path() {
        let data = json!({"files":[{"dir":"src","files":["a.rs (10)","sub/"]},{"path":"top.md"}]});
        assert_eq!(keys(&data), ["src/a.rs", "src/sub/", "top.md"]);
    }

    /// A pull request read: each discussion entry and changed file, under
    /// the pull request's number.
    #[test]
    fn discussion_entries_and_changed_files_are_units() {
        let data = json!({"pullRequests":[{"number":42,"title":"t","body":"long",
            "comments":[{"id":1,"body":"a"},{"id":2,"body":"b"}],
            "reviews":[{"id":9,"body":"c"}],
            "reviewComments":[{"id":5,"path":"src/x.rs","body":"d"}],
            "files":[{"path":"src/x.rs","patch":"@@"}]}]});
        assert_eq!(keys(&data), ["42:src/x.rs", "42:1", "42:2", "42:9", "42:5"]);
        // The pull request's body is not repeated to each entry's judge.
        let units = page_units(&data);
        assert_eq!(
            units[1].state,
            Some(json!({"of":"42","comments":{"id":1,"body":"a"}}))
        );
    }

    /// A credential-named file is never shown to the judge.
    #[test]
    fn a_credential_named_unit_is_withheld() {
        let data = json!({"files":[
            {"path":".env","matches":[{"line":1,"value":"KEY=1"}]},
            {"dir":"cfg","files":[".env.local","app.toml"]},
        ]});
        let units = page_units(&data);
        let withheld: Vec<&str> = units
            .iter()
            .filter(|unit| unit.state.is_none())
            .map(|unit| unit.key.as_str())
            .collect();
        assert_eq!(withheld, [".env:1", "cfg/.env.local"]);
    }

    #[test]
    fn content_items_are_units_by_index_and_a_blank_one_is_withheld() {
        let units = item_units(&[json!("a"), json!(""), json!({"k":1})]);
        assert_eq!(
            units
                .iter()
                .map(|unit| unit.key.as_str())
                .collect::<Vec<_>>(),
            ["0", "1", "2"]
        );
        assert!(units[1].state.is_none());
    }

    /// A line-number gutter is leading digits and a separator; any other
    /// character after the digits, multibyte ones included, is no gutter.
    #[test]
    fn a_gutter_is_leading_digits_and_a_separator() {
        assert_eq!(gutter("12\tlet x"), Some("12".to_owned()));
        assert_eq!(gutter("7: λ"), Some("7".to_owned()));
        assert_eq!(gutter("12λ"), None);
        assert_eq!(gutter("λ12\t"), None);
        assert_eq!(gutter("123"), None);
        assert_eq!(gutter(""), None);
    }

    #[test]
    fn only_file_readers_are_not_row_tools() {
        assert_eq!(
            rows_tool(&json!({"tool":"localSearch","query":{}})),
            Some(ToolId::LocalSearch)
        );
        assert_eq!(rows_tool(&json!({"tool":"localFetch","query":{}})), None);
        assert_eq!(rows_tool(&json!({"url":"https://example.com"})), None);
    }
}
