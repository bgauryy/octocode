//! Text the redaction writes is no source text. A search for a placeholder,
//! or a part of one, finds only the source's own spelling of it, in every
//! view, in file-level terms and in a long line's windows, for a source
//! judged whole and for one past the whole-file ceiling. A match beside a
//! placeholder, or one across it that holds whatever was masked there, is
//! still the source's. A search for a masked value finds
//! nothing, and the empty row says masked secrets are not searched, whatever
//! was masked. Every value is fake.
use super::*;
use octocode_runtime_base::security::SourceLimits;
use serde_json::json;

const TAIL: &str = "H7Kq2Lm9Zp4Vw8Rt1Yb6Nc3Hd5Jf0GsAeUoI";

/// A line holding a token, which a read shows as a placeholder.
fn masked_line() -> String {
    format!("const KEY: &str = \"ghp_{TAIL}\"; // tokenType oauth")
}

/// A source spelling a placeholder itself, beside one whose token is masked.
fn files() -> [(&'static str, String); 2] {
    [
        (
            "masked.rs",
            format!("fn masked() {{}}\n{}\n", masked_line()),
        ),
        (
            "spelled.rs",
            "fn spelled() -> &'static str {\n    \"[REDACTED]\"\n}\n".to_owned(),
        ),
    ]
}

fn run(
    files: &[(&str, String)],
    fields: serde_json::Value,
    limits: SourceLimits,
) -> serde_json::Value {
    let files = files
        .iter()
        .map(|(name, body)| (*name, body.as_str()))
        .collect::<Vec<_>>();
    search_fixture_at(&files, ls_query(fields, None), limits)
}

/// The file names a result lists.
fn listed(rows: &serde_json::Value) -> Vec<String> {
    rows["files"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|file| file["path"].as_str().expect("a path").to_owned())
        .collect()
}

/// The lines a result counts: `matchedLineCount` in a count view, the
/// match rows in a listing one.
fn lines(rows: &serde_json::Value) -> u64 {
    rows.get("matchedLineCount")
        .or_else(|| rows.get("matchCount"))
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0)
}

#[test]
fn a_placeholder_is_matched_only_where_the_source_spells_it() {
    let files = files();
    for pattern in ["REDACTED", "[REDACTED", "GITHUBTOKENS", "REDACTED-"] {
        for view in [
            "content",
            "matchOnly",
            "files",
            "countLines",
            "countMatches",
        ] {
            let rows = run(
                &files,
                json!({"matchString": [pattern], "view": view}),
                SourceLimits::REAL,
            );
            let spelled = usize::from(pattern == "REDACTED" || pattern == "[REDACTED");
            let expected = if spelled == 1 {
                vec!["spelled.rs"]
            } else {
                vec![]
            };
            assert_eq!(listed(&rows), expected, "{pattern} {view}: {rows}");
            if view != "files" {
                assert_eq!(lines(&rows), spelled as u64, "{pattern} {view}: {rows}");
            }
        }
    }
    // A regex that runs into a placeholder takes text the source does not hold.
    let rows = run(
        &files,
        json!({"matchString": [r#"= "\["#], "regexEngine": "rust"}),
        SourceLimits::REAL,
    );
    assert_eq!(listed(&rows), Vec::<String>::new(), "{rows}");
}

#[test]
fn file_terms_never_find_a_placeholder() {
    let files = files();
    let with = run(
        &files,
        json!({"matchString": ["fn "], "fileHas": ["REDACTED"], "view": "files"}),
        SourceLimits::REAL,
    );
    assert_eq!(listed(&with), vec!["spelled.rs"], "{with}");
    let without = run(
        &files,
        json!({"matchString": ["fn "], "fileLacks": ["REDACTED"], "view": "files"}),
        SourceLimits::REAL,
    );
    assert_eq!(listed(&without), vec!["masked.rs"], "{without}");
}

#[test]
fn a_match_beside_a_placeholder_is_the_sources() {
    let files = files();
    for (pattern, regex) in [
        ("tokenType", false),
        ("const KEY: &str = \"", false),
        (r"\]?\x22; //", true),
    ] {
        let mut fields = json!({"matchString": [pattern], "view": "content"});
        if regex {
            fields["regexEngine"] = json!("rust");
        }
        let rows = run(&files, fields, SourceLimits::REAL);
        // `\]?` takes nothing written: the match is the source's `"; //`.
        assert_eq!(listed(&rows), vec!["masked.rs"], "{pattern}: {rows}");
        let value = rows["files"][0]["matches"][0]["value"]
            .as_str()
            .unwrap_or_default();
        assert!(
            value.contains("[REDACTED-GITHUBTOKENS]"),
            "{pattern}: {value}"
        );
        assert!(!rows.to_string().contains(TAIL), "{rows}");
    }
}

/// A match across a masked value that holds whatever was masked (`.*`,
/// `\S+`) is the source's: it counts and shows the redacted line; one the
/// placeholder's spelling makes (`[A-Z]{12}`) is none.
#[test]
fn a_match_that_holds_whatever_was_masked_counts() {
    let files = files();
    for (pattern, found) in [
        (r#"KEY.*oauth"#, true),
        (r#"= "\S+"; //"#, true),
        (r#"= "[A-Z\[\]-]{12}"#, false),
    ] {
        let rows = run(
            &files,
            json!({"matchString": [pattern], "regexEngine": "rust"}),
            SourceLimits::REAL,
        );
        let expected = if found { vec!["masked.rs"] } else { vec![] };
        assert_eq!(listed(&rows), expected, "{pattern}: {rows}");
        assert!(!rows.to_string().contains(TAIL), "{rows}");
    }
}

#[test]
fn a_masked_value_finds_nothing_and_the_empty_row_says_masked_secrets_are_not_searched() {
    let files = files();
    let mut hints = Vec::new();
    for pattern in [
        format!("ghp_{TAIL}"),
        TAIL[..12].to_owned(),
        "zz-not-anywhere".to_owned(),
    ] {
        // The caller chose the case rule, so no other reading of the term
        // explains the miss before the masked spans do.
        let rows = run(
            &files,
            json!({"matchString": [pattern], "caseMode": "sensitive"}),
            SourceLimits::REAL,
        );
        assert_eq!(listed(&rows), Vec::<String>::new(), "{pattern}: {rows}");
        let masked =
            u32::try_from(rows["maskedSpans"].as_u64().expect("maskedSpans")).expect("span count");
        assert_eq!(
            rows["hints"],
            json!([crate::local_search::coverage::masked_hint(masked, false)]),
            "{pattern}: {rows}"
        );
        assert!(rows.get("related").is_none(), "{pattern}: {rows}");
        hints.push(rows["hints"][0].clone());
    }
    // The same sentence whether the pattern was in a masked value or nowhere.
    assert!(hints.windows(2).all(|pair| pair[0] == pair[1]), "{hints:?}");
    // With the case rule left to the tool, a term that mixes case keeps its
    // case recovery (the one call) and the one hint also says the masked
    // values are not searched: no sentence claims absence inside them. The
    // hint depends on the term's spelling alone, so a guess inside the
    // masked value and a term found nowhere read alike.
    let mut mixed = Vec::new();
    for pattern in [
        TAIL[..12].to_owned(),
        "MixedCaseToken9".to_owned(),
        "Mixed Case".to_owned(),
    ] {
        assert!(pattern.chars().any(char::is_uppercase) && pattern.chars().any(char::is_lowercase));
        let rows = run(
            &files,
            json!({"matchString": [pattern]}),
            SourceLimits::REAL,
        );
        assert_eq!(listed(&rows), Vec::<String>::new(), "{pattern}: {rows}");
        assert_eq!(
            rows["hints"].as_array().map(Vec::len),
            Some(1),
            "{pattern}: {rows}"
        );
        let hint = rows["hints"][0].as_str().unwrap_or_default().to_owned();
        assert!(hint.contains("caseMode"), "{pattern}: {rows}");
        assert!(hint.contains("maskedSpans"), "{pattern}: {rows}");
        assert!(!hint.contains("absence is proven"), "{pattern}: {rows}");
        let related = rows["related"].as_array().map(Vec::len);
        assert_eq!(related, Some(1), "{pattern}: {rows}");
        assert_eq!(
            rows["related"][0]["input"]["queries"][0]["caseMode"], "insensitive",
            "{pattern}: {rows}"
        );
        mixed.push(hint);
    }
    assert!(mixed.windows(2).all(|pair| pair[0] == pair[1]), "{mixed:?}");
    // One masked value reads as one.
    assert!(
        hints[0]
            .as_str()
            .is_some_and(|hint| hint.contains("1 masked secret value (maskedSpans) is never")),
        "{hints:?}"
    );
    // The row counts the masked spans of the files it searched, the same
    // whatever the pattern, and a search of files with none states none.
    let counts = [format!("ghp_{TAIL}"), "zz-not-anywhere".to_owned()].map(|pattern| {
        run(
            &files,
            json!({"matchString": [pattern]}),
            SourceLimits::REAL,
        )
    });
    for rows in &counts {
        assert_eq!(rows["maskedSpans"], 1, "{rows}");
    }
    let clean = run(
        &[("clean.rs", "fn clean() {}\n".to_owned())],
        json!({"matchString": ["zz-not-anywhere"]}),
        SourceLimits::REAL,
    );
    assert!(clean.get("maskedSpans").is_none(), "{clean}");
    assert!(clean.get("hints").is_none(), "a plain miss: {clean}");
    // A regex that needs the masked value's own bytes reaches none of them,
    // whatever else it holds: the probe over written text never reads them.
    for pattern in [
        r"ghp_\S+",
        r#"= "ghp_.*"#,
        r"H7Kq2\w*",
        r#""[^"]*H7Kq[^"]*""#,
    ] {
        let rows = run(
            &files,
            json!({"matchString": [pattern], "regexEngine": "rust"}),
            SourceLimits::REAL,
        );
        assert_eq!(listed(&rows), Vec::<String>::new(), "{pattern}: {rows}");
    }
}

/// A line too long for a row is shown as windows around its own hits: none
/// around a placeholder.
#[test]
fn a_long_lines_windows_open_only_on_its_own_text() {
    let filler = "x".repeat(octocode_tools_shared::source_view::pagination::MIN_BYTE_PAGE);
    let line = format!("{filler} {} {filler} needle\n", masked_line());
    let files = [("long.rs", line)];
    for view in ["content", "matchOnly", "countMatches"] {
        let rows = run(
            &files,
            json!({"matchString": ["GITHUBTOKENS"], "view": view}),
            SourceLimits::REAL,
        );
        assert_eq!(listed(&rows), Vec::<String>::new(), "{view}: {rows}");
    }
    let rows = run(
        &files,
        json!({"matchString": ["tokenType"]}),
        SourceLimits::REAL,
    );
    assert_eq!(rows["matchCount"], json!(1), "{rows}");
}

/// A source past the whole-file ceiling streams its changed lines: their
/// placeholders, of a line a window holds and of one longer than a window,
/// take no match either.
#[test]
fn a_source_past_the_whole_file_ceiling_matches_no_placeholder() {
    let limits = SourceLimits::SMALL;
    let mut text = format!("{}\n", masked_line());
    let mut line = 0;
    while text.len() < limits.whole_source + limits.whole_source / 10 {
        text.push_str(&format!("export const filler{line} = {line}; // plain\n"));
        line += 1;
    }
    let long = format!(
        "{} {}\n",
        "y".repeat(limits.window + limits.window / 2),
        masked_line()
    );
    text.push_str(&long);
    let files = [("big.rs", text)];
    for view in ["content", "countLines", "countMatches", "files"] {
        let rows = run(
            &files,
            json!({"matchString": ["GITHUBTOKENS"], "view": view}),
            limits,
        );
        assert_eq!(listed(&rows), Vec::<String>::new(), "{view}: {rows}");
    }
    let rows = run(
        &files,
        json!({"matchString": ["tokenType"], "view": "countLines"}),
        limits,
    );
    assert_eq!(lines(&rows), 2, "{rows}");
}

/// An inverted search keeps the lines the source's own text does not match:
/// a line whose only match is a placeholder's spelling is kept, and one that
/// spells it itself is not.
#[test]
fn an_inverted_search_keeps_a_line_whose_only_match_is_a_placeholder() {
    let files = files();
    let rows = run(
        &files,
        json!({"matchString": ["REDACTED"], "invertMatch": true}),
        SourceLimits::REAL,
    );
    let lines_of = |name: &str| -> Vec<u64> {
        rows["files"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|file| {
                file["path"]
                    .as_str()
                    .is_some_and(|path| path.ends_with(name))
            })
            .flat_map(|file| file["matches"].as_array().cloned().unwrap_or_default())
            .filter_map(|hit| hit["line"].as_u64())
            .collect()
    };
    // The masked line (2) holds no `REDACTED` of the source's own.
    assert_eq!(lines_of("masked.rs"), [1_u64, 2], "{rows}");
    // The spelled line (2) does.
    assert!(!lines_of("spelled.rs").contains(&2), "{rows}");
    assert!(!rows.to_string().contains(TAIL), "{rows}");
}

/// A search that reads only a placeholder's spelling is not decided by case:
/// its empty row offers no case-insensitive rerun.
#[test]
fn a_placeholder_only_miss_offers_no_case_rerun() {
    let files = files();
    for fields in [
        json!({"matchString": [r"\[[A-Z-]+\]"], "regexEngine": "rust"}),
        json!({"matchString": ["REDACTED"]}),
    ] {
        let rows = run(&files[..1], fields, SourceLimits::REAL);
        let text = rows.to_string();
        assert!(!text.contains("No case-sensitive match"), "{rows}");
        assert!(!text.contains("insensitive"), "{rows}");
    }
    // A term the source spells in another case is offered with case ignored
    // when it mixes case, over a file with a masked value as over one
    // without: there the one hint also says the masked value is not searched.
    for (files, masked) in [
        (&files[..1], true),
        (&[("clean.rs", "const A: u8 = 1;\n".to_owned())][..], false),
    ] {
        let rows = run(files, json!({"matchString": ["Const"]}), SourceLimits::REAL);
        let call = one_recovery(&rows).unwrap_or_else(|| panic!("a call: {rows}"));
        assert_eq!(call["caseMode"], "insensitive", "{rows}");
        assert_eq!(
            rows["hints"][0]
                .as_str()
                .is_some_and(|hint| hint.contains("maskedSpans")),
            masked,
            "{rows}"
        );
    }
}

/// A miss claims absence only for what was searched. Over files holding
/// masked values it leaves those out; when the walk also left paths out
/// (`skippedPaths`: here a credential file the path policy withholds), the
/// row's one hint leaves them out too and proves nothing "everywhere else".
#[test]
fn a_miss_beside_skipped_paths_claims_no_absence_in_them() {
    let masked = files();
    let with_withheld = [
        masked[0].clone(),
        masked[1].clone(),
        (".env", "DUMMY_VALUE=placeholder\n".to_owned()),
    ];
    let term = json!({"matchString": ["nosuchtermanywhere"]});
    let whole = run(&masked, term.clone(), SourceLimits::REAL);
    assert!(whole.get("skippedPaths").is_none(), "{whole}");
    let hint = whole["hints"][0].as_str().unwrap_or_default();
    assert!(hint.contains("maskedSpans"), "{whole}");
    assert!(hint.contains("absence is proven"), "{whole}");
    let partial = run(&with_withheld, term, SourceLimits::REAL);
    assert_eq!(listed(&partial), Vec::<String>::new(), "{partial}");
    assert!(
        partial["skippedPaths"]["entries"]
            .as_array()
            .is_some_and(|entries| !entries.is_empty()),
        "{partial}"
    );
    assert_eq!(
        partial["hints"].as_array().map(Vec::len),
        Some(1),
        "{partial}"
    );
    let hint = partial["hints"][0].as_str().unwrap_or_default();
    assert!(hint.contains("maskedSpans"), "{partial}");
    assert!(hint.contains("skippedPaths"), "{partial}");
    assert!(!hint.contains("proven"), "{partial}");
}
