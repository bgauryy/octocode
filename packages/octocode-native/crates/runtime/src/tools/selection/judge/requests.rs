//! The provider requests one page of candidates becomes. A hit is a question,
//! not a request: the hits of one file share one state holding the union of
//! their windows, each hit's own line included, and the question text every
//! target shares; every question names its target line, so a state is sent
//! once and a window informs its hit without passing it. A target whose own
//! line is missing from the state would be judged from its label alone. The provider judges the questions over one state independently
//! and bills the state once (docs.typesafe.ai/cookbooks/parallel_questions).
//! A candidate with no target is a whole unit (a row, path, entry or chunk).
//! Whole units of a page share requests too: a state of their listed ids and
//! compact states, one question per predicate and unit naming its unit by id
//! and identifying text. The group closes at the provider's windows (caps
//! derived from its stated tokens, never a literal), a unit over them goes
//! alone, units never share a request with hits, and identical ones share an
//! entry. A request holding one unit is that unit's own state and question.
use super::{compile, provider_state, section_question, target_question};
use crate::tools::selection::JudgeCall;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};

/// Characters of the windows one request's state holds before the next hit
/// starts another: at the worst measured 1.3 characters a token, under the
/// provider's 32k-token per-question window. Only a batching size: a hit over
/// it goes alone and the provider's own refusal decides.
const GROUP_STATE_CHARS: usize = 32_000;

/// Whole units one request holds at most, whatever room the provider's windows
/// leave; the window cap only closes a request sooner. One: the live A/B
/// (fx-k2size) kept different sets for fuzzy predicates at 4 and at 8 units
/// per request than one request per unit judges (Jaccard 14-41% and 3-14% on a
/// 40-path page against the per-unit run's own 100%, with 17-28 of 40 units
/// left unknown against 0-1), however the units were separated, so a request
/// holds one until a larger size matches. Part of the judge's identity, so a
/// plan judged at another size is not replayed.
pub(super) const GROUP_UNITS: usize = 1;

/// What one request may hold, in serialized bytes, derived from the
/// provider's stated windows at the density its answers taught
/// ([`crate::tools::judgment::Evaluator::fit`]).
#[derive(Clone, Copy)]
pub(super) struct Caps {
    /// The state and the largest question: one question's window.
    pub(super) question_bytes: usize,
    /// The state and every question: the whole request's window.
    pub(super) request_bytes: usize,
    /// Whole units one request holds at most.
    pub(super) group_units: usize,
}

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
    /// Every line of every hit's window, its own line included.
    lines: BTreeMap<u64, String>,
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
/// around the hit and the key path of its line in a keyed file, so a hit
/// whose window lacks the deciding declaration or section still names it. The text is quoted beside the number: with adjacent hits and
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
    if let Some(key) = target["key"].as_str() {
        label.push_str(&format!(" (key {key})"));
    }
    label
}

/// What names a unit in its question beside its id: the path (with the
/// source lines of a chunk), else the title, else the kind of discussion item
/// and its author. Quoted beside the id, as a hit's text is beside its line.
/// One line, so a label never carries a state's structure.
fn unit_identity(state: &Value) -> String {
    let text = |key: &str| {
        state[key]
            .as_str()
            .and_then(|text| text.lines().map(str::trim).find(|line| !line.is_empty()))
    };
    let mut identity = if let Some(path) = text("path") {
        path.to_owned()
    } else if let Some(title) = text("title") {
        title.to_owned()
    } else {
        match (text("kind"), text("author")) {
            (Some(kind), Some(author)) => format!("{kind} by {author}"),
            (Some(only), None) | (None, Some(only)) => only.to_owned(),
            (None, None) => String::new(),
        }
    };
    let ranges = state["sourceLineRanges"].as_array();
    let bounds = |at: usize| {
        ranges
            .into_iter()
            .flatten()
            .filter_map(move |range| range[at].as_u64())
            .filter(|line| *line > 0)
    };
    if let (Some(first), Some(last)) = (bounds(0).min(), bounds(1).max()) {
        identity.push_str(&format!(" lines {first}-{last}"));
    }
    identity
}

fn bytes_of(value: &Value) -> usize {
    serde_json::to_vec(value).map_or(0, |bytes| bytes.len())
}

/// One whole unit of a group.
struct GroupUnit {
    /// The state as the provider takes it.
    state: Value,
    identity: String,
}

/// The whole units of one page being gathered into one request.
#[derive(Default)]
struct UnitGroup {
    units: Vec<GroupUnit>,
    /// Bytes of the units' lines in the shared state, of every question, and
    /// of the largest one.
    line_bytes: usize,
    question_bytes: usize,
    largest_question: usize,
    /// A candidate's ordinal and its unit's index in the group.
    slots: Vec<(usize, usize)>,
}

/// `unit 3: <identifying text>` (or `unit 3`), the label a unit's questions
/// carry; its line in the shared state opens with the same `unit 3` token, so
/// a question and the text it is about match word for word.
fn unit_label(id: usize, identity: &str) -> String {
    if identity.is_empty() {
        format!("unit {id}")
    } else {
        format!("unit {id}: {identity}")
    }
}

/// A unit's line in the shared state: its `unit 3` name, a tab, its compact
/// state. One line (a state's newlines are escaped), so lines never mix.
fn unit_line(id: usize, state: &Value) -> String {
    format!("unit {id}\t{state}")
}

impl UnitGroup {
    /// Whether a unit of these sizes fits the windows beside the group's:
    /// the state's fixed part and every unit's line, then the largest
    /// question beside them (one question's window) and every question (the
    /// request's). An empty group takes any unit: one over the windows goes
    /// alone and the provider's own refusal decides.
    fn accepts(&self, line: usize, questions: &[usize], fixed: usize, caps: Caps) -> bool {
        let state = fixed.saturating_add(self.line_bytes).saturating_add(line);
        let largest = questions.iter().copied().max().unwrap_or(0);
        self.units.is_empty()
            || (self.units.len() < caps.group_units
                && state.saturating_add(self.largest_question.max(largest)) <= caps.question_bytes
                && state
                    .saturating_add(self.question_bytes)
                    .saturating_add(questions.iter().sum::<usize>())
                    <= caps.request_bytes)
    }

    fn push(&mut self, unit: GroupUnit, line: usize, questions: &[usize]) {
        self.line_bytes = self.line_bytes.saturating_add(line);
        self.question_bytes = self
            .question_bytes
            .saturating_add(questions.iter().sum::<usize>());
        self.largest_question = self
            .largest_question
            .max(questions.iter().copied().max().unwrap_or(0));
        self.units.push(unit);
    }
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
pub(super) fn requests(call: &JudgeCall<'_>, caps: Caps) -> Option<Vec<Request>> {
    let mut requests: Vec<Request> = Vec::new();
    // A whole unit's compact state: where its entry is, so an identical one
    // reads that entry's answers (the open group's request is the next index).
    let mut wholes = HashMap::<String, (usize, usize)>::new();
    let mut group = Group::default();
    let mut units = UnitGroup::default();
    let context = call.research_context;
    let question = target_question()?;
    let predicates = call.predicates.len();
    let flush = |group: &mut Group, requests: &mut Vec<Request>| {
        if group.slots.is_empty() {
            return;
        }
        let group = std::mem::take(group);
        let lines = group
            .lines
            .iter()
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
    // The fixed part of a unit group's state: its question and context.
    let fixed = bytes_of(&json!({"question": section_question(), "units": "", "context": context}));
    for &ordinal in call.admitted {
        let Some(state) = call.candidates[ordinal].state.as_ref() else {
            continue;
        };
        let Some(target) = state.get("target") else {
            let compact = provider_state(state).into_owned();
            let identity = compact.to_string();
            if let Some(&(at, first)) = wholes.get(&identity) {
                if at == requests.len() {
                    units.slots.push((ordinal, first));
                } else {
                    requests[at].slots.push((ordinal, first));
                }
                continue;
            }
            flush(&mut group, &mut requests);
            let name = unit_identity(&compact);
            // The unit's questions at the id it would take in this group.
            let ask = |id: usize| -> Option<Vec<Value>> {
                let label = unit_label(id, &name);
                call.predicates
                    .iter()
                    .map(|classifier| compile(classifier, Some(&label)))
                    .collect()
            };
            let line = bytes_of(&json!(unit_line(units.units.len() + 1, &compact)));
            let mut sizes = ask(units.units.len() + 1)?
                .iter()
                .map(bytes_of)
                .collect::<Vec<_>>();
            if !units.accepts(line, &sizes, fixed, caps) {
                flush_units(&mut units, &mut requests, context, call)?;
                sizes = ask(1)?.iter().map(bytes_of).collect();
            }
            let at = units.units.len();
            let line = bytes_of(&json!(unit_line(at + 1, &compact)));
            units.push(
                GroupUnit {
                    state: compact,
                    identity: name,
                },
                line,
                &sizes,
            );
            units.slots.push((ordinal, at * predicates));
            wholes.insert(identity, (requests.len(), at * predicates));
            continue;
        };
        flush_units(&mut units, &mut requests, context, call)?;
        let path = state["path"].as_str().unwrap_or_default();
        let lines = window(state);
        let chars = call.candidates[ordinal].state_chars();
        if !group.accepts(path, &lines, chars) {
            flush(&mut group, &mut requests);
        }
        group.path = Some(path.to_owned());
        group.chars = group.chars.saturating_add(chars);
        for (line, text) in lines {
            group.lines.entry(line).or_insert_with(|| text.to_owned());
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
    flush_units(&mut units, &mut requests, context, call)?;
    Some(requests)
}

/// The group as one request: a lone unit as its own state and question, as
/// a page of one has always been judged; several as the shared state.
fn flush_units(
    group: &mut UnitGroup,
    requests: &mut Vec<Request>,
    context: Option<&str>,
    call: &JudgeCall<'_>,
) -> Option<()> {
    if group.units.is_empty() {
        return Some(());
    }
    let group = std::mem::take(group);
    let request = if let [only] = group.units.as_slice() {
        Request {
            state: with_context(only.state.clone(), context),
            questions: call
                .predicates
                .iter()
                .map(|classifier| compile(classifier, None))
                .collect::<Option<_>>()?,
            slots: group
                .slots
                .iter()
                .map(|&(ordinal, _)| (ordinal, 0))
                .collect(),
        }
    } else {
        let mut questions = Vec::new();
        let mut lines = Vec::new();
        for (index, unit) in group.units.iter().enumerate() {
            let id = index + 1;
            lines.push(unit_line(id, &unit.state));
            let label = unit_label(id, &unit.identity);
            for classifier in call.predicates {
                questions.push(compile(classifier, Some(&label))?);
            }
        }
        let state = json!({"question": section_question()?, "units": lines.join("\n")});
        Request {
            state: with_context(state, context),
            questions,
            slots: group.slots,
        }
    };
    requests.push(request);
    Some(())
}
