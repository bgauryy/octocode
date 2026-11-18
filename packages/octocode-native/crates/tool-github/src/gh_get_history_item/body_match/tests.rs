//! The hit view of a matched discussion body: hit lines with their context,
//! numbered by body line, runs that touch merged and the others split by
//! `--`; the whole body when no single line holds the hit.
use super::*;
use octocode_test_support::json_fixture;

fn matched(extra: &str) -> HistoryItemRequest {
    let mut row = json_fixture(
        r#"{
            "operation": "issue",
            "owner": "o",
            "repo": "r",
            "number": 7,
            "matchString": ["needle"]
        }"#,
    );
    for (key, value) in json_fixture(extra).as_object().into_iter().flatten() {
        row[key] = value.clone();
    }
    HistoryItemRequest::from_row(row).expect("matched issue read")
}

#[test]
fn hit_view_merges_runs_and_marks_gaps() {
    let body = (1..=12)
        .map(|n| match n {
            3 | 4 | 10 => format!("a needle on {n}"),
            n => format!("line {n}"),
        })
        .collect::<Vec<_>>()
        .join("\r\n");
    let query = matched(r#"{"contextLines": {"before": 1, "after": 1}}"#);
    let needle = query.matcher.as_deref().expect("matcher");
    assert_eq!(
        hit_view(&body, needle, (1, 1)).as_deref(),
        Some(
            "2\tline 2\n3\ta needle on 3\n4\ta needle on 4\n5\tline 5\n--\n9\tline 9\n10\ta needle on 10\n11\tline 11"
        )
    );
    // Runs that touch are one run: no gap marker between them.
    assert_eq!(
        hit_view(&body, needle, (1, 4))
            .as_deref()
            .map(|view| view.contains("--")),
        Some(false)
    );
    let (view, full) = matched_view(&body, &query);
    assert!(view.starts_with("2\tline 2\n"), "{view}");
    assert_eq!(full, Some(body.chars().count()));
}

#[test]
fn a_body_with_no_line_hit_is_shown_whole() {
    let query = matched("{}");
    let needle = query.matcher.as_deref().expect("matcher");
    assert_eq!(hit_view("no match here\nnor here", needle, (0, 0)), None);
    // A view of every line leaves nothing out: the body stays whole.
    assert_eq!(hit_view("a needle", needle, (0, 0)), None);
    assert_eq!(hit_view("one\na needle\ntwo", needle, (1, 1)), None);
    let (view, full) = matched_view("no match here", &query);
    assert_eq!(view, "no match here");
    assert_eq!(full, None);
    let unmatched = HistoryItemRequest::from_row(json_fixture(
        r#"{"operation": "issue", "owner": "o", "repo": "r", "number": 7}"#,
    ))
    .expect("issue read");
    assert_eq!(
        matched_view("a needle", &unmatched),
        ("a needle".into(), None)
    );
}
