use super::emit::{answered_cell, emit, terminal, terminal_saved};
use super::*;

use octocode_runtime_base::cache::CacheConfig;
use octocode_runtime_base::execution::SelectionLimits;
use octocode_test_support::json_fixture;
use std::time::{Duration, Instant};

fn test_context(timeout: Duration) -> ExecutionContext {
    ExecutionContext {
        cancellation: octocode_io::cancellation::Cancellation::new(),
        deadline: Some(Instant::now() + timeout),
        response_window: None,
        github_credential: None,
    }
}

pub(super) fn session() -> Session {
    Session {
        id: "a".repeat(64),
        context: None,
        questions: json_fixture(
            r#"{
                "a": {"type": "noul", "instructions": "a"},
                "b": {"type": "noul", "instructions": "b"}
            }"#,
        ),
        keys: vec!["a".into(), "b".into()],
        model: Some("jev-1.13.0".into()),
        resolved_model: None,
        long_context: LongContext::OnRefusal,
        resources: vec![Resource {
            key: "resource".into(),
            metadata: json_fixture(r#"{"status": "ready"}"#),
            local_check: None,
            shown: None,
            units: vec![Unit {
                metadata: json_fixture(
                    r#"{
                        "id": "s0",
                        "kind": "text",
                        "target": {"bytes": [0, 4], "lines": [1, 1]},
                        "context": []
                    }"#,
                ),
                state: json_fixture(r#"{"target": {"text": "text"}}"#),
                source_target: [0, 4],
                chars: 26,
                cells: vec![
                    Some(answered_cell(json_fixture(
                        r#"{"type": "noul", "noul": 0.9}"#,
                    ))),
                    None,
                ],
                emitted: false,
                deferred: std::collections::BTreeSet::new(),
            }],
        }],
        github_identity: None,
        batches: Vec::new(),
        source_disclosures: None,
        active: Vec::new(),
        request: Value::Null,
    }
}

#[test]
fn a_section_state_scopes_every_question_to_its_target() {
    let state = json_fixture(
        r#"{
            "target": {"range": "1-2", "text": "a"},
            "context": [{"range": "3", "text": "b"}]
        }"#,
    );
    let sent = super::super::rows::scoped(std::borrow::Cow::Borrowed(&state));
    let question = sent["question"].as_str().expect("section scope");
    assert!(question.contains("target"), "{question}");
    assert_eq!(
        octocode_tools_shared::selection::section_question().and_then(serde_json::Value::as_str),
        Some(question),
        "the contract's text, verbatim"
    );
    assert_eq!(sent["target"], state["target"]);
    assert_eq!(sent["context"], state["context"]);
    // With task context the scope still sits at the top of the state.
    let wrapped =
        super::super::rows::scoped(super::super::rows::contextual_state(&state, Some("task")));
    assert_eq!(wrapped["question"], question);
    assert_eq!(wrapped["content"], state);
}

#[test]
fn section_answer_projection_preserves_values_and_distributions_with_shared_type() {
    for (provider, expected) in [
        (
            json_fixture(r#"{"type": "noul", "noul": 0.9}"#),
            json_fixture(r#"{"status": "answered", "answer": {"noul": 0.9}}"#),
        ),
        (
            json_fixture(
                r#"{
                    "type": "choice",
                    "choice": "present",
                    "confidence": 0.75,
                    "probabilities": {"present": 0.75, "absent": 0.2, "unknown": 0.05}
                }"#,
            ),
            json_fixture(
                r#"{
                    "status": "answered",
                    "answer": {
                        "choice": "present",
                        "confidence": 0.75,
                        "probabilities": {"present": 0.75, "absent": 0.2, "unknown": 0.05}
                    }
                }"#,
            ),
        ),
        (
            json_fixture(
                r#"{
                    "type": "score",
                    "score": 1,
                    "confidence": 0.8,
                    "probabilities": {"0": 0.2, "1": 0.8},
                    "legend": {"0": "absent", "1": "present"}
                }"#,
            ),
            json_fixture(
                r#"{
                    "status": "answered",
                    "answer": {
                        "score": 1,
                        "confidence": 0.8,
                        "probabilities": {"0": 0.2, "1": 0.8}
                    }
                }"#,
            ),
        ),
    ] {
        assert_eq!(answered_cell(provider), expected);
    }
}

#[test]
fn partial_cells_are_held_until_the_complete_section_emits_exactly_once() {
    let mut session = session();
    let usage = ClassificationUsage::default();
    let first = emit(&mut session, &usage, None);
    assert_eq!(first["resources"]["resource"]["sections"], json!([]));
    assert_eq!(
        first["resources"]["resource"]["coverage"]["answered"],
        json!(1)
    );
    assert_eq!(
        first["resources"]["resource"]["coverage"]["unassessed"],
        json!(1)
    );
    session.resources[0].units[0].cells[1] = Some(json_fixture(
        r#"{
            "status": "error",
            "error": {
                "errorCode": "classificationProviderError",
                "error": "refused"
            }
        }"#,
    ));
    let second = emit(&mut session, &usage, None);
    assert_eq!(
        second["resources"]["resource"]["sections"]
            .as_array()
            .map(Vec::len),
        Some(1)
    );
    assert_eq!(
        second["resources"]["resource"]["sections"][0]["answers"]
            .as_object()
            .map(serde_json::Map::len),
        Some(2)
    );
    assert!(
        second["resources"]["resource"]["coverage"]
            .get("remainingSections")
            .is_none(),
        "an absent count is 0"
    );
    let third = emit(&mut session, &usage, None);
    // Nothing new to say: the contract still needs a resource, with no sections.
    assert_eq!(
        third["resources"]["resource"]["sections"],
        json!([]),
        "{third}"
    );
}

#[test]
fn a_later_batch_repeats_only_resources_with_new_sections_or_coverage() {
    let mut session = session();
    let mut idle = session.resources[0].clone();
    idle.key = "idle".into();
    idle.units[0].cells = vec![None, None];
    session.resources.push(idle);
    let usage = ClassificationUsage::default();
    let first = emit(&mut session, &usage, None);
    assert!(first["resources"].get("resource").is_some());
    assert!(first["resources"].get("idle").is_some());
    session.resources[0].units[0].cells[1] = Some(answered_cell(json_fixture(
        r#"{"type": "noul", "noul": 0.1}"#,
    )));
    let second = emit(&mut session, &usage, None);
    assert_eq!(
        second["resources"]["resource"]["sections"]
            .as_array()
            .map(Vec::len),
        Some(1)
    );
    assert!(
        second["resources"].get("idle").is_none(),
        "a resource with no progress is not resent: {second}"
    );
    session.resources[1].units[0].cells[0] = Some(answered_cell(json_fixture(
        r#"{"type": "noul", "noul": 0.2}"#,
    )));
    let third = emit(&mut session, &usage, None);
    assert!(third["resources"].get("resource").is_none());
    assert_eq!(
        third["resources"]["idle"]["coverage"]["answered"],
        json!(1),
        "partial progress changes coverage and is reported: {third}"
    );
    let terminal = terminal(&mut session, "budget ended", usage);
    assert!(terminal.output["resources"].get("resource").is_none());
    assert_eq!(
        terminal.output["resources"]["idle"]["coverage"]["terminalReason"],
        json!("budget ended")
    );
}

/// The row's `model` is the one that answered; the requested alias of a
/// plan nothing was judged under is not an answering model.
#[test]
fn a_plan_no_model_answered_reports_no_model() {
    let mut session = session();
    session.resolved_model = None;
    let output = emit(&mut session, &ClassificationUsage::default(), None);
    assert!(output.get("model").is_none(), "{output}");
    session.resolved_model = Some("jev-1.14.0".into());
    let output = emit(&mut session, &ClassificationUsage::default(), None);
    assert_eq!(output["model"], "jev-1.14.0");
}

/// The shared questions are every Score cell's only legend, so each batch's
/// stored output carries them and a cursor page is self-contained; a fresh
/// first page loses them as an echo of the request.
#[test]
fn every_batch_carries_the_shared_questions_and_a_fresh_first_page_drops_them() {
    let mut session = session();
    let first = emit(&mut session, &ClassificationUsage::default(), None);
    assert_eq!(first["questions"], session.questions, "{first}");
    assert!(first.get("operation").is_none(), "{first}");
    session.batches.push(first.clone());
    let later = emit(&mut session, &ClassificationUsage::default(), None);
    assert_eq!(later["questions"], session.questions, "{later}");
    let request = json!({"operation":"sections","questions":session.questions});
    let mut page = json!({"results":[{"index":0,"data":first}]});
    octocode_runtime_base::response::pages::drop_echoes(false, &mut page, &[request], &[0]);
    assert!(
        page["results"][0]["data"].get("questions").is_none(),
        "{page}"
    );
    assert!(
        page["results"][0]["data"]["resources"].is_object(),
        "{page}"
    );
}

#[test]
fn terminal_outcome_preserves_answer_and_explains_every_unassessed_cell() {
    let mut session = session();
    let outcome = terminal(&mut session, "budget ended", ClassificationUsage::default());
    let section = &outcome.output["resources"]["resource"]["sections"][0];
    assert_eq!(section["answers"]["a"]["answer"]["noul"], json!(0.9));
    assert_eq!(
        section["answers"]["b"],
        json_fixture(r#"{"status": "unassessed", "reason": "budget ended"}"#)
    );
    assert_eq!(
        outcome.output["resources"]["resource"]["coverage"]["terminalReason"],
        json!("budget ended")
    );
    assert!(outcome.output.get("next").is_none());
}

#[test]
fn assessment_locks_exclude_the_same_plan_but_not_an_independent_plan() {
    let directory = tempfile::tempdir().expect("lock directory");
    let first = Sessions::new(
        CacheConfig::derived(),
        Some(directory.path().to_owned()),
        "policy".into(),
    );
    let second = Sessions::new(
        CacheConfig::derived(),
        Some(directory.path().to_owned()),
        "policy".into(),
    );
    let key = first.key(&[1; 32], &"a".repeat(64));
    let other = first.key(&[1; 32], &"b".repeat(64));
    let held = first
        .transaction_lock(&key, &test_context(Duration::from_secs(1)))
        .expect("first transaction");
    assert!(
        second
            .transaction_lock(&key, &test_context(Duration::from_millis(20)))
            .is_err()
    );
    assert!(
        second
            .transaction_lock(&other, &test_context(Duration::from_millis(20)))
            .is_ok()
    );
    assert!(second.save(
        other.clone(),
        session(),
        &test_context(Duration::from_secs(1))
    ));
    assert!(
        second
            .current(&other, &test_context(Duration::from_secs(1)))
            .expect("read")
            .is_some()
    );
    drop(held);
    assert!(
        second
            .transaction_lock(&key, &test_context(Duration::from_millis(20)))
            .is_ok()
    );
}

/// A failed read's own leads follow its hints by name, as the hint
/// lines a failed row publishes.
#[test]
fn an_error_preserves_executable_recovery_and_prose_separately() {
    let tree: octocode_contract_types::StructureSearchQuery =
        serde_json::from_value(json_fixture(r#"{"operation": "tree", "path": "src"}"#))
            .expect("a tree query");
    let lead = octocode_runtime_base::response::result::Continuation::related_query(tree).build();
    let mut error = ClassificationError::with_hints(
        ErrorCode::NotFound,
        "Path does not exist: nope.rs",
        Vec::new(),
    );
    assert_eq!(
        error_value(&error),
        json_fixture(
            r#"{
                "errorCode": "notFound",
                "error": "Path does not exist: nope.rs"
            }"#
        )
    );
    error.leads.push(("viewTree".into(), lead.clone()));
    error.hints.push("Check the spelling.".into());
    assert_eq!(
        error_value(&error)["hints"],
        json_fixture(r#"["Check the spelling."]"#)
    );
    assert_eq!(error_value(&error)["related"], json!([lead]));
}

#[test]
fn work_token_is_bounded_and_does_not_accept_arbitrary_plan_keys() {
    let id = "c".repeat(64);
    let value = token(&id, usize::MAX);
    assert!(
        value.len() <= 128,
        "work token exceeded the canonical bound"
    );
    assert_eq!(decode(&value), Some((id.as_str(), Some(usize::MAX))));
    assert_eq!(decode(&quiet_token(&id)), Some((id.as_str(), None)));
    assert!(quiet_token(&id).len() <= 128);
    assert!(decode("sections-v1:../other:0").is_none());
    assert!(decode(&format!("sections-v1:{}:no", id)).is_none());
}

#[test]
fn a_page_takes_its_first_unit_whatever_its_size_and_stops_at_its_budget() {
    let budget = Budget::new(SelectionLimits { max_chars: 9 });
    assert!(!budget.has_admissions());
    assert!(budget.admit(50, false), "the first unit always runs");
    assert!(!budget.admit(1, false), "the next page takes the rest");
    assert!(
        budget.admit(1, true),
        "a runtime with no next page takes all"
    );
    let budget = Budget::new(SelectionLimits { max_chars: 9 });
    assert!(budget.admit(5, false));
    assert!(budget.admit(4, false));
    assert!(!budget.admit(1, false));
}

#[test]
fn retained_work_is_partitioned_by_access_and_terminal_receipts_replay() {
    let sessions = Sessions::new(CacheConfig::derived(), None, "policy".into());
    let runtime = Runtime {
        sessions: &sessions,
        access: [1; 32],
        can_resume: true,
    };
    let mut session = session();
    let output = terminal_saved(
        &mut session,
        "terminal",
        ClassificationUsage::default(),
        runtime,
        &test_context(Duration::from_secs(5)),
    )
    .output;
    let key = sessions.key(&runtime.access, &session.id);
    let retained = sessions.store.get(&key).expect("terminal batch retained");
    assert_eq!(retained.value.batches[0], output);
    assert!(
        sessions
            .store
            .get(&sessions.key(&[2; 32], &session.id))
            .is_none()
    );
    let other = Sessions::new(CacheConfig::derived(), None, "other-policy".into());
    assert!(
        other
            .store
            .get(&other.key(&runtime.access, &session.id))
            .is_none()
    );
}

/// A one-`noul`-question session whose sections answered `answers`, ids `s0`, `s1`, ….
fn answered_session(answers: &[f64]) -> Session {
    let mut session = session();
    session.questions = json_fixture(r#"{"a": {"type": "noul", "instructions": "a"}}"#);
    session.keys = vec!["a".into()];
    let template = session.resources[0].units[0].clone();
    session.resources[0].units = answers
        .iter()
        .enumerate()
        .map(|(index, p)| {
            let mut unit = template.clone();
            unit.metadata["id"] = json!(format!("s{index}"));
            unit.cells = vec![Some(answered_cell(json!({"type":"noul","noul":p})))];
            unit
        })
        .collect();
    session
}

fn ids(output: &Value, resource: &str) -> Vec<String> {
    output["resources"][resource]["sections"]
        .as_array()
        .map(|sections| {
            sections
                .iter()
                .map(|section| section["id"].as_str().expect("id").to_owned())
                .collect()
        })
        .unwrap_or_default()
}

fn shown_ids(output: &Value) -> Vec<String> {
    ids(output, "resource")
}

/// One batch as a resumable runtime publishes it: emitted, then the hits view.
fn page(session: &mut Session, last: bool) -> (Value, emit::Held) {
    let mut output = emit(session, &ClassificationUsage::default(), None);
    let held = emit::attach_continuations(&mut output, session, last);
    (output, held)
}

/// Hits view: a section every `noul` answered below 0.3 is held back
/// and counted; the page shows what answered (RFC 2026-11-08 S15).
#[test]
fn quiet_sections_are_held_back_and_counted() {
    let mut answers = vec![0.2; 22];
    answers[7] = 0.53;
    let mut session = answered_session(&answers);
    let (output, _) = page(&mut session, true);
    assert_eq!(shown_ids(&output), ["s7"]);
    let coverage = &output["resources"]["resource"]["coverage"];
    assert_eq!(coverage["quiet"], json!(21), "{coverage}");
    assert_eq!(coverage["totalSections"], json!(22));
    let token = output["next"]["quiet"]["input"]["queries"][0]["assessment"]
        .as_str()
        .expect("next.quiet on the last batch");
    assert_eq!(decode(token), Some((session.id.as_str(), None)));
    let replay = emit::quiet_replay(&session);
    assert_eq!(replay["replayed"], json!(true));
    assert_eq!(shown_ids(&replay).len(), 21);
    assert!(!shown_ids(&replay).contains(&"s7".to_owned()));
}

#[test]
fn shown_sections_rank_by_their_highest_noul_answer() {
    let mut session = answered_session(&[0.4, 0.9, 0.2, 0.6]);
    let (output, _) = page(&mut session, true);
    assert_eq!(shown_ids(&output), ["s1", "s3", "s0"]);
    assert_eq!(
        output["resources"]["resource"]["coverage"]["quiet"],
        json!(1)
    );
}

#[test]
fn failed_cells_and_non_noul_questions_are_never_quiet() {
    let mut session = answered_session(&[0.1, 0.1]);
    session.resources[0].units[1].cells[0] = Some(json_fixture(
        r#"{
            "status": "error",
            "error": {
                "errorCode": "classificationProviderError",
                "error": "refused"
            }
        }"#,
    ));
    let (output, _) = page(&mut session, true);
    assert_eq!(shown_ids(&output), ["s1"], "an error is never quiet");
    let mut session = answered_session(&[0.1]);
    session.questions =
        json_fixture(r#"{"a": {"type": "choice", "criteria": {"x": "x", "y": "y"}}}"#);
    session.resources[0].units[0].cells = vec![Some(answered_cell(json_fixture(
        r#"{
            "type": "choice",
            "choice": "x",
            "confidence": 0.1,
            "probabilities": {"x": 0.55, "y": 0.45}
        }"#,
    )))];
    let (output, _) = page(&mut session, true);
    assert_eq!(shown_ids(&output), ["s0"], "a choice answer is shown");
    assert!(
        output["resources"]["resource"]["coverage"]
            .get("quiet")
            .is_none()
    );
    assert!(output["next"].get("quiet").is_none());
}

/// With nothing quiet the page is today's: no `coverage.quiet`, no `next.quiet`.
#[test]
fn nothing_quiet_keeps_todays_coverage() {
    let mut session = answered_session(&[0.9, 0.3]);
    let (output, _) = page(&mut session, true);
    assert_eq!(shown_ids(&output), ["s0", "s1"]);
    assert!(
        output["resources"]["resource"]["coverage"]
            .get("quiet")
            .is_none()
    );
    assert!(output["next"].get("quiet").is_none());
}

/// A runtime that keeps no plan never holds back: no `next.quiet` could read it.
#[test]
fn a_page_with_no_kept_plan_shows_every_section() {
    let mut session = answered_session(&[0.1, 0.9]);
    let output = emit(&mut session, &ClassificationUsage::default(), None);
    assert_eq!(shown_ids(&output), ["s0", "s1"]);
}

/// A plan that cannot be kept shows what it held back again (`Held::restore`).
#[test]
fn restore_shows_held_sections_and_drops_next_quiet() {
    let mut session = answered_session(&[0.1, 0.9]);
    let (mut output, held) = page(&mut session, true);
    assert_eq!(shown_ids(&output), ["s1"]);
    assert!(output["next"].get("quiet").is_some());
    held.restore(&mut output);
    let mut shown = shown_ids(&output);
    shown.sort();
    assert_eq!(shown, ["s0", "s1"]);
    assert!(output["next"].get("quiet").is_none());
    assert!(
        output["resources"]["resource"]["coverage"]
            .get("quiet")
            .is_none()
    );
}

/// Reachability walk over two resources, two batches, mixed
/// answers. Every planned section reaches the caller exactly once, through a
/// page or through the last batch's `next.quiet`; per resource, shown plus
/// quiet is the total; nothing quiet holds a cell that is not answered.
#[test]
fn every_section_is_reachable_exactly_once_across_pages_and_next_quiet() {
    let mut session = answered_session(&[0.1, 0.8, 0.2, 0.5, 0.05, 0.95]);
    let mut other = session.resources[0].clone();
    other.key = "other".into();
    other.shown = None;
    for (index, unit) in other.units.iter_mut().enumerate() {
        unit.metadata["id"] = json!(format!("o{index}"));
    }
    session.resources.push(other);
    let planned: Vec<(String, Vec<Option<Value>>)> = session
        .resources
        .iter()
        .map(|resource| {
            (
                resource.key.clone(),
                resource
                    .units
                    .iter()
                    .map(|unit| unit.cells[0].clone())
                    .collect(),
            )
        })
        .collect();
    // Batch 1 judges the first half of each resource, batch 2 the rest; an
    // error and an unassessed cell land in batch 2.
    for resource in &mut session.resources {
        for unit in resource.units.iter_mut().skip(3) {
            unit.cells[0] = None;
        }
    }
    let (first, _) = page(&mut session, false);
    assert!(
        first["next"].get("quiet").is_none(),
        "only the last batch lists them"
    );
    for (resource, (_, cells)) in session.resources.iter_mut().zip(&planned) {
        for (unit, cell) in resource.units.iter_mut().zip(cells).skip(3) {
            unit.cells[0] = cell.clone();
        }
    }
    session.resources[1].units[4].cells[0] = Some(json_fixture(
        r#"{
            "status": "error",
            "error": {
                "errorCode": "classificationProviderError",
                "error": "refused"
            }
        }"#,
    ));
    session.resources[1].units[5].cells[0] = Some(json_fixture(
        r#"{"status": "unassessed", "reason": "budget"}"#,
    ));
    let (second, _) = page(&mut session, true);
    assert!(second["next"].get("quiet").is_some());
    let replay = emit::quiet_replay(&session);
    let mut seen = std::collections::BTreeMap::<(String, String), usize>::new();
    for output in [&first, &second, &replay] {
        for key in ["resource", "other"] {
            for id in ids(output, key) {
                *seen.entry((key.to_owned(), id)).or_default() += 1;
            }
        }
    }
    let plan: std::collections::BTreeSet<(String, String)> = session
        .resources
        .iter()
        .flat_map(|resource| {
            resource.units.iter().map(|unit| {
                (
                    resource.key.clone(),
                    unit.metadata["id"].as_str().expect("id").to_owned(),
                )
            })
        })
        .collect();
    assert_eq!(
        seen.keys()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>(),
        plan
    );
    assert!(seen.values().all(|count| *count == 1), "{seen:?}");
    for key in ["resource", "other"] {
        let shown = ids(&first, key).len() + ids(&second, key).len();
        let quiet = second["resources"][key]["coverage"]["quiet"]
            .as_u64()
            .unwrap_or(0);
        let total = second["resources"][key]["coverage"]["totalSections"]
            .as_u64()
            .expect("total");
        assert_eq!(shown as u64 + quiet, total, "{key}");
    }
    for section in replay["resources"]
        .as_object()
        .expect("resources")
        .values()
        .flat_map(|row| row["sections"].as_array().into_iter().flatten())
    {
        assert!(
            section["answers"]
                .as_object()
                .expect("answers")
                .values()
                .all(|cell| cell["status"] == "answered"),
            "{section}"
        );
    }
}

/// Batch 1 is kept and holds a quiet section;
/// the terminal batch's plan cannot be kept, so no `next.quiet` can list
/// anything. Every section of both batches still reaches the caller once.
#[test]
fn a_terminal_batch_whose_plan_is_not_kept_shows_every_held_section() {
    let mut session = answered_session(&[0.1, 0.9, 0.2, 0.8]);
    let planned: Vec<Option<Value>> = session.resources[0]
        .units
        .iter()
        .map(|unit| unit.cells[0].clone())
        .collect();
    for unit in session.resources[0].units.iter_mut().skip(2) {
        unit.cells[0] = None;
    }
    let (first, _) = page(&mut session, false);
    assert_eq!(shown_ids(&first), ["s1"], "s0 is held back on batch 1");
    for (unit, cell) in session.resources[0].units.iter_mut().zip(&planned).skip(2) {
        unit.cells[0] = cell.clone();
    }
    // A disk store that admits no entry: the terminal save fails.
    let dir = tempfile::tempdir().expect("dir");
    let sessions = Sessions::new(
        CacheConfig { disk_bytes: 1 },
        Some(dir.path().to_path_buf()),
        "policy".into(),
    );
    let runtime = Runtime {
        sessions: &sessions,
        access: [1; 32],
        can_resume: true,
    };
    let terminal = terminal_saved(
        &mut session,
        "terminal",
        ClassificationUsage::default(),
        runtime,
        &test_context(Duration::from_secs(5)),
    )
    .output;
    assert!(
        sessions
            .store
            .get(&sessions.key(&runtime.access, &session.id))
            .is_none(),
        "the plan was not kept"
    );
    assert!(terminal["next"].get("quiet").is_none(), "{terminal:#}");
    let mut seen = shown_ids(&first);
    seen.extend(shown_ids(&terminal));
    seen.sort();
    assert_eq!(seen, ["s0", "s1", "s2", "s3"], "{terminal:#}");
}

/// A continuation that finds the previous process stopped
/// mid-batch ends the plan like every terminal batch. Batch 1 was kept and
/// held `s0` back; the interrupted batch's in-flight cells become errors.
/// With the plan kept, `next.quiet` lists `s0`; with no plan kept, the
/// terminal output shows it. Either way every section reaches the caller
/// exactly once, and shown plus `coverage.quiet` is `totalSections`.
#[test]
fn an_interrupted_continuation_keeps_every_held_section_reachable() {
    for kept in [true, false] {
        let mut session = answered_session(&[0.1, 0.9, 0.2, 0.8]);
        for unit in session.resources[0].units.iter_mut().skip(2) {
            unit.cells[0] = None;
        }
        let (first, _) = page(&mut session, false);
        assert_eq!(shown_ids(&first), ["s1"], "s0 is held back on batch 1");
        session.active = vec![[0, 2], [0, 3]];
        let dir = tempfile::tempdir().expect("dir");
        // A disk store that admits no entry: the terminal save fails.
        let sessions = if kept {
            Sessions::new(CacheConfig::derived(), None, "policy".into())
        } else {
            Sessions::new(
                CacheConfig { disk_bytes: 1 },
                Some(dir.path().to_path_buf()),
                "policy".into(),
            )
        };
        let runtime = Runtime {
            sessions: &sessions,
            access: [1; 32],
            can_resume: true,
        };
        let output =
            interrupted(&mut session, runtime, &test_context(Duration::from_secs(5))).output;
        let resource = &output["resources"]["resource"];
        for section in resource["sections"].as_array().expect("sections") {
            if ["s2", "s3"].contains(&section["id"].as_str().expect("id")) {
                assert_eq!(section["answers"]["a"]["status"], "error", "{section}");
            }
        }
        let mut seen = shown_ids(&first);
        seen.extend(shown_ids(&output));
        let shown = seen.len() as u64;
        let retained = sessions
            .store
            .get(&sessions.key(&runtime.access, &session.id));
        if kept {
            let retained = retained.expect("the plan is kept for next.quiet");
            assert!(
                output["next"].get("quiet").is_some(),
                "kept={kept}: {output:#}"
            );
            seen.extend(shown_ids(&emit::quiet_replay(&retained.value)));
        } else {
            assert!(retained.is_none(), "the plan was not kept");
            assert!(output["next"].get("quiet").is_none(), "{output:#}");
        }
        seen.sort();
        assert_eq!(
            seen,
            ["s0", "s1", "s2", "s3"],
            "kept={kept}: next.quiet offered {}: {output:#}",
            output["next"].get("quiet").is_some()
        );
        let quiet = resource["coverage"]["quiet"].as_u64().unwrap_or(0);
        assert_eq!(
            shown + quiet,
            resource["coverage"]["totalSections"]
                .as_u64()
                .expect("total"),
            "kept={kept}: {output:#}"
        );
    }
}

mod quiet_identity;

#[test]
fn classification_disclosures_replay_keeps_original_warnings_and_the_pages_spend() {
    let warnings = json_fixture(
        r#"[
            "One hidden path was withheld from the resource expansion.",
            "1 evaluation refused in the default window retried in the long-context window: 40 input tokens billed."
        ]"#,
    );
    let saved = json!({
        "resources":{"source":{"sections":[],"coverage":{"totalSections":2,"unassessed":1}}},
        "warnings":warnings,
        "usage":{"calls":1,"inputTokens":40,"outputTokens":7},
    });
    let replay = replayed(&saved);
    assert_eq!(replay["warnings"], warnings, "{replay}");
    assert_eq!(replay["resources"], saved["resources"], "{replay}");
    assert_eq!(replay["replayed"], true, "{replay}");
    // The replay billed nothing; its usage is the page's own spend, marked.
    for (counter, spent) in [("calls", 1), ("inputTokens", 40), ("outputTokens", 7)] {
        assert_eq!(replay["usage"][counter], spent, "{counter}: {replay}");
    }
    assert_eq!(replay["usage"]["replayed"], true, "{replay}");
    assert_eq!(saved["usage"].get("replayed"), None);
    assert_eq!(replayed(&saved), replay);
}

#[test]
fn classification_disclosures_captured_scope_survives_terminal_first_emission() {
    let disclosures = json_fixture(
        r#"{
            "warnings": ["A hidden matching path was withheld."],
            "related": [
                {
                    "kind": "related",
                    "tool": "structureSearch",
                    "input": {
                        "queries": [
                            {"path": "src", "operation": "files", "includeHidden": true}
                        ]
                    }
                }
            ]
        }"#,
    );
    let mut captured = session();
    captured.source_disclosures = Some(disclosures.clone());
    captured.active = vec![[0, 0]];
    let saved = serde_json::to_value(&captured).expect("pre-inference plan");
    let mut restored: Session = serde_json::from_value(saved).expect("restored plan");
    let output = emit(
        &mut restored,
        &ClassificationUsage::default(),
        Some("interrupted"),
    );
    assert_eq!(output["warnings"], disclosures["warnings"], "{output}");
    assert_eq!(output["related"], disclosures["related"], "{output}");
    let pending = restored.push_batch(&output);
    assert!(pending.is_some());
    assert!(
        restored.source_disclosures.is_none(),
        "saved once, in the batch"
    );
    assert_eq!(
        replayed(&restored.batches[0])["warnings"],
        disclosures["warnings"]
    );
    assert_eq!(
        replayed(&restored.batches[0])["related"],
        disclosures["related"]
    );
}

/// The JSON-escaped bytes of `text` as a provider request carries them.
fn escaped_bytes(text: &str) -> usize {
    serde_json::to_string(text).expect("json").len() - 2
}

/// A section sized for a byte window stays within it whatever the density
/// of the characters JSON escapes: a source light in quotes and control
/// characters beside a dense stretch is sized by the dense stretch's
/// measured bytes, not by the source's average.
#[test]
fn a_section_dense_in_escaped_characters_stays_within_the_window() {
    let light = "plain words ".repeat(400);
    let dense = "\"\t\u{1}\\\n".repeat(300);
    let text = format!("{light}{dense}{light}");
    let room = 2_000;
    let chars = capture::chars_within(&[text.as_str()], room);
    assert!(chars >= 1, "{chars}");
    let all: Vec<char> = text.chars().collect();
    let worst = all
        .windows(chars)
        .map(|window| escaped_bytes(&window.iter().collect::<String>()))
        .max()
        .expect("a window");
    assert!(
        worst <= room,
        "{chars} chars overshoot the {room} byte window by {}",
        worst - room
    );
    // Light text alone is not cut smaller than its bytes need.
    let plain = capture::chars_within(&[light.as_str()], room);
    assert_eq!(plain, room, "one byte per character");
}
