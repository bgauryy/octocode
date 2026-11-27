use super::{Resource, Row, Session, Unit, error_value, new_id};
use octocode_contract_types::ErrorCode;
use octocode_runtime_base::execution::ExecutionContext;
use octocode_runtime_base::security::scan::ContentScan as _;
use octocode_tools_shared::judgment::{ClassificationError, LongContext};
use octocode_tools_shared::source_capture::SourceReader;
use octocode_tools_shared::{source_capture::resource, source_sections};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// A `sections` plan's target text in the default window.
const SECTION_CHARS: usize = 8_000;
/// A `sections` plan's target text with `longContext:true`.
const LONG_CONTEXT_SECTION_CHARS: usize = 64_000;

/// How large a plan's sections are.
#[derive(Clone)]
pub(super) enum Grain {
    /// A `sections` plan, read a section at a time: each section with its
    /// supporting context, in pages an answer can point into.
    Plan,
    /// A source judged whole ([`super::whole`]): each section alone, with
    /// every question, fills one request of `bytes`; the plan covers the
    /// selected source bytes within `span`.
    Request {
        bytes: usize,
        span: std::ops::Range<usize>,
    },
}

#[derive(Default)]
pub(super) struct Pool {
    inner: Mutex<Captures>,
}

#[derive(Default)]
struct Captures {
    sources: HashMap<String, Result<Arc<resource::TextResource>, ClassificationError>>,
    bytes: usize,
}

impl Pool {
    fn get(
        &self,
        source: &Value,
        dispatcher: &dyn SourceReader,
        context: &ExecutionContext,
        max_bytes: usize,
    ) -> Result<Arc<resource::TextResource>, ClassificationError> {
        let key = resource::acquisition_key(source, dispatcher)?;
        let mut captures = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(saved) = captures.sources.get(&key) {
            return saved.clone();
        }
        let captured = resource::capture(source, dispatcher, context).and_then(|captured| {
            if captures.bytes.saturating_add(captured.text.len()) > max_bytes {
                return Err(ClassificationError::new(ErrorCode::ClassificationContextTooLarge, "The complete source would exceed the aggregate capture budget; no source was truncated.", "Use raw readers or a smaller resource set."));
            }
            captures.bytes += captured.text.len();
            Ok(Arc::new(captured))
        });
        captures.sources.insert(key, captured.clone());
        captured
    }
}

pub(super) fn create(
    row: &Row,
    input: &Value,
    dispatcher: &dyn SourceReader,
    context: &ExecutionContext,
    max_bytes: usize,
    pool: &Pool,
    grain: Grain,
) -> Result<(Session, Vec<String>, Vec<Value>), ClassificationError> {
    let mut resources = Vec::new();
    let mut retained = 0usize;
    let mut notices = Vec::new();
    let mut related = Vec::new();
    if let Some(requested) = input["resources"].as_object() {
        // A glob is one resource per matching file; one that cannot be
        // expanded is its resource's error.
        let expanded =
            octocode_tools_shared::source_capture::glob::expand(requested, dispatcher, context);
        notices = expanded.notices;
        related = expanded.related;
        for (key, expanded) in expanded.resources {
            context.check().map_err(|_| {
                super::unavailable(
                    "The call stopped while acquiring resource identities; no inference ran.",
                )
            })?;
            let (source, acquired) = match expanded {
                Ok(source) => {
                    let acquired =
                        pool.get(&source, dispatcher, context, max_bytes)
                            .and_then(|captured| {
                                prepare(
                                    &captured,
                                    &source,
                                    row,
                                    dispatcher,
                                    context,
                                    max_bytes.saturating_sub(retained),
                                    grain.clone(),
                                )
                            });
                    (source, acquired)
                }
                Err(error) => (requested[&key].clone(), Err(error)),
            };
            let source = &source;
            let (metadata, units, local_check) = match acquired {
                Ok(capture) => {
                    let bytes = octocode_engine::json_size::bytes(&capture.0)
                        .unwrap_or(usize::MAX)
                        .saturating_add(capture.1.iter().map(unit_weight).sum::<usize>());
                    if retained.saturating_add(bytes) > max_bytes {
                        let mut metadata = capture.0;
                        metadata["status"] = json!("unsupported");
                        metadata["error"] = error_value(&ClassificationError::new(
                            ErrorCode::ClassificationContextTooLarge,
                            "This resource would exceed the aggregate retained-plan budget; none of its sections were assessed.",
                            "Use its pinned raw reader or assess a narrower resource set.",
                        ));
                        (metadata, Vec::new(), None)
                    } else {
                        retained += bytes;
                        capture
                    }
                }
                Err(error) => {
                    let mut metadata = json!({"status":if matches!(error.code, ErrorCode::FileTooLarge | ErrorCode::ClassificationContextTooLarge | ErrorCode::CapabilityUnavailable) {"unsupported"} else {"error"},"source":source,"error":error_value(&error)});
                    // The requested read is offered only when repeating it
                    // can succeed (not for a missing path or a refusal). A
                    // glob that did not expand never gets one: it fails only
                    // so, or by a stop, which the check above already turned
                    // into the row's failure.
                    if let Some(tool) = source["tool"]
                        .as_str()
                        .and_then(octocode_runtime_base::id::ToolId::from_name)
                        .filter(|_| super::super::recovery::rereadable(&metadata["error"]))
                    {
                        metadata["read"] = octocode_tools_shared::source_capture::read_lead(
                            tool,
                            &source["query"],
                            None,
                        );
                    }
                    (metadata, Vec::new(), None)
                }
            };
            resources.push(Resource {
                key,
                metadata,
                units,
                local_check,
                shown: None,
            });
        }
    }
    let session = Session {
        id: new_id(input),
        context: row.context.clone(),
        questions: input["questions"].clone(),
        keys: row.keys.clone(),
        model: Some(
            row.model
                .clone()
                .unwrap_or_else(|| dispatcher.evaluator().model.clone()),
        ),
        resolved_model: None,
        long_context: LongContext::from_field(row.long_context),
        resources,
        github_identity: None,
        batches: Vec::new(),
        active: Vec::new(),
        request: Value::Null,
        source_disclosures: None,
        max_requests: None,
    };
    Ok((session, notices, related))
}

fn prepare(
    captured: &resource::TextResource,
    source: &Value,
    row: &Row,
    dispatcher: &dyn SourceReader,
    context: &ExecutionContext,
    remaining_bytes: usize,
    grain: Grain,
) -> Result<(Value, Vec<Unit>, Option<Value>), ClassificationError> {
    let mut selected = resource::selected_ranges(&captured.text, &source["query"])?;
    let mut metadata = resource::selection_metadata(captured, source);
    // How much text a section holds: its grain, not a limit. Every original
    // byte remains in a target even when it needs a fragment. A plan's
    // section sits well inside the provider's 32k-token (262k long-context)
    // window, leaving room for its context and questions; a source judged
    // whole fills each request.
    let (target_chars, pages) = match grain {
        Grain::Plan => (
            if LongContext::from_field(row.long_context) == LongContext::Always {
                LONG_CONTEXT_SECTION_CHARS
            } else {
                SECTION_CHARS
            },
            source_sections::Pages::Share,
        ),
        Grain::Request { bytes, ref span } => {
            selected = selected
                .into_iter()
                .map(|range| range.start.max(span.start)..range.end.min(span.end))
                .filter(|range| !range.is_empty())
                .collect();
            (
                request_chars(&captured.text, &selected, row, dispatcher, bytes)?,
                source_sections::Pages::Grain,
            )
        }
    };
    // A source judged whole in sections sends each section alone: every
    // section is judged, so the page before it is some other request's
    // target, and repeating it as context doubled the input for the same
    // answers (measured 2026-11-11 on a 294,466-char source, three
    // planted-fact questions, 19 sections: 184,677 input tokens with it,
    // 97,679 without, the same answers and deciding lines). A `sections`
    // plan keeps it: its sections are read one at a time.
    let with_context = matches!(grain, Grain::Plan);
    let safe = SafeSource::new(&captured.text, &captured.path, dispatcher)?;
    // A local source is named by the text a read shows of it: a digest of
    // its own bytes would test a guess of a value the read hides. The pin a
    // later cursor checks the file against stays in `local_check`.
    if captured.local_check.is_some() {
        metadata["sourceDigest"] = json!(safe.digest());
    }
    let mut units = Vec::new();
    let mut retained_bytes = octocode_engine::json_size::bytes(&metadata).unwrap_or(usize::MAX);
    let line_starts = octocode_engine::text::line_starts(&captured.text);
    // Lines are the source's (redaction keeps them); bytes count the text a
    // read shows, as every shown coordinate does.
    let location = |range: &std::ops::Range<usize>| {
        let line = |byte| octocode_engine::text::line_at(&line_starts, byte);
        json!({"bytes":[safe.shown_start(range.start),safe.shown_end(range.end)],"lines":{"startLine":line(range.start),"endLine":line(range.end.saturating_sub(1).max(range.start))}})
    };
    let complete = source_sections::visit_grouped(
        &captured.text,
        &captured.path,
        &selected,
        target_chars,
        pages,
        |section| -> Result<bool, ClassificationError> {
            let ordinal = units.len();
            context.check().map_err(|_| {
                super::unavailable("The call stopped while planning sections; no inference ran.")
            })?;
            let target = location(&section.target);
            let supporting: &[std::ops::Range<usize>] =
                if with_context { &section.context } else { &[] };
            let context_ranges = supporting.iter().map(&location).collect::<Vec<_>>();
            let context_text = supporting
                .iter()
                .map(|range| {
                    safe.slice(range)
                        .map(|text| json!({"range":location(range),"text":text}))
                })
                .collect::<Result<Vec<_>, _>>();
            let state = safe.slice(&section.target).and_then(|text| {
                context_text
                    .map(|context| json!({"target":{"range":target,"text":text},"context":context}))
            });
            let mut metadata = json!({"id":format!("s{ordinal}"),"kind":section.kind,"target":target,"context":context_ranges});
            if let Some(name) = section.name {
                let (name, _) = dispatcher
                    .security()
                    .sanitize(&name, &captured.path)
                    .map_err(|_| super::unavailable("A section name could not be sanitized."))?;
                metadata["name"] = json!(name);
            }
            let cells = row.questions.iter().map(|question| {
            if let Err(error) = &state {
                Some(json!({"status":"unsupported","error":error_value(error)}))
            } else {
                (question["type"] == "box").then(|| json!({"status":"unsupported","error":{"errorCode":ErrorCode::CapabilityUnavailable,"error":"Box questions require an explicit image asset; this operation assesses text sections."}}))
            }
        }).collect();
            let state = state.unwrap_or(Value::Null);
            let unit = Unit {
                metadata,
                source_target: [section.target.start, section.target.end],
                chars: octocode_tools_shared::selection::serialized_chars(&state),
                state,
                cells,
                emitted: false,
                emitted_in: 0,
                deferred: std::collections::BTreeSet::new(),
            };
            let bytes = unit_weight(&unit);
            if retained_bytes.saturating_add(bytes) > remaining_bytes {
                return Ok(false);
            }
            retained_bytes += bytes;
            units.push(unit);
            Ok(true)
        },
    )?;
    if !complete {
        metadata["status"] = json!("unsupported");
        metadata["error"] = error_value(&ClassificationError::new(
            ErrorCode::ClassificationContextTooLarge,
            "The complete section/question plan exceeds the remaining retained-plan budget; its sections were not assessed.",
            "Use the pinned raw reader or a smaller resource selection.",
        ));
        return Ok((metadata, Vec::new(), None));
    }
    let mut local_check = captured.local_check.clone();
    if let Some(check) = &mut local_check {
        check["query"]["path"] = source["query"]["path"].clone();
    }
    Ok((metadata, units, local_check))
}

/// The target characters one request of `bytes` holds beside every
/// question of `row`: the bytes the request with an empty target leaves, at
/// the selected text's own JSON-escaped bytes per character.
fn request_chars(
    text: &str,
    selected: &[std::ops::Range<usize>],
    row: &Row,
    dispatcher: &dyn SourceReader,
    bytes: usize,
) -> Result<usize, ClassificationError> {
    let evaluator = dispatcher.evaluator();
    let model = row.model.as_deref().unwrap_or(&evaluator.model);
    // The widest location any section of this source states.
    let (end, lines) = (text.len(), octocode_engine::text::line_starts(text).len());
    let empty = json!({"target":{"range":{"bytes":[end,end],"lines":{"startLine":lines,"endLine":lines}},"text":""},"context":[]});
    let state = super::super::rows::scoped(super::super::rows::contextual_state(
        &empty,
        row.context.as_deref(),
    ));
    let questions: Vec<&Value> = row.questions.iter().collect();
    let overhead = serde_json::to_vec(&evaluator.provider.build(&state, &questions, model, None))
        .map_or(usize::MAX, |body| body.len());
    let room = bytes.saturating_sub(overhead);
    if room == 0 {
        return Err(ClassificationError::new(
            ErrorCode::ClassificationStateTooLarge,
            "The questions alone fill the provider's window: no section of the source fits beside them.",
            "Ask fewer or shorter questions in one row.",
        ));
    }
    // Selected ranges start and end at line or section boundaries, which are
    // character boundaries; `get` keeps a misplaced one from panicking.
    let parts: Vec<&str> = selected
        .iter()
        .filter_map(|range| text.get(range.clone()))
        .collect();
    Ok(chars_within(&parts, room))
}

/// How many characters one section may hold so that its JSON-escaped bytes
/// stay within `room`: the largest `n` for which no stretch of `n`
/// consecutive characters of any part costs more, measured on the text
/// itself. An average over the source would let a stretch dense in quotes,
/// newlines or control characters overshoot the provider's window.
pub(super) fn chars_within(parts: &[&str], room: usize) -> usize {
    // The JSON-escaped bytes of each character, as `serde_json` writes it.
    let cost = |c: char| -> u8 {
        match c {
            '"' | '\\' | '\n' | '\r' | '\t' | '\u{8}' | '\u{c}' => 2,
            c if u32::from(c) < 0x20 => 6,
            c => u8::try_from(c.len_utf8()).unwrap_or(4),
        }
    };
    let costs: Vec<Vec<u8>> = parts
        .iter()
        .map(|part| part.chars().map(cost).collect())
        .collect();
    // The costliest stretch of `n` characters in any part.
    let worst = |n: usize| -> usize {
        costs
            .iter()
            .map(|part| {
                let mut sum: usize = part.iter().take(n).map(|c| usize::from(*c)).sum();
                let mut widest = sum;
                for at in n..part.len() {
                    sum = sum + usize::from(part[at]) - usize::from(part[at - n]);
                    widest = widest.max(sum);
                }
                widest
            })
            .max()
            .unwrap_or(0)
    };
    // A stretch of `n` characters costs at least `n` bytes, so `room` bounds
    // the answer; the cost grows with `n`, so the fit is found by halving.
    let (mut low, mut high) = (1, room.max(1));
    if worst(high) <= room {
        return high;
    }
    while low < high {
        let mid = low + (high - low).div_ceil(2);
        if worst(mid) <= room {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    low
}

fn unit_weight(unit: &Unit) -> usize {
    octocode_engine::json_size::bytes(unit)
        .unwrap_or(usize::MAX)
        .saturating_add(std::mem::size_of::<Unit>())
        .saturating_add(
            unit.cells
                .len()
                .saturating_mul(std::mem::size_of::<Option<Value>>()),
        )
}

pub(super) fn validate_local(
    source: &Value,
    dispatcher: &dyn SourceReader,
    context: &ExecutionContext,
) -> Result<(), ClassificationError> {
    resource::validate_local(source, dispatcher, context)
}

/// A full-source scan happens before slicing. Changed lines are mapped only as
/// complete records; an interior byte boundary cannot be relabeled after a
/// variable-length redaction, so that target fails closed instead of leaking.
struct SafeSource<'a> {
    raw: &'a str,
    changed: Vec<(std::ops::Range<usize>, String)>,
}

impl<'a> SafeSource<'a> {
    fn new(
        raw: &'a str,
        path: &std::path::Path,
        dispatcher: &dyn SourceReader,
    ) -> Result<Self, ClassificationError> {
        let (blocks, _) = dispatcher.security().redact_key_blocks(raw);
        let (safe, _) = dispatcher
            .security()
            .redact_source_lines(&blocks, path)
            .map_err(|_| {
                super::unavailable(
                    "The complete source could not be sanitized before section planning.",
                )
            })?;
        let output = &dispatcher.config().resolved.output;
        let pii = octocode_runtime_base::security::Pii {
            emails: output.redact_emails,
            uuids: output.redact_uuid,
        };
        let safe = match dispatcher.security().redact_pii(&safe, pii) {
            Some(masked) => std::borrow::Cow::Owned(masked),
            None => safe,
        };
        Self::from_redacted(raw, &safe)
    }

    fn from_redacted(raw: &'a str, safe: &str) -> Result<Self, ClassificationError> {
        if raw.split_inclusive('\n').count() != safe.split_inclusive('\n').count() {
            return Err(super::unavailable(
                "Source redaction changed line geometry; exact section coordinates cannot be retained.",
            ));
        }
        let mut offset = 0;
        let mut changed = Vec::new();
        for (original, clean) in raw.split_inclusive('\n').zip(safe.split_inclusive('\n')) {
            let range = offset..offset + original.len();
            offset = range.end;
            if original != clean {
                changed.push((range, clean.to_owned()));
            }
        }
        Ok(Self { raw, changed })
    }

    /// SHA-256 of the whole text this source shows: each changed line as
    /// it shows, every other byte as it is.
    fn digest(&self) -> String {
        let mut hash = octocode_engine::digest::HashWriter::default();
        let mut offset = 0;
        for (line, clean) in &self.changed {
            hash.update(&self.raw.as_bytes()[offset..line.start]);
            hash.update(clean);
            offset = line.end;
        }
        hash.update(&self.raw.as_bytes()[offset..]);
        hex::encode(hash.finalize())
    }

    /// Where source byte `at` falls in the shown text: shifted by what every
    /// changed line before it grew or shrank. A byte inside a changed line
    /// has no counterpart there; `inside` places it in that line as shown.
    fn shown(&self, at: usize, inside: impl Fn(usize) -> usize) -> usize {
        let before = self.changed.partition_point(|(line, _)| line.end <= at);
        // The changed lines before `at` lie wholly below it.
        let (source, shown) = self.changed[..before]
            .iter()
            .fold((0, 0), |(source, shown), (line, clean)| {
                (source + line.len(), shown + clean.len())
            });
        let place = |at: usize| at - source + shown;
        match self.changed.get(before) {
            Some((line, clean)) if line.start < at => place(line.start) + inside(clean.len()),
            _ => place(at),
        }
    }

    /// [`Self::shown`] of a range's first byte: one inside a changed line
    /// starts at that line.
    fn shown_start(&self, at: usize) -> usize {
        self.shown(at, |_| 0)
    }

    /// [`Self::shown`] of a range's end: one inside a changed line ends with
    /// it, so the range covers what the line shows.
    fn shown_end(&self, at: usize) -> usize {
        self.shown(at, |shown| shown)
    }

    fn slice(&self, range: &std::ops::Range<usize>) -> Result<String, ClassificationError> {
        let mut output = String::new();
        let first = self
            .changed
            .partition_point(|(line, _)| line.end <= range.start);
        let mut offset = range.start;
        for (line, clean) in self.changed[first..]
            .iter()
            .take_while(|(line, _)| line.start < range.end)
        {
            if line.start < range.start || line.end > range.end {
                return Err(ClassificationError::new(
                    ErrorCode::ClassificationContextUnavailable,
                    "This section's target or supporting context crosses an interior byte boundary of a redacted line. Its original coordinates remain available, but no partial secret-bearing text was submitted.",
                    "Use the resource's read call to inspect the sanitized source; for a new evaluation, select complete lines.",
                ));
            }
            output.push_str(&self.raw[offset..line.start]);
            output.push_str(clean);
            offset = line.end;
        }
        output.push_str(&self.raw[offset..range.end]);
        Ok(output)
    }
}

#[cfg(test)]
mod tests;
