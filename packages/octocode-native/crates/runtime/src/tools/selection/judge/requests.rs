//! The provider requests one page of candidates becomes. A hit is a question,
//! not a request: the hits of one file share one state holding their windows
//! and the question text every target shares, and every question names its
//! target line, so a state is sent once and a window informs its hit without
//! passing it. The provider judges the questions over one state independently
//! and bills the state once (docs.typesafe.ai/cookbooks/parallel_questions).
//! A candidate with no target is a whole unit: its own state, one question per
//! predicate; identical ones share a request.
use super::{compile, provider_state, target_question};
use crate::tools::selection::JudgeCall;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// Characters of the windows one request's state holds before the next hit
/// starts another: at the worst measured 1.3 characters a token, under the
/// provider's 32k-token per-question window. Only a batching size: a hit over
/// it goes alone and the provider's own refusal decides.
const GROUP_STATE_CHARS: usize = 32_000;

/// One provider request and which candidates read which of its answers.
pub(super) struct Request {
    pub(super) state: Value,
    pub(super) questions: Vec<Value>,
    /// A candidate's ordinal and its first question; it owns one question per
    /// predicate from there.
    pub(super) slots: Vec<(usize, usize)>,
}

/// The hits of one file being gathered into one request.
#[derive(Default)]
struct Group {
    path: Option<String>,
    lines: BTreeMap<u64, String>,
    /// Lines the state sends: each hit's window but the line its own label
    /// quotes, which its question already carries.
    sent: BTreeSet<u64>,
    chars: usize,
    targets: HashMap<String, usize>,
    questions: Vec<Value>,
    slots: Vec<(usize, usize)>,
}

impl Group {
    fn accepts(&self, path: &str, lines: &[(u64, &str)], chars: usize) -> bool {
        self.slots.is_empty()
            || (self.path.as_deref() == Some(path)
                && self.chars.saturating_add(chars) <= GROUP_STATE_CHARS
                && lines
                    .iter()
                    .all(|(line, text)| self.lines.get(line).is_none_or(|known| known == text)))
    }
}

/// `line 14: <its text>`, `lines 14-16: <its text>`, then the declaration
/// around the hit. The text is quoted beside the number: with adjacent hits and
/// no context, a number alone let neighbours bleed (9 of 20 false positives on
/// the frozen fixture, none when quoted, for about 8% more tokens).
fn target_label(target: &Value) -> String {
    let line = &target["line"];
    let mut label = match target.get("lastLine") {
        Some(last) => format!("lines {line}-{last}"),
        None => format!("line {line}"),
    };
    if let Some(text) = target["match"].as_str().or(target["text"].as_str()) {
        label.push_str(&format!(": {}", text.trim()));
    }
    if let Some(symbol) = target["enclosing"]["symbolName"].as_str() {
        label.push_str(&format!(
            " (in {} {symbol})",
            target["enclosing"]["kind"]
                .as_str()
                .unwrap_or("declaration")
        ));
    }
    label
}

/// The line a hit's label quotes whole: its own line, unless the label quotes
/// a match span instead.
fn quoted_line(target: &Value, lines: &[(u64, &str)]) -> Option<u64> {
    if target.get("match").is_some() {
        return None;
    }
    let line = target["line"].as_u64()?;
    let text = target["text"].as_str()?;
    lines
        .iter()
        .any(|&(at, window)| at == line && window == text)
        .then_some(line)
}

fn window(state: &Value) -> Vec<(u64, &str)> {
    state["lines"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|row| Some((row[0].as_u64()?, row[1].as_str()?)))
        .collect()
}

/// The state a request sends: research context once beside the evidence.
fn with_context(mut state: Value, context: Option<&str>) -> Value {
    if let (Some(context), Some(fields)) = (context, state.as_object_mut()) {
        fields.insert("context".into(), json!(context));
    }
    state
}

/// Every admitted candidate as a request. `None` when the template cannot
/// compile a question.
pub(super) fn requests(call: &JudgeCall<'_>) -> Option<Vec<Request>> {
    let mut requests = Vec::new();
    let mut wholes = HashMap::<String, usize>::new();
    let mut group = Group::default();
    let context = call.research_context;
    let question = target_question()?;
    let flush = |group: &mut Group, requests: &mut Vec<Request>| {
        if group.slots.is_empty() {
            return;
        }
        let group = std::mem::take(group);
        let lines = group
            .lines
            .iter()
            .filter(|(line, _)| group.sent.contains(line))
            .map(|(line, text)| format!("{line}\t{text}"))
            .collect::<Vec<_>>()
            .join("\n");
        let mut state = json!({"path":group.path, "question":question});
        if !lines.is_empty() {
            state["lines"] = json!(lines);
        }
        requests.push(Request {
            state: with_context(state, context),
            questions: group.questions,
            slots: group.slots,
        });
    };
    for &ordinal in call.admitted {
        let Some(state) = call.candidates[ordinal].state.as_ref() else {
            continue;
        };
        let Some(target) = state.get("target") else {
            let compact = provider_state(state);
            let identity = compact.to_string();
            if let Some(&at) = wholes.get(&identity) {
                let request: &mut Request = &mut requests[at];
                request.slots.push((ordinal, 0));
                continue;
            }
            flush(&mut group, &mut requests);
            wholes.insert(identity, requests.len());
            requests.push(Request {
                state: with_context(compact.into_owned(), context),
                questions: call
                    .predicates
                    .iter()
                    .map(|classifier| compile(classifier, None))
                    .collect::<Option<_>>()?,
                slots: vec![(ordinal, 0)],
            });
            continue;
        };
        let path = state["path"].as_str().unwrap_or_default();
        let lines = window(state);
        let chars = call.candidates[ordinal].state_chars();
        if !group.accepts(path, &lines, chars) {
            flush(&mut group, &mut requests);
        }
        group.path = Some(path.to_owned());
        group.chars = group.chars.saturating_add(chars);
        let quoted = quoted_line(target, &lines);
        for (line, text) in lines {
            group.lines.entry(line).or_insert_with(|| text.to_owned());
            if quoted != Some(line) {
                group.sent.insert(line);
            }
        }
        let label = target_label(target);
        let first = match group.targets.get(&label) {
            Some(&first) => first,
            None => {
                let first = group.questions.len();
                for classifier in call.predicates {
                    group.questions.push(compile(classifier, Some(&label))?);
                }
                group.targets.insert(label, first);
                first
            }
        };
        group.slots.push((ordinal, first));
    }
    flush(&mut group, &mut requests);
    Some(requests)
}
