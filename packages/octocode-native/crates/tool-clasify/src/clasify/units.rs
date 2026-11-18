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
//! repository, a package). A search row's unit is each hit, however the tool
//! joined their windows ([`hits`]).
use super::{Row, RowOutcome, reply};
use futures_util::{StreamExt as _, stream};
use octocode_contract_types::ErrorCode;
use octocode_runtime_base::execution::ExecutionContext;
use octocode_runtime_base::id::ToolId;
use octocode_runtime_base::providers::classification::gate::{GateLease, call_cap};
use octocode_tools_shared::judgment::{
    ClassificationError, EvaluationInput, Evaluator, Judged, LongContext,
    shared::{Evaluations, Identity},
};
use octocode_tools_shared::selection::entry_name;
use octocode_tools_shared::source_capture::{self, SourceReader};
use serde_json::{Map, Value, json};

mod hits;

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

/// One unit: its key in the row, what the judge sees of it (`None`:
/// withheld, nothing is sent), and whether that names a `target` every
/// question is about (a search hit).
struct Unit {
    key: String,
    state: Option<Value>,
    scoped: bool,
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

/// The units of one page of a tool's `data`, in page order; `query` is the
/// tool's query that listed them.
fn page_units(data: &Value, query: &Value) -> Vec<Unit> {
    let reach = hits::Reach::of(query);
    let mut units = Vec::new();
    let child_unit = |key: String, parent: Option<&Value>, kind: Option<&str>, unit: &Value| {
        let withheld = path_of(parent, unit, &key)
            .is_some_and(octocode_runtime_base::policy::discovery::is_sensitive_repository_file);
        let state = (!withheld).then(|| match (parent.and_then(name), kind) {
            (Some(of), Some(kind)) => json!({"of": of, kind: unit}),
            _ => unit.clone(),
        });
        Unit {
            key,
            state,
            scoped: false,
        }
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
                if kind == "matches"
                    && let Some(found) = own.as_deref().and_then(|own| hits::units(row, own, reach))
                {
                    let withheld = row["path"].as_str().is_some_and(
                        octocode_runtime_base::policy::discovery::is_sensitive_repository_file,
                    );
                    children += found.len();
                    units.extend(found.into_iter().map(|(key, state)| Unit {
                        key,
                        state: (!withheld).then_some(state),
                        scoped: true,
                    }));
                    continue;
                }
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
                    units.push(child_unit(key, Some(row), Some(kind), child));
                }
            }
            if children == 0 {
                let key = own.unwrap_or_else(|| format!("{collection}/{at}"));
                units.push(child_unit(key, None, None, row));
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
            scoped: false,
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
    let query = &source["query"];
    // The caller's row is the tool's public one: its private progress rides
    // only an issued cursor, beside it, as `sourceProgress`.
    if let Some(field) = private_field(tool, query) {
        return RowOutcome::Units(Err(ClassificationError::new(
            ErrorCode::InvalidClassificationContext,
            format!(
                "source.query.{field} is {name}'s private progress, which only an issued cursor carries; nothing was read."
            ),
            "Send the tool's public query; the row's next.nextPage pages its rows.",
        )));
    }
    let progress = row.source_progress.as_ref();
    let data = match source_capture::read_rows(tool, query, progress, dispatcher, context) {
        Ok((data, _)) => data,
        Err(error) => return RowOutcome::Units(Err(error)),
    };
    let units = page_units(&data, query);
    let mut judged = judge(
        evaluator,
        evaluations,
        row,
        units,
        dispatcher,
        context,
        gate,
    );
    let output = &mut judged.output;
    // The tool's own next page, as the same row over it.
    let next = &data["next"]["nextPage"];
    match next["input"]["queries"][0].as_object() {
        Some(query) => match next_page(row, tool, query, can_resume) {
            Some(NextPage::Call(call)) => {
                if can_resume {
                    octocode_runtime_base::response::collection::insert_walk(
                        &mut output["next"],
                        ToolId::Clasify,
                        None,
                        call,
                    );
                } else {
                    octocode_runtime_base::response::result::attach_related(output, call.build());
                }
            }
            Some(NextPage::Unsaved) => {
                octocode_runtime_base::response::paging::push_warnings(
                    output,
                    [format!(
                        "{name} has more rows than this page; its next page is not offered: where it resumes is private progress only a saved cursor carries, and this command exits after one call and keeps no saved work. Run the call where saved work is kept (storage.mode persistent, or one running MCP server)."
                    )],
                );
            }
            None => {}
        },
        None if next.is_object() || data["pagination"]["hasMore"] == true => {
            octocode_runtime_base::response::paging::push_warnings(
                output,
                [format!(
                    "{name} has more rows than this page: page it with its own query fields and pass that query as source.query."
                )],
            );
        }
        None => {}
    }
    RowOutcome::Units(Ok(judged))
}

/// The first of `tool`'s private progress fields (its x-cursor fields) in
/// `query`, a row the caller wrote.
fn private_field(tool: ToolId, query: &Value) -> Option<&str> {
    let private = octocode_runtime_base::contracts::state_fields(tool);
    query
        .as_object()?
        .keys()
        .find(|field| private.contains(*field))
        .map(String::as_str)
}

/// How the row continues over the tool's next page.
enum NextPage {
    Call(octocode_runtime_base::response::result::Continuation),
    /// The next page needs private progress, which only a saved cursor
    /// carries, and this runtime keeps no saved work.
    Unsaved,
}

/// The same row over the tool's next page `query`: its public fields as
/// `source.query`, and the tool's private progress (its x-cursor fields,
/// a walk's position and what it listed) beside them as `sourceProgress`,
/// which only the issued cursor carries, so the row a cursor restores passes
/// the gate as the runtime wrote it however long the walk grew. A fresh
/// related call carries no private progress: without saved work, a page that
/// needs it is [`NextPage::Unsaved`]. `None` when the call breaks the
/// contract.
fn next_page(
    row: &Row,
    tool: ToolId,
    query: &Map<String, Value>,
    can_resume: bool,
) -> Option<NextPage> {
    let private = octocode_runtime_base::contracts::state_fields(tool);
    let (progress, public): (Map<String, Value>, Map<String, Value>) = query
        .iter()
        .map(|(field, value)| (field.clone(), value.clone()))
        .partition(|(field, _)| private.contains(field));
    if !can_resume && !progress.is_empty() {
        return Some(NextPage::Unsaved);
    }
    let questions = row
        .keys
        .iter()
        .cloned()
        .zip(row.questions.iter().cloned())
        .collect::<Map<_, _>>();
    let mut call = json!({
        "operation":"evaluate",
        "source":{"tool":tool.as_str(),"query":public},
        "questions":questions,
    });
    if !progress.is_empty() {
        call["sourceProgress"] = Value::Object(progress);
    }
    if let Some(context) = &row.context {
        call["context"] = json!(context);
    }
    if let Some(model) = &row.model {
        call["model"] = json!(model);
    }
    if let Some(long_context) = row.long_context {
        call["longContext"] = json!(long_context);
    }
    octocode_runtime_base::contracts::validate_built_query(ToolId::Clasify.as_str(), call.clone())
        .ok()?;
    let continuation = if can_resume {
        octocode_runtime_base::response::result::Continuation::continued(ToolId::Clasify, call)
    } else {
        octocode_runtime_base::response::result::Continuation::related(ToolId::Clasify, call)
    };
    Some(NextPage::Call(continuation))
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
/// first) and the coverage. The spend is stated by [`Judged::usage`] once
/// the call has settled which asker of a shared request carries it.
fn judge(
    evaluator: &Evaluator,
    evaluations: &Evaluations,
    row: &Row,
    mut units: Vec<Unit>,
    dispatcher: &dyn SourceReader,
    context: &ExecutionContext,
    gate: &GateLease,
) -> UnitsOutcome {
    distinct(&mut units);
    let budget = evaluator.budget(
        octocode_runtime_base::execution::cancel::deadline_of(context),
        context.cancellation.clone(),
    );
    let width = call_cap(evaluator.max_concurrency);
    let budget = &budget;
    let judged: Vec<Option<Evaluated>> = dispatcher.handle().block_on(
        stream::iter(units.iter())
            .map(|unit| async move {
                let state = sent_state(unit, row.context.as_deref())?;
                let input = EvaluationInput {
                    state: &state,
                    questions: &row.questions,
                    assets: None,
                    long_context: LongContext::from_field(row.long_context),
                    companions: 0,
                };
                Some(
                    evaluator
                        .evaluate_once(evaluations, input, row.model.as_deref(), budget, gate)
                        .await,
                )
            })
            .buffered(width.max(1))
            .collect(),
    );
    shape(row, units, judged)
}

/// What the provider is sent of `unit`, with the row's task `context`; a
/// unit with a target scopes every question to it. `None`: withheld.
fn sent_state<'a>(unit: &'a Unit, context: Option<&str>) -> Option<std::borrow::Cow<'a, Value>> {
    let state = super::rows::contextual_state(unit.state.as_ref()?, context);
    Some(if unit.scoped {
        std::borrow::Cow::Owned(super::rows::scoped(state))
    } else {
        state
    })
}

/// A stop of the call, not a failure of the unit: the same call judges it.
fn stopped(error: &ClassificationError) -> bool {
    matches!(error.code, ErrorCode::Timeout | ErrorCode::Cancelled)
}

/// One unit's evaluation and its identity: units that share one provider
/// request share an identity.
type Evaluated = (Result<Judged, ClassificationError>, Option<Identity>);

/// What one unit's request cost, and whose request it was. Units of one
/// request (here or in a sibling row) hold the same answer and tokens, so
/// the call settles which one carries the spend.
pub struct UnitSpend {
    pub identity: Option<Identity>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub calls: u64,
}

/// A units row: its data (the spend is stated by [`UnitsOutcome::finish`])
/// and each judged unit's spend in unit order.
pub struct UnitsOutcome {
    pub output: Value,
    pub spends: Vec<UnitSpend>,
}

impl UnitsOutcome {
    /// The row's data with its usage: the spend that stayed with its units.
    pub(super) fn finish(self) -> Value {
        let mut output = self.output;
        let sum = |count: fn(&UnitSpend) -> u64| {
            self.spends
                .iter()
                .fold(0_u64, |sum, spend| sum.saturating_add(count(spend)))
        };
        output["usage"] = reply::evaluation_usage(
            sum(|spend| spend.input_tokens),
            sum(|spend| spend.output_tokens),
            sum(|spend| spend.calls),
            0,
        );
        output
    }
}

fn shape(row: &Row, units: Vec<Unit>, judged: Vec<Option<Evaluated>>) -> UnitsOutcome {
    let total = units.len();
    let mut spends = Vec::new();
    let (mut answered, mut quiet, mut failed, mut unassessed, mut withheld) = (0, 0, 0, 0, 0);
    let mut model = None;
    let mut shown: Vec<(String, Value)> = Vec::new();
    for (unit, outcome) in units.into_iter().zip(judged) {
        let cells = match outcome {
            None => {
                withheld += 1;
                continue;
            }
            Some((Ok(judged), identity)) => {
                spends.push(UnitSpend {
                    identity,
                    input_tokens: judged.input_tokens,
                    output_tokens: judged.output_tokens,
                    calls: judged.provider_calls,
                });
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
            Some((Err(error), identity)) => {
                let usage = error.usage();
                spends.push(UnitSpend {
                    identity,
                    input_tokens: usage.input_tokens,
                    output_tokens: usage.output_tokens,
                    calls: error.provider_calls,
                });
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
        "usage": Value::Null,
    });
    if let Some(model) = model {
        output["model"] = json!(model);
    }
    UnitsOutcome { output, spends }
}

#[cfg(test)]
mod tests;
