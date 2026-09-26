//! Compact, resource-major clasify output and provider-page coalescing.
//!
//! Output is `queries[] → resources[] → pages[] → answers[questionId]`: the
//! source coordinates appear once per page and answers carry typed hints only.
//! Provider telemetry and capture hashes stay internal.
use super::clasify_context::PAGE_ONLY_LIMITATION;
use crate::tools::clasify::transport::ClassificationError;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

/// Provider state budget for coalesced file pages (~600 lines of code,
/// ~6–8k tokens). Evidence-only state makes page count nearly free in tokens,
/// so the budget is set for localization: a 1k-line file still yields two
/// scoped verdicts instead of one whole-file scope (48 KiB judged a 1,079-line
/// README as a single page in the GitHub eval).
pub(super) const COALESCE_BYTES: usize = 24 * 1024;

/// Outcome of one assessed (or failed) page, before rendering.
pub(super) enum PageOutcome {
    Failed {
        error: ClassificationError,
        receipt: Value,
    },
    Assessed {
        receipt: Value,
        answers: Vec<Result<Value, ClassificationError>>,
    },
}

fn error_value(error: &ClassificationError) -> Value {
    let mut value = json!({"code":error.code,"message":error.message});
    if !error.hints.is_empty() {
        value["hints"] = json!(error.hints);
    }
    value
}

/// Project a provider answer (`{type, noul|choice|score, ...}`) to the typed
/// verdict only: no `type` or echoed `legend`. Numeric values and complete
/// probability distributions are preserved from the validated response.
pub(super) fn compact_answer(answer: &Value) -> Value {
    match answer["type"].as_str() {
        Some("noul") => json!({"noul":answer["noul"]}),
        Some("choice") => json!({
            "choice":answer["choice"],
            "confidence":answer["confidence"],
            "probabilities":answer["probabilities"]
        }),
        Some("score") => json!({
            "score":answer["score"],
            "confidence":answer["confidence"],
            "probabilities":answer["probabilities"]
        }),
        Some("locate") => json!({
            "exists":answer["exists"],
            "matches":answer["matches"]
        }),
        _ => answer.clone(),
    }
}

fn limitations(receipt: &Value) -> Option<Value> {
    let kept = receipt
        .get("limitations")?
        .as_array()?
        .iter()
        .filter(|limitation| limitation.as_str() != Some(PAGE_ONLY_LIMITATION))
        .cloned()
        .collect::<Vec<_>>();
    (!kept.is_empty()).then(|| Value::Array(kept))
}

fn page_base(receipt: &Value) -> Map<String, Value> {
    let mut page = Map::new();
    if receipt.get("source").is_some_and(Value::is_object) {
        let mut source = receipt["source"].clone();
        source
            .as_object_mut()
            .map(|source| source.remove("evidenceHash"));
        if source.as_object().is_some_and(|source| !source.is_empty()) {
            page.insert("source".into(), source);
        }
    }
    if let Some(scope) = receipt.get("scope") {
        page.insert("scope".into(), scope.clone());
    }
    if let Some(view) = receipt.get("view") {
        page.insert("view".into(), view.clone());
    }
    if let Some(limitations) = limitations(receipt) {
        page.insert("limitations".into(), limitations);
    }
    if let Some(read) = receipt.get("read") {
        page.insert("next".into(), json!({"read":read}));
    }
    page
}

/// Render one resource. `question_ids` orders the per-page answer map.
pub(super) fn resource(
    resource_id: &Value,
    question_ids: &[&Value],
    pages: Vec<PageOutcome>,
    has_continuation: bool,
) -> Value {
    let mut answered = false;
    let mut failed = false;
    let mut terminal_partial = false;
    let rendered = pages
        .into_iter()
        .map(|page| match page {
            PageOutcome::Failed { error, receipt } => {
                failed = true;
                terminal_partial = receipt["coverage"] == "partial";
                let mut page = page_base(&receipt);
                page.insert("error".into(), error_value(&error));
                Value::Object(page)
            }
            PageOutcome::Assessed { receipt, answers } => {
                terminal_partial = receipt["coverage"] == "partial";
                let mut page = page_base(&receipt);
                let mut by_question = Map::new();
                for (id, answer) in question_ids.iter().zip(answers) {
                    let key = id.as_str().unwrap_or_default().to_owned();
                    let value = match answer {
                        Ok(data) => {
                            answered = true;
                            compact_answer(&data["answer"])
                        }
                        Err(error) => {
                            failed = true;
                            json!({"error":error_value(&error)})
                        }
                    };
                    by_question.insert(key, value);
                }
                page.insert("answers".into(), Value::Object(by_question));
                Value::Object(page)
            }
        })
        .collect::<Vec<_>>();
    let coverage = if !answered {
        "error"
    } else if failed || has_continuation || terminal_partial {
        "partial"
    } else {
        "complete"
    };
    json!({"resourceId":resource_id,"coverage":coverage,"pages":rendered})
}

fn merge_scope(first: Option<&Value>, last: Option<&Value>) -> Option<Value> {
    let (first, last) = (first?, last?);
    if let (Some(start), Some(end)) = (first.get("startLine"), last.get("endLine")) {
        return Some(json!({"startLine":start,"endLine":end,"totalLines":last["totalLines"]}));
    }
    if let (Some(start), Some(end)) = (first.get("byteOffset"), last.get("byteEnd")) {
        return Some(json!({"byteOffset":start,"byteEnd":end,"totalBytes":last["totalBytes"]}));
    }
    None
}

fn adjacent_scopes(previous: &Value, next: &Value) -> bool {
    // Adjacent line numbers from different observed file versions are not a
    // continuous captured excerpt.
    for field in ["path", "modified", "ref"] {
        let left = previous.pointer(&format!("/source/{field}"));
        let right = next.pointer(&format!("/source/{field}"));
        if left != right {
            return false;
        }
    }
    let (Some(previous), Some(next)) = (previous.get("scope"), next.get("scope")) else {
        return false;
    };
    match (
        previous.get("endLine").and_then(Value::as_u64),
        next.get("startLine").and_then(Value::as_u64),
    ) {
        (Some(end), Some(start)) => {
            end.checked_add(1) == Some(start)
                && previous.get("totalLines") == next.get("totalLines")
        }
        _ => {
            previous.get("byteEnd").and_then(Value::as_u64)
                == next.get("byteOffset").and_then(Value::as_u64)
                && previous.get("byteEnd").is_some()
                && previous.get("totalBytes") == next.get("totalBytes")
        }
    }
}

fn merge_receipts(receipts: &[Value]) -> Value {
    let (Some(first), Some(last)) = (receipts.first(), receipts.last()) else {
        return Value::Null;
    };
    let mut merged = last.clone();
    match merge_scope(first.get("scope"), last.get("scope")) {
        Some(scope) => merged["scope"] = scope,
        None => {
            if let Some(object) = merged.as_object_mut() {
                object.remove("scope");
            }
        }
    }
    let mut all = Vec::new();
    for receipt in receipts {
        for limitation in receipt
            .get("limitations")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if !all.contains(limitation) {
                all.push(limitation.clone());
            }
        }
    }
    if let Some(object) = merged.as_object_mut() {
        if all.is_empty() {
            object.remove("limitations");
        } else {
            object.insert("limitations".into(), Value::Array(all));
        }
    }
    merged
}

/// Adjacent pages of one file read as one contiguous `{path, lines, content}`
/// so the provider judges a single excerpt instead of an array of fragments.
fn merge_file_evidence(states: &[Value]) -> Option<Value> {
    let first = states.first()?.as_object()?;
    first.get("path")?;
    let mut content = String::new();
    for state in states {
        let entry = state.as_object()?;
        if entry.get("path") != first.get("path") || entry.get("repo") != first.get("repo") {
            return None;
        }
        content.push_str(entry.get("content")?.as_str()?);
    }
    let mut merged = first.clone();
    let start = first.get("lines").and_then(|lines| lines.get(0)).cloned();
    let end = states
        .last()
        .and_then(|state| state.get("lines"))
        .and_then(|lines| lines.get(1))
        .cloned();
    match (start, end) {
        (Some(start), Some(end)) => {
            merged.insert("lines".into(), json!([start, end]));
        }
        _ => {
            merged.remove("lines");
        }
    }
    merged.insert("content".into(), json!(content));
    Some(Value::Object(merged))
}

/// Join adjacent same-resource pages into provider states of at most
/// `max_bytes` serialized JSON, so one judgment covers a larger contiguous
/// scope. A page larger than the budget stays alone.
pub(super) fn coalesce(pages: Vec<(Value, Value)>, max_bytes: usize) -> Vec<(Value, Value)> {
    let mut output = Vec::new();
    let mut states: Vec<Value> = Vec::new();
    let mut receipts: Vec<Value> = Vec::new();
    let mut bytes = 0usize;
    let flush =
        |states: &mut Vec<Value>, receipts: &mut Vec<Value>, output: &mut Vec<(Value, Value)>| {
            match states.len() {
                0 => {}
                1 => output.push((states.remove(0), receipts.remove(0))),
                _ => {
                    let joined = std::mem::take(states);
                    let state = merge_file_evidence(&joined).unwrap_or(Value::Array(joined));
                    let mut receipt = merge_receipts(receipts);
                    let evidence_hash = hex::encode(Sha256::digest(state.to_string().as_bytes()));
                    receipt["resultHash"] = json!(evidence_hash);
                    if receipt.get("source").is_some_and(Value::is_object) {
                        receipt["source"]["evidenceHash"] = json!(evidence_hash);
                    }
                    output.push((state, receipt));
                    receipts.clear();
                }
            }
        };
    for (state, receipt) in pages {
        let size = state.to_string().len();
        if !states.is_empty()
            && (bytes.saturating_add(size) > max_bytes
                || !receipts
                    .last()
                    .is_some_and(|previous| adjacent_scopes(previous, &receipt)))
        {
            flush(&mut states, &mut receipts, &mut output);
            bytes = 0;
        }
        bytes = bytes.saturating_add(size);
        states.push(state);
        receipts.push(receipt);
    }
    flush(&mut states, &mut receipts, &mut output);
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider_error(code: &str) -> ClassificationError {
        ClassificationError {
            code: code.into(),
            message: "failed".into(),
            hints: Vec::new(),
            ..Default::default()
        }
    }

    #[test]
    fn answers_drop_redundant_fields_but_keep_all_probabilities_and_precision() {
        assert_eq!(
            compact_answer(&json!({"type":"noul","noul":0.973456789})),
            json!({"noul":0.973456789})
        );
        assert_eq!(
            compact_answer(
                &json!({"type":"choice","choice":"a","confidence":0.923456789,
                "probabilities":{"a":0.995,"b":0.004,"c":0.001,"d":0.0}})
            ),
            json!({"choice":"a","confidence":0.923456789,
                "probabilities":{"a":0.995,"b":0.004,"c":0.001,"d":0.0}})
        );
        assert_eq!(
            compact_answer(
                &json!({"type":"score","score":2.123456789,"confidence":0.823456789,
                "probabilities":{"0":0.0001,"1":0.0999,"2":0.7,"3":0.2},"legend":{"0":"low"}})
            ),
            json!({"score":2.123456789,"confidence":0.823456789,
                "probabilities":{"0":0.0001,"1":0.0999,"2":0.7,"3":0.2}})
        );
        assert_eq!(
            compact_answer(&json!({"type":"locate","exists":0.98,"matches":[{
                "startLine":41,"endLine":44,"probability":0.91
            }]})),
            json!({"exists":0.98,"matches":[{
                "startLine":41,"endLine":44,"probability":0.91
            }]})
        );
    }

    #[test]
    fn resource_is_rendered_once_per_page_with_answers_keyed_by_question() {
        let ids = [json!("retry"), json!("role")];
        let ids = ids.iter().collect::<Vec<_>>();
        let receipt = json!({"source":"tool","tool":"localFetch","resultHash":"x","coverage":"bounded",
        "scope":{"startLine":1,"endLine":9,"totalLines":9},
        "read":{"tool":"localFetch","confidence":"exact","query":{
            "reasoning":"Verify evidence.","path":"/repo/a.rs","startLine":1,"endLine":9
        }}});
        let rendered = resource(
            &json!("file"),
            &ids,
            vec![PageOutcome::Assessed {
                receipt,
                answers: vec![
                    Ok(json!({"answer":{"type":"noul","noul":0.9},"resolvedModel":"m"})),
                    Err(provider_error("timeout")),
                ],
            }],
            false,
        );
        assert_eq!(
            rendered,
            json!({"resourceId":"file","coverage":"partial","pages":[{
                "scope":{"startLine":1,"endLine":9,"totalLines":9},
                "next":{"read":{"tool":"localFetch","confidence":"exact","query":{
                    "reasoning":"Verify evidence.","path":"/repo/a.rs","startLine":1,"endLine":9
                }}},
                "answers":{"retry":{"noul":0.9},"role":{"error":{"code":"timeout","message":"failed"}}}
            }]})
        );
    }

    #[test]
    fn coverage_is_error_without_answers_and_partial_with_continuation() {
        let ids = [json!("q")];
        let ids = ids.iter().collect::<Vec<_>>();
        let failed = resource(
            &json!("r"),
            &ids,
            vec![PageOutcome::Failed {
                error: provider_error("classificationContextFailed"),
                receipt: json!({"coverage":"partial","limitations":["Context retrieval failed; classification was not run."]}),
            }],
            false,
        );
        assert_eq!(failed["coverage"], "error");
        assert_eq!(
            failed["pages"][0]["error"]["code"],
            "classificationContextFailed"
        );
        assert_eq!(
            failed["pages"][0]["limitations"],
            json!(["Context retrieval failed; classification was not run."])
        );
        let pending = resource(
            &json!("r"),
            &ids,
            vec![PageOutcome::Assessed {
                receipt: json!({"coverage":"partial","limitations":[PAGE_ONLY_LIMITATION]}),
                answers: vec![Ok(json!({"answer":{"type":"noul","noul":0.1}}))],
            }],
            true,
        );
        assert_eq!(pending["coverage"], "partial");
        assert!(pending["pages"][0].get("limitations").is_none());
    }

    #[test]
    fn adjacent_pages_coalesce_within_budget_and_merge_scope() {
        let page = |start: u64, end: u64, body: &str| {
            (
                json!({"data":{"content":body}}),
                json!({"coverage":"bounded","scope":{"startLine":start,"endLine":end,"totalLines":300}}),
            )
        };
        let body = "x".repeat(100);
        let merged = coalesce(
            vec![
                page(1, 100, &body),
                page(101, 200, &body),
                page(201, 300, &body),
            ],
            250,
        );
        assert_eq!(merged.len(), 2);
        assert!(merged[0].0.is_array());
        assert_eq!(
            merged[0].1["scope"],
            json!({"startLine":1,"endLine":200,"totalLines":300})
        );
        assert_eq!(merged[1].1["scope"]["startLine"], 201);
        assert!(
            merged[1].0.is_object(),
            "a lone page keeps its original state"
        );
    }

    #[test]
    fn coalesced_evidence_has_its_own_hash_and_versions_do_not_merge() {
        let page = |start: u64, end: u64, modified: &str| {
            (
                json!({"path":"a.rs","lines":[start,end],"content":"x\n"}),
                json!({"coverage":"bounded","resultHash":format!("capture-{start}"),
                "source":{"evidenceHash":format!("capture-{start}"),"path":"a.rs","modified":modified},
                "scope":{"startLine":start,"endLine":end,"totalLines":3}}),
            )
        };
        let merged = coalesce(vec![page(1, 1, "v1"), page(2, 2, "v1")], 10000);
        assert_eq!(merged.len(), 1);
        assert_eq!(
            merged[0].1["source"]["evidenceHash"],
            merged[0].1["resultHash"]
        );
        assert_ne!(merged[0].1["source"]["evidenceHash"], "capture-2");
        assert_eq!(
            merged[0].1["scope"],
            json!({"startLine":1,"endLine":2,"totalLines":3})
        );
        let distinct = coalesce(vec![page(1, 1, "v1"), page(2, 2, "v2")], 10000);
        assert_eq!(distinct.len(), 2);
    }

    #[test]
    fn disjoint_pages_do_not_claim_one_contiguous_scope() {
        let page = |start: u64, end: u64| {
            (
                json!({"path":"/tmp/a.rs","lines":[start,end],"content":"x\n"}),
                json!({"coverage":"bounded","scope":{"startLine":start,"endLine":end,"totalLines":1000}}),
            )
        };
        let result = coalesce(vec![page(1, 1), page(900, 900)], 10_000);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].1["scope"]["endLine"], 1);
        assert_eq!(result[1].1["scope"]["startLine"], 900);
    }

    #[test]
    fn adjacent_file_evidence_merges_into_one_excerpt() {
        let page = |start: u64, end: u64, text: &str| {
            (
                json!({"path":"a.rs","lines":[start,end],"content":text}),
                json!({"coverage":"bounded","scope":{"startLine":start,"endLine":end,"totalLines":20}}),
            )
        };
        let merged = coalesce(vec![page(1, 10, "one\n"), page(11, 20, "two\n")], 1_000);
        assert_eq!(merged.len(), 1);
        assert_eq!(
            merged[0].0,
            json!({"path":"a.rs","lines":[1,20],"content":"one\ntwo\n"})
        );
    }

    #[test]
    fn oversized_page_is_never_merged() {
        let big = (json!("y".repeat(500)), json!({"coverage":"bounded"}));
        let small = (json!("z"), json!({"coverage":"bounded"}));
        let merged = coalesce(vec![small.clone(), big, small], 100);
        assert_eq!(merged.len(), 3);
    }
}
