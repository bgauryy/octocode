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
        vec!["alpha|zz", "gamma"],
    ] {
        let row = search_fixture(&FILES, literal(&terms));
        assert_eq!(
            row["files"],
            serde_json::json!([]),
            "the literal answer stands: {row}"
        );
        // The one recovery of the miss is the regex reading: its hint names
        // the field the call sets.
        assert!(
            row["hints"][0]
                .as_str()
                .is_some_and(|hint| hint.contains("regexEngine")),
            "{row}"
        );
        let call = one_recovery(&row).unwrap_or_else(|| panic!("a call: {row}"));
        assert_eq!(call["regexEngine"], "rust", "{row}");
        assert_eq!(call["matchString"], serde_json::json!(terms), "{row}");
    }
}

/// A regex class escape (`\S`, `\W`, `\D`) is regex syntax, not a second
/// spelling of the term: ignoring case reaches nothing new, so the miss is
/// offered as a regex, never as a case reading.
#[test]
fn a_class_escape_in_a_literal_miss_is_offered_as_a_regex_not_as_a_case_reading() {
    let row = search_fixture(&FILES, literal(&[r"alph\S"]));
    assert_eq!(row["files"], serde_json::json!([]), "{row}");
    let call = one_recovery(&row).unwrap_or_else(|| panic!("a call: {row}"));
    assert_eq!(call["regexEngine"], "rust", "{row}");
    assert!(call.get("caseMode").is_none(), "{row}");
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

/// A term whose regex reading is one other fixed text (`pub(crate) fn`
/// reads as `pubcrate fn`, `arr[i]` as `arri`) is code written literally:
/// neither an empty row nor a dead term beside a hit offers the regex
/// reading.
#[test]
fn a_term_whose_regex_reading_is_one_fixed_text_gets_no_offer() {
    for terms in [vec!["pub(crate) fn alpha"], vec!["arr[i]"]] {
        let row = search_fixture(&FILES, literal(&terms));
        assert_eq!(row["files"], serde_json::json!([]), "{terms:?}: {row}");
        assert!(!offers_regex(&row), "{terms:?}: {row}");
        assert!(!row["hints"].to_string().contains("as a regex"), "{row}");
    }
    let row = search_fixture(&FILES, literal(&["fn alpha", "pub(crate) fn beta7"]));
    assert_eq!(row["matchCount"], 1, "{row}");
    assert!(
        !row["warnings"].to_string().contains("regex syntax"),
        "{row}"
    );
}

/// The failure of `fields` over a one-file fixture.
fn refused(fields: serde_json::Value) -> octocode_runtime_base::response::result::ToolError {
    let root = tempfile::tempdir().expect("fixture");
    fs::write(root.path().join("a.rs"), "fn a() {}\n").expect("fixture");
    let (policy, security) = policy_for(root.path());
    let mut fields = fields;
    fields["path"] = root.path().to_string_lossy().into_owned().into();
    let request = ls_query(fields, None);
    match search_in(&request, &policy, &security, None) {
        Ok(_) => panic!("the search runs: {request:?}"),
        Err(error) => error,
    }
}

/// Each regex refusal carries the fix of its own cause, and only that: a
/// line break is fixed by `multiline` (its error names it, so no hint tells
/// another fix), a pattern only PCRE2 compiles by that engine, a pattern no
/// engine compiles by correcting it or reading it as literal text. The
/// tool's shared fallback adds none of them to another cause.
#[test]
fn a_regex_refusal_hints_the_fix_of_its_own_cause() {
    use super::super::pattern::{MALFORMED_REGEX_HINT, NEEDS_PCRE2_HINT};
    use octocode_runtime_base::response::output::ToolOutput;
    assert_eq!(
        super::super::output::Output.error_hint(
            octocode_contract_types::ErrorCode::InvalidPattern,
            &serde_json::Value::Null
        ),
        None,
        "no fix is right for every cause"
    );
    let line_break = refused(serde_json::json!({"matchString": ["a\nb"], "regexEngine": "rust"}));
    assert_eq!(
        line_break.code,
        octocode_contract_types::ErrorCode::InvalidPattern
    );
    assert!(line_break.message.contains("multiline"), "{line_break:?}");
    assert!(line_break.hints.is_empty(), "{line_break:?}");
    for pattern in ["(?=a)b", "(?<!a)b", "(a)\\1"] {
        let error = refused(serde_json::json!({"matchString": [pattern], "regexEngine": "rust"}));
        assert_eq!(
            error.code,
            octocode_contract_types::ErrorCode::InvalidPattern,
            "{pattern}"
        );
        assert_eq!(error.hints, [NEEDS_PCRE2_HINT], "{pattern}");
    }
    for engine in ["rust", "pcre2"] {
        let error = refused(serde_json::json!({"matchString": ["a("], "regexEngine": engine}));
        assert_eq!(
            error.code,
            octocode_contract_types::ErrorCode::InvalidPattern,
            "{engine}"
        );
        assert_eq!(error.hints, [MALFORMED_REGEX_HINT], "{engine}");
    }
}

/// A `fileType` that is no ripgrep type is refused with the type its files
/// are of when the name is an extension, and with the field that selects by
/// name when no type holds such files; a type name is taken.
#[test]
fn an_unknown_file_type_names_the_type_of_its_extension() {
    let typed = |name: &str| serde_json::json!({"matchString": ["fn"], "fileType": name});
    let error = refused(typed("rs"));
    assert_eq!(error.code, octocode_contract_types::ErrorCode::InvalidInput);
    assert!(error.message.contains("rs"), "{error:?}");
    let [hint] = error.hints.as_slice() else {
        panic!("one hint: {error:?}");
    };
    assert!(hint.contains("\"rust\""), "{hint}");
    // The named type is one the search takes.
    let root = tempfile::tempdir().expect("fixture");
    fs::write(root.path().join("a.rs"), "fn a() {}\n").expect("fixture");
    let found = search_root(root.path(), typed("rust"));
    assert!(values(&found).contains("a.rs"), "{}", values(&found));
    let none = refused(typed("nosuchtype0"));
    assert_eq!(none.code, octocode_contract_types::ErrorCode::InvalidInput);
    let [hint] = none.hints.as_slice() else {
        panic!("one hint: {none:?}");
    };
    assert!(
        hint.contains("include") && hint.contains("*.nosuchtype0"),
        "{hint}"
    );
}
