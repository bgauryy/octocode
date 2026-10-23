use super::*;
use crate::security::scan::ContentScan;
use crate::tools::source_view::pagination::{LINE_PAGE_BYTES, LineBudget, page_within};
use crate::tools::source_view::types::{LocalFetchQuery, wire_count};
use octocode_contract_types::ErrorCode;

#[test]
fn offsets_track_unchanged_text_and_snap_into_placeholders() {
    let raw = "aaaa SECRETSECRET bbbb SECRET2 cccc";
    let clean = "aaaa [REDACTED-X] bbbb [REDACTED-Y] cccc";
    assert_eq!(clean_offset(raw, clean, 0), (0, None));
    assert_eq!(clean_offset(raw, clean, 4), (4, None));
    // A secret's first byte is a boundary, not inside it.
    assert_eq!(clean_offset(raw, clean, 5), (5, None));
    // Inside the first secret: the placeholder start, never secret
    // bytes, and where the secret and its placeholder end.
    let secret_end = raw.find(" bbbb").expect("end");
    let placeholder_end = clean.find(" bbbb").expect("end");
    assert_eq!(
        clean_offset(raw, clean, 8),
        (5, Some((secret_end, placeholder_end)))
    );
    // After it, raw and clean realign on " bbbb".
    let raw_b = raw.find("bbbb").expect("b");
    assert_eq!(&clean[clean_offset(raw, clean, raw_b).0..][..4], "bbbb");
    let raw_c = raw.find("cccc").expect("c");
    assert_eq!(&clean[clean_offset(raw, clean, raw_c).0..], "cccc");
    assert_eq!(clean_offset(raw, clean, raw.len()), (clean.len(), None));
}

/// Redacts every `SECRET…` run of word bytes to one placeholder.
struct SecretScan;
impl ContentScan for SecretScan {
    fn sanitize(
        &self,
        text: &str,
        _: &std::path::Path,
    ) -> Result<(String, Vec<String>), (ErrorCode, String)> {
        let pattern = regex::Regex::new(r"SECRET\w*").expect("pattern");
        Ok((
            pattern.replace_all(text, "[REDACTED-X]").into_owned(),
            vec![],
        ))
    }
}

/// Byte pages through a line whose secret spans several pages: the page
/// the secret starts on shows its whole placeholder and ends after the
/// secret, so no page is left empty and the walk reaches every byte
/// once; an offset inside the secret returns the placeholder.
#[test]
fn a_secret_across_byte_pages_shows_on_the_page_it_starts_on() {
    let secret = format!("SECRET{}", "s".repeat(70));
    let view = format!("{}{secret} {}\n", "a".repeat(25), "b".repeat(25));
    let secret_at = view.find("SECRET").expect("secret");
    let path = std::path::Path::new("v.txt");
    let query = |offset: usize| -> LocalFetchQuery {
        serde_json::from_value(serde_json::json!({
            "path": "v.txt", "unit": "bytes", "offset": offset, "length": 20
        }))
        .expect("query")
    };
    let mut offset = 0;
    let mut pages = Vec::new();
    loop {
        let raw = page_within(
            &view,
            &query(offset),
            LineBudget::bytes(LINE_PAGE_BYTES),
            None,
        );
        let (page, _) = sanitize_byte_page(&view, raw, path, &SecretScan)
            .expect("scan")
            .expect("page");
        assert_eq!(page.pagination.offset, offset);
        assert!(!page.text.is_empty(), "{offset}: an empty page");
        assert!(!page.text.contains("SECRET"), "{offset}: {}", page.text);
        pages.push((offset, page.text.clone()));
        match page.pagination.next_offset {
            Some(next) => offset = next,
            None => break,
        }
    }
    // The secret starts on the second page, which ends after it.
    assert_eq!(pages[1].0, 20);
    assert_eq!(
        pages[1].1,
        format!("{}[REDACTED-X]", "a".repeat(secret_at - 20))
    );
    assert_eq!(pages[2].0, secret_at + secret.len());
    assert_eq!(
        pages
            .iter()
            .map(|(_, text)| text.as_str())
            .collect::<String>(),
        SecretScan.sanitize(&view, path).expect("scan").0
    );
    // An offset inside the secret starts at its placeholder.
    let raw = page_within(
        &view,
        &query(secret_at + 30),
        LineBudget::bytes(LINE_PAGE_BYTES),
        None,
    );
    let (inside, _) = sanitize_byte_page(&view, raw, path, &SecretScan)
        .expect("scan")
        .expect("page");
    assert_eq!(inside.text, "[REDACTED-X]");
    assert_eq!(
        inside.pagination.next_offset,
        Some(secret_at + secret.len())
    );
}

/// Redacts `SECRET` and counts the bytes it scanned.
struct CountingScan(std::cell::Cell<usize>);
impl ContentScan for CountingScan {
    fn sanitize(
        &self,
        text: &str,
        _: &std::path::Path,
    ) -> Result<(String, Vec<String>), (ErrorCode, String)> {
        self.0.set(self.0.get() + text.len());
        Ok((text.replace("SECRET", "[R]"), vec![]))
    }
}

/// The scan of a line page before it was made proportional: the whole
/// view split into lines, the page's lines plus margins scanned.
fn reference(view: &str, raw: &Page) -> Option<String> {
    let lines = view.split_inclusive('\n').collect::<Vec<_>>();
    let (first, last) = (raw.view_lines.0.saturating_sub(1), raw.view_lines.1);
    if raw.out_of_range || first >= last || last > lines.len() {
        return Some(raw.text.clone());
    }
    let (mut start, mut margin) = (first, 0);
    while start > 0 && margin < PAGE_SCAN_MARGIN_BYTES {
        start -= 1;
        margin += lines[start].len();
    }
    let (mut end, mut margin) = (last, 0);
    while end < lines.len() && margin < PAGE_SCAN_MARGIN_BYTES {
        margin += lines[end].len();
        end += 1;
    }
    let clean = lines[start..end].concat().replace("SECRET", "[R]");
    let clean_lines = clean.split_inclusive('\n').collect::<Vec<_>>();
    (clean_lines.len() == end - start).then(|| clean_lines[first - start..last - start].concat())
}

/// Every page of a view scans what the whole-view split scanned, and
/// returns the same text, whatever the line lengths; a page deep in a
/// large view scans only its page and margins.
#[test]
fn a_line_page_scan_matches_the_whole_view_split() {
    let mut view = String::new();
    for line in 0..3_000 {
        let text = match line % 7 {
            0 => "SECRET here".to_owned(),
            1 => String::new(),
            _ => "z".repeat(line % 300),
        };
        view.push_str(&text);
        view.push('\n');
    }
    view.push_str("tail SECRET without newline");
    let path = std::path::Path::new("v.txt");
    let mut query: LocalFetchQuery =
        serde_json::from_value(serde_json::json!({"path": "v.txt"})).expect("query");
    loop {
        let raw = page_within(&view, &query, LineBudget::bytes(LINE_PAGE_BYTES), None);
        let scan = CountingScan(std::cell::Cell::new(0));
        let again = page_within(&view, &query, LineBudget::bytes(LINE_PAGE_BYTES), None);
        let page = sanitize_line_page(&view, again, path, &scan).expect("scan");
        assert_eq!(
            page.map(|(page, _)| page.text),
            reference(&view, &raw),
            "{:?}",
            raw.view_lines
        );
        assert!(scan.0.get() <= raw.text.len() + 2 * (PAGE_SCAN_MARGIN_BYTES + 400));
        match raw.pagination.next_offset {
            Some(next) => query.offset = Some(wire_count(next)),
            None => break,
        }
    }
}
