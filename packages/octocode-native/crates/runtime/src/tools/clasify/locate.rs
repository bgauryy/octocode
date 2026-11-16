//! Where a whole-window answer sits. The provider answers typed values
//! (`noul`, `choice`, `score`, `box`) and never a span of the text it
//! judged, so a text judged whole in one request is sent with each of its
//! parts between an opening and a closing marker line that name the lines
//! it holds (`<lines 1-167>` … `</lines 1-167>`), and every question that
//! can name lines takes one companion `choice` in the same request: which
//! part holds the text that decides it. Its labels are those spans, so the
//! judge answers in lines; they are the answer's `lines`, and the row's
//! `next.read` reads only them.
//!
//! Both ends of a part carry its label so ownership reads the same from
//! either side: a bare label before each part leaves the lines above it
//! open to reading as the next label's, and a judge then names the next part
//! for a fact near the end of its own.
//!
//! A part is at most one page of the read that follows: its share of the
//! text's lines, counted as a read renders them (JSON text and `N\t`
//! gutter), split into the line pages of the automatic response page
//! ([`RuntimeLimits::auto_page_chars`], `DEFAULT_RESPONSE_CHARS` unless
//! configured, less the row around its lines:
//! [`crate::response::page_windows::page_budget`]), at least two parts, at
//! most the labels one choice may list. Finer parts would cost the
//! provider's output a probability per label and still leave the agent one
//! read.
//!
//! A part ends at the last block edge within its share ([`block_ends`]), so
//! a paragraph, a table, a fenced block or an indented body stays in one
//! part with the text around it, and a fact is judged with its context. A
//! block larger than a share is cut at a line, and the next part opens
//! saying it carries that block on (`<lines 168-313 continues>`).
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
    /// The state to send: the judged text with each part between its
    /// opening and closing marker lines.
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
    let (text, at) = match target {
        Target::Source(source) if source.get("url").is_none() => {
            let text = state.get("text")?.as_str()?;
            (
                text,
                file_lines(source, text.split_inclusive('\n').count())?,
            )
        }
        Target::Content(Value::String(text)) => (
            text.as_str(),
            (1..=text.split_inclusive('\n').count()).collect(),
        ),
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
    let (marked, parts) = mark(clean, page_chars, Some(&at))?;
    match &mut screened {
        Value::String(text) => *text = marked,
        other => other["text"] = json!(marked),
    }
    let continued = parts.continues.contains(&true);
    let companions = serves
        .iter()
        .map(|&index| companion(&questions[index], &parts.spans, continued))
        .collect();
    Some(Located {
        state: screened,
        companions,
        parts: parts.spans,
        serves,
    })
}

/// The file line of each line of a source's text: its own numbers for a
/// whole read, the lines its `ranges` name, in order, for a selection. `None`
/// when the text does not hold exactly those lines (a range cut at the end of
/// the file, or markers between ranges), so no label could name a wrong line.
fn file_lines(source: &Value, count: usize) -> Option<Vec<usize>> {
    let Some(ranges) = source["query"].get("ranges") else {
        return Some((1..=count).collect());
    };
    let mut at = Vec::new();
    for range in ranges.as_array()? {
        let line = |key: &str| usize::try_from(range[key].as_u64()?).ok();
        let (start, end) = (line("startLine")?, line("endLine")?);
        if start == 0 || end < start {
            return None;
        }
        at.extend(start..=end);
    }
    (at.len() == count).then_some(at)
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

/// The label of the part holding lines `start..=end`, as its marker lines
/// and its companion's criteria name it: the span itself.
fn label((start, end): (usize, usize)) -> String {
    format!("lines {start}-{end}")
}

/// The response characters a line read takes as it renders: its JSON text
/// and its gutter, as the reader counts a page
/// ([`crate::tools::source_view::pagination::LineBudget`]).
fn rendered(line: &str, gutter: usize) -> usize {
    octocode_engine::json_size::string_body_units(line) + gutter
}

/// A text cut into parts: each part's first and last line, and whether it
/// carries on a block the part before it ends in.
struct Parts {
    spans: Vec<(usize, usize)>,
    continues: Vec<bool>,
}

/// Whether a part may end after each of `lines`: where the next line opens
/// a top-level block. In a text with blank lines a block runs from one blank
/// line to the next, so a part may end on a blank line whose next line is
/// neither blank nor indented, outside a fenced block (```` ``` ```` or
/// `~~~`): tables, paragraphs, fenced code, and declarations whose bodies
/// are indented stay whole. A text without blank lines (a log, a listing) is
/// made of its lines; one indented under another (a stack trace, a wrapped
/// record) stays with it. A lead-in stays with what it introduces: no part
/// ends on a block whose last line is a heading or ends with `:`
/// ([`leads_in`]). The last line always ends a part.
fn block_ends(lines: &[&str]) -> Vec<bool> {
    let blank = |line: &str| line.trim().is_empty();
    let indented = |line: &str| line.starts_with([' ', '\t']);
    let separated = lines.iter().any(|line| blank(line));
    let mut fence: Option<&str> = None;
    let mut last_text = "";
    let mut ends = Vec::with_capacity(lines.len());
    for (index, line) in lines.iter().enumerate() {
        if !blank(line) {
            last_text = line;
        }
        let opened = line.trim_start();
        match fence {
            Some(open) if opened.starts_with(open) => fence = None,
            Some(_) => {}
            None => {
                fence = ["```", "~~~"]
                    .into_iter()
                    .find(|open| opened.starts_with(open));
            }
        }
        ends.push(match lines.get(index + 1) {
            None => true,
            Some(next) => {
                fence.is_none()
                    && !blank(next)
                    && !indented(next)
                    && (!separated || blank(line))
                    && !leads_in(last_text)
            }
        });
    }
    ends
}

/// Whether `line` introduces what follows it: a heading (`#` to `######`
/// and a space, as Markdown and comment banners write one) or a line that
/// ends with `:`.
fn leads_in(line: &str) -> bool {
    let line = line.trim();
    let heading = line.trim_start_matches('#');
    let hashes = line.len() - heading.len();
    line.ends_with(':') || ((1..=6).contains(&hashes) && heading.starts_with(' '))
}

/// The parts of whole `lines`: each at most one share of the text as a read
/// renders it (the line pages it fills at `page_chars`, at least two and at
/// most the labels one choice may list, and the lines there are), ending at
/// the last block edge ([`block_ends`]) within its share. A block larger
/// than a share is cut at the line that fills it, and the next part carries
/// it on. A part holds at least one line; past the label limit the last
/// part holds the rest.
fn spans(lines: &[&str], page_chars: usize) -> Parts {
    let gutter = crate::tools::source_view::pagination::gutter_chars(lines.len());
    let widths: Vec<usize> = lines.iter().map(|line| rendered(line, gutter)).collect();
    let total: usize = widths.iter().sum();
    let pages = total
        .div_ceil(page_chars.max(1))
        .clamp(2, LABELS - 1)
        .min(lines.len());
    let share = total.div_ceil(pages);
    let ends = block_ends(lines);
    let mut parts = Parts {
        spans: Vec::with_capacity(pages),
        continues: Vec::with_capacity(pages),
    };
    let (mut start, mut left, mut carried) = (0, total, false);
    while start < lines.len() {
        if left <= share || parts.spans.len() == LABELS - 2 {
            parts.spans.push((start + 1, lines.len()));
            parts.continues.push(carried);
            break;
        }
        let (mut filled, mut fits, mut edge) = (0, start, None);
        for (index, width) in widths.iter().enumerate().skip(start) {
            filled += width;
            if filled > share && index > start {
                break;
            }
            fits = index;
            if ends[index] {
                edge = Some(index);
            }
        }
        let end = edge.unwrap_or(fits);
        parts.spans.push((start + 1, end + 1));
        parts.continues.push(carried);
        carried = edge.is_none();
        left -= widths[start..=end].iter().sum::<usize>();
        start = end + 1;
    }
    parts
}

/// `text` with each part ([`spans`]) between an opening and a closing
/// marker line naming its lines (`<lines 1-40>` … `</lines 1-40>`), and its
/// parts; `None` for a text of one line. A part that carries on the block
/// the part before it ends in opens saying so: `<lines 41-80 continues>`.
/// `at` gives the file line of each text line (`None`: the text's own
/// numbers); parts and markers name file lines.
fn mark(text: &str, page_chars: usize, at: Option<&[usize]>) -> Option<(String, Parts)> {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    if lines.len() < 2 {
        return None;
    }
    let mut parts = spans(&lines, page_chars);
    let local = parts.spans.clone();
    if let Some(at) = at {
        for (start, end) in &mut parts.spans {
            (*start, *end) = (at[*start - 1], at[*end - 1]);
        }
    }
    let mut marked = String::with_capacity(text.len() + parts.spans.len() * 24);
    for ((&(start, end), &(first, last)), &continues) in
        local.iter().zip(&parts.spans).zip(&parts.continues)
    {
        let name = label((first, last));
        if continues {
            marked.push_str(&format!("<{name} continues>\n"));
        } else {
            marked.push_str(&format!("<{name}>\n"));
        }
        for line in &lines[start - 1..end] {
            marked.push_str(line);
        }
        if !marked.ends_with('\n') {
            marked.push('\n');
        }
        marked.push_str(&format!("</{name}>\n"));
    }
    Some((marked, parts))
}

/// The companion of `question`: a choice of the part whose text decides it.
/// When a block was cut (`continued`), it says how a carried-on part opens.
fn companion(question: &Value, parts: &[(usize, usize)], continued: bool) -> Value {
    let asked = match &question["instructions"] {
        Value::String(text) => text.clone(),
        _ => question.to_string(),
    };
    let mut criteria: Map<String, Value> = parts
        .iter()
        .map(|&span| (label(span), Value::Null))
        .collect();
    let first = parts.first().map(|&span| label(span)).unwrap_or_default();
    criteria.insert(NO_PART.into(), Value::Null);
    json!({
        "type": "choice",
        "instructions": format!(
            "The text is cut into parts, each between an opening and a closing marker line naming the lines it holds (<{first}> … </{first}>).{} Which part holds the text that decides this question: {asked} Choose {NO_PART} when no part does.",
            if continued {
                " A part opening <lines A-B continues> carries on the block the part before it ends in."
            } else {
                ""
            }
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
                .and_then(|chosen| self.parts.iter().find(|&&span| label(span) == chosen))
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
    // The deciding lines are file lines; a selection of `ranges` holds
    // only some of them, in order.
    let at = file_lines(source, text.len()).unwrap_or_default();
    let chars = text
        .iter()
        .zip(&at)
        .filter(|&(_, &line)| {
            lines.iter().any(|&(start, end)| {
                u64::try_from(line).is_ok_and(|line| start <= line && line <= end)
            })
        })
        .map(|(line, _)| line.chars().count())
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
