use super::{Resource, Row, Session, Unit, error_value, new_id};
use crate::execution::ExecutionContext;
use crate::security::scan::ContentScan as _;
use crate::tools::source_capture::SourceReader;
use crate::tools::{judgment::ClassificationError, source_capture::resource, source_sections};
use octocode_contract_types::ErrorCode;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

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
) -> Result<Session, ClassificationError> {
    let mut resources = Vec::new();
    let mut retained = 0usize;
    if let Some(requested) = input["resources"].as_object() {
        for (key, source) in requested {
            context.check().map_err(|_| {
                super::unavailable(
                    "The call stopped while acquiring resource identities; no inference ran.",
                )
            })?;
            let acquired = pool
                .get(source, dispatcher, context, max_bytes)
                .and_then(|captured| {
                    prepare(
                        &captured,
                        source,
                        row,
                        dispatcher,
                        context,
                        max_bytes.saturating_sub(retained),
                    )
                });
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
                        metadata["coverage"] = json!({"totalSections":null,"completedSections":0,"remainingSections":null,"answered":0,"error":0,"unsupported":0,"unassessed":null});
                        (metadata, Vec::new(), None)
                    } else {
                        retained += bytes;
                        capture
                    }
                }
                Err(error) => {
                    let mut metadata = json!({"status":if matches!(error.code, ErrorCode::FileTooLarge | ErrorCode::ClassificationContextTooLarge | ErrorCode::CapabilityUnavailable) {"unsupported"} else {"error"},"source":source,"error":error_value(&error),"coverage":{"totalSections":null,"completedSections":0,"remainingSections":null,"answered":0,"error":0,"unsupported":0,"unassessed":null}});
                    // The requested read is offered only when repeating it
                    // can succeed (not for a missing path or a refusal).
                    if let Some(tool) = source["tool"]
                        .as_str()
                        .and_then(crate::id::ToolId::from_name)
                        .filter(|_| super::super::recovery::rereadable(&metadata["error"]))
                    {
                        metadata["read"] =
                            crate::tools::source_capture::read_lead(tool, &source["query"], None);
                    }
                    (metadata, Vec::new(), None)
                }
            };
            resources.push(Resource {
                key: key.clone(),
                metadata,
                units,
                local_check,
            });
        }
    }
    Ok(Session {
        id: new_id(input),
        query_id: row.id.clone(),
        questions: input["questions"].clone(),
        keys: row.keys.clone(),
        model: Some(
            row.model
                .clone()
                .unwrap_or_else(|| dispatcher.evaluator().model.clone()),
        ),
        resolved_model: None,
        long_context: row.long_context,
        resources,
        github_identity: None,
        batches: Vec::new(),
        active: Vec::new(),
    })
}

fn prepare(
    captured: &resource::TextResource,
    source: &Value,
    row: &Row,
    dispatcher: &dyn SourceReader,
    context: &ExecutionContext,
    remaining_bytes: usize,
) -> Result<(Value, Vec<Unit>, Option<Value>), ClassificationError> {
    let selected = resource::selected_ranges(&captured.text, &source["query"])?;
    let mut metadata = resource::selection_metadata(captured, source);
    // Leave room for escaped target text, supporting context and coordinates.
    // Every original byte remains in a target even when it needs a fragment.
    let target_chars = super::super::max_state_chars(row.long_context) / 4;
    let safe = SafeSource::new(
        &captured.text,
        &captured.path,
        dispatcher,
        captured.metadata["snapshot"].as_str().unwrap_or_default(),
    )?;
    let mut units = Vec::new();
    let mut retained_bytes = octocode_engine::json_size::bytes(&metadata).unwrap_or(usize::MAX);
    let line_starts = octocode_engine::text::line_starts(&captured.text);
    let location = |range: &std::ops::Range<usize>| {
        let line = |byte| octocode_engine::text::line_at(&line_starts, byte);
        json!({"bytes":[range.start,range.end],"lines":[line(range.start),line(range.end.saturating_sub(1).max(range.start))]})
    };
    let complete = source_sections::visit(
        &captured.text,
        &captured.path,
        &selected,
        target_chars,
        |section| -> Result<bool, ClassificationError> {
            let ordinal = units.len();
            context.check().map_err(|_| {
                super::unavailable("The call stopped while planning sections; no inference ran.")
            })?;
            let target = location(&section.target);
            let contexts = section
                .context
                .iter()
                .filter_map(|range| safe.slice(range).ok().map(|text| (range, text)))
                .collect::<Vec<_>>();
            let context_ranges = contexts
                .iter()
                .map(|(range, _)| location(range))
                .collect::<Vec<_>>();
            let context_text = contexts
                .iter()
                .map(|(range, text)| json!({"range":location(range),"text":text}))
                .collect::<Vec<_>>();
            let target_text = safe.slice(&section.target);
            let state = target_text.as_ref().map_or(
                Value::Null,
                |text| json!({"target":{"range":target,"text":text},"context":context_text}),
            );
            let mut metadata = json!({"id":format!("s{ordinal}"),"kind":section.kind,"target":target,"context":context_ranges});
            if let Some(name) = section.name {
                let (name, _) = dispatcher
                    .security()
                    .sanitize(&name, &captured.path)
                    .map_err(|_| super::unavailable("A section name could not be sanitized."))?;
                metadata["name"] = json!(name);
            }
            let cells = row.questions.iter().map(|question| {
            if let Err(error) = &target_text {
                Some(json!({"status":"unsupported","error":error_value(error)}))
            } else {
                (question["type"] == "box").then(|| json!({"status":"unsupported","error":{"errorCode":ErrorCode::CapabilityUnavailable,"error":"Box questions require an explicit image asset; this operation assesses text sections."}}))
            }
        }).collect();
            let unit = Unit {
                metadata,
                chars: crate::tools::selection::serialized_chars(&state),
                state,
                cells,
                emitted: false,
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
        metadata["coverage"] = json!({"totalSections":null,"completedSections":0,"remainingSections":null,"answered":0,"error":0,"unsupported":0,"unassessed":null});
        return Ok((metadata, Vec::new(), None));
    }
    let mut local_check = captured.local_check.clone();
    if let Some(check) = &mut local_check {
        check["query"]["path"] = source["query"]["path"].clone();
    }
    Ok((metadata, units, local_check))
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
        digest: &str,
    ) -> Result<Self, ClassificationError> {
        let (blocks, _) = dispatcher.security().redact_key_blocks(raw);
        let (safe, _) = dispatcher
            .security()
            .redact_source_lines(&blocks, path, digest)
            .map_err(|_| {
                super::unavailable(
                    "The complete source could not be sanitized before section planning.",
                )
            })?;
        let safe = if dispatcher.config().resolved.output.redact_emails {
            dispatcher.security().redact_emails(&safe)
        } else {
            safe
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
                return Err(super::unavailable(
                    "This target crosses an interior byte boundary of a redacted line. Its original coordinates remain available, but no partial secret-bearing text was submitted.",
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
mod tests {
    use super::*;

    #[test]
    fn unchanged_lines_do_not_allocate_per_line_records() {
        let raw = format!("{}secret=original\r\nעברית🙂", "x\n".repeat(100_000));
        let clean = raw.replace("secret=original", "secret=[REDACTED]");
        let source = SafeSource::from_redacted(&raw, &clean).expect("stable line geometry");
        assert_eq!(source.changed.len(), 1);
        assert_eq!(source.slice(&(0..raw.len())).expect("whole source"), clean);
        let unchanged = SafeSource::from_redacted(&raw, &raw).expect("unchanged source");
        assert!(unchanged.changed.is_empty());
        assert_eq!(unchanged.slice(&(0..4)).expect("prefix"), "x\nx\n");
        assert!(SafeSource::from_redacted("a\nb", "a b").is_err());
    }

    #[test]
    fn redaction_is_whole_line_or_explicitly_unavailable_never_a_partial_secret() {
        let raw = "🙂 safe\r\nsecret=original\nעברית\n";
        let first = "🙂 safe\r\n".len();
        let second = first + "secret=original\n".len();
        let source = SafeSource {
            raw,
            changed: vec![(first..second, "secret=[REDACTED]\n".into())],
        };
        assert_eq!(source.slice(&(0..4)).expect("UTF-8 prefix"), "🙂");
        assert_eq!(
            source.slice(&(first..second)).expect("whole redacted line"),
            "secret=[REDACTED]\n"
        );
        assert!(source.slice(&(first + 2..second)).is_err());
        assert!(source.slice(&(0..second - 1)).is_err());
        let complete = source.slice(&(0..raw.len())).expect("whole source");
        assert!(!complete.contains("original"));
        assert!(complete.contains("עברית"));
    }
}
