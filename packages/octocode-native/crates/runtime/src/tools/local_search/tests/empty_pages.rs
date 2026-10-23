//! Empty and partial results: the probes, spelling variants and leads a
//! page offers when its query found nothing or less than it asked.
use super::*;
use octocode_contract_types::ErrorCode;

mod leads;
mod skipped;
mod walk_limits;

/// Without `regex`, text is literal, so `.unwrap()`, an empty group `()`,
/// `x[0]`, `.get(0)?` and `a|b` find only that text, and an explicit `regex`
/// always wins. An empty literal search mentions the regex reading; a page
/// with hits does only for an alternation that matches more lines
/// ([`a_literal_alternation_with_hits_names_a_wider_regex_reading`]).
#[test]
fn an_unset_regex_reads_every_text_literally() {
    let files = [("a.rs", "x.unwrap();\nunwrap_or(1);\nfoo_unwrap()\n")];
    let run = |fields: serde_json::Value| search_fixture(&files, ls_query(fields, None));
    let lines = |body: &serde_json::Value| body["matchedLineCount"].as_u64().unwrap_or(0);
    let warned = |body: &serde_json::Value, text: &str| {
        body["warnings"].as_array().is_some_and(|all| {
            all.iter()
                .any(|w| w.as_str().is_some_and(|w| w.contains(text)))
        })
    };
    let literal = run(serde_json::json!({"matchString": ".unwrap()"}));
    assert_eq!(lines(&literal), 1, "{literal}");
    assert!(!warned(&literal, "ran as literal text"), "{literal}");
    let missed = run(serde_json::json!({"matchString": ".unwrap_or(2)"}));
    assert!(warned(&missed, "ran as literal text"), "{missed}");
    // Code punctuation is not a regex: indexing and `?` find their text.
    let code = [(
        "b.rs",
        "let a = matches[0];\nlet b = v.get(0)?;\nlet c = x;\n",
    )];
    for text in ["matches[0]", ".get(0)?"] {
        let body = search_fixture(
            &code,
            ls_query(serde_json::json!({"matchString": text}), None),
        );
        assert_eq!(lines(&body), 1, "{text}: {body}");
    }
    assert!(literal["next"].get("searchLiteral").is_none(), "{literal}");
    let group = run(serde_json::json!({"matchString": "()"}));
    assert_eq!(lines(&group), 2, "{group}");
    // `a|b` is no exception: without `regex` it is literal text too.
    let alternation = run(serde_json::json!({"matchString": "unwrap_or|foo_"}));
    assert_eq!(lines(&alternation), 0, "{alternation}");
    assert!(warned(&alternation, "ran as literal text"), "{alternation}");
    // The regex reading is an executable retry, not only prose.
    let retry = &alternation["next"]["findSimilar"];
    let row = retry.pointer("/query/queries/0").unwrap_or(&retry["query"]);
    assert_eq!(row["regex"], "rust", "{alternation}");
    assert_eq!(row["matchString"], "unwrap_or|foo_", "{alternation}");
    let retried = run(row.clone());
    assert_eq!(lines(&retried), 2, "{retried}");
    let alternation = run(serde_json::json!({"matchString": "unwrap_or|foo_", "regex": "rust"}));
    assert_eq!(lines(&alternation), 2, "{alternation}");
    let explicit = run(serde_json::json!({"matchString": ".unwrap()", "regex": "rust"}));
    assert_eq!(lines(&explicit), 2, "{explicit}");
    let exact = run(serde_json::json!({"matchString": "unwrap_or|foo_", "regex": "literal"}));
    assert_eq!(lines(&exact), 0, "{exact}");
    for quiet in [
        &explicit,
        &exact,
        &run(serde_json::json!({"matchString": "unwrap"})),
    ] {
        assert!(
            !warned(quiet, "ran as literal text") && !warned(quiet, "ran as a regex"),
            "{quiet}"
        );
    }
}

/// The hint is not special to `|`: an empty literal search whose text holds
/// any regex metacharacter (the general case of an agent meaning a pattern)
/// warns, and its `findSimilar` lead is a runnable `regex:"rust"` query that
/// finds the text it was meant for.
#[test]
fn an_empty_literal_search_with_any_regex_character_leads_to_a_regex_run() {
    let files = [("a.txt", "abc\nfoo\nxyz\n123\nzz9\n")];
    for (text, found) in [
        ("a.c", 1),
        ("fo+", 1),
        ("^xy", 1),
        ("\\d+", 2),
        ("[xz]yz?$", 1),
        ("abc|xyz", 2),
        ("z{2}", 1),
    ] {
        let body = search_fixture(
            &files,
            ls_query(serde_json::json!({"matchString": text}), None),
        );
        assert_eq!(body["matchedLineCount"], 0, "{text}: {body}");
        let lead = &body["next"]["findSimilar"];
        let row = lead.pointer("/query/queries/0").unwrap_or(&lead["query"]);
        assert_eq!(row["regex"], "rust", "{text}: {body}");
        assert_eq!(row["matchString"], text, "{text}: {body}");
        let retried = search_fixture(&files, ls_query(row.clone(), None));
        assert_eq!(retried["matchedLineCount"], found, "{text}: {retried}");
    }
}

/// A literal page with hits whose text reads as an alternation (`a|b`)
/// warns, with the regex reading as an executable lead, only when one count
/// of that reading finds more matched lines; an explicit `regex:"literal"`
/// and an alternation with an empty side are taken as written.
#[test]
fn a_literal_alternation_with_hits_names_a_wider_regex_reading() {
    let warned = |body: &serde_json::Value| {
        body["warnings"].as_array().into_iter().flatten().any(|w| {
            w.as_str()
                .is_some_and(|w| w.contains("it matches") && w.contains("with regex:"))
        })
    };
    let wider = [("a.txt", "alpha\nbeta\nalpha|beta\n")];
    let body = search_fixture(
        &wider,
        ls_query(serde_json::json!({"matchString": "alpha|beta"}), None),
    );
    assert_eq!(body["matchedLineCount"], 1, "{body}");
    assert!(
        body["warnings"].as_array().into_iter().flatten().any(|w| w
            == "matchString ran as literal text (1 matched line); with regex:\"rust\" it matches 3."),
        "{body}"
    );
    let lead = &body["next"]["findSimilar"];
    assert_eq!(lead["query"]["queries"][0]["regex"], "rust", "{body}");
    let retried = search_fixture(&wider, ls_query(lead["query"]["queries"][0].clone(), None));
    assert_eq!(retried["matchedLineCount"], 3, "{retried}");
    for (files, fields) in [
        // The regex reading matches no more lines.
        (
            [("b.txt", "alpha|beta\nother\n")],
            serde_json::json!({"matchString": "alpha|beta"}),
        ),
        // Literal on purpose.
        (
            [("c.txt", "alpha\nalpha|beta\n")],
            serde_json::json!({"matchString": "alpha|beta", "regex": "literal"}),
        ),
        // An empty alternative is not an alternation of terms.
        (
            [("d.txt", "alpha|\nalpha\nother\n")],
            serde_json::json!({"matchString": "alpha|"}),
        ),
    ] {
        let body = search_fixture(&files, ls_query(fields, None));
        assert!(!warned(&body), "{body}");
        assert!(body["next"].get("findSimilar").is_none(), "{body}");
    }
}

/// An empty literal search offers the regex reading under the engine the
/// text compiles in (lookaround: pcre2), across lines when it matches a line
/// break, and every such lead runs and finds the line.
#[test]
fn a_literal_miss_leads_to_the_regex_reading_that_runs() {
    let files = [(
        "p.txt",
        "price 100 dollars\nprice 200 euros\nstart\nmiddle\n",
    )];
    let run = |fields: serde_json::Value| search_fixture(&files, ls_query(fields, None));
    for (text, engine, across_lines) in [
        ("price (?=\\d+ euros)", "pcre2", false),
        ("(?<=price )200", "pcre2", false),
        ("start\\nmiddle", "rust", true),
    ] {
        let missed = run(serde_json::json!({"matchString": text}));
        assert_eq!(
            missed["matchedLineCount"].as_u64().unwrap_or(0),
            0,
            "{missed}"
        );
        let retry = &missed["next"]["findSimilar"];
        let row = retry.pointer("/query/queries/0").unwrap_or(&retry["query"]);
        assert_eq!(row["regex"], engine, "{text}: {missed}");
        assert_eq!(row["multiline"] == "on", across_lines, "{text}: {missed}");
        let retried = run(row.clone());
        assert_eq!(retried["matchedLineCount"], 1, "{text}: {retried}");
    }
}

/// An empty page the `fileHas`/`fileLacks` terms caused says so, and its
/// `broadenSearch` lead (the same search without them) finds the matches.
#[test]
fn an_empty_page_the_file_terms_caused_names_them() {
    let files = [("a.txt", "needle\n"), ("b.txt", "needle\nother\n")];
    let run = |fields: serde_json::Value| search_fixture(&files, ls_query(fields, None));
    for filter in [
        serde_json::json!({"fileHas": ["absent"]}),
        serde_json::json!({"fileLacks": ["needle"]}),
    ] {
        let mut fields = serde_json::json!({"matchString": "needle"});
        fields
            .as_object_mut()
            .unwrap()
            .extend(filter.as_object().unwrap().clone());
        let empty = run(fields);
        assert_eq!(
            empty["matchedLineCount"].as_u64().unwrap_or(0),
            0,
            "{empty}"
        );
        assert!(
            empty["warnings"]
                .to_string()
                .contains("2 file(s) matched matchString but fileHas/fileLacks removed them"),
            "{empty}"
        );
        let lead = &empty["next"]["broadenSearch"];
        let row = lead.pointer("/query/queries/0").unwrap_or(&lead["query"]);
        assert!(
            row.get("fileHas").is_none() && row.get("fileLacks").is_none(),
            "{row}"
        );
        assert_eq!(run(row.clone())["matchedLineCount"], 2, "{row}");
    }
    let kept = run(serde_json::json!({"matchString": "needle", "fileHas": ["other"]}));
    assert_eq!(kept["matchedLineCount"], 1, "{kept}");
    assert!(kept["next"].get("broadenSearch").is_none(), "{kept}");
}

/// A case-sensitive miss (smart case on uppercase text, or `sensitive`)
/// whose text matches in another case says so, and its `ignoreCase` lead
/// finds it.
#[test]
fn a_case_only_miss_leads_to_ignore_case() {
    let files = [("a.ts", "const structuredContent = 1;\n")];
    let run = |fields: serde_json::Value| search_fixture(&files, ls_query(fields, None));
    for fields in [
        serde_json::json!({"matchString": "STRUCTUREDCONTENT"}),
        serde_json::json!({"matchString": "StructuredContent", "caseMode": "sensitive"}),
    ] {
        let empty = run(fields);
        assert_eq!(
            empty["matchedLineCount"].as_u64().unwrap_or(0),
            0,
            "{empty}"
        );
        assert!(
            empty["hints"]
                .to_string()
                .contains("matches ignoring case in 1 file(s)"),
            "{empty}"
        );
        let lead = &empty["next"]["ignoreCase"];
        let row = lead.pointer("/query/queries/0").unwrap_or(&lead["query"]);
        assert_eq!(row["caseMode"], "insensitive", "{empty}");
        assert_eq!(run(row.clone())["matchedLineCount"], 1, "{row}");
    }
}

/// The case probe applies the row's own `wholeWord`: text that occurs only
/// inside a longer word is no case miss, so no `ignoreCase` lead that finds
/// nothing again. The hint names `wholeWord` as what blocks the hit, and its
/// `broadenSearch` lead drops it and finds the file. A whole-word hit in
/// another case still leads to `ignoreCase`, which keeps `wholeWord`.
#[test]
fn a_whole_word_miss_names_whole_word_not_case() {
    let files = [
        ("src/a.rs", "const MAX_COLLECTED_FILES: usize = 1;\n"),
        ("src/b.rs", "let max_collected_files = 1;\n"),
    ];
    let run = |fields: serde_json::Value| search_fixture(&files, ls_query(fields, None));
    let lead_row = |lead: &serde_json::Value| {
        lead.pointer("/query/queries/0")
            .unwrap_or(&lead["query"])
            .clone()
    };
    for (case_mode, files) in [("smart", 1), ("sensitive", 1), ("insensitive", 2)] {
        let empty = run(serde_json::json!({
            "matchString": "MAX_COLLECTED", "wholeWord": true, "caseMode": case_mode
        }));
        assert_eq!(
            empty["matchedLineCount"].as_u64().unwrap_or(0),
            0,
            "{empty}"
        );
        assert!(empty["next"].get("ignoreCase").is_none(), "{empty}");
        assert!(
            empty["hints"].to_string().contains(&format!(
                "matches inside longer words in {files} file(s); wholeWord excludes them"
            )),
            "{case_mode}: {empty}"
        );
        let row = lead_row(&empty["next"]["broadenSearch"]);
        assert!(row.get("wholeWord").is_none(), "{row}");
        assert_eq!(run(row.clone())["matchedLineCount"], files, "{row}");
    }
    let files = [("src/a.rs", "let max_collected = 1;\n")];
    let empty = search_fixture(
        &files,
        ls_query(
            serde_json::json!({"matchString": "MAX_COLLECTED", "wholeWord": true}),
            None,
        ),
    );
    let row = lead_row(&empty["next"]["ignoreCase"]);
    assert_eq!(row["wholeWord"], true, "{empty}");
    assert_eq!(
        search_fixture(&files, ls_query(row.clone(), None))["matchedLineCount"],
        1,
        "{row}"
    );
}
