//! The provider requests one page of candidates becomes. A hit is a question,
//! not a request: the hits of one file share one state holding the union of
//! their windows, each hit's own line included, and the question text every
//! target shares; every question names its target line, so a state is sent
//! once and a window informs its hit without passing it. A target whose own
//! line is missing from the state is judged from its label alone, which is
//! what a hit that lists no window (a clasify hit whose query asked no
//! context) is meant to be. The provider judges the questions over one state
//! independently and bills the state once
//! (docs.typesafe.ai/cookbooks/parallel_questions).
//! A candidate with no target is a whole unit (a row, path, entry or chunk).
//! Whole units of a page share requests too: a state of their listed ids and
//! compact states, one question per predicate and unit naming its unit by id
//! and identifying text. Either group closes at the provider's windows (caps
//! derived from its stated tokens, never a literal; the provider sets no limit
//! on questions per request), a unit over them goes alone, units never share a
//! request with hits, and identical ones share an entry. A request holding one
//! unit is that unit's own state and question.
//!
//! The selection judge and clasify's units ([`super::units`]) pack alike;
//! what differs is how each asks ([`Asking`]).
use super::{compile, provider_state, section_question, target_question};
use crate::selection::JudgeCall;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};

/// How a caller asks its questions of the units it packs.
pub(super) struct Asking<'a> {
    /// One unit's questions, one per predicate or caller question: about its
    /// own state (`None`), or about the target or unit a label names.
    pub(super) ask: &'a dyn Fn(Option<&str>) -> Option<Vec<Value>>,
    /// The scope a hit group's shared state carries as its `question`.
    pub(super) hit_scope: &'a Value,
    /// The scope a unit group's shared state carries as its `question`.
    pub(super) unit_scope: &'a Value,
    /// Research context, sent once beside each request's evidence.
    pub(super) context: Option<&'a str>,
    /// Whether a hit's window rides in its own question instead of the
    /// file's shared state: each question then shows exactly what the hit's
    /// own request would (clasify's units). The selection judge's hits share
    /// the union of their windows, as they always have.
    pub(super) own_windows: bool,
    /// The question text every unit of a packed request is asked, stated
    /// once in its state (`questions`), each question naming its entry
    /// instead of repeating it (clasify's units). The selection judge's
    /// questions carry their own text, as they always have.
    pub(super) shared: Option<&'a Value>,
}

/// The field of a packed request's state that states the shared question
/// text once ([`Asking::shared`]).
pub(super) const SHARED_QUESTIONS: &str = "questions";

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
/// ([`crate::judgment::Evaluator::fit`]).
#[derive(Clone, Copy)]
pub(super) struct Caps {
    /// The state and the largest question: one question's window.
    pub(super) question_bytes: usize,
    /// The state and every question: the whole request's window.
    pub(super) request_bytes: usize,
    /// Whole units one request holds at most.
    pub(super) group_units: usize,
    /// Whether a hit group closes at the two windows too, not only at
    /// [`GROUP_STATE_CHARS`]: clasify's units, whose plan prices the requests
    /// before any is sent. The selection judge's hit groups leave the windows
    /// to the provider's refusal, as they always have.
    pub(super) hit_windows: bool,
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
    /// Bytes of the state's lines, of every question, and of the largest one.
    line_bytes: usize,
    question_bytes: usize,
    largest_question: usize,
}

/// What one hit would add to a group: its window, its state's characters,
/// the bytes of its lines the group lacks, and its questions' bytes (none
/// when the group already asks about its target).
struct HitSize<'a> {
    path: &'a str,
    lines: &'a [(u64, &'a str)],
    chars: usize,
    line_bytes: usize,
    questions: &'a [usize],
}

impl Group {
    /// Whether the hit fits beside the group's: the same file, lines that
    /// agree, the windows' characters under [`GROUP_STATE_CHARS`], and (when
    /// the caps ask) the state with the largest question (one question's
    /// window) and with every question (the request's) under them. An empty
    /// group takes any hit: one over the windows goes alone and the
    /// provider's own refusal decides.
    fn accepts(&self, hit: &HitSize<'_>, fixed: usize, caps: Caps) -> bool {
        let state = fixed
            .saturating_add(self.line_bytes)
            .saturating_add(hit.line_bytes);
        let largest = hit.questions.iter().copied().max().unwrap_or(0);
        self.slots.is_empty()
            || (self.path.as_deref() == Some(hit.path)
                && self.chars.saturating_add(hit.chars) <= GROUP_STATE_CHARS
                && hit
                    .lines
                    .iter()
                    .all(|(line, text)| self.lines.get(line).is_none_or(|known| known == text))
                && (!caps.hit_windows
                    || (state.saturating_add(self.largest_question.max(largest))
                        <= caps.question_bytes
                        && state
                            .saturating_add(self.question_bytes)
                            .saturating_add(hit.questions.iter().sum::<usize>())
                            <= caps.request_bytes)))
    }

    /// The bytes `lines` add to the group's state: those it lacks.
    fn new_line_bytes(&self, lines: &[(u64, &str)]) -> usize {
        lines
            .iter()
            .filter(|(line, _)| !self.lines.contains_key(line))
            .map(|(line, text)| line_bytes(*line, text))
            .sum()
    }
}

/// A numbered line's bytes in a shared state's `lines`: `<n>\t<text>` and
/// its newline, as JSON escapes them.
fn line_bytes(line: u64, text: &str) -> usize {
    bytes_of(&json!(format!("{line}\t{text}\n"))).saturating_sub(2)
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

/// What a question adds to a request besides itself: its key in the
/// request's `questions` (`"answer_123":`) and the comma after it.
const QUESTION_KEY_BYTES: usize = 16;

/// A question's bytes in a request: itself and its key.
fn question_bytes(question: &Value) -> usize {
    bytes_of(question).saturating_add(QUESTION_KEY_BYTES)
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

/// `label` with the lines of its hit's own window beside it, one numbered
/// line each, when the window shows more than the hit's own lines.
fn with_window(label: String, lines: &[(u64, &str)], target: &Value) -> String {
    let first = target["line"].as_u64().unwrap_or(0);
    let last = target["lastLine"].as_u64().unwrap_or(first);
    if lines.iter().all(|(line, _)| (first..=last).contains(line)) {
        return label;
    }
    let window = lines
        .iter()
        .map(|(line, text)| format!("{line}\t{text}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!("{label}\n{window}")
}

/// A hit's window: the state's numbered `lines`. A hit that lists none (a
/// clasify hit whose query asked no context) brings no line to the shared
/// state: its question's label quotes its text, which is all its own request
/// would show of it, so the other hits' lines never inform it (measured on
/// flowD1: with every hit line in the state, 6 of 63 lines near the deciding
/// ones crossed 0.5 that their own requests had judged no).
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

/// Every admitted candidate as a request, as the selection judge asks: the
/// template's question over each classifier. `None` when the template cannot
/// compile a question.
pub(super) fn requests(call: &JudgeCall<'_>, caps: Caps) -> Option<Vec<Request>> {
    let ask = |target: Option<&str>| {
        call.predicates
            .iter()
            .map(|classifier| compile(classifier, target))
            .collect::<Option<Vec<_>>>()
    };
    let asking = Asking {
        ask: &ask,
        hit_scope: target_question()?,
        unit_scope: section_question()?,
        context: call.research_context,
        own_windows: false,
        shared: None,
    };
    let units = call
        .admitted
        .iter()
        .filter_map(|&ordinal| Some((ordinal, call.candidates[ordinal].state.as_ref()?)));
    pack(units, call.predicates.len(), &asking, caps)
}

/// Every unit (its ordinal and the state the judge reads) as a request, each
/// unit owning `per_unit` questions from its slot's first. `None` when a
/// question cannot be asked.
pub(super) fn pack<'v>(
    units: impl IntoIterator<Item = (usize, &'v Value)>,
    per_unit: usize,
    asking: &Asking<'_>,
    caps: Caps,
) -> Option<Vec<Request>> {
    let mut requests: Vec<Request> = Vec::new();
    // A whole unit's compact state: where its entry is, so an identical one
    // reads that entry's answers (the open group's request is the next index).
    let mut wholes = HashMap::<String, (usize, usize)>::new();
    let mut group = Group::default();
    let mut units_group = UnitGroup::default();
    let context = asking.context;
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
        let mut state = json!({"path":group.path, "question":asking.hit_scope});
        if let Some(shared) = asking.shared {
            state[SHARED_QUESTIONS] = shared.clone();
        }
        if !lines.is_empty() {
            state["lines"] = json!(lines);
        }
        requests.push(Request {
            state: with_context(state, context),
            questions: group.questions,
            slots: group.slots,
        });
    };
    // The fixed part of a group's state: its path, question and context.
    // The shared question text, when stated once, is part of every state.
    let with_shared = |mut state: Value| {
        if let Some(shared) = asking.shared {
            state[SHARED_QUESTIONS] = shared.clone();
        }
        state
    };
    let hit_fixed = |path: &str| {
        bytes_of(&with_shared(
            json!({"path": path, "question": asking.hit_scope, "lines": "", "context": context}),
        ))
    };
    let unit_fixed = bytes_of(&with_shared(
        json!({"question": asking.unit_scope, "units": "", "context": context}),
    ));
    for (ordinal, state) in units {
        let Some(target) = state.get("target") else {
            let compact = provider_state(state).into_owned();
            let identity = compact.to_string();
            if let Some(&(at, first)) = wholes.get(&identity) {
                if at == requests.len() {
                    units_group.slots.push((ordinal, first));
                } else {
                    requests[at].slots.push((ordinal, first));
                }
                continue;
            }
            flush(&mut group, &mut requests);
            let name = unit_identity(&compact);
            // The unit's questions at the id it would take in this group.
            let ask = |id: usize| (asking.ask)(Some(&unit_label(id, &name)));
            let line = bytes_of(&json!(unit_line(units_group.units.len() + 1, &compact)));
            let mut sizes = ask(units_group.units.len() + 1)?
                .iter()
                .map(question_bytes)
                .collect::<Vec<_>>();
            if !units_group.accepts(line, &sizes, unit_fixed, caps) {
                flush_units(&mut units_group, &mut requests, asking)?;
                sizes = ask(1)?.iter().map(question_bytes).collect();
            }
            let at = units_group.units.len();
            let line = bytes_of(&json!(unit_line(at + 1, &compact)));
            units_group.push(
                GroupUnit {
                    state: compact,
                    identity: name,
                },
                line,
                &sizes,
            );
            units_group.slots.push((ordinal, at * per_unit));
            wholes.insert(identity, (requests.len(), at * per_unit));
            continue;
        };
        flush_units(&mut units_group, &mut requests, asking)?;
        let path = state["path"].as_str().unwrap_or_default();
        let own = window(state);
        // A hit whose window rides in its question brings no line to the
        // shared state.
        let (lines, label) = if asking.own_windows {
            (Vec::new(), with_window(target_label(target), &own, target))
        } else {
            (own, target_label(target))
        };
        let questions = (asking.ask)(Some(&label))?;
        let sizes = questions.iter().map(question_bytes).collect::<Vec<_>>();
        let known = group.path.as_deref() == Some(path) && group.targets.contains_key(&label);
        let hit = HitSize {
            path,
            lines: &lines,
            chars: crate::selection::serialized_chars(state),
            line_bytes: group.new_line_bytes(&lines),
            questions: if known { &[] } else { &sizes },
        };
        if !group.accepts(&hit, hit_fixed(path), caps) {
            flush(&mut group, &mut requests);
        }
        group.path = Some(path.to_owned());
        group.chars = group.chars.saturating_add(hit.chars);
        group.line_bytes = group
            .line_bytes
            .saturating_add(group.new_line_bytes(&lines));
        for (line, text) in lines {
            group.lines.entry(line).or_insert_with(|| text.to_owned());
        }
        let first = match group.targets.get(&label) {
            Some(&first) => first,
            None => {
                let first = group.questions.len();
                group.question_bytes = group
                    .question_bytes
                    .saturating_add(sizes.iter().sum::<usize>());
                group.largest_question = group
                    .largest_question
                    .max(sizes.iter().copied().max().unwrap_or(0));
                group.questions.extend(questions);
                group.targets.insert(label, first);
                first
            }
        };
        group.slots.push((ordinal, first));
    }
    flush(&mut group, &mut requests);
    flush_units(&mut units_group, &mut requests, asking)?;
    Some(requests)
}

/// The group as one request: a lone unit as its own state and question, as
/// a page of one has always been judged; several as the shared state.
fn flush_units(
    group: &mut UnitGroup,
    requests: &mut Vec<Request>,
    asking: &Asking<'_>,
) -> Option<()> {
    if group.units.is_empty() {
        return Some(());
    }
    let group = std::mem::take(group);
    let request = if let [only] = group.units.as_slice() {
        Request {
            state: with_context(only.state.clone(), asking.context),
            questions: (asking.ask)(None)?,
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
            questions.extend((asking.ask)(Some(&unit_label(id, &unit.identity)))?);
        }
        let mut state = json!({"question": asking.unit_scope, "units": lines.join("\n")});
        if let Some(shared) = asking.shared {
            state[SHARED_QUESTIONS] = shared.clone();
        }
        Request {
            state: with_context(state, asking.context),
            questions,
            slots: group.slots,
        }
    };
    requests.push(request);
    Some(())
}
