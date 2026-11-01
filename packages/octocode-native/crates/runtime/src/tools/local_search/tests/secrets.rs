use super::*;
use octocode_contract_types::ErrorCode;

mod failures;
mod ranking;

#[test]
fn source_secret_guard_fails_closed_when_the_source_cannot_be_reread() {
    let security = ContentSecurity::new();
    let mut file = octocode_engine::types::TextSearchFile {
        term_counts: Vec::new(),
        path: "gone.txt".into(),
        match_count: 1,
        matches: vec![octocode_engine::types::TextSearchMatch {
            deferred: None,
            line: 1,
            last_line: None,
            column: 0,
            byte_offset: 0,
            value: "…token = ghp_abcdefghijklmnop".into(),
            value_digest: <sha2::Sha256 as sha2::Digest>::digest(
                "…token = ghp_abcdefghijklmnop".as_bytes(),
            )
            .into(),
            count: None,
            rank: None,
        }],
        source: None,
    };
    let missing = tempfile::tempdir()
        .expect("fixture directory")
        .path()
        .join("gone.txt");
    let verified = verify::guard_source_secrets(
        &mut file,
        &missing,
        crate::tools::local_search::sources::Expected::Fresh,
        0..10,
        &verify::ValueCheck {
            security: &security,
            match_only: false,
            cancel: &NeverCancel,
        },
    )
    .expect("not cancelled");
    assert_eq!(verified, verify::Verification::Unverified);
    assert!(!file.matches[0].value.contains("ghp_"));
    assert!(file.matches[0].value.contains("REDACTED"));
}

/// A source that shrank or was replaced after the search no longer holds
/// the matched line, so the clipped value cannot be verified.
#[test]
fn source_secret_guard_fails_closed_when_the_match_line_is_gone() {
    let security = ContentSecurity::new();
    let root = tempfile::tempdir().expect("fixture directory");
    let source = root.path().join("shrunk.txt");
    fs::write(&source, "one line now\n").expect("fixture");
    let mut file = octocode_engine::types::TextSearchFile {
        term_counts: Vec::new(),
        path: "shrunk.txt".into(),
        match_count: 1,
        matches: vec![octocode_engine::types::TextSearchMatch {
            deferred: None,
            line: 40,
            last_line: None,
            column: 0,
            byte_offset: 0,
            value: "…token = ghp_abcdefghijklmnop".into(),
            value_digest: <sha2::Sha256 as sha2::Digest>::digest(
                "…token = ghp_abcdefghijklmnop".as_bytes(),
            )
            .into(),
            count: None,
            rank: None,
        }],
        source: None,
    };
    let verified = verify::guard_source_secrets(
        &mut file,
        &source,
        crate::tools::local_search::sources::Expected::Fresh,
        0..10,
        &verify::ValueCheck {
            security: &security,
            match_only: false,
            cancel: &NeverCancel,
        },
    )
    .expect("not cancelled");
    assert_eq!(verified, verify::Verification::Unverified);
    assert!(
        !file.matches[0].value.contains("ghp_"),
        "{}",
        file.matches[0].value
    );
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
    for request in cases {
        let body = search_fixture(&[("sec.txt", &file)], request);
        let values = all_values(&body);
        assert!(!values.is_empty());
        for fragment in ["1234567890", "klmnopqrstuvwxyz"] {
            assert!(!values.contains(fragment), "leaked {fragment}: {values}");
        }
    }
    // Clean lines in the same file keep their exact values.
    let body = search_fixture(
        &[("sec.txt", &format!("{file}plain needle line\n"))],
        ls_query(
            serde_json::json!({"matchString": ["needle".to_string()], "view": "matchOnly"}),
            None,
        ),
    );
    assert_eq!(all_values(&body), "needle");
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
    let body = search_fixture(
        &[("src.rs", literal)],
        ls_query(
            serde_json::json!({"matchString": ["needle_"], "view": "matchOnly"}),
            None,
        ),
    );
    assert_eq!(all_values(&body), "needle_\nneedle_", "{body}");
    assert!(!all_values(&body).contains("REDACTED"), "{body}");
    let key = "-----BEGIN OPENSSH PRIVATE KEY-----\nMIIEpQIBAAKCAQEAinteriorKeyBodyQWERTYAAAAAAAAAAAAAAAAAAAAAAAAAAAA\nfn needle_after() {}\n";
    let body = search_fixture(
        &[("key.rs", key)],
        ls_query(
            serde_json::json!({"matchString": ["interior|needle_after"], "regexEngine": "rust", "view": "matchOnly"}),
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
    let filler = "let filler_value = 0;\n".repeat(560_000);
    let body = "MIIEpQIBAAKCAQEAinteriorKeyBodyQWERTYAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    let innocent = "QUJDREVGR0hJSktMTU5PUFFSU1RVinnocentB64VldYWVowMTIzNDU2Nzg5";
    let terminated = format!(
        "{innocent}\n{filler}-----BEGIN RSA PRIVATE KEY-----\n{body}\n-----END RSA PRIVATE KEY-----\n"
    );
    let unterminated =
        format!("{innocent}\n{filler}-----BEGIN OPENSSH PRIVATE KEY-----\n{body}\n{body}\n");
    for content in [&terminated, &unterminated] {
        assert!(content.len() as u64 > 10 * 1024 * 1024);
        for view in ["matchOnly", "content"] {
            let found = search_fixture(
                &[("big.txt", content.as_str())],
                ls_query(
                    serde_json::json!({"matchString": ["interior"], "view": view}),
                    None,
                ),
            );
            assert_eq!(found["files"][0]["matches"][0]["line"], 560_003, "{found}");
            assert!(!all_values(&found).contains("interior"), "{found}");
        }
        let readable = search_fixture(
            &[("big.txt", content.as_str())],
            ls_query(
                serde_json::json!({"matchString": ["innocentB64"], "view": "content", "contextLines": {"before":0,"after":0}}),
                None,
            ),
        );
        assert!(all_values(&readable).contains(innocent), "{readable}");
    }
}

#[test]
fn classification_candidates_never_receive_partial_secrets_even_when_excluded() {
    use crate::tools::selection::{Decision, JudgeCall, SelectionJudge};
    use serde_json::{Value, json};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    struct RecordingJudge(Arc<Mutex<Vec<Value>>>);
    impl SelectionJudge for RecordingJudge {
        fn identity(&self) -> Value {
            json!({"test":"record-safe-candidates"})
        }
        fn select(&self, call: JudgeCall<'_>) -> crate::tools::selection::Judgments {
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
    for fields in [
        json!({"matchString":["ghp_[0-9a-z]{20}|needle"], "regexEngine":"rust", "view":"matchOnly"}),
        json!({"matchString":["token|needle"], "regexEngine":"rust"}),
    ] {
        let recorded = Arc::new(Mutex::new(Vec::new()));
        let selection = crate::tools::test_support::selection_fake::runtime(
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
            &request,
            &policy,
            &security,
            &NeverCancel,
            None,
            Some(&selection),
        )
        .expect("selected search");
        let recorded = recorded.lock().expect("recorded states");
        assert_eq!(
            recorded.len(),
            2,
            "selected and excluded candidates were judged"
        );
        for state in recorded.iter() {
            let text = state["matches"].to_string();
            assert!(text.contains("needle"), "clean evidence remains available");
            assert!(
                text.contains("[REDACTED"),
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
        assert_eq!(
            (
                receipt.selected_items,
                receipt.excluded_items,
                receipt.unknown_items
            ),
            (1, 1, 0)
        );
        assert_eq!(result.files.len(), 1);
        assert_eq!(result.files[0].path, "keep.txt");
    }
}
