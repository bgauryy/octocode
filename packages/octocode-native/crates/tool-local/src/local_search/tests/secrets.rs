use super::*;
use octocode_contract_types::ErrorCode;
use octocode_test_support::json_fixture;

mod char_walks;
mod context_spans;
mod credential_named_file;
mod failures;
mod guard;
mod hits;
mod judged_files;
mod key_formats;
mod known_formats;
mod large_sources;
mod long_lines;
mod member_context;
mod ranking;
mod row_windows;
mod sources;

/// A page's cost follows the rows it shows: redacting the scan reads no file
/// again, so a hit the page never shows costs no second read. A secret in a
/// file the page does not show is still redacted on the page that shows it.
#[test]
fn a_page_rereads_only_the_files_it_shows() {
    let dir = octocode_runtime_base::test_support::Fixture::new();
    let root = dir.0.as_path().canonicalize().expect("root");
    for n in 0..6 {
        fs::write(root.join(format!("f{n}.txt")), "needle here\n").expect("fixture");
    }
    let token = format!("ghp_{}", "a1B2c3D4".repeat(5));
    fs::write(root.join("f9.txt"), format!("needle {token}\n")).expect("fixture");
    let policy = octocode_runtime_base::test_support::workspace_policy(&root);
    let security = ContentSecurity::new();
    let path = root.to_string_lossy().into_owned();
    let reread = || {
        crate::local_search::verify::lines::REREAD_FILES
            .lock()
            .expect("reread log")
            .iter()
            .filter(|file| file.starts_with(&root))
            .cloned()
            .collect::<Vec<_>>()
    };
    execute_local_search(
        octocode_runtime_base::test_support::BUILD_IDENTITY,
        &ls_query(
            serde_json::json!({"path": path, "matchString": ["needle"], "limit": 1}),
            None,
        ),
        &policy,
        &security,
        &NeverCancel,
        None,
        None,
    )
    .expect("first page");
    assert_eq!(reread().len(), 1, "only the shown file: {:?}", reread());
    let whole = execute_local_search(
        octocode_runtime_base::test_support::BUILD_IDENTITY,
        &ls_query(
            serde_json::json!({"path": path, "matchString": ["needle"]}),
            None,
        ),
        &policy,
        &security,
        &NeverCancel,
        None,
        None,
    )
    .expect("whole page");
    let shown = values(&whole);
    assert!(shown.contains("f9.txt"), "{shown}");
    assert!(
        !shown.contains(&token),
        "the secret stays redacted: {shown}"
    );
}

/// Every match value of `body` as its reader takes it ([`read_value`]): a
/// span without one is `request`'s one term.
fn read_values(body: &serde_json::Value, request: &LocalSearchQuery) -> String {
    let asked = serde_json::to_value(request).expect("query");
    let term = match asked["matchString"].as_array().map(Vec::as_slice) {
        Some([term]) => term.as_str().unwrap_or_default().to_owned(),
        _ => String::new(),
    };
    body["files"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|f| f["matches"].as_array().cloned().unwrap_or_default())
        .map(|m| read_value(&m, &term))
        .collect::<Vec<_>>()
        .join("\n")
}

fn all_values(body: &serde_json::Value) -> String {
    body["files"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|f| f["matches"].as_array().cloned().unwrap_or_default())
        .map(|m| m["value"].as_str().unwrap_or_default().to_owned())
        .collect::<Vec<_>>()
        .join("\n")
}

// A selected matchOnly span must not
// leak a partial secret that no longer matches the secret patterns.
#[test]
fn selected_spans_never_leak_partial_secrets() {
    let token = "ghp_1234567890abcdefghijklmnopqrstuvwxyzAB";
    let file = format!("token: {token} end\n");
    let cases = [
        ls_query(
            serde_json::json!({"matchString": ["ghp_[0-9a-z]{20}".to_string()], "regexEngine": "rust", "view": "matchOnly"}),
            None,
        ),
        ls_query(
            serde_json::json!({"matchString": ["token".to_string()], "view": "matchOnly"}),
            None,
        ),
        ls_query(
            serde_json::json!({"matchString": ["end".to_string()], "view": "matchOnly"}),
            None,
        ),
        ls_query(
            serde_json::json!({"matchString": ["token".to_string()]}),
            None,
        ),
    ];
    for (index, request) in cases.into_iter().enumerate() {
        let body = search_fixture(&[("sec.txt", &file)], request.clone());
        let values = read_values(&body, &request);
        // A read shows the line as `token: [REDACTED-…] end`: a pattern
        // inside the hidden token is no hit; the others show rows.
        assert_eq!(values.is_empty(), index == 0, "{body}");
        for fragment in ["1234567890", "klmnopqrstuvwxyz"] {
            assert!(!values.contains(fragment), "leaked {fragment}: {values}");
        }
    }
    // Clean lines in the same file keep their exact values.
    let request = ls_query(
        serde_json::json!({"matchString": ["needle".to_string()], "view": "matchOnly"}),
        None,
    );
    let body = search_fixture(
        &[("sec.txt", &format!("{file}plain needle line\n"))],
        request.clone(),
    );
    assert_eq!(read_values(&body, &request), "needle");
}

#[test]
fn short_spans_inside_private_key_bodies_are_redacted() {
    let file = "-----BEGIN RSA PRIVATE KEY-----\nMIIEpQIBAAKCAQEAinteriorKeyBodyQWERTYAAAAAAAAAAAAAAAAAAAAAAAAAAAA\n-----END RSA PRIVATE KEY-----\n";
    let body = search_fixture(
        &[("key.rs", file)],
        ls_query(
            serde_json::json!({"matchString": ["interior".to_string()], "view": "matchOnly"}),
            None,
        ),
    );
    assert!(!all_values(&body).contains("interior"), "{body}");
}

/// A literal marker string in ordinary source opens a block that never
/// closes; the code after it must stay readable, while a real key's body
/// (here also unterminated) stays redacted.
#[test]
fn code_after_a_literal_key_marker_stays_readable() {
    let literal = "const PEM = \"-----BEGIN RSA PRIVATE KEY-----\";\nlet needle_one = compute();\nfn needle_two() {}\n";
    let request = ls_query(
        json_fixture(r#"{"matchString": ["needle_"], "view": "matchOnly"}"#),
        None,
    );
    let body = search_fixture(&[("src.rs", literal)], request.clone());
    assert_eq!(read_values(&body, &request), "needle_\nneedle_", "{body}");
    assert!(!all_values(&body).contains("REDACTED"), "{body}");
    let key = "-----BEGIN OPENSSH PRIVATE KEY-----\nMIIEpQIBAAKCAQEAinteriorKeyBodyQWERTYAAAAAAAAAAAAAAAAAAAAAAAAAAAA\nfn needle_after() {}\n";
    let body = search_fixture(
        &[("key.rs", key)],
        ls_query(
            json_fixture(
                r#"{
                    "matchString": ["interior|needle_after"],
                    "regexEngine": "rust",
                    "view": "matchOnly"
                }"#,
            ),
            None,
        ),
    );
    let values = all_values(&body);
    assert!(!values.contains("interior"), "{body}");
    assert!(values.contains("needle_after"), "{body}");
}

/// Sources above the whole-file key-scan cap are verified by the streamed
/// prefix pass: late interior key bodies stay redacted (terminated or
/// not), while innocent base64 outside any block stays readable.
#[test]
fn late_key_bodies_in_large_sources_stay_redacted() {
    let limits = octocode_runtime_base::security::SourceLimits::SMALL;
    let line = "let filler_value = 0;\n";
    let lines = usize::try_from(limits.key_scan).expect("fits") / line.len() + 1_000;
    let filler = line.repeat(lines);
    let body = "MIIEpQIBAAKCAQEAinteriorKeyBodyQWERTYAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    let innocent = "QUJDREVGR0hJSktMTU5PUFFSU1RVinnocentB64VldYWVowMTIzNDU2Nzg5";
    let terminated = format!(
        "{innocent}\n{filler}-----BEGIN RSA PRIVATE KEY-----\n{body}\n-----END RSA PRIVATE KEY-----\n"
    );
    let unterminated =
        format!("{innocent}\n{filler}-----BEGIN OPENSSH PRIVATE KEY-----\n{body}\n{body}\n");
    for content in [&terminated, &unterminated] {
        assert!(content.len() as u64 > limits.key_scan);
        for view in ["matchOnly", "content"] {
            let found = search_fixture_at(
                &[("big.txt", content.as_str())],
                ls_query(
                    serde_json::json!({"matchString": ["interior"], "view": view}),
                    None,
                ),
                limits,
            );
            // A body a read hides is no hit; a source too large to judge
            // whole shows its body's row redacted.
            let line = &found["files"][0]["matches"][0]["line"];
            assert!(line.is_null() || *line == lines + 3, "{found}");
            assert!(!all_values(&found).contains("interior"), "{found}");
        }
        let readable = search_fixture_at(
            &[("big.txt", content.as_str())],
            ls_query(
                json_fixture(
                    r#"{
                        "matchString": ["innocentB64"],
                        "view": "content",
                        "contextLines": {"before": 0, "after": 0}
                    }"#,
                ),
                None,
            ),
            limits,
        );
        assert!(all_values(&readable).contains(innocent), "{readable}");
    }
}

#[test]
fn classification_candidates_never_receive_partial_secrets_even_when_excluded() {
    use octocode_tools_shared::selection::{Decision, JudgeCall, SelectionJudge};
    use serde_json::{Value, json};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    struct RecordingJudge(Arc<Mutex<Vec<Value>>>);
    impl SelectionJudge for RecordingJudge {
        fn identity(&self) -> Value {
            json_fixture(r#"{"test": "record-safe-candidates"}"#)
        }
        fn select(&self, call: JudgeCall<'_>) -> octocode_tools_shared::selection::Judgments {
            let mut recorded = self.0.lock().expect("recorded states");
            Ok(call
                .admitted
                .iter()
                .map(|&index| {
                    let state = call.candidates[index]
                        .state
                        .as_ref()
                        .expect("prepared state");
                    recorded.push(state.clone());
                    let decision = if state["path"]
                        .as_str()
                        .expect("candidate path")
                        .ends_with("keep.txt")
                    {
                        Decision::Selected
                    } else {
                        Decision::Excluded
                    };
                    (index, decision)
                })
                .collect())
        }
    }

    let root = tempfile::tempdir().expect("fixture");
    let token = format!("{}{}", "ghp_", "1234567890abcdefghijklmnopqrstuvwxyz");
    let body = format!("token: {token} end\nplain needle line\n");
    for name in ["keep.txt", "drop.txt"] {
        fs::write(root.path().join(name), &body).expect("fixture");
    }
    let (policy, security) = policy_for(root.path());
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    // A span inside the token is no candidate; a line beside it is, redacted.
    for (fields, redacted) in [
        (
            json_fixture(
                r#"{
                    "matchString": ["ghp_[0-9a-z]{20}|needle"],
                    "regexEngine": "rust",
                    "view": "matchOnly"
                }"#,
            ),
            false,
        ),
        (
            json_fixture(r#"{"matchString": ["token|needle"], "regexEngine": "rust"}"#),
            true,
        ),
    ] {
        let recorded = Arc::new(Mutex::new(Vec::new()));
        let selection = octocode_tools_shared::test_support::selection_fake::runtime(
            RecordingJudge(recorded.clone()),
            executor.handle().clone(),
            Duration::from_secs(5),
        );
        let request = ls_query(
            json!({
                "path": root.path(),
                "classification": {"predicates":["Does this contain needle?"]}
            }),
            Some(&ls_query(fields, None)),
        );
        let result = execute_local_search(
            octocode_runtime_base::test_support::BUILD_IDENTITY,
            &request,
            &policy,
            &security,
            &NeverCancel,
            None,
            Some(&selection),
        )
        .expect("selected search");
        let recorded = recorded.lock().expect("recorded states");
        // Each hit is a candidate, in both the kept and the dropped file.
        for name in ["keep.txt", "drop.txt"] {
            let text = recorded
                .iter()
                .filter(|state| {
                    state["path"]
                        .as_str()
                        .is_some_and(|path| path.contains(name))
                })
                .map(serde_json::Value::to_string)
                .collect::<String>();
            assert!(text.contains("needle"), "clean evidence remains available");
            assert_eq!(
                text.contains("[REDACTED"),
                redacted,
                "secret overlap was redacted before judgment"
            );
            for fragment in ["1234567890", "klmnopqrstuvwxyz"] {
                assert!(
                    !text.contains(fragment),
                    "provider received a secret fragment"
                );
            }
        }
        let receipt = result.selection.expect("selection receipt");
        assert!(
            receipt.selected_items >= 1 && receipt.excluded_items >= 1,
            "{receipt:?}"
        );
        assert_eq!(receipt.unknown_items, 0, "{receipt:?}");
        assert_eq!(result.files.len(), 1);
        assert_eq!(result.files[0].path, "keep.txt");
    }
}
