//! Where a whole-window answer sits. The provider answers typed values
//! (`noul`, `choice`, `score`, `box`) and never a span of the text it
//! judged, so a text judged whole in one request is sent with a marker line
//! before each of its parts, and every question that can name lines takes one
//! companion `choice` in the same request: which part holds the text that
//! decides it. Its label is a part; its lines are the answer's `lines`, and
//! the row's `next.read` reads only them.
//!
//! A part is about one page of the read that follows: as many parts as the
//! text's lines, counted as a read renders them (JSON text and `N\t`
//! gutter), fill line pages of the automatic response page
//! ([`RuntimeLimits::auto_page_chars`], `DEFAULT_RESPONSE_CHARS` unless
//! configured, less the row around its lines:
//! [`crate::response::page_windows::page_budget`]), at least two, at most
//! the labels one choice may list. Finer parts would cost the provider's output
//! a probability per label and still leave the agent one read.
//!
//! No request is added for it: a companion rides only in a grouped request
//! and is dropped when the provider's window splits the questions into
//! single requests ([`EvaluationInput::companions`]); then the answers name
//! no lines, as before. Its cost is the marker lines and each companion's
//! labels, in the same request's tokens.
//!
//! An answer names lines as a sectioned one does
//! ([`super::combined`]): a `noul` below 0.5 or an `unknown` `choice` names
//! none, and neither does an answer whose companion chose no part.
//!
//! [`EvaluationInput::companions`]: crate::tools::judgment::EvaluationInput::companions
//! [`RuntimeLimits::auto_page_chars`]: crate::execution::RuntimeLimits::auto_page_chars
use super::Target;
use super::combined::{YES, abstains, deciding_lines, lines_read};
use crate::tools::source_capture::Captured;
use serde_json::{Map, Value, json};

/// Labels one choice question may list: the provider's limit. One of them
/// is `unknown`; the rest bound the parts.
const LABELS: usize = crate::id::clasify_policy::CHOICE_LABELS;

/// The label a companion chooses when no part decides its question.
const NO_PART: &str = "unknown";

/// A text cut into parts for one request, and the companion questions that
/// point into it.
pub(super) struct Located {
    /// The state to send: the judged text with a marker line before each
    /// part.
    pub state: Value,
    /// One companion per question that can name lines, in question order.
    pub companions: Vec<Value>,
    /// Each part's first and last line of the judged text.
    parts: Vec<(usize, usize)>,
    /// The question index each companion serves.
    serves: Vec<usize>,
}

/// The parts of `target`'s judged `state` for `questions`, or `None` when no
/// answer could name lines: a text of one line, a URL capture, a selection
/// of `ranges` (whose text lines are not the file's), a state that is not
/// text, or no `noul`, `choice` or `score` question.
pub(super) fn locate(
    target: &Target,
    state: &Value,
    questions: &[Value],
    page_chars: usize,
) -> Option<Located> {
    let serves: Vec<usize> = questions
        .iter()
        .enumerate()
        .filter(|(_, question)| {
            matches!(question["type"].as_str(), Some("noul" | "choice" | "score"))
        })
        .map(|(index, _)| index)
        .collect();
    if serves.is_empty() {
        return None;
    }
    let text = match target {
        Target::Source(source)
            if source.get("url").is_none() && source["query"].get("ranges").is_none() =>
        {
            state.get("text")?.as_str()?
        }
        Target::Content(Value::String(text)) => text.as_str(),
        _ => return None,
    };
    // The provider boundary sanitizes what it sends; a secret spread over
    // lines must be found before markers come between them. The text is
    // screened whole here, by the rules the request would apply to it, and
    // cut only when the screen kept every line in place.
    let mut screened = state.clone();
    crate::security::ContentSecurity::new().sanitize_data(&mut screened);
    let clean = match &screened {
        Value::String(clean) => clean.as_str(),
        other => other.get("text")?.as_str()?,
    };
    if clean.split_inclusive('\n').count() != text.split_inclusive('\n').count() {
        return None;
    }
    let (marked, parts) = mark(clean, page_chars)?;
    match &mut screened {
        Value::String(text) => *text = marked,
        other => other["text"] = json!(marked),
    }
    let companions = serves
        .iter()
        .map(|&index| companion(&questions[index], parts.len()))
        .collect();
    Some(Located {
        state: screened,
        companions,
        parts,
        serves,
    })
}

/// The questions one request sends: the asked ones, then the companions.
pub(super) fn questions<'a>(
    located: Option<&Located>,
    asked: &'a [Value],
) -> std::borrow::Cow<'a, [Value]> {
    match located {
        Some(located) => asked
            .iter()
            .chain(&located.companions)
            .cloned()
            .collect::<Vec<_>>()
            .into(),
        None => asked.into(),
    }
}

/// The label of part `index` (0-based), as its marker line names it.
fn label(index: usize) -> String {
    format!("p{}", index + 1)
}

/// The response characters a line read takes as it renders: its JSON text
/// and its gutter, as the reader counts a page
/// ([`crate::tools::source_view::pagination::LineBudget`]).
fn rendered(line: &str, gutter: usize) -> usize {
    octocode_engine::json_size::string_body_units(line) + gutter
}

/// The spans of whole `lines` a text is cut into: about one line page of
/// `page_chars` rendered characters each, at least two and at most the labels
/// one choice may list (and the lines there are). A part ends at the first
/// line that brings the text read so far to its share of the whole, and
/// holds at least one line.
fn spans(lines: &[&str], page_chars: usize) -> Vec<(usize, usize)> {
    let gutter = crate::tools::source_view::pagination::gutter_chars(lines.len());
    let widths: Vec<usize> = lines.iter().map(|line| rendered(line, gutter)).collect();
    let total: usize = widths.iter().sum();
    let parts = total
        .div_ceil(page_chars.max(1))
        .clamp(2, LABELS - 1)
        .min(lines.len());
    let mut spans = Vec::with_capacity(parts);
    let (mut end, mut filled) = (0, 0);
    for part in 1..parts {
        let start = end + 1;
        let due = (total * part).div_ceil(parts);
        // Every later part keeps at least one line.
        let last = lines.len() - (parts - part);
        loop {
            end += 1;
            filled += widths[end - 1];
            if filled >= due || end >= last {
                break;
            }
        }
        spans.push((start, end));
    }
    spans.push((end + 1, lines.len()));
    spans
}

/// `text` with a marker line (`[p1]`, `[p2]`, …) before each part
/// ([`spans`]), and each part's lines; `None` for a text of one line.
fn mark(text: &str, page_chars: usize) -> Option<(String, Vec<(usize, usize)>)> {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    if lines.len() < 2 {
        return None;
    }
    let parts = spans(&lines, page_chars);
    let mut marked = String::with_capacity(text.len() + parts.len() * 8);
    for (index, &(start, end)) in parts.iter().enumerate() {
        marked.push_str(&format!("[{}]\n", label(index)));
        for line in &lines[start - 1..end] {
            marked.push_str(line);
        }
        if !marked.ends_with('\n') {
            marked.push('\n');
        }
    }
    Some((marked, parts))
}

/// The companion of `question`: a choice of the part whose text decides it.
fn companion(question: &Value, parts: usize) -> Value {
    let asked = match &question["instructions"] {
        Value::String(text) => text.clone(),
        _ => question.to_string(),
    };
    let mut criteria: Map<String, Value> = (0..parts)
        .map(|index| (label(index), Value::Null))
        .collect();
    criteria.insert(NO_PART.into(), Value::Null);
    json!({
        "type": "choice",
        "instructions": format!(
            "The text is cut into parts, each after its marker line ([p1], [p2], …). Which part holds the text that decides this question: {asked} Choose {NO_PART} when no part does."
        ),
        "criteria": criteria,
    })
}

impl Located {
    /// Split the companions' answers off `answers` (the first `asked` are the
    /// asked questions') and put the deciding lines on the rest
    /// ([`Self::attach`]). Answers judged without their companions (the
    /// window split the request) stay as they are.
    pub(super) fn settle(&self, answers: &mut Vec<Value>, asked: usize) {
        let companions = answers.split_off(asked.min(answers.len()));
        self.attach(answers, &companions);
    }

    /// Put the deciding lines on `answers` (the asked questions' answers, in
    /// question order) from `companions`, the companions' answers; a
    /// companion that was dropped or failed (`null`) names none.
    pub(super) fn attach(&self, answers: &mut [Value], companions: &[Value]) {
        for (&index, choice) in self.serves.iter().zip(companions) {
            let Some(part) = choice["choice"]
                .as_str()
                .and_then(|label| label.strip_prefix('p'))
                .and_then(|number| number.parse::<usize>().ok())
                .and_then(|number| self.parts.get(number.checked_sub(1)?))
            else {
                continue;
            };
            let Some(answer) = answers.get_mut(index) else {
                continue;
            };
            if names_lines(answer) {
                answer["lines"] = json!({"startLine": part.0, "endLine": part.1});
            }
        }
    }
}

/// `captured`'s read narrowed to the deciding lines of `answers`, pinned as
/// the capture was: `readChars` counts what that read returns. `state` is the
/// captured `{source, text}`. A row whose answers name no lines keeps the
/// read of the whole source.
pub(super) fn narrow_read(
    captured: &mut Captured,
    source: &Value,
    state: &Value,
    answers: &[Value],
    can_resume: bool,
    auto_page_chars: usize,
) {
    let lines = deciding_lines(answers);
    let Some(read) = lines_read(source, captured.pin.as_deref(), &lines, can_resume) else {
        return;
    };
    let text: Vec<&str> = state["text"]
        .as_str()
        .unwrap_or_default()
        .split_inclusive('\n')
        .collect();
    let chars = lines
        .iter()
        .filter_map(|&(start, end)| {
            let start = usize::try_from(start).ok()?.checked_sub(1)?;
            let end = usize::try_from(end).ok()?;
            text.get(start..end)
        })
        .flatten()
        .map(|line| line.chars().count())
        .sum();
    captured.read = read;
    captured.read_chars = chars;
    captured.read_pages = !can_resume && chars > auto_page_chars;
}

/// Whether `answer` decides anything a line can show, by the rule of a
/// sectioned answer: a `noul` yes, a `choice` other than `unknown`, a
/// `score`.
fn names_lines(answer: &Value) -> bool {
    match answer["type"].as_str() {
        Some("noul") => answer["noul"].as_f64().is_some_and(|noul| noul >= YES),
        Some("choice") => answer["choice"]
            .as_str()
            .is_some_and(|label| !abstains(label)),
        Some("score") => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests;
