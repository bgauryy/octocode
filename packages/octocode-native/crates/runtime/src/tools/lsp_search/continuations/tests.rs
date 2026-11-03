use super::*;
use serde_json::json;

fn query() -> LspSearchQuery {
    serde_json::from_value(
        json!({"operation":"references","path":"/repo/a.ts","symbolName":"target","line":1}),
    )
    .expect("query")
}
fn selected(value: &str) -> ResponseScope {
    ResponseScope {
        generation: "source-revision".into(),
        answers: Some(Arc::new(Mutex::new(
            serde_json::from_value(json!({"provider-request":{"text":value,"rows":[1,2,3]}}))
                .expect("answers"),
        ))),
        ..Default::default()
    }
}
fn row() -> Value {
    let call = crate::response::paging::next_page_call(
        crate::id::ToolId::LspSearch,
        lsp_search_query::to_row(&query()),
        "page",
        2,
        &[("snapshot", json!("lsp-v1:selected"))],
    )
    .expect("call");
    json!({"status":"success","operation":"references","pagination":{"hasMore":true,"next":{"nextPage":call}}})
}
fn next(row: &Value) -> LspSearchQuery {
    serde_json::from_value(row["pagination"]["next"]["nextPage"]["input"]["queries"][0].clone())
        .expect("issued query")
}
fn raw(scope: &ResponseScope) -> Value {
    serde_json::to_value(
        &*scope
            .answers
            .as_ref()
            .expect("answers")
            .lock()
            .expect("lock"),
    )
    .expect("raw")
}

#[test]
fn issued_pages_keep_the_original_answer_when_a_fresh_provider_answer_changes() {
    let dir = tempfile::tempdir().expect("directory");
    let store = ContinuationAnswers::new(
        CacheConfig::derived(),
        Some(dir.path().into()),
        "policy".into(),
    );
    let mut first = row();
    store
        .retain(&mut first, &selected("FIRST_PROVIDER_EVIDENCE"))
        .expect("retain");
    let first_query = next(&first);
    let mut fresh = row();
    store
        .retain(&mut fresh, &selected("CHANGED_PROVIDER_EVIDENCE"))
        .expect("retain changed answer");
    assert_ne!(
        lsp_search_query::snapshot(&first_query),
        lsp_search_query::snapshot(&next(&fresh))
    );
    assert!(
        !first.to_string().contains("PROVIDER_EVIDENCE"),
        "private raw evidence stays in owned state"
    );
    let restored = ContinuationAnswers::new(
        CacheConfig::derived(),
        Some(dir.path().into()),
        "policy".into(),
    );
    let (native, original) = restored
        .open(first_query.clone())
        .expect("original continuation");
    assert_eq!(lsp_search_query::snapshot(&native), Some("lsp-v1:selected"));
    assert_eq!(raw(&original), raw(&selected("FIRST_PROVIDER_EVIDENCE")));
    assert_eq!(
        raw(&restored.open(next(&fresh)).expect("new continuation").1),
        raw(&selected("CHANGED_PROVIDER_EVIDENCE"))
    );
    let other_policy = ContinuationAnswers::new(
        CacheConfig::derived(),
        Some(dir.path().into()),
        "other-policy".into(),
    );
    assert!(
        matches!(other_policy.open(first_query.clone()), Err(failure) if failure.code == ErrorCode::ResponseSnapshotExpired)
    );
    let mut changed = lsp_search_query::to_row(&first_query);
    changed["symbolName"] = json!("different");
    assert!(
        matches!(restored.open(serde_json::from_value(changed).expect("changed query")), Err(failure) if failure.code == ErrorCode::InvalidInput)
    );
    let (_, fresh) = restored.open(query()).expect("fresh query");
    assert_eq!(
        raw(&fresh),
        json!({}),
        "fresh requests never load saved provider evidence"
    );
}

#[test]
fn retention_failure_refuses_progress_and_incidental_objects_are_not_continuations() {
    let storage = tempfile::tempdir().unwrap();
    let refused = ContinuationAnswers::new(
        CacheConfig { disk_bytes: 0 },
        Some(storage.path().into()),
        "policy".into(),
    );
    assert!(
        matches!(refused.retain(&mut row(), &selected("evidence")), Err(failure) if failure.code == ErrorCode::RequestFailed)
    );
    let mut incidental = json!({"status":"success","unrelatedProviderData":row()["pagination"]["next"]["nextPage"].clone()});
    let unchanged = incidental.clone();
    refused
        .retain(&mut incidental, &selected("evidence"))
        .expect("no declared continuation");
    assert_eq!(incidental, unchanged);
    let mut related = row();
    related["pagination"]["next"]["nextPage"]["kind"] = json!("related");
    let unchanged = related.clone();
    refused
        .retain(&mut related, &selected("evidence"))
        .expect("related calls do not own selected pages");
    assert_eq!(related, unchanged);
}

const TOKEN: &str = "ghp_abcdefghijklmnopqrstuvwxyz0123456789";

fn answered(value: Value) -> ResponseScope {
    ResponseScope {
        generation: "source-revision".into(),
        answers: Some(Arc::new(Mutex::new(
            serde_json::from_value(json!({ "provider-request": value })).expect("answers"),
        ))),
        ..Default::default()
    }
}

/// Every byte the store wrote under `dir`, as text.
fn stored_bytes(dir: &std::path::Path) -> String {
    let mut text = String::new();
    for entry in std::fs::read_dir(dir).expect("store directory").flatten() {
        let path = entry.path();
        if path.is_dir() {
            text.push_str(&stored_bytes(&path));
        } else {
            text.push_str(&String::from_utf8_lossy(
                &std::fs::read(&path).expect("stored file"),
            ));
        }
    }
    text
}

#[test]
fn retained_answers_hold_no_credential_and_replay_what_the_output_gate_shows() {
    let dir = tempfile::tempdir().expect("directory");
    let store = ContinuationAnswers::new(
        CacheConfig::derived(),
        Some(dir.path().into()),
        "policy".into(),
    );
    let hover = json!({"contents":{"kind":"markdown","value":format!("const token = \"{TOKEN}\"")},"range":{"start":{"line":0,"character":6},"end":{"line":0,"character":11}}});
    let mut first = row();
    store
        .retain(&mut first, &answered(hover.clone()))
        .expect("text evidence is retained sealed");
    assert!(
        !stored_bytes(dir.path()).contains(TOKEN),
        "no credential reaches the store"
    );
    let restored = ContinuationAnswers::new(
        CacheConfig::derived(),
        Some(dir.path().into()),
        "policy".into(),
    );
    let replayed = raw(&restored.open(next(&first)).expect("continuation").1);
    let shown = ContentSecurity::new()
        .seal_output(raw(&answered(hover)), false, || {
            Ok::<_, std::convert::Infallible>(())
        })
        .unwrap_or_else(|never| match never {})
        .into_value();
    assert!(!shown.to_string().contains(TOKEN), "the gate redacts it");
    assert_eq!(
        replayed, shown,
        "a later page renders from what the output gate shows"
    );
}

#[test]
fn a_credential_in_protocol_data_refuses_the_continuation_without_storing_it() {
    let range = json!({"start":{"line":0,"character":0},"end":{"line":0,"character":4}});
    for answer in [
        json!([{"name":"load","kind":12,"uri":"file:///repo/a.ts","range":range,"selectionRange":range,"detail":format!("token = \"{TOKEN}\"")}]),
        json!([{"name":"load","kind":12,"location":{"uri":"file:///repo/a.ts","range":range},"data":{"token":TOKEN}}]),
    ] {
        let dir = tempfile::tempdir().expect("directory");
        let store = ContinuationAnswers::new(
            CacheConfig::derived(),
            Some(dir.path().into()),
            "policy".into(),
        );
        let refused = store.retain(&mut row(), &answered(answer.clone()));
        assert!(
            matches!(&refused, Err(failure) if failure.code == ErrorCode::ContentSecurityLimit),
            "{answer}"
        );
        assert!(
            !stored_bytes(dir.path()).contains(TOKEN),
            "a refused answer is not stored"
        );
    }
}
