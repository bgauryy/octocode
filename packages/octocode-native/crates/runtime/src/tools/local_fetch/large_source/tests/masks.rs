//! Masks on a held window: exact spans, and the fallback when a walk cannot keep a
//! source's structure. Every value is fake.
use super::super::masks::{Judged, cover_every_value, judge, with_placeholders};
use super::super::stream::stream_pass;
use super::*;
use octocode_engine::security::Named;

const VALUE: &str = "RmFrZTBWYWx1ZTlOb3Q0UmVhbDhCbG9iN0xvbmc2RW5vdWdo";
const JSON: &str = "[REDACTED-KUBERNETESSECRETJSON]";
const YAML: &str = "[REDACTED-KUBERNETESSECRETS]";

fn every_value(text: &str, cut: bool, objects: bool, yaml: bool) -> String {
    let mut window = text.as_bytes().to_vec();
    cover_every_value(&mut window, cut, Named { objects, yaml });
    assert_eq!(window.len(), text.len(), "a mask keeps the view's length");
    with_placeholders(std::str::from_utf8(&window).expect("utf8")).into_owned()
}

/// Past the budget, no value can be told from another: names and structure stay.
#[test]
fn the_fallback_hides_every_json_string_but_names() {
    assert_eq!(
        every_value(
            "{\"a\": \"plain words\", \"b\" : [\"c\", 1], \"n\": 7,\n  \"esc\": \"x\\\"y\"\n}\n",
            false,
            true,
            false
        ),
        format!(
            "{{\"a\": \"{JSON}\", \"b\" : [\"{JSON}\", 1], \"n\": 7,\n  \"esc\": \"{JSON}\"\n}}\n"
        )
    );
    // A value held escaped goes with the string that holds it.
    assert_eq!(
        every_value("\"a\": \"{\\\"data\\\":\\\"x\\\"}\"\n", false, true, false),
        format!("\"a\": \"{JSON}\"\n")
    );
    // A string left open by the window's end, or by its line's.
    assert_eq!(
        every_value("\"a\": \"open\n\"b\": \"cut", false, true, false),
        format!("\"a\": \"{JSON}\n\"b\": \"{JSON}")
    );
}

/// A window cut inside a line starts in an unknown state: that line goes.
#[test]
fn the_fallback_hides_the_line_a_window_starts_inside() {
    assert_eq!(
        every_value(
            "of a value\", \"x\": \"y\"\n\"name\": \"z\"\n",
            true,
            true,
            false
        ),
        format!("{JSON}\n\"name\": \"{JSON}\"\n")
    );
}

#[test]
fn the_fallback_hides_every_yaml_value_but_names() {
    assert_eq!(
        every_value(
            "---\nstringData:\n  blob: plain words\n  block: |\n    more plain words\n- item: x\n\n",
            false,
            false,
            true
        ),
        format!(
            "---\nstringData:\n  blob: {YAML}\n  block: {YAML}\n    {YAML}\n- item: {YAML}\n\n"
        )
    );
}

fn secret_source() -> String {
    let filler = "  {\"id\": 1, \"note\": \"public\"},\n".repeat(120_000);
    format!(
        "{{\"items\":[\n  {{\"data\":{{\"blob\":\"{VALUE}\"}},\"kind\":\"Secret\"}},\n{filler}  {{\"data\":{{\"blob\":\"{VALUE}\"}},\"kind\":\"Secret\"}},\n  {{\"data\":{{\"blob\":\"public\"}},\"kind\":\"ConfigMap\"}}\n]}}\n"
    )
}

/// A walk past its budget hides what it could not place; it never shows it.
#[test]
fn a_walk_over_its_budget_falls_back_to_every_value() {
    let text = secret_source();
    let file = temp(text.as_bytes());
    let named = Named {
        objects: true,
        yaml: false,
    };
    let digest = hex::encode(Sha256::digest(text.as_bytes()));
    let judged = judge(&opened(file.path()), named, &digest, 1, &NeverCancel).expect("judged");
    assert!(matches!(judged, Judged::EveryValue(_)));
    let mut lines = super::super::held::WindowLines::new(
        &[Want::lines(2, 3)],
        crate::security::KeyBlockState::default(),
    );
    for (at, line) in text.split_inclusive('\n').take(3).enumerate() {
        lines.push(line.as_bytes(), at + 1);
        lines.finish(at + 1);
    }
    lines.seal(&[], &judged).expect("sealed");
    let window = String::from_utf8(lines.held[0].window.clone()).expect("utf8");
    assert!(!window.contains(VALUE), "{window}");
    assert!(lines.held[0].every_value, "the page says why");
    assert!(with_placeholders(&window).contains(&format!("\"blob\":\"{JSON}\"")));
}

/// Bytes that are no longer the pass's fail the read, whose windows came from it.
#[test]
fn a_walk_of_other_bytes_fails_the_read() {
    let file = temp(secret_source().as_bytes());
    let named = Named {
        objects: true,
        yaml: false,
    };
    let walked = judge(
        &opened(file.path()),
        named,
        "not-its-digest",
        usize::MAX,
        &NeverCancel,
    );
    assert!(walked.is_err());
}

/// A key block's placeholder is shorter than the block: the spans after it still
/// land on their own bytes.
#[test]
fn a_mask_lands_on_its_bytes_after_a_key_block() {
    let text = format!(
        "-----BEGIN RSA PRIVATE KEY-----\nAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\n-----END RSA PRIVATE KEY-----\n{{\"data\":{{\"blob\":\"{VALUE}\"}},\"kind\":\"Secret\"}}\n\"tail\": \"public\"\n"
    );
    let file = temp(text.as_bytes());
    let windows = stream(&opened(file.path()), &[Want::lines(1, 5)], &NeverCancel).expect("stream");
    let window = String::from_utf8(windows[0].window.clone()).expect("utf8");
    let shown = with_placeholders(&window);
    assert!(
        shown.contains(&format!("{{\"data\":{{{JSON}}},\"kind\":\"Secret\"}}\n")),
        "{shown}"
    );
    assert!(shown.ends_with("\"tail\": \"public\"\n"), "{shown}");
    assert!(!shown.contains(VALUE), "{shown}");
}

/// A window read from a checkpoint of the kept pass lands on the same bytes as
/// one the pass held, on either side of the checkpoints.
#[test]
fn a_window_from_a_checkpoint_is_masked_the_same() {
    let text = secret_source();
    assert!(text.len() > 3 * 1024 * 1024, "past the first checkpoints");
    let lines = text.lines().count();
    let file = temp(text.as_bytes());
    let handle = opened(file.path());
    let pass = stream_pass(
        &handle,
        file.path(),
        &[],
        &crate::security::ContentSecurity::new(),
        &NeverCancel,
    )
    .expect("pass");
    let windows = pass
        .proven
        .windows(
            &handle,
            &[Want::lines(2, 2), Want::lines(lines - 2, lines - 1)],
            pass.total_lines,
            &NeverCancel,
        )
        .expect("windows");
    let shown = |index: usize| {
        let text = String::from_utf8(windows[index].window.clone()).expect("utf8");
        with_placeholders(&text).into_owned()
    };
    assert_eq!(
        shown(0),
        format!("  {{\"data\":{{{JSON}}},\"kind\":\"Secret\"}},\n")
    );
    assert_eq!(
        shown(1),
        format!(
            "  {{\"data\":{{{JSON}}},\"kind\":\"Secret\"}},\n  {{\"data\":{{\"blob\":\"public\"}},\"kind\":\"ConfigMap\"}}\n"
        )
    );
}

/// A source that names nothing is held as it is, read once, with no mask.
#[test]
fn a_source_that_names_nothing_is_read_once_and_untouched() {
    let text = "  {\"id\": 1, \"note\": \"public\"},\n".repeat(1_000);
    let file = temp(text.as_bytes());
    let before = source_bytes_read();
    let windows = stream(&opened(file.path()), &[Want::lines(1, 1)], &NeverCancel).expect("stream");
    assert_eq!(source_bytes_read() - before, text.len());
    assert_eq!(windows[0].window, b"  {\"id\": 1, \"note\": \"public\"},\n");
}

/// A source that names one takes one more read, and no third.
#[test]
fn a_source_that_names_one_is_read_at_most_twice() {
    let text = secret_source();
    let file = temp(text.as_bytes());
    let handle = opened(file.path());
    let before = source_bytes_read();
    // A window held as the pass streams is shown: the walk judges the source.
    let pass = stream_pass(
        &handle,
        file.path(),
        &[Want::lines(1, 1)],
        &crate::security::ContentSecurity::new(),
        &NeverCancel,
    )
    .expect("pass");
    let read = source_bytes_read() - before;
    // The pass, the walk, and the bytes of what the walk hides: no third pass.
    assert!(
        read >= 2 * text.len() && read < 3 * text.len(),
        "{read} bytes read of {}",
        text.len()
    );
    let after = source_bytes_read();
    pass.proven
        .windows(
            &handle,
            &[Want::lines(2, 2)],
            pass.total_lines,
            &NeverCancel,
        )
        .expect("windows");
    assert!(
        source_bytes_read() - after < text.len(),
        "a window reads from a checkpoint, with no second walk"
    );
}

/// A byte page whose sealed offset the pass's record cannot place (an edit
/// starting inside a key line breaks the record's order) fails the seal with
/// the read's `UNPLACED` error, never a clamped offset (review B33 r4, F-F).
#[test]
fn an_unplaceable_byte_page_fails_its_seal() {
    use super::super::held::{SealMap, WindowLines};
    use super::super::sealed::Edit;
    let edits: std::sync::Arc<[Edit]> = vec![Edit {
        range: 20..30,
        text: "[R]".into(),
    }]
    .into();
    let map = SealMap {
        edits: edits.clone(),
        key_lines: vec![(10..50, 7)].into(),
    };
    let mut lines = WindowLines::new(
        &[Want::bytes(1, 1, 30)],
        crate::security::KeyBlockState::default(),
    )
    .sealed_by(map);
    lines.push(&[b'x'; 60], 1);
    lines.push(b"\n", 1);
    lines.finish(1);
    let error = lines
        .seal(&edits, &Judged::Exact(Vec::new()))
        .expect_err("the offset is not placed");
    assert!(error.contains("cannot be placed"), "{error}");
}

/// One line of Secret objects longer than a whole-file read, each value fake.
fn every_value_line() -> String {
    let mut line = String::from(r#"{"items":["#);
    let mut index = 0;
    while line.len() < past_whole() {
        line.push_str(&format!(
            r#"{{"data":{{"k":"v{index:06}"}},"kind":"Secret"}},"#
        ));
        index += 1;
    }
    line.push_str("{\"kind\":\"ConfigMap\"}]}\n");
    line
}

/// Every page of a ranges read of `file` past the walk budget, following
/// `next` to its end.
fn walk_ranges_every_value(
    file: &tempfile::NamedTempFile,
    ranges: serde_json::Value,
) -> Vec<LocalFetchResult> {
    super::super::set_walk_budget(Some(1));
    let mut next = query(serde_json::json!({"path":file.path(),"ranges":ranges}));
    let mut pages = Vec::new();
    loop {
        let page = fetch(file, &next, None);
        assert_eq!(page.status, ReadStatus::Success, "{page:?}");
        assert!(pages.len() < 64, "the walk ends");
        let call = page.next.clone().and_then(|next| next.continue_window);
        pages.push(page);
        let Some(call) = call else {
            break;
        };
        next = call.query;
    }
    super::super::set_walk_budget(None);
    pages
}

/// The disclosure a withheld line's page owes: terminal, with its reason and the
/// warning naming the unread scope, and no note promising pages of it.
fn assert_withheld(pages: &[LocalFetchResult], line: usize) {
    let shown: String = pages
        .iter()
        .filter_map(|page| page.content.clone())
        .collect();
    assert!(!shown.contains("\"v0"), "no value shows");
    let page = pages
        .iter()
        .find(|page| {
            page.warnings
                .iter()
                .any(|warning| warning.starts_with(&format!("Line {line} is not read")))
        })
        .expect("a page says the line's rest is not read");
    assert_eq!(page.terminal_limit, Some(true), "{:?}", page.warnings);
    assert!(
        page.partial_reasons
            .contains(&crate::tools::source_view::PartialReason::SecuritySelectedView),
        "{:?}",
        page.partial_reasons
    );
    for page in pages {
        assert!(
            !page.warnings.iter().any(|warning| warning
                .contains("later pages read that range alone")
                || warning.contains("first the rest of those lines")),
            "no page promises the withheld rest: {:?}",
            page.warnings
        );
    }
}

/// The withhold warning says the lines after the withheld one read on only when
/// the page's next reads a later line: never for the last line requested
/// (review B33 r5).
#[test]
fn a_withheld_last_line_promises_no_later_lines() {
    let text = format!("{}{{\"b\":2}}\n", every_value_line());
    let file = temp(text.as_bytes());
    super::super::set_walk_budget(Some(1));
    let withheld = |page: &LocalFetchResult| {
        page.warnings
            .iter()
            .find(|warning| warning.starts_with("Line 1 is not read"))
            .cloned()
            .expect("the line's rest is withheld")
    };
    let last = fetch(
        &file,
        &query(serde_json::json!({"path":file.path(),"ranges":[{"startLine":1,"endLine":1}]})),
        None,
    );
    let whole = fetch(
        &file,
        &query(serde_json::json!({"path":file.path(),"ranges":[{"startLine":1,"endLine":2}]})),
        None,
    );
    super::super::set_walk_budget(None);
    assert!(
        last.next
            .as_ref()
            .and_then(|next| next.continue_window.as_ref())
            .is_none()
    );
    assert!(!withheld(&last).contains("read on"), "{:?}", last.warnings);
    let read_on = whole
        .next
        .as_ref()
        .and_then(|next| next.continue_window.as_ref());
    assert!(
        read_on.is_some(),
        "line 2 is read next: {:?}",
        whole.warnings
    );
    assert!(withheld(&whole).contains("read on"), "{:?}", whole.warnings);
}

/// A withheld line in a later range keeps its terminal disclosure through the
/// join of the ranges' parts (review B33 r4, F-A).
#[test]
fn a_withheld_line_in_a_later_range_keeps_its_disclosure() {
    let text = format!(
        "{{\"a\":1}}\n{{\"b\":2}}\n{}{{\"c\":3}}\n",
        every_value_line()
    );
    let file = temp(text.as_bytes());
    let pages = walk_ranges_every_value(
        &file,
        serde_json::json!([{"startLine":1,"endLine":1},{"startLine":3,"endLine":3}]),
    );
    assert_withheld(&pages, 3);
    let shown: String = pages
        .iter()
        .filter_map(|page| page.content.clone())
        .collect();
    assert_eq!(shown.matches("{\"a\":1}").count(), 1, "{shown:.200}");
    assert!(!shown.contains("{\"b\":2}") && !shown.contains("{\"c\":3}"));
}

/// A withheld line in the first range: next reads the later range, once, and no
/// note says the withheld rest comes first (F-A).
#[test]
fn a_withheld_first_range_reads_on_to_the_later_range() {
    let text = format!("{}{{\"b\":2}}\n{{\"c\":3}}\n", every_value_line());
    let file = temp(text.as_bytes());
    let pages = walk_ranges_every_value(
        &file,
        serde_json::json!([{"startLine":1,"endLine":1},{"startLine":3,"endLine":3}]),
    );
    assert_withheld(&pages, 1);
    let shown: String = pages
        .iter()
        .filter_map(|page| page.content.clone())
        .collect();
    assert_eq!(shown.matches("{\"c\":3}").count(), 1, "{shown:.200}");
    assert!(!shown.contains("{\"b\":2}"));
}

/// Past the walk budget each window hides every value on its own, so no two
/// windows share one sealed view and a byte page past the first of a long line
/// would gap or repeat (review B33 finding 1). The walk shows a prefix of the
/// line's every-value view, says the rest of that line is not read, and reaches
/// every later line once.
#[test]
fn an_every_value_long_line_is_never_paged_from_another_view() {
    let mut line = String::from(r#"{"items":["#);
    let mut index = 0;
    while line.len() < past_whole() {
        line.push_str(&format!(
            r#"{{"data":{{"k":"v{index:06}"}},"kind":"Secret"}},"#
        ));
        index += 1;
    }
    line.push_str("{\"kind\":\"ConfigMap\"}]}\n");
    let tail = "{\"note\":\"after\"}\n";
    let file = temp(format!("{line}{tail}").as_bytes());
    // The scanner's edits first, then every value of each line, as a window
    // from a line's start seals it.
    let scanned = |text: &str| {
        crate::tools::source_view::prepare::sealed_view(
            text.as_bytes(),
            file.path(),
            &crate::security::ContentSecurity::new(),
        )
        .expect("sealed")
        .text
        .into_owned()
    };
    let expected_line = every_value(&scanned(&line), false, true, false);
    let expected_tail = every_value(&scanned(tail), false, true, false);
    super::super::set_walk_budget(Some(1));
    let mut next =
        query(serde_json::json!({"path":file.path(),"ranges":[{"startLine":1,"endLine":2}]}));
    let mut pages = Vec::new();
    let mut terminal = None;
    loop {
        let page = fetch(&file, &next, None);
        assert_eq!(page.status, ReadStatus::Success, "{page:?}");
        let content = page.content.clone().expect("a page");
        assert!(!content.contains("\"v0"), "no value shows");
        if page.terminal_limit == Some(true) {
            terminal = Some(page.warnings.clone());
            // The line's tail is not read: its byte pages end here (F-C).
            let bytes = page.pagination.as_ref().expect("a byte page");
            assert!(!bytes.has_more && bytes.next_offset.is_none(), "{bytes:?}");
        }
        pages.push(content);
        assert!(pages.len() < 64, "the walk ends");
        let Some(call) = page.next.and_then(|next| next.continue_window) else {
            break;
        };
        next = call.query;
    }
    super::super::set_walk_budget(None);
    let shown = pages.concat();
    let warnings = terminal.expect("the page that stops inside line 1 says so");
    assert!(
        warnings
            .iter()
            .any(|warning| warning.starts_with("Line 1 is not read")),
        "{warnings:?}"
    );
    let line_part = shown
        .strip_suffix(&expected_tail)
        .expect("line 2 is read once, last");
    assert!(!line_part.is_empty());
    let differs = line_part
        .bytes()
        .zip(expected_line.bytes())
        .position(|(shown, view)| shown != view)
        .unwrap_or(line_part.len().min(expected_line.len()));
    assert!(
        expected_line.starts_with(line_part),
        "line 1 shows a prefix of its one view, with no page from another: {} of {} bytes agree; shown {:?}, view {:?}",
        differs,
        line_part.len(),
        line_part.get(differs.saturating_sub(40)..(differs + 60).min(line_part.len())),
        expected_line.get(differs.saturating_sub(40)..(differs + 60).min(expected_line.len())),
    );
}
