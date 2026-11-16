//! The rules of [`super::done_facts`] over recorded results: what every tool
//! returned for one call per operation and row state (hit, empty, error,
//! paged) on the runtime's provider doubles. Wave 0 of the one Octocode
//! protocol: each rule fails on a result its allowlist does not name and on
//! a listed one that now obeys, so the lists only shrink.
//!
//! `fixtures/recorded_results.jsonl` is written by the runtime's
//! `runtime_output_alignment` test, one `{tool, operation, state, input,
//! output}` line per call. Refresh it after a shape change, from
//! `packages/octocode-native`:
//!
//! ```sh
//! OCTOCODE_ALIGNMENT_DUMP=/tmp/alignment cargo dev-test -- runtime_output_alignment::
//! cp /tmp/alignment.outputs crates/cli/src/cli/lines/tests/fixtures/recorded_results.jsonl
//! ```
use super::done_facts::{commands, page_states, prints_alike_in_a_batch, roots};
use super::*;
use std::collections::BTreeSet;

pub(super) struct Recorded {
    /// `tool operation state`, with `#n` after a repeated one.
    pub(super) label: String,
    pub(super) tool: ToolId,
    pub(super) query: Value,
    pub(super) result: Value,
}

pub(super) fn recorded() -> Vec<Recorded> {
    let mut seen = BTreeSet::new();
    include_str!("fixtures/recorded_results.jsonl")
        .lines()
        .map(|line| {
            let call: Value = serde_json::from_str(line).expect("a recorded call");
            let field = |key: &str| call[key].as_str().expect("a recorded field").to_owned();
            let name = format!(
                "{} {} {}",
                field("tool"),
                field("operation"),
                field("state")
            );
            let label = (1..)
                .map(|n| match n {
                    1 => name.clone(),
                    n => format!("{name} #{n}"),
                })
                .find(|label| seen.insert(label.clone()))
                .expect("a free label");
            Recorded {
                label,
                tool: ToolId::from_name(&field("tool")).expect("a known tool"),
                query: call["input"].clone(),
                result: call["output"].clone(),
            }
        })
        .collect()
}

fn text(call: &Recorded) -> Lines {
    render(call.tool, &call.result, &call.query, None)
        .unwrap_or_else(|| panic!("{} prints text", call.label))
}

/// The labels of the calls `breaks` holds for, against `allowed`: both lists
/// empty when the rule holds everywhere but where the allowlist says.
fn drift(allowed: &[&str], breaks: impl Fn(&Recorded) -> bool) -> (Vec<String>, Vec<String>) {
    let found: Vec<String> = recorded()
        .into_iter()
        .filter(|call| breaks(call))
        .map(|call| call.label)
        .collect();
    let new = found
        .iter()
        .filter(|label| !allowed.contains(&label.as_str()))
        .cloned()
        .collect();
    let fixed = allowed
        .iter()
        .filter(|label| !found.iter().any(|found| found == *label))
        .map(|label| (*label).to_owned())
        .collect();
    (new, fixed)
}

fn assert_aligned(rule: &str, (new, fixed): (Vec<String>, Vec<String>)) {
    assert!(
        new.is_empty() && fixed.is_empty(),
        "{rule}\nnot allowlisted: {new:#?}\nallowlisted, now obeys (delete): {fixed:#?}"
    );
}

/// Removed by Wave 1 (non-breaking, text renderer): these rows print as
/// indented `key: value` text, which repeats each key on every item of a
/// list. The roast also named the history commit, the pull request and
/// issue comments: the recorded commit and pull request now fit, and no
/// recording holds issue comments.
const LARGER_THAN_JSON: &[&str] = &["securityCheck advisory hit"];

/// Text stdout is no larger than the JSON of the same result.
#[test]
fn recorded_text_is_no_larger_than_its_json() {
    let larger = |call: &Recorded| text(call).out.len() > call.result.to_string().len();
    assert_aligned("text larger than JSON", drift(LARGER_THAN_JSON, larger));
}

/// Whether stdout alone misses a fact that decides "am I done": the row's
/// status, a page state, or the command that continues the answer.
fn stdout_misses_a_fact(call: &Recorded) -> bool {
    let out = text(call).out;
    let rows = call.result["results"]
        .as_array()
        .map_or(&[][..], Vec::as_slice);
    let status_missing = rows
        .iter()
        .filter_map(|row| row["status"].as_str())
        .any(|status| !out.contains(&format!("# status: {status}")));
    let mut calls = Vec::new();
    commands(&call.result, &mut calls);
    let continuation_missing = calls.iter().filter(|(_, lead)| !lead).any(|(command, _)| {
        !out.lines()
            .any(|line| line.starts_with("# ") && line.ends_with(command.as_str()))
    });
    status_missing
        || continuation_missing
        || out.matches("hasMore").count() < page_states(&call.result)
}

/// Removed by Wave 1 (non-breaking, text renderer).
const STDOUT_MISSES_A_FACT: &[&str] = &[];

/// With stderr dropped, stdout states status, `hasMore` and a runnable
/// continuation.
#[test]
fn recorded_stdout_alone_says_whether_the_answer_is_whole() {
    assert_aligned(
        "stdout alone misses status, hasMore or a continuation",
        drift(STDOUT_MISSES_A_FACT, stdout_misses_a_fact),
    );
}

/// Every string under a `path` key and every number under a key that names a
/// line (`line`, `startLine`, `lines`, `matchedLines`), at any depth.
fn locators(value: &Value, key: &str, paths: &mut BTreeSet<String>, lines: &mut BTreeSet<u64>) {
    match value {
        Value::Object(fields) => {
            for (key, value) in fields {
                locators(value, key, paths, lines);
            }
        }
        Value::Array(items) => items
            .iter()
            .for_each(|item| locators(item, key, paths, lines)),
        Value::String(text) if key == "path" => {
            paths.insert(text.clone());
        }
        Value::Number(number) if matches!(key, "line" | "startLine" | "lines" | "matchedLines") => {
            lines.extend(number.as_u64());
        }
        _ => {}
    }
}

/// Whether a stdout line reads `path:N` where N is no line of the result.
fn a_number_after_a_path_is_no_line(call: &Recorded) -> bool {
    let (mut paths, mut lines) = (BTreeSet::new(), BTreeSet::new());
    locators(&call.result, "", &mut paths, &mut lines);
    let out = text(call).out;
    // A path printed relative to its row's root is checked as printed.
    for root in roots(&out) {
        let relative: Vec<String> = paths
            .iter()
            .filter_map(|path| path.strip_prefix(&format!("{root}/")).map(str::to_owned))
            .collect();
        paths.extend(relative);
    }
    out.lines().any(|line| {
        paths.iter().any(|path| {
            line.strip_prefix(path.as_str())
                .and_then(|rest| rest.strip_prefix(':'))
                .map(|rest| {
                    rest.chars()
                        .take_while(char::is_ascii_digit)
                        .collect::<String>()
                })
                .filter(|digits| !digits.is_empty())
                .is_some_and(|digits| !digits.parse().is_ok_and(|n: u64| lines.contains(&n)))
        })
    })
}

/// Removed by Wave 1 (non-breaking, text renderer).
const PATH_NUMBER_IS_NO_LINE: &[&str] = &[];

/// `path:N` names a line and nothing else.
#[test]
fn recorded_path_numbers_are_lines() {
    assert_aligned(
        "path:N where N is not a line",
        drift(PATH_NUMBER_IS_NO_LINE, a_number_after_a_path_is_no_line),
    );
}

/// Whether a row prints differently in a batch of itself, apart from its
/// heading, its name on its `# ` lines and the fields both rows share
/// ([`prints_alike_in_a_batch`]).
fn batch_differs(call: &Recorded) -> bool {
    let row = &call.result["results"][0];
    // A response page of its own belongs to the envelope, not to the row.
    if call.result.get("responsePagination").is_some() || row.is_null() {
        return false;
    }
    !prints_alike_in_a_batch(call.tool, row, &call.query["queries"][0])
}

/// Removed by Wave 1 (non-breaking, text renderer).
const BATCH_DIFFERS: &[&str] = &[];

/// A row prints alike alone and in a batch.
#[test]
fn recorded_rows_print_alike_alone_and_in_a_batch() {
    assert_aligned(
        "a row prints differently in a batch",
        drift(BATCH_DIFFERS, batch_differs),
    );
}

/// The fixture holds every tool: a tool without a recorded call is unchecked.
#[test]
fn every_tool_has_a_recorded_result() {
    let recorded: BTreeSet<&str> = recorded().iter().map(|call| call.tool.as_str()).collect();
    let tools: BTreeSet<&str> = ToolId::ALL.iter().map(|tool| tool.as_str()).collect();
    assert_eq!(recorded, tools);
}
