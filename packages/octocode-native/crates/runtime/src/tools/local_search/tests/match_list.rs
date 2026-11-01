use super::*;

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

/// Every `endLine` a lead in `value` names.
fn end_lines(value: &serde_json::Value, found: &mut Vec<u64>) {
    match value {
        serde_json::Value::Object(fields) => {
            if let Some(end) = fields.get("endLine").and_then(serde_json::Value::as_u64) {
                found.push(end);
            }
            fields.values().for_each(|inner| end_lines(inner, found));
        }
        serde_json::Value::Array(items) => items.iter().for_each(|inner| end_lines(inner, found)),
        _ => {}
    }
}

/// An empty directory's empty result says no file was there to search.
#[test]
fn an_empty_directory_says_nothing_was_searched() {
    let out = run(&[], serde_json::json!({"matchString":["x"]}));
    assert!(
        out.to_string()
            .contains("Nothing searched: no file under path"),
        "{out}"
    );
    let missed = run(
        &[("a.txt", "y\n")],
        serde_json::json!({"matchString":["x"]}),
    );
    assert!(!missed.to_string().contains("Nothing searched"), "{missed}");
}

/// An empty case-sensitive search that matches with case ignored names
/// caseMode; one that misses either way, or already ignored case, does not.
#[test]
fn an_empty_case_sensitive_search_names_case_mode_when_case_decided_it() {
    let files = [("a.txt", "hello world\n")];
    for (fields, named) in [
        (
            serde_json::json!({"matchString":["HELLO"],"caseMode":"sensitive"}),
            true,
        ),
        (serde_json::json!({"matchString":["Hello"]}), true),
        (
            serde_json::json!({"matchString":["absent"],"caseMode":"sensitive"}),
            false,
        ),
    ] {
        let out = run(&files, fields.clone());
        assert_eq!(
            out.to_string().contains("caseMode:\\\"insensitive\\\""),
            named,
            "{fields}: {out}"
        );
    }
}

/// The read lead around hits near the end of a file stops at its last line.
#[test]
fn a_read_lead_never_ends_past_the_last_line() {
    let body = (1..=10)
        .map(|line| {
            if line == 9 {
                "needle\n".to_owned()
            } else {
                format!("line {line}\n")
            }
        })
        .collect::<String>();
    let out = run(
        &[("a.txt", &body)],
        serde_json::json!({"matchString":["needle"]}),
    );
    let mut found = Vec::new();
    end_lines(&out, &mut found);
    assert!(found.contains(&10), "the lead reaches the last line: {out}");
    assert!(found.iter().all(|end| *end <= 10), "{out}");
}

#[test]
fn a_list_matches_a_line_holding_any_of_its_entries() {
    let files = [("a.txt", "alpha\nbeta\ngamma\ndelta\n")];
    let body = run(
        &files,
        serde_json::json!({"matchString": ["alpha", "gamma"]}),
    );
    assert_eq!(sorted(lines(&body)), ["alpha", "gamma"], "{body}");
    assert_eq!(body["matchedLineCount"], 2, "{body}");
}

/// Without `regexEngine` every entry is literal text, `|` and `.` included; the
/// entries are not joined into a pattern the caller can see.
#[test]
fn list_entries_are_literal_unless_regex_is_set() {
    let files = [("a.txt", "abc\na.c\nx|y\nx\ny\n")];
    let literal = run(&files, serde_json::json!({"matchString": ["a.c", "x|y"]}));
    assert_eq!(sorted(lines(&literal)), ["a.c", "x|y"], "{literal}");
    let named = run(
        &files,
        serde_json::json!({"matchString": ["a.c", "x|y"], "regexEngine": "literal"}),
    );
    assert_eq!(sorted(lines(&named)), ["a.c", "x|y"], "{named}");
    // `regexEngine:"rust"` makes every entry a pattern of its own.
    let regex = run(
        &files,
        serde_json::json!({"matchString": ["a.c", "x|y"], "regexEngine": "rust"}),
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
        serde_json::json!({"matchString": ["Alpha", "beta"]}),
    );
    assert_eq!(sorted(lines(&smart)), ["Alpha", "BETA", "beta"], "{smart}");
    let sensitive = run(
        &files,
        serde_json::json!({"matchString": ["alpha", "beta"], "caseMode": "sensitive"}),
    );
    assert_eq!(sorted(lines(&sensitive)), ["alpha", "beta"], "{sensitive}");
    let insensitive = run(
        &files,
        serde_json::json!({"matchString": ["Alpha", "BETA"], "caseMode": "insensitive"}),
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
        serde_json::json!({"matchString": ["cat", "dog"], "wholeWord": true}),
    );
    assert_eq!(sorted(lines(&words)), ["cat", "dog"], "{words}");
    let miss = run(&files, serde_json::json!({"matchString": ["zzz", "yyy"]}));
    assert_eq!(miss["matchCount"], 0, "{miss}");
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
        crate::tools::test_support::assert_row_contract(crate::id::ToolId::LocalSearch, &body);
        body["files"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|file| file.get("termCounts").cloned())
            .collect::<Vec<_>>()
    };
    let entries = serde_json::json!(["alpha", "beta", "missing"]);
    assert_eq!(
        counted("countMatches", entries.clone()),
        [
            Some(serde_json::json!([2, 1, 0])),
            Some(serde_json::json!([0, 1, 0]))
        ]
    );
    assert_eq!(
        counted("countLines", entries.clone()),
        [
            Some(serde_json::json!([2, 1, 0])),
            Some(serde_json::json!([0, 1, 0]))
        ]
    );
    assert!(
        counted("countMatches", serde_json::json!(["alpha"]))
            .iter()
            .all(Option::is_none)
    );
    assert!(counted("files", entries).iter().all(Option::is_none));
}
