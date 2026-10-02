//! clasify `locate`: tag one contiguous source page with passage IDs, ask the
//! provider which passage answers a target, and project that distribution onto
//! declaration-aligned verification windows.
//!
//! Passage IDs are provider-facing routing buckets, not syntax nodes. Where
//! the engine can outline the page, passages are grouped by their innermost
//! declaration (its leading doc comment included), because a choice
//! distribution over mutually exclusive passages sums to P(answer lies in
//! that declaration). A doc-comment passage snaps to the declaration line.
use crate::tools::clasify::transport::ClassificationError;
use crate::tools::id::ToolId;
use serde_json::{Map, Value, json};

#[derive(Clone, Debug)]
pub(super) struct LocatedPassage {
    id: String,
    start_line: u64,
    end_line: u64,
}

/// One declaration on the page: `start_line` includes its leading comment
/// block (the engine's `docLine`), `line` is the declaration's name line.
#[derive(Clone, Debug, PartialEq)]
struct Unit {
    start_line: u64,
    line: u64,
    end_line: u64,
}

/// A tagged page: its passages plus the declarations that group them.
#[derive(Clone, Debug, Default)]
pub(super) struct LocatedPage {
    passages: Vec<LocatedPassage>,
    units: Vec<Unit>,
    /// Original page lines (the first is `passages[0].start_line`) and the
    /// file extension, for the doc-comment rule.
    lines: Vec<String>,
    ext: String,
}

/// A JSON record or prose sentence can cross a passage edge; the window keeps
/// the adjacent lines that complete it.
const VERIFY_CONTEXT_LINES: u64 = 2;
/// Lines shown above and below a declaration name when its doc comment won.
const DECLARATION_HEAD: (u64, u64) = (3, 4);
/// A second window is returned when its probability reaches this share of the
/// winner's: the provider could not separate the two, so both need a read.
const RUNNER_UP_SHARE: f64 = 0.5;
/// Only a page that plausibly answers gets a runner-up; on a non-answering
/// page the distribution is flat and a second window is noise.
const RUNNER_UP_MIN_EXISTS: f64 = 0.5;

fn unsupported(message: &str, hint: &str) -> ClassificationError {
    ClassificationError::new("classificationLocateUnsupported", message, hint)
}

pub(super) fn located_state(state: &Value) -> Result<(Value, LocatedPage), ClassificationError> {
    let object = state.as_object().ok_or_else(|| {
        unsupported(
            "locate requires one contiguous file page with original source lines.",
            "Use localFetch or ghGetFileContent without a transformed/minified view.",
        )
    })?;
    let content = object
        .get("content")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            unsupported(
                "locate could not find contiguous file content in this resource page.",
                "Use localFetch or ghGetFileContent with original source content.",
            )
        })?;
    let lines = object
        .get("lines")
        .and_then(Value::as_array)
        .filter(|v| v.len() == 2);
    let (start, end) = lines
        .and_then(|v| Some((v[0].as_u64()?, v[1].as_u64()?)))
        .filter(|(start, end)| *start >= 1 && end >= start)
        .ok_or_else(|| {
            unsupported(
                "locate requires a verified contiguous original-source line range.",
                "Use an unminified line-based localFetch or ghGetFileContent read.",
            )
        })?;
    let source_lines = content.lines().collect::<Vec<_>>();
    if source_lines.is_empty() || source_lines.len() as u64 > end - start + 1 {
        return Err(unsupported(
            "locate could not align captured text with its original source range.",
            "Read a smaller unminified line range and retry.",
        ));
    }
    let passage_lines = 4usize.max(source_lines.len().div_ceil(255));
    let mut passages = Vec::new();
    let mut tagged = String::new();
    for (index, chunk) in source_lines.chunks(passage_lines).enumerate() {
        let id = format!("P{index:03}");
        let start_line = start + (index * passage_lines) as u64;
        let end_line = (start_line + chunk.len() as u64 - 1).min(end);
        passages.push(LocatedPassage {
            id: id.clone(),
            start_line,
            end_line,
        });
        for line in chunk {
            tagged.push_str(&id);
            tagged.push_str("| ");
            tagged.push_str(line);
            tagged.push('\n');
        }
    }
    let units = object
        .get("path")
        .and_then(Value::as_str)
        .map(|path| declaration_units(path, content, start))
        .unwrap_or_default();
    let mut tagged_state = object.clone();
    tagged_state.insert("content".into(), json!(tagged));
    let ext = object
        .get("path")
        .and_then(Value::as_str)
        .and_then(|path| path.rsplit_once('.'))
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .unwrap_or_default();
    let lines = source_lines.iter().map(|line| (*line).to_owned()).collect();
    Ok((
        Value::Object(tagged_state),
        LocatedPage {
            passages,
            units,
            lines,
            ext,
        },
    ))
}

/// Declarations the engine outlines on this page (none for unsupported
/// languages), in original-source lines.
fn declaration_units(path: &str, content: &str, first_line: u64) -> Vec<Unit> {
    let Some(facts) = octocode_engine::portable::extract_declarations(content, path)
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
    else {
        return Vec::new();
    };
    let line_at = |declaration: &Value, pointer: &str| {
        declaration
            .pointer(pointer)
            .and_then(Value::as_u64)
            .and_then(|line| usize::try_from(line).ok())
    };
    facts["declarations"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|declaration| {
            let start = line_at(declaration, "/range/start/line")?;
            let end = line_at(declaration, "/range/end/line")?.max(start);
            let name = line_at(declaration, "/selectionRange/start/line").unwrap_or(start);
            // The engine attaches the comment block directly above (docLine).
            let doc = line_at(declaration, "/docLine").unwrap_or(start).min(start);
            Some(Unit {
                start_line: first_line + doc as u64,
                line: first_line + name as u64,
                end_line: first_line + end as u64,
            })
        })
        .collect()
}

pub(super) fn locate_provider_questions(target: &str, page: &LocatedPage) -> [Value; 2] {
    let mut criteria = Map::new();
    for passage in &page.passages {
        criteria.insert(passage.id.clone(), Value::Null);
    }
    if criteria.len() == 1 {
        criteria.insert(
            "NONE".into(),
            json!("No passage in the supplied source answers the target."),
        );
    }
    [
        json!({
            "type":"choice",
            "instructions":{
                "question":"Which passage ID best answers the target?",
                "target":target
            },
            "criteria":criteria
        }),
        json!({
            "type":"noul",
            "instructions":{
                "question":"Does any passage directly address or answer the target?",
                "target":target
            },
            "criteria":{
                "true":"At least one passage states or directly implies an answer.",
                "false":"No passage addresses the target."
            }
        }),
    ]
}

/// Passages that share an innermost declaration, or that anchor to the same
/// passage outside every declaration.
struct Group {
    unit: Option<usize>,
    /// Passage holding the anchor line when no declaration covers it.
    lone: Option<usize>,
    probability: f64,
    best: usize,
    best_probability: f64,
}

fn innermost_unit(units: &[Unit], line: u64) -> Option<usize> {
    units
        .iter()
        .enumerate()
        .filter(|(_, unit)| unit.start_line <= line && line <= unit.end_line)
        .min_by_key(|(_, unit)| unit.end_line - unit.start_line)
        .map(|(index, _)| index)
}

fn window(page: &LocatedPage, group: &Group) -> (u64, u64) {
    let passage = &page.passages[group.best];
    let page_start = page.passages.first().map_or(1, |first| first.start_line);
    let page_end = page
        .passages
        .last()
        .map_or(passage.end_line, |last| last.end_line);
    let middle = (passage.start_line + passage.end_line) / 2;
    // A doc-comment passage shows the code line its comment block documents.
    if let Some(target) = doc_target(page, passage) {
        return (
            target.saturating_sub(DECLARATION_HEAD.0).max(page_start),
            (target + DECLARATION_HEAD.1).min(page_end),
        );
    }
    let (start, end) = match group.unit.map(|index| &page.units[index]) {
        // Doc comment or attributes above the name: show the declaration.
        Some(unit) if middle < unit.line => (
            passage
                .start_line
                .saturating_sub(VERIFY_CONTEXT_LINES)
                .max(unit.line.saturating_sub(DECLARATION_HEAD.0)),
            unit.line + DECLARATION_HEAD.1,
        ),
        _ => (
            passage.start_line.saturating_sub(VERIFY_CONTEXT_LINES),
            passage.end_line + VERIFY_CONTEXT_LINES,
        ),
    };
    (start.max(page_start), end.min(page_end))
}

/// Whether a source line is a comment line in this language (`#` only in
/// Python, where C/C++ `#include`/`#define` are code).
fn is_comment_line(line: &str, ext: &str) -> bool {
    let line = line.trim_start();
    if ext == "py" {
        line.starts_with('#')
    } else {
        line.starts_with("//") || line.starts_with("/*") || line.starts_with('*')
    }
}

/// A passage inside a comment block anchors to the code line directly after
/// the block (the item the comment documents), when that line is on the page.
/// Needs no parser, so it holds on page fragments a parser cannot outline.
fn doc_target(page: &LocatedPage, passage: &LocatedPassage) -> Option<u64> {
    let first = page.passages.first()?.start_line;
    let middle = (passage.start_line + passage.end_line) / 2;
    let mut index = usize::try_from(middle.checked_sub(first)?).ok()?;
    if !is_comment_line(page.lines.get(index)?, &page.ext) {
        return None;
    }
    while page
        .lines
        .get(index)
        .is_some_and(|line| is_comment_line(line, &page.ext))
    {
        index += 1;
    }
    // Rust attributes and decorators sit between a doc comment and its item.
    while page.lines.get(index).is_some_and(|line| {
        let line = line.trim_start();
        line.starts_with("#[") || line.starts_with('@')
    }) {
        index += 1;
    }
    let code = page.lines.get(index)?;
    (!code.trim().is_empty()).then(|| first + index as u64)
}

fn anchor_line(page: &LocatedPage, passage: &LocatedPassage) -> u64 {
    doc_target(page, passage).unwrap_or((passage.start_line + passage.end_line) / 2)
}

fn passage_at(page: &LocatedPage, line: u64) -> Option<usize> {
    page.passages
        .iter()
        .position(|passage| passage.start_line <= line && line <= passage.end_line)
}

fn rounded(value: f64) -> f64 {
    (value * 10_000.0).round() / 10_000.0
}

pub(super) fn collapse_locate_answer(
    choice: &Result<Value, ClassificationError>,
    exists: &Result<Value, ClassificationError>,
    page: &LocatedPage,
) -> Result<Value, ClassificationError> {
    let choice = choice.as_ref().map_err(Clone::clone)?;
    let exists = exists.as_ref().map_err(Clone::clone)?;
    let probabilities = choice
        .pointer("/answer/probabilities")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            ClassificationError::new(
                "invalidClassificationResponse",
                "locate choice response omitted passage probabilities.",
                "Inspect provider compatibility before using the answer.",
            )
        })?;
    let exists_probability = exists
        .pointer("/answer/noul")
        .and_then(Value::as_f64)
        .ok_or_else(|| {
            ClassificationError::new(
                "invalidClassificationResponse",
                "locate existence response omitted its probability.",
                "Inspect provider compatibility before using the answer.",
            )
        })?;
    let mut groups: Vec<Group> = Vec::new();
    for (index, passage) in page.passages.iter().enumerate() {
        let probability = probabilities
            .get(&passage.id)
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let anchor = anchor_line(page, passage);
        let unit = innermost_unit(&page.units, anchor);
        let lone = unit
            .is_none()
            .then(|| passage_at(page, anchor).unwrap_or(index));
        match groups
            .iter_mut()
            .find(|group| group.unit == unit && group.lone == lone)
        {
            Some(group) => {
                group.probability += probability;
                if probability > group.best_probability {
                    group.best = index;
                    group.best_probability = probability;
                }
            }
            None => groups.push(Group {
                unit,
                lone,
                probability,
                best: index,
                best_probability: probability,
            }),
        }
    }
    groups.sort_by(|left, right| {
        right
            .probability
            .partial_cmp(&left.probability)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    // One atomic target yields one window, plus the runner-up only when the
    // provider could not separate them.
    let top = groups.first().map_or(0.0, |group| group.probability);
    let kept = if exists_probability >= RUNNER_UP_MIN_EXISTS {
        2
    } else {
        1
    };
    let mut windows: Vec<(u64, u64, f64)> = Vec::new();
    for group in groups
        .iter()
        .take(kept)
        .enumerate()
        .filter_map(|(rank, group)| {
            (rank == 0 || (group.probability > 0.0 && group.probability >= top * RUNNER_UP_SHARE))
                .then_some(group)
        })
    {
        let (start, end) = window(page, group);
        // Adjacent declarations can yield overlapping verification windows;
        // one merged window is one read, not two reads of the same lines.
        if let Some(kept) = windows
            .iter_mut()
            .find(|(kept_start, kept_end, _)| start <= *kept_end && *kept_start <= end)
        {
            kept.0 = kept.0.min(start);
            kept.1 = kept.1.max(end);
            kept.2 += group.probability;
        } else {
            windows.push((start, end, group.probability));
        }
    }
    let matches = windows
        .into_iter()
        .map(|(start_line, end_line, probability)| {
            json!({
                "startLine":start_line,
                "endLine":end_line,
                "probability":rounded(probability.min(1.0))
            })
        })
        .collect::<Vec<_>>();
    let mut projected = choice.clone();
    projected["answer"] = json!({
        "type":"locate",
        "exists":rounded(exists_probability),
        "matches":matches
    });
    Ok(projected)
}

/// Locate candidates across every page and resource of one query, ordered by
/// `exists` and then window probability (never their product: probability
/// only ranks passages within a page that answers). Rows here carry no read:
/// this is the copyable `carry` form (see `with_row_reads` for `best`).
pub(super) fn rank_locate(
    resources: &[Value],
    locate_ids: &[&str],
    carry: Option<&Value>,
) -> Option<Value> {
    const KEPT: usize = 3;
    // A compact carry row may omit its resource when the matrix has one.
    let single = match resources {
        [resource] => resource["resourceId"].as_str(),
        _ => None,
    };
    let mut best = Map::new();
    for id in locate_ids {
        // Rows carried from earlier calls of the same walk, structurally
        // checked (they are caller-supplied, advisory ranking state).
        let mut rows = carry
            .and_then(|carry| carry.get(*id))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .take(KEPT)
            .filter_map(|row| super::clasify_compact::ranking_row(row, single))
            .filter(is_candidate_row)
            .collect::<Vec<_>>();
        for resource in resources {
            for page in resource["pages"].as_array().into_iter().flatten() {
                let answer = &page["answers"][*id];
                let Some(exists) = answer["exists"].as_f64() else {
                    continue;
                };
                // One resource may span several files (a search), so each
                // window names its page's file.
                let path = page.pointer("/source/path").filter(|path| path.is_string());
                for window in answer["matches"].as_array().into_iter().flatten() {
                    let mut row = json!({
                        "resourceId":resource["resourceId"],
                        "exists":exists,
                        "startLine":window["startLine"],
                        "endLine":window["endLine"],
                        "probability":window["probability"]
                    });
                    if let Some(path) = path {
                        row["path"] = path.clone();
                    }
                    rows.push(row);
                }
            }
        }
        if rows.is_empty() {
            continue;
        }
        let key = |row: &Value, field: &str| row[field].as_f64().unwrap_or(0.0);
        rows.sort_by(|left, right| {
            key(right, "exists")
                .total_cmp(&key(left, "exists"))
                .then_with(|| key(right, "probability").total_cmp(&key(left, "probability")))
        });
        // A carried row and a fresh one (or two pages' windows) can cover the
        // same lines of one file; keep only the higher-ranked of them.
        let mut kept: Vec<Value> = Vec::new();
        for row in rows {
            let same_lines = |other: &Value| {
                other["resourceId"] == row["resourceId"]
                    && other.get("path") == row.get("path")
                    && key(&row, "startLine") <= key(other, "endLine")
                    && key(other, "startLine") <= key(&row, "endLine")
            };
            if !kept.iter().any(same_lines) {
                kept.push(row);
            }
        }
        let mut rows = kept;
        rows.truncate(KEPT);
        best.insert((*id).to_owned(), Value::Array(rows));
    }
    (!best.is_empty()).then_some(Value::Object(best))
}

/// Publish `best` when the walk is finished, or when its top window already
/// answers; a lone strong window is published too. A low-exists ranking on
/// an open walk stays in `carry` only. Public rows are the answering windows
/// (`exists` ≥ 0.5); a finished walk with none names only its closest
/// passage. Every row stays in `carry`.
pub(super) fn readable_best(best: &Value, walk_open: bool) -> Option<Value> {
    let exists = |row: &Value| row.get("exists").and_then(Value::as_f64).unwrap_or(0.0);
    let mut kept = Map::new();
    for (id, rows) in best.as_object()? {
        let Some(rows) = rows.as_array().filter(|rows| !rows.is_empty()) else {
            continue;
        };
        let answering = rows
            .iter()
            .filter(|row| exists(row) >= RUNNER_UP_MIN_EXISTS)
            .cloned()
            .collect::<Vec<_>>();
        if !answering.is_empty() {
            kept.insert(id.clone(), Value::Array(answering));
        } else if !walk_open {
            kept.insert(id.clone(), Value::Array(rows[..1].to_vec()));
        }
    }
    (!kept.is_empty()).then_some(Value::Object(kept))
}

/// One assessed page's executable read: the resource and source path it reads,
/// and the call a `best` row narrows to its own window.
pub(super) struct LocateRead {
    pub(super) resource_id: String,
    pub(super) path: Option<String>,
    /// Source lines the page assessed; its read pins that observed version.
    pub(super) scope: Option<(u64, u64)>,
    pub(super) read: Value,
}

/// Give each public `best` row `next.read` of exactly its window, from a page
/// of the same resource and file. A row with no line-addressable file read
/// (e.g. a carried row whose file this call did not read) stays without one.
pub(super) fn with_row_reads(mut best: Value, reads: &[LocateRead]) -> Value {
    let rows = best
        .as_object_mut()
        .into_iter()
        .flat_map(|best| best.values_mut())
        .filter_map(Value::as_array_mut)
        .flatten();
    for row in rows {
        let (Some(start), Some(end)) = (row["startLine"].as_u64(), row["endLine"].as_u64()) else {
            continue;
        };
        let resource = row["resourceId"].as_str();
        let path = row.get("path").and_then(Value::as_str);
        let same_file = reads.iter().filter(|read| {
            Some(read.resource_id.as_str()) == resource && read.path.as_deref() == path
        });
        // Prefer the page that assessed the window: its read carries that
        // page's snapshot, so the row replays exactly like the page's read.
        let (assessed, other): (Vec<_>, Vec<_>) = same_file.partition(|read| {
            read.scope
                .is_some_and(|(first, last)| first <= start && end <= last)
        });
        let read = assessed
            .into_iter()
            .chain(other)
            .find_map(|read| super::clasify_output::window_read(&read.read, start, end));
        if let Some(read) = read {
            row["next"] = json!({"read":read});
        }
    }
    best
}

/// A located page's `next.read` points at its top window. It stays only for
/// an answering page (`exists` ≥ 0.5) whose read `best` does not already
/// carry: a non-answer is not a read to run (its window stays in `matches`,
/// and `next.clasify` or `best` is the route). Pages with other answers keep
/// their read.
pub(super) fn drop_redundant_page_reads(resources: &mut [Value], best: Option<&Value>) {
    let best_reads = best
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(|best| best.values())
        .filter_map(Value::as_array)
        .flatten()
        .filter_map(|row| row.pointer("/next/read"))
        .collect::<Vec<_>>();
    let pages = resources
        .iter_mut()
        .filter_map(|resource| resource.get_mut("pages").and_then(Value::as_array_mut))
        .flatten();
    for page in pages {
        let Some(exists) = page
            .get("answers")
            .and_then(Value::as_object)
            .filter(|answers| !answers.is_empty())
            .and_then(|answers| {
                answers
                    .values()
                    .map(|answer| answer.get("exists").and_then(Value::as_f64))
                    .collect::<Option<Vec<_>>>()
            })
        else {
            continue;
        };
        let answering = exists.iter().any(|exists| *exists >= RUNNER_UP_MIN_EXISTS);
        let redundant = page
            .pointer("/next/read")
            .is_some_and(|read| !answering || best_reads.contains(&read));
        if redundant && let Some(page) = page.as_object_mut() {
            page.remove("next");
        }
    }
}

fn is_candidate_row(row: &Value) -> bool {
    let probability = |field: &str| {
        row[field]
            .as_f64()
            .is_some_and(|p| (0.0..=1.0).contains(&p))
    };
    let line = |field: &str| row[field].as_u64().filter(|line| *line >= 1);
    row["resourceId"].is_string()
        && probability("exists")
        && probability("probability")
        && line("startLine")
            .zip(line("endLine"))
            .is_some_and(|(start, end)| end >= start)
        && row
            .get("path")
            .is_none_or(|path| path.as_str().is_some_and(|path| !path.is_empty()))
        && row
            .as_object()
            .is_some_and(|object| object.len() == 5 + usize::from(row.get("path").is_some()))
}

/// A target that names a code identifier is usually cheaper and exact with
/// localSearch; locate earns its cost on described behavior.
pub(super) fn literal_target_hint(target: &str) -> Option<String> {
    let identifier = literal_target(target)?;
    Some(format!(
        "Target names `{identifier}`; if that literal is what you need, localSearch finds it exactly and cheaper than locate."
    ))
}

/// A target that is nothing but one identifier token: a literal lookup, not
/// described behavior.
pub(super) fn bare_identifier(target: &str) -> Option<&str> {
    let token = target
        .trim()
        .trim_matches(['`', '"', '\''])
        .trim_end_matches("()");
    (!token.contains(char::is_whitespace) && looks_like_identifier(token)).then_some(token)
}

pub(super) fn bare_target_hint(identifier: &str) -> String {
    format!(
        "Target `{identifier}` is a bare identifier: locate was skipped (no read or provider call); run next.localSearch for its exact matches."
    )
}

/// Identifier-shaped tokens a target names (`snake_case`, `camelCase`,
/// `Path::name`), in order.
fn identifier_tokens(target: &str) -> impl Iterator<Item = &str> {
    target
        .split(|c: char| c.is_whitespace() || matches!(c, ',' | ';' | '"' | '\'' | '`' | '?' | '!'))
        .map(|token| token.trim_end_matches(['.', ')']).trim_end_matches('('))
        .filter(|token| looks_like_identifier(token))
}

fn literal_target(target: &str) -> Option<&str> {
    identifier_tokens(target).next()
}

/// Up to `limit` distinct identifiers a goal names, as prefilter literals.
pub(super) fn goal_literals(goal: &str, limit: usize) -> Vec<String> {
    let mut literals: Vec<String> = Vec::new();
    for token in identifier_tokens(goal) {
        if literals.len() == limit {
            break;
        }
        if !literals.iter().any(|seen| seen == token) {
            literals.push(token.to_owned());
        }
    }
    literals
}

/// `next.localSearch`: the first identifier a locate target names, searched
/// literally over the local resources (their one path, or the deepest
/// directory they share). Remote or path-less resources keep the hint alone.
/// It is a follow-up of this query, so it inherits the brief.
pub(super) fn literal_search<'a>(
    targets: impl IntoIterator<Item = &'a str>,
    resources: &[Value],
) -> Option<Value> {
    let literal = targets.into_iter().find_map(literal_target)?;
    let path = local_scope(resources)?;
    let query: crate::contracts::tool_types::LocalSearchQuery = serde_json::from_value(
        // The typed query needs a brief; it is dropped again so the response
        // stage gives this hint the matrix's own brief.
        json!({"path":path,"searchText":literal,"regex":"literal","goal":"-","reasoning":"-"}),
    )
    .ok()?;
    let mut query = serde_json::to_value(query).ok()?;
    super::continuations::compact_input(ToolId::LocalSearch.as_str(), &mut query);
    let object = query.as_object_mut()?;
    object.retain(|_, value| !value.is_null());
    object.remove("goal");
    object.remove("reasoning");
    Some(json!({"tool":ToolId::LocalSearch.as_str(),"query":query}))
}

/// The one path every local resource reads under: a single file or directory,
/// else their deepest shared directory. A remote resource, or paths sharing
/// no directory below the root, leaves no scope.
fn local_scope(resources: &[Value]) -> Option<String> {
    use std::path::{Path, PathBuf};
    let mut paths = Vec::<&str>::new();
    for resource in resources {
        let context = &resource["context"];
        // Local-family reads scope by their `path` (a path-less local read,
        // e.g. lspSearch's `uri`, leaves no scope). astTopology is excluded:
        // its `path` is a graph-analysis root, not the evidence it read.
        let local = context["tool"]
            .as_str()
            .and_then(ToolId::from_name)
            .is_some_and(|id| id.is_local() && id != ToolId::AstTopology);
        if !local {
            return None;
        }
        let path = context["query"]["path"]
            .as_str()
            .filter(|path| !path.is_empty())?;
        if !paths.contains(&path) {
            paths.push(path);
        }
    }
    let (first, rest) = paths.split_first()?;
    if rest.is_empty() {
        return Some((*first).to_owned());
    }
    let mut common: PathBuf = Path::new(first).parent()?.to_path_buf();
    for path in rest {
        while !Path::new(path).starts_with(&common) {
            common = common.parent()?.to_path_buf();
        }
    }
    let scope = common.to_str()?;
    (!scope.is_empty() && common.parent().is_some()).then(|| scope.to_owned())
}

fn looks_like_identifier(token: &str) -> bool {
    if token.len() < 3
        || !token
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == ':')
        || !token.chars().any(char::is_alphabetic)
    {
        return false;
    }
    let camel = token
        .chars()
        .zip(token.chars().skip(1))
        .any(|(a, b)| a.is_lowercase() && b.is_uppercase());
    token.contains("::") || token.contains('_') || camel
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answers(probabilities: Value, exists: f64) -> [Result<Value, ClassificationError>; 2] {
        [
            Ok(
                json!({"resolvedModel":"jev","answer":{"type":"choice","choice":"P000",
                "confidence":0.9,"probabilities":probabilities}}),
            ),
            Ok(json!({"answer":{"type":"noul","noul":exists}})),
        ]
    }

    #[test]
    fn locate_tags_unread_file_passages_and_projects_compact_source_ranges() {
        let state = json!({
            "path":"/tmp/guide.md",
            "lines":[101,108],
            "content":"one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\n"
        });
        let (tagged, page) = located_state(&state).expect("contiguous file state");
        assert_eq!(page.passages.len(), 2);
        assert_eq!(page.passages[0].start_line, 101);
        assert_eq!(page.passages[0].end_line, 104);
        assert!(page.units.is_empty(), "markdown has no declarations");
        assert!(tagged["content"].as_str().unwrap().contains("P001| five"));
        let [choice, exists] = answers(
            json!({"P000":0.06,"P001":0.9400000000000001}),
            0.9700000000000001,
        );
        let projected = collapse_locate_answer(&choice, &exists, &page).unwrap();
        assert_eq!(
            projected["answer"],
            json!({"type":"locate","exists":0.97,"matches":[
                {"startLine":103,"endLine":108,"probability":0.94}
            ]})
        );
        assert_eq!(projected["resolvedModel"], "jev");
    }

    #[test]
    fn locate_rejects_values_and_transformed_or_disjoint_source_views() {
        for state in [
            json!({"message":"already observed"}),
            json!({"path":"a","content":"x","lines":[[1,1],[9,9]]}),
            json!({"path":"a","content":"x"}),
        ] {
            assert_eq!(
                located_state(&state).unwrap_err().code,
                "classificationLocateUnsupported"
            );
        }
    }

    fn js_page() -> (Value, LocatedPage) {
        // Lines 1-12: a doc comment (1-8) above `debounce` (9-12); 13-20 `other`.
        let mut content = String::from("/**\n");
        for _ in 0..6 {
            content.push_str(" * Creates a debounced function.\n");
        }
        content.push_str(" */\nfunction debounce(func, wait) {\n  let a = 1;\n  let b = 2;\n  return a + b;\n}\n");
        content.push_str("function other() {\n  let c = 3;\n  let d = 4;\n  let e = 5;\n  let f = 6;\n  let g = 7;\n  return c;\n}\n");
        located_state(&json!({"path":"/src/a.js","lines":[1,21],"content":content})).unwrap()
    }

    #[test]
    fn doc_comment_passages_group_with_their_declaration_and_snap_to_its_name() {
        let (_, page) = js_page();
        assert_eq!(
            page.units,
            vec![
                Unit {
                    start_line: 1,
                    line: 9,
                    end_line: 13
                },
                Unit {
                    start_line: 14,
                    line: 14,
                    end_line: 21
                },
            ]
        );
        // The doc passages P000/P001 carry the mass; P002 holds the name line.
        let [choice, exists] = answers(
            json!({"P000":0.4,"P001":0.3,"P002":0.1,"P003":0.1,"P004":0.1}),
            0.95,
        );
        let projected = collapse_locate_answer(&choice, &exists, &page).unwrap();
        // Group debounce = 0.8; the window reaches the declaration line 9.
        assert_eq!(
            projected["answer"]["matches"],
            json!([{"startLine":6,"endLine":13,"probability":0.8}])
        );
    }

    #[test]
    fn a_doc_comment_passage_joins_the_line_it_documents_without_a_parser() {
        // A fragment no outline covers (`.txt`: no units): the doc block
        // (lines 3-9) documents `function debounce` on line 10.
        let content = "  x();\n}\n/**\n * Delays calls.\n * More.\n * More.\n * More.\n * More.\n */\nfunction debounce(func) {\n  return func;\n}\n";
        let (_, page) =
            located_state(&json!({"path":"/page.txt","lines":[100,111],"content":content}))
                .unwrap();
        assert!(page.units.is_empty());
        // Doc passage P001 (103-106) and declaration passage P002 (107-110) tie.
        let [choice, exists] = answers(json!({"P000":0.1,"P001":0.45,"P002":0.45}), 0.95);
        let projected = collapse_locate_answer(&choice, &exists, &page).unwrap();
        // One group (0.9), window on the documented line 109.
        assert_eq!(
            projected["answer"]["matches"],
            json!([{"startLine":106,"endLine":111,"probability":0.9}])
        );
    }

    #[test]
    fn split_mass_inside_one_declaration_outranks_a_sharper_lone_passage() {
        let (_, page) = js_page();
        // `other` spans P003-P005 (0.2+0.2+0.2); debounce's best passage is 0.3.
        let [choice, exists] = answers(
            json!({"P000":0.1,"P001":0.1,"P002":0.1,"P003":0.3,"P004":0.2,"P005":0.2}),
            0.9,
        );
        let projected = collapse_locate_answer(&choice, &exists, &page).unwrap();
        let matches = projected["answer"]["matches"].as_array().unwrap();
        // Inside `other` the best passage (P003, lines 13-16) keeps its window.
        assert_eq!(
            matches[0],
            json!({"startLine":11,"endLine":18,"probability":0.7})
        );
        // debounce (0.3) is below half of 0.7: no runner-up.
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn overlapping_winner_and_runner_up_windows_merge_into_one_read() {
        let (_, page) = js_page();
        // P003 and P004 both sit in `other`-adjacent passages whose ±2-line
        // windows overlap; the host should get one window, not two copies.
        let [choice, exists] = answers(json!({"P003":0.5,"P004":0.45,"P005":0.05}), 0.9);
        let projected = collapse_locate_answer(&choice, &exists, &page).unwrap();
        let matches = projected["answer"]["matches"].as_array().unwrap();
        for (index, left) in matches.iter().enumerate() {
            for right in &matches[index + 1..] {
                assert!(
                    left["endLine"].as_u64() < right["startLine"].as_u64()
                        || right["endLine"].as_u64() < left["startLine"].as_u64(),
                    "{matches:?}"
                );
            }
        }
    }

    #[test]
    fn ranking_drops_rows_covering_lines_a_better_row_covers() {
        let row = |start: u64, end: u64, probability: f64| json!({"resourceId":"r","path":"/a.rs","exists":0.9,"startLine":start,"endLine":end,"probability":probability});
        let carry = json!({"t":[row(10, 17, 0.8)]});
        let resources = [json!({"resourceId":"r","pages":[{
            "source":{"path":"/a.rs"},
            "answers":{"t":{"exists":0.9,"matches":[
                {"startLine":14,"endLine":21,"probability":0.5},
                {"startLine":40,"endLine":47,"probability":0.4}
            ]}}
        }]})];
        let best = rank_locate(&resources, &["t"], Some(&carry)).unwrap();
        let lines: Vec<_> = best["t"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| {
                (
                    row["startLine"].as_u64().unwrap(),
                    row["endLine"].as_u64().unwrap(),
                )
            })
            .collect();
        assert_eq!(lines, [(10, 17), (40, 47)]);
    }

    #[test]
    fn a_close_runner_up_is_returned_and_a_distant_one_is_not() {
        let (_, page) = js_page();
        let [choice, exists] = answers(json!({"P002":0.45,"P004":0.55}), 0.9);
        let close = collapse_locate_answer(&choice, &exists, &page).unwrap();
        assert_eq!(close["answer"]["matches"].as_array().unwrap().len(), 2);
        assert_eq!(close["answer"]["matches"][0]["probability"], 0.55);
        let [choice, exists] = answers(json!({"P002":0.1,"P004":0.9}), 0.9);
        let far = collapse_locate_answer(&choice, &exists, &page).unwrap();
        assert_eq!(far["answer"]["matches"].as_array().unwrap().len(), 1);
        // The same near-tie on a page that does not answer stays one window.
        let [choice, exists] = answers(json!({"P002":0.45,"P004":0.55}), 0.2);
        let unanswered = collapse_locate_answer(&choice, &exists, &page).unwrap();
        assert_eq!(unanswered["answer"]["matches"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn ranking_is_exists_first_then_probability() {
        let single = vec![json!({"resourceId":"f","pages":[]})];
        let order = |carry: Value| {
            rank_locate(&single, &["t"], Some(&carry)).unwrap()["t"]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| row["startLine"].as_u64().unwrap())
                .collect::<Vec<_>>()
        };
        // A window trailing by a small exists gap stays behind despite a higher p.
        assert_eq!(
            order(json!({"t":[
                {"lines":[5020,5027],"exists":0.89,"p":0.37},
                {"lines":[10,17],"exists":0.40,"p":0.99},
                {"lines":[4657,4664],"exists":0.85,"p":0.86}
            ]})),
            vec![5020, 4657, 10]
        );
        // Equal exists: probability decides.
        assert_eq!(
            order(json!({"t":[
                {"lines":[1,8],"exists":0.9,"p":0.2},
                {"lines":[20,28],"exists":0.9,"p":0.8}
            ]})),
            vec![20, 1]
        );
    }

    #[test]
    fn compact_carry_rows_join_the_ranking() {
        let single = vec![json!({"resourceId":"f","pages":[]})];
        let carry = json!({"t":[
            {"lines":[5,12],"exists":0.99,"p":0.9},
            {"r":"f","path":"a.c","lines":[40,48],"exists":0.6,"p":0.2},
            {"lines":[9],"exists":0.9,"p":0.9}
        ]});
        let ranked = rank_locate(&single, &["t"], Some(&carry)).unwrap();
        assert_eq!(
            ranked["t"],
            json!([
                {"resourceId":"f","exists":0.99,"startLine":5,"endLine":12,"probability":0.9},
                {"resourceId":"f","exists":0.6,"startLine":40,"endLine":48,"probability":0.2,"path":"a.c"}
            ])
        );
        // Without a resource id a row is ambiguous across several resources.
        let several = vec![
            json!({"resourceId":"f","pages":[]}),
            json!({"resourceId":"g","pages":[]}),
        ];
        assert!(
            rank_locate(
                &several,
                &["t"],
                Some(&json!({"t":[{"lines":[5,12],"exists":0.99,"p":0.9}]}))
            )
            .is_none()
        );
    }

    #[test]
    fn ranking_orders_windows_by_exists_before_probability() {
        let page = |path: &str, exists: f64, probability: f64, line: u64| {
            json!({"source":{"path":path},"answers":{"t":{"exists":exists,
                "matches":[{"startLine":line,"endLine":line + 7,"probability":probability}]}}})
        };
        let resources = vec![json!({"resourceId":"r","pages":[
            page("a.js", 0.3, 0.95, 1),
            page("a.js", 0.97, 0.4, 700),
            page("a.js", 0.97, 0.6, 900),
        ]})];
        let best = rank_locate(&resources, &["t"], None).unwrap();
        let lines = best["t"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["startLine"].as_u64().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(lines, vec![900, 700, 1]);
        assert_eq!(best["t"][0]["resourceId"], "r");
        // A single strong window is public on its own, open walk or not.
        let single = vec![json!({"resourceId":"r","pages":[page("a.js", 0.9, 0.9, 1)]})];
        let lone = rank_locate(&single, &["t"], None).unwrap();
        assert_eq!(lone["t"].as_array().unwrap().len(), 1);
        assert_eq!(readable_best(&lone, false).unwrap(), lone);
        assert_eq!(readable_best(&lone, true).unwrap(), lone);
        // A lone carried row survives the next call with no new windows.
        let carried = rank_locate(&[], &["t"], Some(&lone)).unwrap();
        assert_eq!(carried["t"][0]["startLine"], 1);
        // A carried row from an earlier call joins the ranking; malformed
        // ones (out-of-range values, an inverted window) are ignored.
        let carry = json!({"t":[
            {"resourceId":"r","exists":0.99,"startLine":5,"endLine":12,"probability":0.9},
            {"resourceId":"r","exists":2.0,"startLine":0,"endLine":1,"probability":0.9},
            {"resourceId":"r","exists":0.98,"startLine":40,"endLine":39,"probability":0.9}
        ]});
        let merged = rank_locate(&single, &["t"], Some(&carry)).unwrap();
        assert_eq!(merged["t"][0]["startLine"], 5);
        assert_eq!(merged["t"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn ranked_windows_name_the_file_they_are_in() {
        // One search resource spans several files: resourceId alone cannot
        // say which file a window belongs to.
        let page = |path: &str, exists: f64, line: u64| {
            json!({"source":{"path":path},"answers":{"t":{"exists":exists,
                "matches":[{"startLine":line,"endLine":line + 7,"probability":0.9}]}}})
        };
        let resources = vec![json!({"resourceId":"s","pages":[
            page("/repo/a.go", 0.2, 10),
            page("/repo/b.go", 0.9, 40),
        ]})];
        let best = rank_locate(&resources, &["t"], None).unwrap();
        assert_eq!(best["t"][0]["path"], "/repo/b.go");
        assert_eq!(best["t"][1]["path"], "/repo/a.go");
        // A carried row keeps its path through the next call.
        let carried = rank_locate(&[], &["t"], Some(&best)).unwrap();
        assert_eq!(carried["t"][0]["path"], "/repo/b.go");
    }

    #[test]
    fn an_open_walk_hides_a_low_exists_ranking_and_keeps_an_answer() {
        let low = json!({"t":[
            {"resourceId":"r","exists":0.06,"startLine":926,"endLine":933,"probability":0.23},
            {"resourceId":"r","exists":0.05,"startLine":115,"endLine":122,"probability":0.59}
        ]});
        assert!(readable_best(&low, true).is_none());
        assert_eq!(
            readable_best(&low, false).unwrap()["t"][0]["startLine"],
            926
        );
        let mixed = json!({"found":[
            {"resourceId":"r","exists":0.98,"startLine":1473,"endLine":1480,"probability":0.94},
            {"resourceId":"r","exists":0.06,"startLine":926,"endLine":933,"probability":0.23}
        ],"waiting":[
            {"resourceId":"r","exists":0.04,"startLine":1,"endLine":8,"probability":0.2},
            {"resourceId":"r","exists":0.02,"startLine":9,"endLine":16,"probability":0.1}
        ]});
        let visible = readable_best(&mixed, true).unwrap();
        assert_eq!(visible["found"][0]["startLine"], 1473);
        assert!(visible.get("waiting").is_none());
    }

    #[test]
    fn best_lists_only_answering_windows_or_a_finished_walk_s_closest_passage() {
        let row = |exists: f64, line: u64| json!({"resourceId":"r","exists":exists,"startLine":line,"endLine":line + 7,"probability":0.5});
        let lines = |visible: &Value| {
            visible["t"]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| row["startLine"].as_u64().unwrap())
                .collect::<Vec<_>>()
        };
        // A non-answering window is not a read to run, open walk or not.
        let one = json!({"t":[row(0.98, 43), row(0.08, 745), row(0.05, 901)]});
        for walk_open in [true, false] {
            assert_eq!(lines(&readable_best(&one, walk_open).unwrap()), [43]);
        }
        let two = json!({"t":[row(0.9, 10), row(0.7, 40), row(0.2, 90)]});
        assert_eq!(lines(&readable_best(&two, false).unwrap()), [10, 40]);
        // A finished walk with no answer names only its closest passage.
        let none = json!({"t":[row(0.3, 5), row(0.2, 50), row(0.1, 500)]});
        assert_eq!(lines(&readable_best(&none, false).unwrap()), [5]);
        assert!(readable_best(&none, true).is_none());
    }

    #[test]
    fn best_rows_read_exactly_their_window_and_carry_stays_copyable() {
        let page = |path: &str, line: u64| {
            json!({"source":{"path":path},"answers":{"t":{"exists":0.9,
                "matches":[{"startLine":line,"endLine":line + 7,"probability":0.9}]}}})
        };
        let resources = vec![json!({"resourceId":"g","pages":[page("o/r/src/a.rs", 40)]})];
        let best = rank_locate(&resources, &["t"], None).unwrap();
        let reads = [
            LocateRead {
                resource_id: "g".into(),
                path: Some("o/r/src/other.rs".into()),
                scope: None,
                read: json!({"tool":"ghGetFileContent","query":{"owner":"o","repo":"r","path":"src/other.rs"}}),
            },
            LocateRead {
                resource_id: "g".into(),
                path: Some("o/r/src/a.rs".into()),
                scope: None,
                read: json!({"tool":"ghGetFileContent","query":{"owner":"o","repo":"r",
                    "path":"src/a.rs","branch":"abc","fullContent":true}}),
            },
        ];
        let visible = with_row_reads(readable_best(&best, true).unwrap(), &reads);
        assert_eq!(
            visible["t"][0]["next"]["read"],
            json!({"tool":"ghGetFileContent","query":{"owner":"o","repo":"r",
                "path":"src/a.rs","branch":"abc","startLine":40,"endLine":47}})
        );
        // The ranking itself (the carry form) never gains a read.
        assert!(best["t"][0].get("next").is_none());
        // No read for another resource, or a non-file read.
        let search = [LocateRead {
            resource_id: "g".into(),
            path: Some("o/r/src/a.rs".into()),
            scope: None,
            read: json!({"tool":"ghSearchCode","query":{"keywords":["x"]}}),
        }];
        assert!(
            with_row_reads(best.clone(), &search)["t"][0]
                .get("next")
                .is_none()
        );
        assert!(
            with_row_reads(best.clone(), &[])["t"][0]
                .get("next")
                .is_none()
        );
    }

    #[test]
    fn a_best_row_reads_through_the_page_that_assessed_its_window() {
        let best = json!({"t":[{"resourceId":"r","path":"/a.rs","exists":0.9,
            "startLine":745,"endLine":752,"probability":0.8}]});
        let page = |scope: (u64, u64), snapshot: &str| LocateRead {
            resource_id: "r".into(),
            path: Some("/a.rs".into()),
            scope: Some(scope),
            read: json!({"tool":"localFetch","query":{"path":"/a.rs","snapshot":snapshot}}),
        };
        let reads = [page((1, 468), "first"), page((469, 900), "second")];
        let visible = with_row_reads(best, &reads);
        assert_eq!(
            visible["t"][0]["next"]["read"]["query"]["snapshot"],
            "second"
        );
    }

    #[test]
    fn only_answering_located_pages_outside_best_keep_their_read() {
        let read = |line: u64| json!({"tool":"localFetch","query":{"path":"/a.rs","startLine":line,"endLine":line + 7}});
        let page = |exists: f64, line: u64| {
            json!({"answers":{"t":{"exists":exists,"matches":[
                {"startLine":line,"endLine":line + 7,"probability":0.8}
            ]}},"next":{"read":read(line)}})
        };
        let best = json!({"t":[{"resourceId":"r","exists":0.98,"startLine":43,"endLine":50,
            "probability":0.8,"next":{"read":read(43)}}]});
        let judged = json!({"answers":{"n":{"noul":0.1}},"next":{"read":read(1)}});
        let mut resources = vec![json!({"resourceId":"r","pages":[
            page(0.98, 43), page(0.08, 745), page(0.7, 1000), judged.clone()
        ]})];
        drop_redundant_page_reads(&mut resources, Some(&best));
        let pages = resources[0]["pages"].as_array().unwrap();
        assert!(pages[0].get("next").is_none(), "best already reads it");
        assert!(pages[1].get("next").is_none(), "a non-answer is no read");
        assert_eq!(pages[2]["next"]["read"], read(1000));
        assert_eq!(pages[3], judged, "non-locate pages keep their read");
        // The page keeps its window; only the executable read leaves.
        assert_eq!(pages[1]["answers"]["t"]["matches"][0]["startLine"], 745);
    }

    #[test]
    fn identifier_targets_get_a_literal_search_hint() {
        for target in [
            "Where is parse_cbor_internal defined?",
            "The body of Engine::execute.",
            "What does applyEdits() return?",
        ] {
            assert!(literal_target_hint(target).is_some(), "{target}");
        }
        for target in [
            "The periodic timer function that runs background housekeeping tasks.",
            "The maximum permitted retry count.",
            "The method that resolves a $type name into a CLR type.",
        ] {
            assert!(literal_target_hint(target).is_none(), "{target}");
        }
    }

    #[test]
    fn only_a_lone_identifier_token_is_a_bare_target() {
        for target in [
            "MAX_EXPANDED_CELLS",
            " `parse_cbor_internal` ",
            "Engine::execute",
            "applyEdits()",
        ] {
            assert!(bare_identifier(target).is_some(), "{target}");
        }
        for target in [
            "Where is MAX_EXPANDED_CELLS defined?",
            "retry",
            "The retry loop.",
            "a.b",
            "",
        ] {
            assert!(bare_identifier(target).is_none(), "{target}");
        }
        assert!(bare_target_hint("MAX_EXPANDED_CELLS").contains("skipped"));
    }

    fn local(tool: &str, path: &str) -> Value {
        json!({"id":path,"context":{"tool":tool,"query":{"path":path}}})
    }

    #[test]
    fn identifier_targets_get_an_executable_literal_search_over_local_resources() {
        let targets = ["The retry loop.", "Where is parse_cbor_internal defined?"];
        let one = [local("localFetch", "/repo/src/cbor.rs")];
        let next = literal_search(targets, &one).unwrap();
        assert_eq!(next["tool"], "localSearch");
        assert_eq!(next["query"]["path"], "/repo/src/cbor.rs");
        assert_eq!(next["query"]["searchText"], "parse_cbor_internal");
        assert_eq!(next["query"]["regex"], "literal");
        assert!(next["query"].get("page").is_none(), "compact: {next}");
        let mut replay = next["query"].clone();
        replay["goal"] = json!("Find the parser.");
        replay["reasoning"] = json!("Literal target.");
        crate::contracts::validate_query("localSearch", replay).unwrap();
        // Several local resources search their deepest shared directory.
        let many = [
            local("localFetch", "/repo/src/a/cbor.rs"),
            local("localSearch", "/repo/src/b"),
            local("localFetch", "/repo/src/a/cbor.rs"),
        ];
        assert_eq!(
            literal_search(targets, &many).unwrap()["query"]["path"],
            "/repo/src"
        );
        // No shared directory below the root, a remote resource, or no
        // identifier target: the string hint stands alone.
        let apart = [
            local("localFetch", "/a/x.rs"),
            local("localFetch", "/b/y.rs"),
        ];
        assert!(literal_search(targets, &apart).is_none());
        let remote = [
            local("localFetch", "/repo/a.rs"),
            json!({"id":"g","context":{"tool":"ghGetFileContent","query":{"owner":"o","repo":"r","path":"a.rs"}}}),
        ];
        assert!(literal_search(targets, &remote).is_none());
        assert!(literal_search(["The retry loop."], &one).is_none());
    }
}
