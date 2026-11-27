//! A walk page continued after the file holding the hits its page before
//! left for it was emptied, rewritten without a match or removed: the page
//! is served within the contract, gives no case hint the change caused, and
//! says the hits are gone with their file's change.
use super::*;
use octocode_runtime_base::id::ToolId;
use serde_json::{Value, json};

/// Eight lines of `word` and their number.
fn eight_lines(word: &str) -> String {
    (1..=8).map(|n| format!("{word} line {n}\n")).collect()
}

/// One call's result as an agent receives it: the row staged as the runtime
/// stages it (an empty one says so).
fn served(result: &LocalSearchResult) -> Value {
    let mut data = result.to_row().expect("row");
    if result.status == SearchStatus::Empty {
        data["status"] = json!("empty");
    }
    octocode_runtime_base::test_support::staged_response(ToolId::LocalSearch, vec![data])
}

/// The two pages of a two-row walk over `a-static.txt` and `live.txt`,
/// `change` made to `live.txt` between them: page 1 shows the static file's
/// two hits and leaves live.txt's for page 2. Page 1 is its row (its cursor
/// is not issued here), page 2 as served.
fn two_pages(word: &str, change: impl FnOnce(&std::path::Path)) -> (Value, Value) {
    let dir = tempfile::tempdir().expect("fixture");
    let root = dir.path().canonicalize().expect("real root");
    fs::write(root.join("a-static.txt"), eight_lines(word)).expect("fixture");
    let live = root.join("live.txt");
    fs::write(&live, eight_lines(word)).expect("fixture");
    let (policy, security) = policy_for(&root);
    let start = ls_query(
        json!({
            "path": root,
            "matchString": [format!("{word} line 7"), format!("{word} line 3")],
            "contextLines": {"before": 1, "after": 1},
            "limit": 2
        }),
        None,
    );
    let first = search_in(&start, &policy, &security, None).expect("page 1");
    let first_row = first.to_row().expect("row");
    let next = first_row["next"]["nextPage"]["input"]["queries"][0].clone();
    assert!(
        !next.is_null(),
        "page 1 leaves live.txt's hits: {first_row}"
    );
    change(&live);
    let second = search_in(&ls_query(next, None), &policy, &security, None).expect("page 2");
    (first_row, served(&second))
}

/// A change made to live.txt between the pages, by name.
type Change = (&'static str, fn(&std::path::Path));

/// The served row of a one-query result.
fn row(out: &Value) -> &Value {
    &out["results"][0]
}

#[test]
fn a_continued_page_whose_hits_file_changed_is_served_within_the_contract() {
    let changes: [Change; 3] = [
        ("emptied", |live| fs::write(live, "").expect("empty")),
        ("rewritten", |live| {
            fs::write(live, "other text\n").expect("rewrite");
        }),
        ("removed", |live| fs::remove_file(live).expect("remove")),
    ];
    // Lowercase terms read case-blind; capitalized ones are case-sensitive,
    // so an empty page would offer the case-ignored reading.
    for word in ["needle", "Needle"] {
        for (change, write) in changes {
            let case = format!("{word} {change}");
            let (first, second) = two_pages(word, write);
            octocode_runtime_base::contracts::validate_output("localSearch", &second)
                .unwrap_or_else(|error| panic!("{case}: {error}\n{second}"));
            assert_ne!(row(&second)["status"], "error", "{case}: {second}");
            let data = &row(&second)["data"];
            assert_ne!(row(&second)["status"], "empty", "{case}: {second}");
            // No row is left; the row states its (empty) files or the
            // walk's totals.
            assert!(
                data["files"].as_array().is_none_or(Vec::is_empty),
                "{case}: {second}"
            );
            assert!(
                data.get("files").is_some() || data.get("matchCount").is_some(),
                "{case}: {second}"
            );
            assert!(data.get("hints").is_none(), "{case}: {second}");
            assert!(
                !second.to_string().contains("insensitive"),
                "{case}: no case reading is offered for a content change: {second}"
            );
            assert_ne!(data["pagination"]["hasMore"], true, "{case}: {second}");
            let warnings = data["warnings"].to_string();
            assert!(
                warnings.contains("changed"),
                "{case}: the page says the hits left for it changed: {second}"
            );
            assert!(
                !first["warnings"].to_string().contains("changed"),
                "{case}: {first}"
            );
        }
    }
}

/// The same walk with live.txt left alone: page 2 shows its hits and says
/// nothing of a change.
#[test]
fn a_continued_page_whose_hits_file_held_shows_them_without_a_change_note() {
    let (_, second) = two_pages("needle", |_| {});
    let data = &row(&second)["data"];
    let shown = data["files"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|file| file["matches"].as_array().cloned().unwrap_or_default())
        .count();
    assert!(shown > 0, "{second}");
    assert!(
        !data["warnings"].to_string().contains("changed"),
        "{second}"
    );
}
