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
        let rows = run(
            &files,
            json!({"matchString": [pattern]}),
            SourceLimits::REAL,
        );
        assert_eq!(listed(&rows), Vec::<String>::new(), "{pattern}: {rows}");
        let hint = rows["hints"][0].as_str().unwrap_or_default().to_owned();
        assert!(
            hint.contains(crate::local_search::coverage::MASKED_UNSEARCHED),
            "{pattern}: {rows}"
        );
        hints.push(hint);
    }
    // The same sentence whether the pattern was in a masked value or nowhere.
    assert!(hints.windows(2).all(|pair| pair[0] == pair[1]), "{hints:?}");
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
