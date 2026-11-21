//! Regex dialects, the match limit, and invalid patterns.

use super::*;
use octocode_runtime_base::response::output::ToolOutput as _;
use octocode_test_support::json_fixture;

#[test]
fn regex_engines_are_explicit_and_errors_never_rewrite_the_request() {
    let root = Fixture::new();
    let file = root.0.join("regex.txt");
    fs::write(&file, "needle one\nhello aa\n").expect("fixture");
    let paths = Paths(root.0.clone());
    for pattern in [r"(\w)\1", "(?<=needle )one"] {
        let request = qj(
            &file,
            serde_json::json!({"matchString":[pattern],"regexEngine":"rust"}),
        );
        let invalid = fetch(&request, &paths);
        assert_eq!(
            invalid.error_code,
            Some(ErrorCode::InvalidPattern),
            "{invalid:?}"
        );
        assert!(
            invalid
                .error
                .as_ref()
                .is_some_and(|error| error.contains("matchString")),
            "{invalid:?}"
        );
        assert!(
            invalid.next.is_none(),
            "an invalid regex must not infer another request: {invalid:?}"
        );
        let explicit = qj(
            &file,
            serde_json::json!({"matchString":[pattern],"regexEngine":"pcre2"}),
        );
        let found = fetch(&explicit, &paths);
        assert!(found.error_code.is_none(), "{found:?}");
        assert!(
            found.content.as_ref().is_some_and(|text| !text.is_empty()),
            "{found:?}"
        );
    }
    for engine in ["rust", "pcre2"] {
        let invalid = fetch(
            &qj(
                &file,
                serde_json::json!({"matchString":["hello", "route("],"regexEngine":engine}),
            ),
            &paths,
        );
        assert_eq!(
            invalid.error_code,
            Some(ErrorCode::InvalidPattern),
            "{invalid:?}"
        );
        assert!(invalid.next.is_none(), "{invalid:?}");
    }
}

/// The verdict is the pattern's, never the file's size: a pattern that
/// compiles matches a file past 64 KiB, and one that does not fails there
/// with the very error it gives on a small file.
#[test]
fn rust_regex_verdict_does_not_depend_on_file_size() {
    let t = Fixture::new();
    let small = t.0.join("small.txt");
    let big = t.0.join("big.txt");
    let filler = "word xx yy\n";
    fs::write(&small, "tail aa\n").expect("fixture");
    fs::write(&big, format!("{}tail aa\n", filler.repeat(7_000))).expect("fixture");
    assert!(fs::metadata(&big).expect("meta").len() > 64 * 1024);
    let paths = Paths(t.0.clone());
    let run = |file: &Path, pattern: &str| {
        fetch(
            &qj(
                file,
                serde_json::json!({"matchString": [pattern], "regexEngine": "rust", "contextLines":{"before":0,"after":0}}),
            ),
            &paths,
        )
    };
    for file in [&small, &big] {
        let found = run(file, "t[a]il");
        assert_eq!(found.error_code, None, "{found:?}");
        assert!(found.content.is_some_and(|c| c.contains("tail aa")));
    }
    let on_small = run(&small, r"(\w)\1");
    let on_big = run(&big, r"(\w)\1");
    assert_eq!(on_small.error_code, Some(ErrorCode::InvalidPattern));
    assert_eq!(on_big.error, on_small.error);
}

/// A regex read with more matches than the old 10,000 in-process cap
/// selects every matching line (never a silent prefix of them).
#[test]
fn regex_read_selects_every_match_past_ten_thousand() {
    use octocode_contract_types::RegexEngine;
    let t = Fixture::new();
    let p = t.0.join("many.txt");
    let source: String = (1..=12_000).map(|i| format!("hit {i}\n")).collect();
    fs::write(&p, &source).expect("fixture");
    for engine in [RegexEngine::Rust, RegexEngine::Pcre2] {
        let mut request = q(&p);
        amend(
            &mut request,
            json_fixture(r#"{"matchString": ["hit [0-9]+"]}"#),
        );
        amend(
            &mut request,
            serde_json::json!({"regexEngine":Some(engine)}),
        );
        amend(
            &mut request,
            json_fixture(r#"{"contextLines": {"before": 0, "after": 0}}"#),
        );
        let result = fetch(&request, &Paths(t.0.clone()));
        assert_eq!(result.error, None, "{engine:?}");
        assert_eq!(result.selected_match_count, Some(12_000), "{engine:?}");
        assert!(
            result.warnings.iter().all(|w| !w.contains("match limit")),
            "{engine:?}: {:?}",
            result.warnings
        );
    }
}

/// A regex read that reaches the in-process match limit discloses it:
/// isPartial, a warning naming the limit, and a localSearch lead that pages
/// every match of the same pattern in the same file.
#[test]
fn regex_read_past_the_match_limit_is_partial_with_a_search_lead() {
    use octocode_contract_types::RegexEngine;
    let t = Fixture::new();
    let p = t.0.join("dense.txt");
    let budget = octocode_tools_shared::page_memo::test_budget!(4096);
    let _scope = octocode_tools_shared::source_view::extraction::SpanBudgetOverride::of(budget);
    let span = std::mem::size_of::<(usize, usize)>();
    let source = "x\n".repeat(budget.limit() / span + 1);
    fs::write(&p, &source).expect("fixture");
    for engine in [RegexEngine::Rust, RegexEngine::Pcre2] {
        let mut request = q(&p);
        amend(&mut request, json_fixture(r#"{"matchString": ["x"]}"#));
        amend(
            &mut request,
            serde_json::json!({"regexEngine":Some(engine)}),
        );
        amend(
            &mut request,
            json_fixture(r#"{"contextLines": {"before": 0, "after": 0}}"#),
        );
        let result = fetch(&request, &Paths(t.0.clone()));
        assert_eq!(budget.used(), 0, "{engine:?}: the spans are released");
        assert_eq!(result.error, None, "{engine:?}");
        assert_eq!(result.is_partial, Some(true), "{engine:?}");
        assert_eq!(
            result.partial_reasons,
            vec![PartialReason::ScanBudget],
            "{engine:?}"
        );
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.contains("match limit") && w.contains("memory budget")),
            "{engine:?}: {:?}",
            result.warnings
        );
        let lead = result
            .next
            .as_ref()
            .and_then(|next| next.text_search.clone())
            .expect("findText lead");
        assert_eq!(lead["tool"], "localSearch", "{lead}");
        let row = &lead["input"]["queries"][0];
        assert_eq!(row["matchString"], json_fixture(r#"["x"]"#), "{lead}");
        // The lead names the file as the row does: relative to the root.
        assert_eq!(row["path"], "dense.txt", "{lead}");
        // The row the runtime hands to MCP and CLI (its serialization, as
        // dispatch sends it) carries the same disclosure under its wire names.
        let wire = serde_json::to_value(&result).expect("row");
        assert_eq!(wire["isPartial"], true, "{engine:?}: {wire}");
        assert_eq!(
            wire["partialReasons"][0], "scanBudget",
            "{engine:?}: {wire}"
        );
        assert_eq!(wire["related"][0], lead, "{engine:?}: {wire}");
    }
}

/// The match limit is the memory budget's, not a fixed count: a read of
/// 300,000 matches selects every one, in both engines, with no partial reason.
#[test]
fn a_regex_read_of_many_matches_selects_every_one() {
    use octocode_contract_types::RegexEngine;
    let t = Fixture::new();
    let p = t.0.join("many.txt");
    let hits = 300_000;
    let source: String = (1..=hits).map(|n| format!("hit {n}\n")).collect();
    fs::write(&p, &source).expect("fixture");
    for engine in [RegexEngine::Rust, RegexEngine::Pcre2] {
        let mut request = q(&p);
        amend(&mut request, json_fixture(r#"{"matchString": ["hit"]}"#));
        amend(
            &mut request,
            serde_json::json!({"regexEngine":Some(engine)}),
        );
        amend(
            &mut request,
            json_fixture(r#"{"contextLines": {"before": 0, "after": 0}}"#),
        );
        let result = fetch(&request, &Paths(t.0.clone()));
        assert_eq!(result.error, None, "{engine:?}");
        assert!(result.partial_reasons.is_empty(), "{engine:?}");
        assert_eq!(result.selected_match_count, Some(hits), "{engine:?}");
    }
}

/// A budget with no room for even one span stops the scan before its first
/// match: the read says so instead of reporting that nothing matched.
#[test]
fn a_budget_with_no_room_for_a_span_is_a_partial_read_not_a_miss() {
    use octocode_contract_types::RegexEngine;
    let t = Fixture::new();
    let p = t.0.join("some.txt");
    fs::write(&p, "x\n".repeat(10)).expect("fixture");
    let budget =
        octocode_tools_shared::page_memo::test_budget!(std::mem::size_of::<(usize, usize)>() - 1);
    let _scope = octocode_tools_shared::source_view::extraction::SpanBudgetOverride::of(budget);
    for engine in [RegexEngine::Rust, RegexEngine::Pcre2] {
        let request = qj(
            &p,
            serde_json::json!({"matchString":["x"],"regexEngine":Some(engine)}),
        );
        let result = fetch(&request, &Paths(t.0.clone()));
        assert_eq!(result.is_partial, Some(true), "{engine:?}: {result:?}");
        assert_eq!(
            result.partial_reasons,
            vec![PartialReason::ScanBudget],
            "{engine:?}"
        );
        assert!(
            result
                .next
                .as_ref()
                .is_some_and(|n| n.text_search.is_some()),
            "{engine:?}: {result:?}"
        );
        assert_eq!(budget.used(), 0, "{engine:?}");
    }
}

/// When the read pages, the tool emits the findText lead beside the
/// page continuation (the response moves it to `hints.findText`).
#[test]
fn paged_regex_read_past_the_match_limit_keeps_its_search_lead() {
    use octocode_contract_types::RegexEngine;
    let t = Fixture::new();
    let p = t.0.join("hits.txt");
    let budget = octocode_tools_shared::page_memo::test_budget!(64 * 1024);
    let _scope = octocode_tools_shared::source_view::extraction::SpanBudgetOverride::of(budget);
    let span = std::mem::size_of::<(usize, usize)>();
    let source: String = (1..=budget.limit() / span + 5)
        .map(|n| format!("hit {n} {}\n", "-".repeat(64)))
        .collect();
    fs::write(&p, &source).expect("fixture");
    let mut request = q(&p);
    amend(&mut request, json_fixture(r#"{"matchString": ["hit"]}"#));
    amend(
        &mut request,
        serde_json::json!({"regexEngine":Some(RegexEngine::Rust)}),
    );
    let result = execute_local_fetch(
        &request,
        &Paths(t.0.clone()),
        &Safe,
        &NeverCancel,
        None,
        false,
    );
    assert!(
        result.warnings.iter().any(|w| w.contains("match limit")),
        "{:?}",
        result.warnings
    );
    let next = result.next.as_ref().expect("next");
    assert!(next.continue_window.is_some(), "the read pages: {next:?}");
    let lead = next
        .text_search
        .as_ref()
        .expect("findText lead on a paged read");
    assert_eq!(lead["tool"], "localSearch", "{lead}");
}

/// A list item that does not compile as a regex is named by index and text
/// (`"def post("` in a `regex:"rust"` list), on both regex engines.
#[test]
fn invalid_regex_items_are_named_by_index() {
    let t = Fixture::new();
    let p = t.0.join("a.py");
    fs::write(&p, "def post(self):\n    pass\n").expect("fixture");
    let paths = Paths(t.0.clone());
    for engine in ["rust", "pcre2"] {
        let list = fetch(
            &qj(
                &p,
                serde_json::json!({"matchString": ["pass", "def post("], "regexEngine": engine}),
            ),
            &paths,
        );
        assert_eq!(list.error_code, Some(ErrorCode::InvalidPattern), "{list:?}");
        assert!(
            list.next.is_none(),
            "invalid input has no guessed repair: {list:?}"
        );
        let error = list.error.unwrap_or_default();
        assert!(
            error.starts_with("Invalid regex pattern: matchString[1] \"def post(\": "),
            "{engine}: {error}"
        );
        let one = fetch(
            &qj(
                &p,
                serde_json::json!({"matchString": ["def post("], "regexEngine": engine}),
            ),
            &paths,
        );
        let error = one.error.unwrap_or_default();
        assert!(
            error.starts_with("Invalid regex pattern: matchString \"def post(\": "),
            "{engine}: {error}"
        );
    }
    assert_eq!(
        crate::local_fetch::Output.error_hint(ErrorCode::InvalidPattern, &serde_json::Value::Null),
        Some(octocode_runtime_base::response::output::NO_HINT)
    );
}

/// `contextLines` widens matchString hits; a read without matchString is
/// refused by the contract, the rule's one owner, before any file is read.
#[test]
fn context_lines_without_match_string_is_refused_by_the_contract() {
    let refused = octocode_runtime_base::contracts::prepare_rows(
        "localFetch",
        json_fixture(
            r#"{
                "queries": [
                    {
                        "path": "a.rs",
                        "ranges": [{"startLine": 1, "endLine": 1}],
                        "contextLines": {"before": 5, "after": 5}
                    }
                ]
            }"#,
        ),
    )
    .expect_err("contextLines without matchString");
    assert!(
        refused.issues.iter().any(|issue| issue
            .path
            .last()
            .is_some_and(|field| field == "matchString")),
        "{refused:?}"
    );
}
