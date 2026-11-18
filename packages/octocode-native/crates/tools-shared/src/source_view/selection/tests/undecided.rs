//! `next.unfilteredUnknown`: the shared undecided view of a classified read.
use super::*;
use crate::source_view::SourceFacts;
use octocode_runtime_base::execution::cancel::NeverCancel;
use octocode_test_support::json_fixture;
use std::path::Path;

/// A judge that must never be asked: the replay is unbilled.
struct Unreachable;

impl SelectionJudge for Unreachable {
    fn identity(&self) -> serde_json::Value {
        json_fixture(r#"{"test": "words"}"#)
    }
    fn select(&self, _call: JudgeCall<'_>) -> crate::selection::Judgments {
        panic!("an undecided view never calls the provider");
    }
}

fn read(
    text: &str,
    query: &serde_json::Value,
    judge: Arc<dyn SelectionJudge>,
) -> crate::source_view::LocalFetchResult {
    let executor = tokio::runtime::Runtime::new().unwrap();
    let mut runtime = budget_runtime(executor.handle().clone(), Arc::new(AtomicUsize::new(0)));
    runtime.judge = judge;
    runtime.limits.max_chars = 1_000_000;
    let query: LocalFetchQuery = serde_json::from_value(query.clone()).unwrap();
    crate::source_view::process_fetched_content_selected(
        &query,
        text.as_bytes(),
        Path::new("hits.txt"),
        "hits.txt",
        &SourceFacts::default(),
        &ContentSecurity::new(),
        &NeverCancel,
        Some(&runtime),
    )
}

/// A 19-hit file with one hit the judge cannot decide: following
/// `unfilteredUnknown` shows exactly that hit, marked in `matchedLines`,
/// under the undecided note and no receipt, without asking the provider.
#[test]
fn unfiltered_unknown_of_a_hit_read_marks_the_undecided_hits() {
    let text = (1..=19)
        .map(|n| match n {
            7 => "maybe item\n".to_owned(),
            n if n % 2 == 0 => format!("keep item {n}\n"),
            n => format!("drop item {n}\n"),
        })
        .collect::<String>();
    let query = json_fixture(
        r#"{
            "path": "hits.txt",
            "matchString": ["item"],
            "classification": {"predicates": ["Is it kept?"]}
        }"#,
    );
    let first = read(&text, &query, Arc::new(WordJudge { fail: None }));
    assert_eq!(first.error_code, None, "{first:?}");
    let receipt = first.selection.as_ref().expect("receipt");
    assert_eq!((receipt.total_items, receipt.unknown_items), (19, 1));
    let unknown = first
        .next
        .expect("leads")
        .unfiltered_unknown
        .expect("replay");
    let replay = serde_json::to_value(&unknown.query).unwrap();
    assert!(replay.get("ranges").is_none(), "{replay}");
    assert!(replay.get("matchString").is_some(), "{replay}");

    let second = read(&text, &replay, Arc::new(Unreachable));
    assert_eq!(second.error_code, None, "{second:?}");
    assert_eq!(second.matched_lines, [7]);
    let content = second.content.as_deref().unwrap_or_default();
    assert!(content.contains("maybe item"), "{content}");
    assert!(!content.contains("keep item"), "{content}");
    assert!(
        second.selection.is_none(),
        "an undecided view has no receipt"
    );
    assert!(
        second
            .warnings
            .iter()
            .any(|warning| warning.contains("left undecided")),
        "{:?}",
        second.warnings
    );
    assert!(
        second
            .next
            .is_none_or(|next| next.unfiltered_unknown.is_none())
    );
}
