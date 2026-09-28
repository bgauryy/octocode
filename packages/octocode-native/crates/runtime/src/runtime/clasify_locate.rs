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
    let matches = groups
        .iter()
        .take(kept)
        .enumerate()
        .filter(|(rank, group)| {
            *rank == 0 || (group.probability > 0.0 && group.probability >= top * RUNNER_UP_SHARE)
        })
        .map(|(_, group)| {
            let (start_line, end_line) = window(page, group);
            json!({
                "startLine":start_line,
                "endLine":end_line,
                "probability":rounded(group.probability)
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
/// only ranks passages within a page that answers). Present only when a
/// question has more than one candidate window.
pub(super) fn rank_locate(
    resources: &[Value],
    locate_ids: &[&str],
    carry: Option<&Value>,
) -> Option<Value> {
    const KEPT: usize = 3;
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
            .filter(|row| is_candidate_row(row))
            .cloned()
            .collect::<Vec<_>>();
        for resource in resources {
            for page in resource["pages"].as_array().into_iter().flatten() {
                let answer = &page["answers"][*id];
                let Some(exists) = answer["exists"].as_f64() else {
                    continue;
                };
                // resourceId maps to the page's source.path; rows stay small.
                for window in answer["matches"].as_array().into_iter().flatten() {
                    rows.push(json!({
                        "resourceId":resource["resourceId"],
                        "exists":exists,
                        "startLine":window["startLine"],
                        "endLine":window["endLine"],
                        "probability":window["probability"]
                    }));
                }
            }
        }
        if rows.len() < 2 {
            continue;
        }
        let key = |row: &Value, field: &str| row[field].as_f64().unwrap_or(0.0);
        rows.sort_by(|left, right| {
            key(right, "exists")
                .partial_cmp(&key(left, "exists"))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    key(right, "probability")
                        .partial_cmp(&key(left, "probability"))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        });
        rows.truncate(KEPT);
        best.insert((*id).to_owned(), Value::Array(rows));
    }
    (!best.is_empty()).then_some(Value::Object(best))
}

fn is_candidate_row(row: &Value) -> bool {
    let probability = |field: &str| {
        row[field]
            .as_f64()
            .is_some_and(|p| (0.0..=1.0).contains(&p))
    };
    let line = |field: &str| row[field].as_u64().is_some_and(|line| line >= 1);
    row["resourceId"].is_string()
        && probability("exists")
        && probability("probability")
        && line("startLine")
        && line("endLine")
        && row.as_object().is_some_and(|object| object.len() == 5)
}

/// A target that names a code identifier is usually cheaper and exact with
/// localSearch; locate earns its cost on described behavior.
pub(super) fn literal_target_hint(target: &str) -> Option<String> {
    let identifier = target
        .split(|c: char| c.is_whitespace() || matches!(c, ',' | ';' | '"' | '\'' | '`' | '?' | '!'))
        .map(|token| token.trim_end_matches(['.', ')']).trim_end_matches('('))
        .find(|token| looks_like_identifier(token))?;
    Some(format!(
        "Target names `{identifier}`; if that literal is what you need, localSearch finds it exactly and cheaper than locate."
    ))
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
        let single = vec![json!({"resourceId":"r","pages":[page("a.js", 0.9, 0.9, 1)]})];
        assert!(rank_locate(&single, &["t"], None).is_none());
        // A carried row from an earlier call joins the ranking; a malformed
        // one is ignored.
        let carry = json!({"t":[
            {"resourceId":"r","exists":0.99,"startLine":5,"endLine":12,"probability":0.9},
            {"resourceId":"r","exists":2.0,"startLine":0,"endLine":1,"probability":0.9}
        ]});
        let merged = rank_locate(&single, &["t"], Some(&carry)).unwrap();
        assert_eq!(merged["t"][0]["startLine"], 5);
        assert_eq!(merged["t"].as_array().unwrap().len(), 2);
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
}
