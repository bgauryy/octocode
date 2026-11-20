//! Many small things judged one by one in a single row: the rows of a tool's
//! page (`source: {tool, query}` over a search, listing or history tool), or
//! the items of a `content` array (`perItem`). Each unit is asked every
//! question and answered about itself alone; the row holds the answers keyed
//! by the unit's identity, never the unit's text, and one usage over every
//! request the units took. The units share requests as the in-tool
//! `classification` filter's candidates do
//! ([`octocode_tools_shared::selection::judge::UnitCall`]): the hits of one
//! file are one request, a question per hit naming its line; any other unit
//! is its own request, as it always was.
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
use octocode_contract_types::ErrorCode;
use octocode_runtime_base::execution::ExecutionContext;
use octocode_runtime_base::id::ToolId;
use octocode_runtime_base::providers::classification::gate::GateLease;
use octocode_tools_shared::judgment::{
    ClassificationError, Evaluator, RequestCap,
    shared::{Evaluations, Identity},
};
use octocode_tools_shared::selection::entry_name;
use octocode_tools_shared::selection::judge::{UnitAnswers, UnitCall};
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
/// has no other, its owner the page's `owner` when a one-owner page states
/// it once), with its `line` when it has one.
fn name(row: &Value, owner: Option<&str>) -> Option<String> {
    if let Some(entry) = row.as_str() {
        return Some(entry_name(entry).to_owned());
    }
    let named = NAMES
        .iter()
        .find_map(|field| scalar(&row[*field]))
        .or_else(|| {
            Some(format!(
                "{}/{}",
                row["owner"].as_str().or(owner)?,
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
    // A one-owner page states its owner once, beside its rows.
    let owner = data["owner"].as_str();
    let child_unit = |key: String, parent: Option<&Value>, kind: Option<&str>, unit: &Value| {
        let withheld = path_of(parent, unit, &key)
            .is_some_and(octocode_runtime_base::policy::discovery::is_sensitive_repository_file);
        let state =
            (!withheld).then(
                || match (parent.and_then(|parent| name(parent, owner)), kind) {
                    (Some(of), Some(kind)) => json!({"of": of, kind: unit}),
                    _ => unit.clone(),
                },
            );
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
            let own = name(row, owner);
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
                        _ => name(child, owner),
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

/// A cell's answer: the cell itself when it is one (no `status`), else the
/// answer an `answered` wrapper holds; `None` for any other status.
pub(super) fn answer_of(cell: &Value) -> Option<&Value> {
    match cell.get("status") {
        None => Some(cell),
        Some(status) if status == "answered" => cell.get("answer"),
        Some(_) => None,
    }
}

/// Whether a unit's `cells` hold back from a summary: every one answered
/// with a `noul` below [`QUIET_BELOW`].
pub(super) fn is_quiet(cells: &Value) -> bool {
    cells.as_object().is_some_and(|cells| {
        !cells.is_empty()
            && cells.values().all(|cell| {
                answer_of(cell)
                    .and_then(|answer| answer["noul"].as_f64())
                    .is_some_and(|noul| noul < QUIET_BELOW)
            })
    })
}

fn top_noul(cells: &Value) -> f64 {
    cells
        .as_object()
        .into_iter()
        .flat_map(|cells| cells.values())
        .filter_map(|cell| answer_of(cell)?["noul"].as_f64())
        .fold(-1.0, f64::max)
}

/// A line key under `files`: `<line>` or `<line>#<n>`.
fn line_key(key: &str) -> bool {
    let (line, twin) = key
        .split_once('#')
        .map_or((key, None), |(line, n)| (line, Some(n)));
    let digits = |text: &str| !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit());
    digits(line) && twin.is_none_or(digits)
}

/// Every unit key of a units row, in the order it lists them: its files'
/// hits (`path:line`), then its other units.
pub(super) fn unit_keys(output: &Value) -> Vec<String> {
    let files = output["files"]
        .as_object()
        .into_iter()
        .flatten()
        .flat_map(|(path, lines)| {
            lines
                .as_object()
                .into_iter()
                .flatten()
                .map(move |(line, _)| format!("{path}:{line}"))
        });
    let units = output["units"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(key, _)| key.clone());
    files.chain(units).collect()
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
    let mut judged = match judge(
        evaluator,
        evaluations,
        row,
        units,
        dispatcher,
        context,
        gate,
    ) {
        Ok(judged) => judged,
        Err(over) => return RowOutcome::OverBudget(over),
    };
    // Deciding hits in rank order: the order their units are listed.
    let ranked = rank_deciding(&judged);
    reads(&mut judged.output, tool, query, &data, &ranked, can_resume);
    repository(&mut judged.output, tool, query, &data);
    judged.offer_retry(row, can_resume);
    let output = &mut judged.output;
    // The tool's own next page, as the same row over it; a retry re-asks
    // units of a page whose next page its first call offered.
    let next = &data["next"]["nextPage"];
    let next_query = next["input"]["queries"][0]
        .as_object()
        .filter(|_| row.retry_units.is_none());
    match next_query {
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
        None if row.retry_units.is_none()
            && (next.is_object() || data["pagination"]["hasMore"] == true) =>
        {
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

/// The row of `data` that lists the file at `path`.
fn page_row<'a>(data: &'a Value, path: &str) -> Option<&'a Value> {
    data.as_object()?
        .values()
        .filter_map(Value::as_array)
        .flatten()
        .find(|row| row["path"] == path)
}

/// A GitHub page of one repository states it once, with the commit its rows
/// were read at when they share one: `owner/repo@commitSha:path:line` cites
/// any unit of it.
fn repository(output: &mut Value, tool: ToolId, query: &Value, data: &Value) {
    if !tool.as_str().starts_with("gh") {
        return;
    }
    let (Some(owner), Some(repo)) = (query["owner"].as_str(), query["repo"].as_str()) else {
        return;
    };
    output["owner"] = json!(owner);
    output["repo"] = json!(repo);
    let shas = data
        .as_object()
        .into_iter()
        .flat_map(|fields| fields.values())
        .filter_map(Value::as_array)
        .flatten()
        .filter_map(|row| row["commitSha"].as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let sha = data["commitSha"].as_str().or(match shas.len() {
        1 => shas.first().copied(),
        _ => None,
    });
    if let Some(sha) = sha {
        output["commitSha"] = json!(sha);
    }
}

/// The deciding hits in the order the row lists their units (highest
/// `noul` first).
fn rank_deciding(judged: &UnitsOutcome) -> Vec<(String, u64, u64)> {
    let order = unit_keys(&judged.output);
    let mut ranked = judged.deciding.clone();
    let rank = |(path, line, _): &(String, u64, u64)| {
        let key = format!("{path}:{line}");
        let twin = format!("{key}#");
        order
            .iter()
            .position(|known| *known == key || known.starts_with(&twin))
            .unwrap_or(usize::MAX)
    };
    ranked.sort_by_key(rank);
    ranked
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
    if row.per_hit {
        call[super::rows::PER_HIT] = json!(true);
    }
    if let Some(max) = row.max_requests {
        call["maxRequests"] = json!(max);
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
#[expect(
    clippy::too_many_arguments,
    reason = "one row's units: its runtime, its content and where its retry goes"
)]
pub(super) fn judge_items(
    evaluator: &Evaluator,
    evaluations: &Evaluations,
    row: &Row,
    content: &Value,
    dispatcher: &dyn SourceReader,
    context: &ExecutionContext,
    gate: &GateLease,
    can_resume: bool,
) -> RowOutcome {
    let Some(items) = content.as_array().filter(|items| !items.is_empty()) else {
        return RowOutcome::Units(Err(ClassificationError::new(
            ErrorCode::InvalidClassificationContext,
            "perItem judges each item of a content array; content is not a non-empty array.",
            "Pass the items as a content array, or drop perItem to judge content whole.",
        )));
    };
    let units = item_units(items);
    match judge(
        evaluator,
        evaluations,
        row,
        units,
        dispatcher,
        context,
        gate,
    ) {
        Ok(mut judged) => {
            judged.offer_retry(row, can_resume);
            RowOutcome::Units(Ok(judged))
        }
        Err(over) => RowOutcome::OverBudget(over),
    }
}

/// Ask every question of every unit, in as few provider requests as keep
/// each answer about its unit, and shape the row: the units' answers
/// (highest `noul` first) and the coverage. The spend is stated by
/// [`UnitsOutcome::finish`] once the call has settled which asker of a
/// shared request carries it. The requests are priced before any is sent
/// ([`Evaluator::plan_units`]): a row whose ceiling is over its
/// `maxRequests` sends nothing, and a capped row's sends are counted.
fn judge(
    evaluator: &Evaluator,
    evaluations: &Evaluations,
    row: &Row,
    mut units: Vec<Unit>,
    dispatcher: &dyn SourceReader,
    context: &ExecutionContext,
    gate: &GateLease,
) -> Result<UnitsOutcome, Box<reply::OverBudget>> {
    distinct(&mut units);
    // A retry asks again only the units it names.
    let mut gone = 0;
    if let Some(keys) = &row.retry_units {
        let wanted = keys
            .iter()
            .map(String::as_str)
            .collect::<std::collections::HashSet<_>>();
        units.retain(|unit| wanted.contains(unit.key.as_str()));
        gone = wanted.len().saturating_sub(units.len());
    }
    let sent = units
        .iter()
        .map(|unit| {
            Some(octocode_tools_shared::selection::judge::Unit {
                state: unit.state.as_ref()?,
                alone: sent_state(unit, row.context.as_deref())?.into_owned(),
            })
        })
        .collect::<Vec<_>>();
    let mut call = UnitCall {
        units: &sent,
        questions: &row.questions,
        context: row.context.as_deref(),
        model: row.model.as_deref(),
        long_context: row.long_context,
        cap: None,
        isolate: row.per_hit,
    };
    let plan = evaluator.plan_units(&call);
    let allowed = evaluator.retry_room();
    let priced = |room| reply::Price {
        planned: u64::try_from(plan.requests).unwrap_or(u64::MAX),
        ceiling: Some(RequestCap::ceiling(plan.requests_at_most, room)),
        room,
    };
    if let Some(max) = row.max_requests
        && plan.requests_at_most > max
    {
        return Err(Box::new(reply::OverBudget {
            units: sent.iter().flatten().count(),
            price: priced(allowed),
            input_tokens_at_most: plan.input_tokens_at_most,
            max,
            room: allowed,
        }));
    }
    // The retries its cap leaves room for; held to its ceiling, capped or
    // not.
    let room = super::evaluate::room(row.max_requests, plan.requests_at_most, allowed);
    let price = priced(room);
    let cap = super::evaluate::held(row, plan.requests_at_most, room);
    call.cap = Some(&cap);
    let judged =
        dispatcher
            .handle()
            .block_on(evaluator.judge_units(&call, evaluations, gate, context));
    // The units a transient failure (or the call's end) left unanswered, and
    // the price of asking only them again.
    let unanswered = judged
        .answers
        .iter()
        .enumerate()
        .filter(|(_, answers)| {
            answers
                .as_ref()
                .is_some_and(|answers| answers.iter().any(left_unanswered))
        })
        .map(|(at, _)| at)
        .collect::<Vec<_>>();
    // At most [`RETRY_KEYS`] keys per retry, so its own cursor passes the
    // input gate: each part is its own retry, priced alone.
    let retries = unanswered
        .chunks(RETRY_KEYS)
        .map(|part| {
            let again = sent
                .iter()
                .enumerate()
                .map(|(at, unit)| {
                    let unit = unit.as_ref().filter(|_| part.contains(&at))?;
                    Some(octocode_tools_shared::selection::judge::Unit {
                        state: unit.state,
                        alone: unit.alone.clone(),
                    })
                })
                .collect::<Vec<_>>();
            let plan = evaluator.plan_units(&UnitCall {
                units: &again,
                cap: None,
                ..call
            });
            // Asked again under the row's own cap.
            let room = super::evaluate::room(row.max_requests, plan.requests_at_most, allowed);
            Retry {
                keys: part.iter().map(|&at| units[at].key.clone()).collect(),
                ceiling: RequestCap::ceiling(plan.requests_at_most, room),
            }
        })
        .collect::<Vec<_>>();
    let mut outcome = shape(row, units, judged);
    outcome.price = price;
    outcome.retries = retries;
    if gone > 0 {
        octocode_runtime_base::response::paging::push_warnings(
            &mut outcome.output,
            [format!(
                "{} the retry named {} no longer on the source's page; {} not judged.",
                octocode_engine::text::counted(
                    u64::try_from(gone).unwrap_or(u64::MAX),
                    "unit",
                    "units"
                ),
                if gone == 1 { "is" } else { "are" },
                if gone == 1 { "it was" } else { "they were" },
            )],
        );
    }
    Ok(outcome)
}

/// Whether an answer is still to be had: a transient failure, the call's
/// end, or a spent cap left it unanswered, and asking again may answer it.
fn left_unanswered(answer: &Result<Value, ClassificationError>) -> bool {
    answer.as_ref().is_err_and(|error| {
        stopped(error)
            || error.code == ErrorCode::ClassificationOverBudget
            || octocode_runtime_base::id::error_codes::class(error.code)
                == octocode_runtime_base::id::error_codes::ErrorClass::Transient
    })
}

/// The unit keys one retry names at most: the input gate's bound on an
/// array (`retryUnits` states it too), so a retry's own cursor is admitted.
const RETRY_KEYS: usize = octocode_runtime_base::id::MAX_INPUT_ARRAY_ITEMS;

/// What one of a units row's retries asks again ([`UnitsOutcome::offer_retry`]).
pub struct Retry {
    /// The keys of the units left unanswered, in page order.
    pub keys: Vec<String>,
    /// The ceiling of asking only them: the retry's `maxRequests`.
    pub ceiling: u64,
}

/// What the provider is sent of `unit` when a request holds it alone, with
/// the row's task `context`; a unit with a target scopes every question to
/// it. `None`: withheld.
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

/// What one provider request cost, and whose request it was. Identical
/// requests (here or in a sibling row) hold the same answers and tokens, so
/// the call settles which one carries the spend.
pub struct UnitSpend {
    pub identity: Option<Identity>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub calls: u64,
}

/// A units row: its data (the spend is stated by [`UnitsOutcome::finish`]),
/// each request's spend, and how many the window refused before a split
/// answered their units.
pub struct UnitsOutcome {
    pub output: Value,
    pub spends: Vec<UnitSpend>,
    pub refused: u64,
    /// The search hits that decide an answer, by path: the lines a read
    /// cites them from ([`reads`]).
    pub deciding: Vec<(String, u64, u64)>,
    /// What the row's requests were planned at.
    pub price: reply::Price,
    /// The units left unanswered, which `next.retry` (then `next.retry2`,
    /// …, [`RETRY_KEYS`] units each) asks again.
    pub retries: Vec<Retry>,
}

impl UnitsOutcome {
    /// Offers the units left unanswered again, priced: in a runtime that
    /// keeps saved work, `next.retry` is the same row over only those units
    /// (`retryUnits`, which only an issued call carries) with `maxRequests`
    /// at their ceiling; in one that keeps nothing, the related retry is the
    /// whole row at its own ceiling, and its hint says so.
    pub(super) fn offer_retry(&mut self, row: &Row, can_resume: bool) {
        let Some(query) = row.query.as_object().filter(|_| !self.retries.is_empty()) else {
            return;
        };
        let count = self
            .retries
            .iter()
            .map(|retry| retry.keys.len())
            .sum::<usize>();
        let units = octocode_engine::text::counted(
            u64::try_from(count).unwrap_or(u64::MAX),
            "unit",
            "units",
        );
        let valid = |call: &Value| {
            octocode_runtime_base::contracts::validate_built_query(
                ToolId::Clasify.as_str(),
                call.clone(),
            )
            .is_ok()
        };
        if !can_resume {
            let Some(ceiling) = self.price.ceiling else {
                return;
            };
            let mut query = query.clone();
            query.remove(super::rows::RETRY_UNITS);
            query.insert("maxRequests".into(), json!(ceiling));
            let call = Value::Object(query);
            if !valid(&call) {
                return;
            }
            let call = octocode_runtime_base::response::result::Continuation::related(
                ToolId::Clasify,
                call,
            );
            octocode_runtime_base::response::result::attach_related(&mut self.output, call.build());
            octocode_runtime_base::response::result::add_hints(
                &mut self.output,
                vec![format!(
                    "{units} left unanswered by a transient failure. This command keeps no saved work, so the related retry asks every unit of the row again (billed, at most {ceiling} requests)."
                )],
            );
            return;
        }
        let mut calls = Map::new();
        let mut total = 0_u64;
        for retry in &self.retries {
            let mut query = query.clone();
            query.insert(super::rows::RETRY_UNITS.into(), json!(retry.keys));
            query.insert("maxRequests".into(), json!(retry.ceiling));
            let call = Value::Object(query);
            if !valid(&call) {
                return;
            }
            let name = match calls.len() {
                0 => "retry".to_owned(),
                at => format!("retry{}", at + 1),
            };
            let call = octocode_runtime_base::response::result::Continuation::continued(
                ToolId::Clasify,
                call,
            );
            calls.insert(name, call.build());
            total = total.saturating_add(retry.ceiling);
        }
        let which = match calls.len() {
            1 => "next.retry asks only them again".to_owned(),
            parts => format!(
                "next.retry through next.retry{parts} ask only them again, {RETRY_KEYS} units each"
            ),
        };
        octocode_runtime_base::response::result::attach_calls(
            &mut self.output,
            Value::Object(calls),
        );
        octocode_runtime_base::response::result::add_hints(
            &mut self.output,
            vec![format!(
                "{units} left unanswered by a transient failure: {which} (billed, at most {total} requests)."
            )],
        );
    }

    /// The row's data with its usage: the spend that stayed with its
    /// requests, summed once.
    pub(super) fn finish(self) -> Value {
        let mut output = self.output;
        let sum = |count: fn(&UnitSpend) -> u64| {
            self.spends
                .iter()
                .fold(0_u64, |sum, spend| sum.saturating_add(count(spend)))
        };
        let calls = sum(|spend| spend.calls);
        output["usage"] = reply::evaluation_usage(
            sum(|spend| spend.input_tokens),
            sum(|spend| spend.output_tokens),
            calls,
            self.refused.min(calls),
        );
        reply::priced(&mut output, Some(self.price));
        output
    }
}

/// One answer as a cell: an answered one is its answer, an unassessed one
/// (the call's time ended) and an error keep their status.
fn cell(answer: Result<Value, ClassificationError>) -> Value {
    match answer {
        Ok(mut answer) => {
            if let Some(fields) = answer.as_object_mut() {
                fields.remove("type");
                fields.remove("legend");
            }
            answer
        }
        Err(error) if stopped(&error) => {
            json!({"status":"unassessed","reason":error.message})
        }
        Err(error) => {
            json!({"status":"error","error":{"errorCode":error.code,"error":error.message}})
        }
    }
}

fn shape(row: &Row, units: Vec<Unit>, judged: UnitAnswers) -> UnitsOutcome {
    let total = units.len();
    let (mut answered, mut quiet, mut failed, mut unassessed, mut withheld) = (0, 0, 0, 0, 0);
    // Each shown unit's key, cells and, for a search hit, its path.
    let mut shown: Vec<(String, Value, Option<String>)> = Vec::new();
    let mut deciding = Vec::new();
    let any_noul = row
        .questions
        .iter()
        .any(|question| question["type"] == "noul");
    for (unit, answers) in units.into_iter().zip(judged.answers) {
        let Some(answers) = answers else {
            withheld += 1;
            continue;
        };
        if unit.scoped
            && let Some(span) = unit.state.as_ref().and_then(hit_span)
            && answers.iter().any(|answer| decides(answer, any_noul))
        {
            deciding.push(span);
        }
        let cells = row
            .keys
            .iter()
            .cloned()
            .zip(answers.into_iter().map(cell))
            .collect::<Map<_, _>>();
        let status = |status: &str| cells.values().any(|cell| cell["status"] == status);
        if status("error") {
            failed += 1;
        } else if status("unassessed") {
            unassessed += 1;
        } else {
            answered += 1;
        }
        let cells = Value::Object(cells);
        quiet += usize::from(is_quiet(&cells));
        let path = unit
            .scoped
            .then(|| unit.state.as_ref().and_then(|state| state["path"].as_str()))
            .flatten()
            .map(str::to_owned);
        shown.push((unit.key, cells, path));
    }
    // Highest yes first; units with no noul answer keep page order.
    shown.sort_by(|a, b| top_noul(&b.1).total_cmp(&top_noul(&a.1)));
    // A search hit sits under its file, by line; files list in the order of
    // their best hit.
    let mut files = Map::new();
    let mut others = Map::new();
    for (key, cells, path) in shown {
        let line = path.as_deref().and_then(|path| {
            key.strip_prefix(path)?
                .strip_prefix(':')
                .filter(|line| line_key(line))
        });
        match (path.as_deref(), line) {
            (Some(path), Some(line)) => {
                let lines = files.entry(path.to_owned()).or_insert_with(|| json!({}));
                lines[line] = cells;
            }
            _ => {
                others.insert(key, cells);
            }
        }
    }
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
    let mut output = json!({});
    if !files.is_empty() {
        output["files"] = Value::Object(files);
    }
    if !others.is_empty() || output.get("files").is_none() {
        output["units"] = Value::Object(others);
    }
    output["coverage"] = coverage;
    output["usage"] = Value::Null;
    if let Some(model) = judged.model {
        output["model"] = json!(model);
    }
    let spends = judged
        .spends
        .into_iter()
        .map(|spend| UnitSpend {
            identity: spend.identity,
            input_tokens: spend.input_tokens,
            output_tokens: spend.output_tokens,
            calls: spend.calls,
        })
        .collect();
    UnitsOutcome {
        output,
        spends,
        refused: judged.refused,
        deciding,
        price: reply::Price {
            planned: 0,
            ceiling: Some(0),
            room: 0,
        },
        retries: Vec::new(),
    }
}

/// A hit's path and its first and last line.
fn hit_span(state: &Value) -> Option<(String, u64, u64)> {
    let path = state["path"].as_str()?;
    let line = state["target"]["line"].as_u64()?;
    let last = state["target"]["lastLine"]
        .as_u64()
        .unwrap_or(line)
        .max(line);
    Some((path.to_owned(), line, last))
}

/// Whether an answer is worth citing: a `noul` yes; on a row that asks no
/// `noul`, any choice that did not abstain and any score above its lowest
/// level.
fn decides(answer: &Result<Value, ClassificationError>, any_noul: bool) -> bool {
    let Ok(answer) = answer else {
        return false;
    };
    if let Some(noul) = answer["noul"].as_f64() {
        return noul >= super::combined::YES;
    }
    !any_noul
        && (answer["choice"]
            .as_str()
            .is_some_and(|label| !super::combined::abstains(label))
            || answer["score"].as_f64().is_some_and(|score| score > 0.0))
}

/// Lines either side of a deciding hit a read adds, and the gap below which
/// two ranges of one file read as one.
const READ_REACH: u64 = 2;

/// One read per file holding a deciding hit, of those hits' lines (each
/// with [`READ_REACH`] lines either side, overlapping or near ranges
/// merged): `next.read`, `next.read2`, … in the order the files' first
/// deciding hits rank. A GitHub code page's file is read at the commit the
/// page names; a local one rereads its path. Other tools' units have no
/// file to read.
fn reads(
    output: &mut Value,
    tool: ToolId,
    query: &Value,
    data: &Value,
    deciding: &[(String, u64, u64)],
    can_resume: bool,
) {
    let reader = match tool {
        ToolId::GhSearchCode => ToolId::GhGetFileContent,
        ToolId::LocalSearch | ToolId::AstSearch | ToolId::LspSearch => ToolId::LocalFetch,
        _ => return,
    };
    let mut files: Vec<(&str, Vec<(u64, u64)>)> = Vec::new();
    for (path, line, last) in deciding {
        let span = (
            line.saturating_sub(READ_REACH).max(1),
            last.saturating_add(READ_REACH),
        );
        match files.iter_mut().find(|(known, _)| known == path) {
            Some((_, spans)) => spans.push(span),
            None => files.push((path, vec![span])),
        }
    }
    let mut calls = Map::new();
    for (path, mut spans) in files {
        spans.sort_unstable();
        let mut ranges: Vec<(u64, u64)> = Vec::new();
        for (start, end) in spans {
            match ranges.last_mut() {
                Some(last) if start <= last.1.saturating_add(READ_REACH + 1) => {
                    last.1 = last.1.max(end);
                }
                _ => ranges.push((start, end)),
            }
        }
        let mut row = json!({"path": path});
        if reader == ToolId::GhGetFileContent {
            // The file's own repository and commit, else the page's.
            let file = page_row(data, path);
            let field = |name: &str| {
                file.and_then(|file| file[name].as_str())
                    .or_else(|| data[name].as_str())
                    .or_else(|| query[name].as_str())
            };
            let (Some(owner), Some(repo)) = (field("owner"), field("repo")) else {
                continue;
            };
            row["owner"] = json!(owner);
            row["repo"] = json!(repo);
            if let Some(sha) = file
                .and_then(|file| file["commitSha"].as_str())
                .or_else(|| data["commitSha"].as_str())
            {
                row["ref"] = json!(sha);
            }
        }
        row["ranges"] = ranges
            .iter()
            .map(|(start, end)| json!({"startLine": start, "endLine": end}))
            .collect();
        let call = if can_resume {
            octocode_runtime_base::response::result::Continuation::continued(reader, row)
        } else {
            octocode_runtime_base::response::result::Continuation::related(reader, row)
        };
        let name = match calls.len() {
            0 => "read".to_owned(),
            at => format!("read{}", at + 1),
        };
        calls.insert(name, call.build());
    }
    if !calls.is_empty() {
        octocode_runtime_base::response::result::attach_calls(output, Value::Object(calls));
    }
}

#[cfg(test)]
mod tests;
