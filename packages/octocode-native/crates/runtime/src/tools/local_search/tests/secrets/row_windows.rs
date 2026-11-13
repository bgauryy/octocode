//! A content hit in a line too long for a row: the row is a window of the
//! line around the hit, placed by its column, and shows of those bytes what
//! a row of the whole line shows of them. Every value below is fake.
use super::*;

const TOKEN: &str = "ghp_aB3dE5gH7jK9mN1pQ3sT5vX7zA9cE1gI3kM5";
const MARGIN: usize = crate::tools::source_view::extraction::MATCH_MARGIN;

/// One line of about `size` bytes with `middle` in its middle.
fn one_line(size: usize, middle: &str) -> String {
    let unit = "word(a,b){return a+b};";
    let half = unit.repeat(size / 2 / unit.len());
    format!("{half}{middle}{half}")
}

fn found(text: &str, fields: serde_json::Value) -> serde_json::Value {
    let root = tempfile::tempdir().expect("fixture directory");
    let path = root.path().join("min.js");
    fs::write(&path, text).expect("fixture");
    let (policy, security) = policy_for(root.path());
    let mut fields = fields;
    fields["path"] = serde_json::json!(path.to_string_lossy());
    search_in(&ls_query(fields, None), &policy, &security, None)
        .expect("search")
        .to_row()
        .expect("serialize")
}

fn rows(found: &serde_json::Value) -> Vec<serde_json::Value> {
    found["files"][0]["matches"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

/// The line as a row of it whole shows it: what the scanner leaves of the
/// source.
fn whole_line_shown(text: &str, line: usize) -> String {
    let security = ContentSecurity::new();
    let shown = security
        .sanitize_text(text, Some(std::path::Path::new("min.js")))
        .content
        .into_owned();
    shown.lines().nth(line - 1).expect("line").to_owned()
}

#[test]
fn a_hit_in_a_line_too_long_for_a_row_is_a_window_placed_by_its_column() {
    let line = one_line(80_000, " the needle here ");
    let text = format!("// header\n{line}\n");
    let found = found(&text, serde_json::json!({"matchString":["needle"]}));
    let rows = rows(&found);
    assert_eq!(rows.len(), 1, "{found}");
    let value = rows[0]["value"].as_str().expect("value");
    let hit = line.find("needle").expect("hit");
    assert_eq!(rows[0]["line"], 2);
    assert_eq!(rows[0]["column"], hit + 1, "the hit's 1-based column");
    assert_eq!(value, &line[hit - MARGIN..hit + "needle".len() + MARGIN]);
    assert!(found.get("rowPart").is_none() && found.get("pagination").is_none());
    // The line whole is one ready read away.
    let lead = &found["related"][0];
    assert_eq!(lead["tool"], "localFetch", "{found}");
    assert_eq!(
        lead["input"]["queries"][0]["ranges"],
        serde_json::json!([{"startLine":2,"endLine":2}]),
        "{found}"
    );
    assert!(
        found.to_string().len() < 1_000,
        "{}",
        found.to_string().len()
    );
}

#[test]
fn every_hit_of_the_line_is_in_one_window_and_far_hits_have_their_own() {
    let filler = "x=f(1);".repeat(2_000);
    let line = format!("{filler}needle;needle{filler}needle{filler}");
    let found = found(
        &format!("{line}\n"),
        serde_json::json!({"matchString":["needle"]}),
    );
    let rows = rows(&found);
    assert_eq!(rows.len(), 2, "{found}");
    assert_eq!(found["matchCount"], 3);
    assert_eq!(found["matchedLineCount"], 1);
    let first = filler.len();
    assert_eq!(rows[0]["column"], first + 1);
    assert_eq!(
        rows[0]["value"]
            .as_str()
            .expect("value")
            .matches("needle")
            .count(),
        2
    );
    assert_eq!(
        rows[1]["column"],
        first + "needle;needle".len() + filler.len() + 1
    );
}

#[test]
fn a_window_shows_of_its_bytes_what_the_whole_line_row_shows() {
    let near = format!(" needle token={TOKEN} after ");
    let far = format!(" needle {} token={TOKEN} ", "pad(1);".repeat(40));
    // The window's edge falls inside the token.
    let edge = format!(" needle {}{TOKEN} ", "p".repeat(MARGIN - 10));
    for (name, middle) in [("near", near), ("far", far), ("edge", edge)] {
        let line = one_line(40_000, &middle);
        let text = format!("{line}\n");
        let found = found(&text, serde_json::json!({"matchString":["needle"]}));
        let rows = rows(&found);
        assert_eq!(rows.len(), 1, "{name}: {found}");
        let value = rows[0]["value"].as_str().expect("value");
        assert!(value.contains("needle"), "{name}: {value}");
        assert!(value.len() < 1_000, "{name}: a window, not the line");
        for at in 0..TOKEN.len() - 7 {
            assert!(!value.contains(&TOKEN[at..at + 8]), "{name}: {value}");
        }
        let whole = whole_line_shown(&text, 1);
        assert!(!whole.contains(TOKEN), "{name}: the fixture's token is one");
        assert!(
            whole.contains(value),
            "{name}: {value:?} is not what the whole line shows there"
        );
        let hides = name != "far";
        assert_eq!(value.contains("[REDACTED"), hides, "{name}: {value}");
    }
}

#[test]
fn span_rows_and_lines_that_fit_a_row_are_unchanged() {
    let line = one_line(40_000, " the needle here ");
    let short = "a short needle line";
    let text = format!("{short}\n{line}\n");
    let content = found(&text, serde_json::json!({"matchString":["needle"]}));
    let rows_of = rows(&content);
    assert_eq!(rows_of[0]["value"], short);
    assert!(rows_of[0].get("column").is_none(), "{content}");
    let spans = found(
        &text,
        serde_json::json!({"matchString":["needle"],"view":"matchOnly"}),
    );
    for row in rows(&spans) {
        assert_eq!(row["value"], "needle", "{spans}");
    }
}

#[test]
fn a_window_row_stands_alone_among_context_blocks() {
    let line = one_line(40_000, " the needle here ");
    let text = format!("above needle\n{line}\nbelow\n");
    let found = found(
        &text,
        serde_json::json!({"matchString":["needle"],"contextLines":{"before":1,"after":1}}),
    );
    let rows = rows(&found);
    let window = rows
        .iter()
        .find(|row| row["line"] == 2)
        .unwrap_or_else(|| panic!("{found}"));
    let value = window["value"].as_str().expect("value");
    let hit = line.find("needle").expect("hit");
    assert_eq!(value, &line[hit - MARGIN..hit + "needle".len() + MARGIN]);
    assert_eq!(window["column"], hit + 1);
    // No other row carries the long line as its context.
    assert!(
        found.to_string().len() < 2_000,
        "{}",
        found.to_string().len()
    );
}

/// Window rows keep their place and column on every page of a stored scan,
/// and each is shown once.
#[test]
fn window_rows_page_like_any_row() {
    let root = tempfile::tempdir().expect("fixture directory");
    let mut expected = Vec::new();
    for file in 0..6 {
        let filler = "y=g(2);".repeat(1_500);
        let line = format!("{filler}needle{file}{filler}needle{file}{filler}");
        fs::write(root.path().join(format!("f{file}.js")), format!("{line}\n")).expect("fixture");
        let first = filler.len() + 1;
        expected.push(format!("f{file}.js:1:{first}"));
        expected.push(format!("f{file}.js:1:{}", first + 7 + filler.len()));
    }
    let (walk, pages) = walk_search(
        root.path(),
        serde_json::json!({"path": root.path(), "matchString": ["needle"], "sort": {"by":"path","order":"asc"}}),
        Some(1_200),
    );
    assert!(pages.len() > 1, "the walk pages: {}", pages.len());
    let mut seen = pages.iter().flat_map(row_ids).collect::<Vec<_>>();
    seen.sort();
    assert_eq!(seen, expected);
    drop(walk);
}
