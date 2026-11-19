//! A literal search that misses says when its text matches as a regex. The row
//! stays the literal answer: `a|b` is usually a regex, `arr[i]` and `char*` are
//! usually code, and the caller chooses with the ready call.
use super::*;

const FILES: [(&str, &str); 1] = [("a.rs", "fn alpha() {}\nfn beta7() {}\n")];

fn fixture() -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("fixture");
    fs::write(root.path().join(FILES[0].0), FILES[0].1).expect("a");
    root
}

fn literal(terms: &[&str]) -> LocalSearchQuery {
    ls_query(serde_json::json!({"matchString": terms}), None)
}

fn offers_regex(row: &serde_json::Value) -> bool {
    octocode_runtime_base::test_support::checks::leads(row)
        .iter()
        .any(|call| call.tool == "localSearch" && call.query["queries"][0]["regexEngine"] == "rust")
}

#[test]
fn a_literal_miss_says_it_matches_as_a_regex_and_offers_the_call() {
    for terms in [
        vec!["alpha|beta"],
        vec!["beta[0-9]"],
        vec!["^fn alpha"],
        vec!["fn .*alpha"],
        vec![r"alph\S"],
        vec!["alpha|zz", "gamma"],
    ] {
        let row = search_fixture(&FILES, literal(&terms));
        assert_eq!(
            row["files"],
            serde_json::json!([]),
            "the literal answer stands: {row}"
        );
        assert!(row["hints"].to_string().contains("as a regex"), "{row}");
        assert!(offers_regex(&row), "{row}");
    }
}

#[test]
fn a_literal_hit_gets_no_regex_offer() {
    let row = search_fixture(&[("a.rs", "a|b\nab\n")], literal(&["a|b"]));
    assert_eq!(row["matchCount"], 1, "the literal line only: {row}");
    assert!(!offers_regex(&row), "{row}");
}

/// The regex reading is offered from the terms alone, never run: plain words
/// and patterns that do not compile get none.
#[test]
fn plain_words_and_broken_regexes_stay_plain() {
    for term in ["gamma", "fn gamma("] {
        let row = search_fixture(&FILES, literal(&[term]));
        assert_eq!(row["files"], serde_json::json!([]), "{term}: {row}");
        assert!(!offers_regex(&row), "{term}: {row}");
    }
}

/// The error names the entry the caller wrote, never the joined pattern.
#[test]
fn a_bad_entry_of_a_list_is_named_as_written() {
    let root = fixture();
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({
            "path": root.path().to_string_lossy(),
            "matchString": ["alpha", "threads("],
            "regexEngine": "rust",
        }),
        None,
    );
    let Err(error) = search_in(&request, &policy, &security, None) else {
        panic!("a broken regex must fail");
    };
    let shown = format!("{error:?}");
    assert!(shown.contains("`threads(`"), "{shown}");
    assert!(!shown.contains("(?i"), "{shown}");
}

/// A term whose only "regex syntax" is a character a regex reads as itself
/// (`-` in `fs-minipass`) means the same either way: no regex reading.
#[test]
fn a_term_that_reads_the_same_as_a_regex_gets_no_offer() {
    for terms in [vec!["fs-minipass"], vec!["no-such-term-here"]] {
        let row = search_fixture(&FILES, literal(&terms));
        assert!(!offers_regex(&row), "{terms:?}: {row}");
        assert!(!row["hints"].to_string().contains("as a regex"), "{row}");
    }
}
