//! What a candidate carries: the enclosing declaration, merged snippet
//! windows, and an exact glob.

use super::*;

/// A candidate names the declaration around each hit, so a predicate
/// about scope can be judged.
#[test]
fn candidates_carry_the_enclosing_declaration() {
    struct Capture(std::sync::Mutex<Vec<Value>>);
    impl SelectionJudge for Capture {
        fn identity(&self) -> Value {
            json!({"test":"capture"})
        }
        fn select(&self, call: JudgeCall<'_>) -> Judgments {
            let mut seen = self.0.lock().expect("lock");
            seen.extend(
                call.admitted
                    .iter()
                    .filter_map(|&i| call.candidates[i].state.clone()),
            );
            Ok(Vec::new())
        }
    }
    let root = tempfile::tempdir().expect("fixture");
    std::fs::write(
        root.path().join("a.rs"),
        "fn production() {\n    needle();\n}\n\n#[test]\nfn checks_it() {\n    needle();\n}\n",
    )
    .expect("a");
    let query: LocalSearchQuery = serde_json::from_value(json!({"path": root.path(),
            "matchString": "needle", "contextLines": 0,
            "clasifyContext": {"classifiers": ["Is this test code?"]}}))
    .expect("query");
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let judge = std::sync::Arc::new(Capture(std::sync::Mutex::new(Vec::new())));
    let selection = Runtime {
        judge: judge.clone(),
        ..runtime(None, &executor)
    };
    execute_local_search(
        &query,
        &crate::tools::test_support::workspace_policy(root.path()),
        &ContentSecurity::new(),
        &NeverCancel,
        None,
        None,
        Some(&selection),
    )
    .expect("search");
    let seen = judge.0.lock().expect("lock").clone();
    let names = seen
        .iter()
        .map(|state| state["matches"][0]["enclosing"]["symbolName"].clone())
        .collect::<Vec<_>>();
    assert_eq!(names, [json!("production"), json!("checks_it")], "{seen:?}");
}

/// A provider that cannot answer (down, overloaded, unreachable) never
/// withholds a search that succeeded: every hit is shown under an `unavailable` receipt naming
/// the provider's code, the warning states its error and hint, and the
/// next page replays that plan without another provider call.

#[test]
fn snippet_groups_merge_overlapping_or_touching_windows() {
    let hit = |line| octocode_engine::types::TextSearchMatch {
        deferred: None,
        line,
        last_line: None,
        column: 0,
        byte_offset: 0,
        value: String::new(),
        value_digest: <sha2::Sha256 as sha2::Digest>::digest(b"").into(),
        count: None,
        rank: None,
        original_chars: None,

        original_bytes: None,
    };
    let file = TextSearchFile {
        path: "a.rs".into(),
        match_count: 4,
        matches: vec![hit(20), hit(1), hit(2), hit(9)],
        source: None,
    };
    assert_eq!(
        groups(&file, SearchContext::symmetric(0)),
        [vec![1, 2], vec![3], vec![0]]
    );
    assert_eq!(
        groups(&file, SearchContext::symmetric(3)),
        [vec![1, 2, 3], vec![0]]
    );
}

#[test]
fn an_exact_glob_anchors_and_brackets_metacharacters() {
    assert_eq!(exact_glob("a.rs").as_deref(), Some("/a.rs"));
    assert_eq!(exact_glob("src/*x?.rs").as_deref(), Some("/src/[*]x[?].rs"));
    assert_eq!(exact_glob("app/[id].tsx"), None);
}
