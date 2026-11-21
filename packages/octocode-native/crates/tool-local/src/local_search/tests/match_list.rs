use super::*;
use octocode_test_support::json_fixture;

/// `matchString` is a pattern array in localSearch as in localFetch: a
/// list matches a line holding any of its entries.
fn lines(body: &serde_json::Value) -> Vec<String> {
    body["files"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|file| file["matches"].as_array().into_iter().flatten())
        .map(|row| row["value"].as_str().unwrap_or_default().trim().to_owned())
        .collect()
}

fn sorted(mut lines: Vec<String>) -> Vec<String> {
    lines.sort();
    lines
}

fn run(files: &[(&str, &str)], fields: serde_json::Value) -> serde_json::Value {
    search_fixture(files, ls_query(fields, None))
}

/// An empty directory's empty result says no file was there to search.
#[test]
fn an_empty_directory_says_nothing_was_searched() {
    let out = run(&[], json_fixture(r#"{"matchString": ["x"]}"#));
    assert!(
        out.to_string()
            .contains("Nothing searched: no file under path"),
        "{out}"
    );
    let missed = run(
        &[("a.txt", "y\n")],
        json_fixture(r#"{"matchString": ["x"]}"#),
    );
    assert!(!missed.to_string().contains("Nothing searched"), "{missed}");
}

/// An empty search whose case rule may have decided it offers the same
/// search with case ignored, from its terms alone and without running it: a
/// term that mixes upper and lower case, searched case-sensitive without the
/// caller asking for it. A term of one case throughout, a case rule the
/// caller chose and a search that already ignored case get no offer.
#[test]
fn an_empty_search_offers_case_mode_only_for_a_mixed_case_term() {
    let files = [("a.txt", "hello world\n")];
    for (fields, offered) in [
        (json_fixture(r#"{"matchString": ["Hello"]}"#), true),
        (json_fixture(r#"{"matchString": ["HELLO"]}"#), false),
        (
            json_fixture(r#"{"matchString": ["Hello"], "caseMode": "sensitive"}"#),
            false,
        ),
        (
            json_fixture(r#"{"matchString": ["absent"], "caseMode": "insensitive"}"#),
            false,
        ),
    ] {
        let out = run(&files, fields.clone());
        assert_eq!(out["files"], serde_json::json!([]), "{fields}: {out}");
        let call = one_recovery(&out);
        assert_eq!(call.is_some(), offered, "{fields}: {out}");
        assert_eq!(out.get("hints").is_some(), offered, "{fields}: {out}");
    }
}

/// The hint's advice is a typed related call: the same search, same terms,
/// with `caseMode:"insensitive"`, not a wider scope; run, it finds the
/// match the case rule hid.
#[test]
fn the_case_hint_comes_with_the_insensitive_request_as_related() {
    let root = tempfile::tempdir().expect("fixture");
    fs::write(root.path().join("a.txt"), "hello world\n").expect("fixture");
    let miss = search_root(root.path(), json_fixture(r#"{"matchString": ["Hello"]}"#));
    let out = miss.to_row().expect("row");
    assert_eq!(out["files"], serde_json::json!([]), "{out}");
    let call = one_recovery(&out).unwrap_or_else(|| panic!("a call: {out}"));
    assert_eq!(call["caseMode"], "insensitive", "{out}");
    assert_eq!(call["matchString"], serde_json::json!(["Hello"]), "{out}");
    let (policy, security) = policy_for(root.path());
    let found = search_in(&ls_query(call, None), &policy, &security, None)
        .expect("the offered search")
        .to_row()
        .expect("row");
    assert_eq!(found["matchCount"], 1, "{found}");
}

#[test]
fn a_list_matches_a_line_holding_any_of_its_entries() {
    let files = [("a.txt", "alpha\nbeta\ngamma\ndelta\n")];
    let body = run(
        &files,
        json_fixture(r#"{"matchString": ["alpha", "gamma"]}"#),
    );
    assert_eq!(sorted(lines(&body)), ["alpha", "gamma"], "{body}");
    // Two lines, one hit each: the line count is the stated matchCount.
    assert_eq!(body["matchCount"], 2, "{body}");
    assert!(body.get("matchedLineCount").is_none(), "{body}");
}

/// Without `regexEngine` every entry is literal text, `|` and `.` included; the
/// entries are not joined into a pattern the caller can see.
#[test]
fn list_entries_are_literal_unless_regex_is_set() {
    let files = [("a.txt", "abc\na.c\nx|y\nx\ny\n")];
    let literal = run(&files, json_fixture(r#"{"matchString": ["a.c", "x|y"]}"#));
    assert_eq!(sorted(lines(&literal)), ["a.c", "x|y"], "{literal}");
    let named = run(
        &files,
        json_fixture(r#"{"matchString": ["a.c", "x|y"], "regexEngine": "literal"}"#),
    );
    assert_eq!(sorted(lines(&named)), ["a.c", "x|y"], "{named}");
    // `regexEngine:"rust"` makes every entry a pattern of its own.
    let regex = run(
        &files,
        json_fixture(r#"{"matchString": ["a.c", "x|y"], "regexEngine": "rust"}"#),
    );
    assert_eq!(
        sorted(lines(&regex)),
        ["a.c", "abc", "x", "x|y", "y"],
        "{regex}"
    );
}

/// `caseMode:"smart"` judges each entry on its own, as localFetch does: an
/// uppercase letter makes that entry sensitive, the others stay insensitive.
#[test]
fn a_list_applies_the_case_rule_to_each_entry() {
    let files = [("a.txt", "alpha\nAlpha\nBETA\nbeta\n")];
    let smart = run(
        &files,
        json_fixture(r#"{"matchString": ["Alpha", "beta"]}"#),
    );
    assert_eq!(sorted(lines(&smart)), ["Alpha", "BETA", "beta"], "{smart}");
    let sensitive = run(
        &files,
        json_fixture(r#"{"matchString": ["alpha", "beta"], "caseMode": "sensitive"}"#),
    );
    assert_eq!(sorted(lines(&sensitive)), ["alpha", "beta"], "{sensitive}");
    let insensitive = run(
        &files,
        json_fixture(
            r#"{
                "matchString": ["Alpha", "BETA"],
                "caseMode": "insensitive"
            }"#,
        ),
    );
    assert_eq!(
        sorted(lines(&insensitive)),
        ["Alpha", "BETA", "alpha", "beta"],
        "{insensitive}"
    );
}

#[test]
fn a_list_honors_whole_word_and_reports_a_miss_as_empty() {
    let files = [("a.txt", "concat\ncat\ndogma\ndog\n")];
    let words = run(
        &files,
        json_fixture(r#"{"matchString": ["cat", "dog"], "wholeWord": true}"#),
    );
    assert_eq!(sorted(lines(&words)), ["cat", "dog"], "{words}");
    let miss = run(&files, json_fixture(r#"{"matchString": ["zzz", "yyy"]}"#));
    assert_eq!(miss["files"], serde_json::json!([]), "{miss}");
}

/// A count view over a list states each entry's count per file, in entry
/// order; one entry, or a view that lists lines, states none.
#[test]
fn count_views_count_each_list_entry() {
    let files = [("a.txt", "alpha beta\nalpha\ngamma\n"), ("b.txt", "beta\n")];
    let counted = |view: &str, entries: serde_json::Value| {
        let body = run(
            &files,
            serde_json::json!({"matchString": entries, "view": view, "sort": {"by": "path", "order": "asc"}}),
        );
        octocode_runtime_base::test_support::assert_row_contract(
            octocode_runtime_base::id::ToolId::LocalSearch,
            &body,
        );
        body["files"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|file| file.get("termCounts").cloned())
            .collect::<Vec<_>>()
    };
    let entries = json_fixture(r#"["alpha", "beta", "missing"]"#);
    assert_eq!(
        counted("countMatches", entries.clone()),
        [
            Some(json_fixture(r#"[2, 1, 0]"#)),
            Some(json_fixture(r#"[0, 1, 0]"#))
        ]
    );
    assert_eq!(
        counted("countLines", entries.clone()),
        [
            Some(json_fixture(r#"[2, 1, 0]"#)),
            Some(json_fixture(r#"[0, 1, 0]"#))
        ]
    );
    assert!(
        counted("countMatches", json_fixture(r#"["alpha"]"#))
            .iter()
            .all(Option::is_none)
    );
    assert!(counted("files", entries).iter().all(Option::is_none));
}
