//! Standard and minified views cite and page in source lines.

use super::*;
use octocode_contract_types::ReadProgressUnit;

/// `view:"compact"` keeps the citation gutter: every kept line opens
/// with its own source line number and a tab; comments and blank lines
/// the view drops leave gaps in the numbers, never a wrong number.
#[test]
fn standard_view_lines_cite_their_source_lines() {
    let t = Fixture::new();
    let paths = Paths(t.0.clone());
    let p = t.0.join("a.js");
    fs::write(
            &p,
            "// header comment\n\nimport { a } from 'a';\n\n/* block\n   comment */\nexport function run(x) {\n  return a(x); // trailing\n}\n",
        )
        .expect("fixture");
    let read = |fields: serde_json::Value| fetch(&qj(&p, fields), &paths).content.expect("content");
    let content = read(serde_json::json!({"view":"compact"}));
    let source = fs::read_to_string(&p).expect("source");
    let squash = |text: &str| {
        text.chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>()
    };
    let mut numbers = Vec::new();
    for line in content.lines() {
        let (number, text) = line
            .split_once('\t')
            .unwrap_or_else(|| panic!("every line numbered: {content:?}"));
        let number = number.parse::<usize>().expect("number");
        let original = source.lines().nth(number - 1).expect("source line");
        assert!(
            squash(original).contains(&squash(text)),
            "{number}: {text:?} vs {original:?}"
        );
        numbers.push(number);
    }
    assert_eq!(numbers.first(), Some(&3), "{content:?}");
    assert!(numbers.contains(&8), "{content:?}");
    // A line window keeps the window's own numbers.
    let window = read(serde_json::json!({"view":"compact","ranges":[{"startLine":7,"endLine":9}]}));
    assert!(window.starts_with("7\t"), "{window:?}");
    assert!(
        window.lines().last().is_some_and(|l| l.starts_with("9\t")),
        "{window:?}"
    );
}

/// A minified view pages in raw source lines, the unit of `totalLines`,
/// the gutter numbers and `ranges`: each page covers raw lines
/// `offset+1..=offset+length`, its gutter numbers lie inside that span,
/// pages are contiguous, and together they reach every raw line and show
/// every kept line exactly once.
#[test]
fn minified_views_page_in_raw_source_lines() {
    let t = Fixture::new();
    let paths = Paths(t.0.clone());
    let p = t.0.join("a.js");
    // A kept view over the response window is cut to a bounded first page.
    // Every page re-minifies the whole file, so the view stays near the
    // window (a quarter over, for the bytes minifying trims from each line).
    let window = 2 * crate::tools::source_view::pagination::LINE_PAGE_BYTES;
    let kept_line = |group: usize| format!("{}\texport const value{group} = {group};\n", group * 3);
    let (mut groups, mut kept_bytes) = (0, 0);
    while kept_bytes <= window + window / 4 {
        groups += 1;
        kept_bytes += kept_line(groups).len();
    }
    let source: String = (1..=groups)
        .map(|n| format!("// note {n}\n\nexport const value{n} = {n};\n"))
        .collect();
    fs::write(&p, &source).expect("fixture");
    let raw_total = groups * 3;
    let kept: Vec<usize> = (1..=groups).map(|n| n * 3).collect();
    for budget in [800, window] {
        let first = serde_json::json!({"view":"compact"});
        let mut query = qj(&p, first.clone());
        let (mut expected_offset, mut shown, mut pages) = (0, Vec::new(), 0);
        for _ in 0..1_000 {
            pages += 1;
            let page = execute_local_fetch(
                &query,
                &paths,
                &Safe,
                &NeverCancel,
                Some(budget),
                None,
                &Default::default(),
            );
            assert_eq!(page.total_lines, Some(raw_total), "{first}");
            let pagination = page.pagination.clone().expect("paged");
            assert_eq!(pagination.unit, ReadProgressUnit::Lines, "{first}");
            assert_eq!(pagination.total_lines, raw_total, "{first} {pagination:?}");
            assert_eq!(pagination.offset, expected_offset, "{first} {pagination:?}");
            let span = pagination.offset + 1..=pagination.offset + pagination.length;
            for line in page.content.as_deref().unwrap_or_default().lines() {
                let number: usize = line
                    .split_once('\t')
                    .and_then(|(number, _)| number.parse().ok())
                    .unwrap_or_else(|| panic!("numbered line: {line:?}"));
                assert!(span.contains(&number), "{first} {number} outside {span:?}");
                shown.push(number);
            }
            expected_offset = pagination.offset + pagination.length;
            match page.next.and_then(|next| next.continue_window) {
                Some(next) => {
                    assert!(pagination.has_more, "{first} {pagination:?}");
                    assert_eq!(
                        local_fetch_query::offset(&next.query),
                        Some(expected_offset),
                        "{first}"
                    );
                    query = next.query;
                }
                None => break,
            }
        }
        assert_eq!(
            expected_offset, raw_total,
            "{first}: pages reach the last raw line"
        );
        assert_eq!(shown, kept, "{first}: every kept line exactly once");
        assert!(pages > 1, "{first}: the view is one page");
    }
}

/// `view:"compact"` pages in the lines a raw read of the same selection
/// pages in: the extracted lines of a `ranges` read, as the source lines
/// of a whole-file read.
#[test]
fn a_standard_view_of_ranges_pages_in_the_extracted_lines() {
    let t = Fixture::new();
    let p = t.0.join("m.rs");
    let text: String = (1..=30)
        .map(|line| {
            if line % 2 == 0 {
                format!("// comment {line}\n")
            } else {
                format!("const V{line}: u8 = {line};\n")
            }
        })
        .collect();
    fs::write(&p, text).expect("fixture");
    let r = fetch(
        &qj(
            &p,
            serde_json::json!({"ranges":[{"startLine":13,"endLine":20}], "view": "compact", "readProgress":{"unit":"lines","offset":2}}),
        ),
        &Paths(t.0.clone()),
    );
    let page = r.pagination.as_ref().expect("pagination");
    assert_eq!((page.offset, page.total_lines), (2, 8), "{r:?}");
    let first = r
        .content
        .as_deref()
        .and_then(|content| content.split('\t').next())
        .and_then(|line| line.parse::<usize>().ok())
        .expect("gutter");
    assert_eq!(first, 15, "{r:?}");
}

/// A minified read whose view removes every selected line (a comment-only
/// range) is empty, says why, and leads to the same read as is; it states
/// no span of text it does not show.
#[test]
fn a_minified_read_of_only_comments_leads_to_the_read_as_is() {
    let t = Fixture::new();
    let paths = Paths(t.0.clone());
    let p = t.0.join("lib.rs");
    let source = format!(
        "//! doc one\n//! doc two\n/* block\n   comment */\n{}",
        "pub fn f() {}\n".repeat(30)
    );
    fs::write(&p, &source).expect("fixture");
    let query = qj(
        &p,
        serde_json::json!({"view":"compact","ranges":[{"startLine":1,"endLine":4}]}),
    );
    let page = execute_local_fetch(
        &query,
        &paths,
        &Safe,
        &NeverCancel,
        None,
        None,
        &Default::default(),
    );
    assert_eq!(page.status, ReadStatus::Empty, "{page:?}");
    assert_eq!(page.content.as_deref(), Some(""));
    assert!(
        page.hints
            .iter()
            .any(|hint| hint.contains("compact view removed")),
        "{:?}",
        page.hints
    );
    let read = page
        .next
        .as_ref()
        .and_then(|next| next.read.clone())
        .expect("read");
    assert_eq!(
        local_fetch_query::view(&read.query),
        octocode_contract_types::ReadView::Content
    );
    let raw = execute_local_fetch(
        &read.query,
        &paths,
        &Safe,
        &NeverCancel,
        None,
        None,
        &Default::default(),
    );
    assert!(
        raw.content
            .as_deref()
            .is_some_and(|text| text.contains("doc one")),
        "{raw:?}"
    );
    let wire = serde_json::to_value(&page).expect("wire");
    assert!(wire.get("startLine").is_none(), "{wire}");
}
