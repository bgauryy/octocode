//! Masks on a held window: exact spans, and the fallback when a walk cannot keep a
//! source's structure. Every value is fake.
use super::super::masks::{Judged, judge};
use super::super::stream::stream_pass;
use super::*;
use crate::tools::source_view::PartialReason;
use octocode_engine::security::{Named, Objects, every_value_spans};

const VALUE: &str = "RmFrZTBWYWx1ZTlOb3Q0UmVhbDhCbG9iN0xvbmc2RW5vdWdo";
const JSON: &str = "[REDACTED-KUBERNETESSECRETJSON]";
const YAML: &str = "[REDACTED-KUBERNETESSECRETS]";

/// `text` as its whole-document every-value scan hides it: one placeholder per
/// line of each span, from the line's first byte that is not a blank.
pub(super) fn every_value(text: &str, objects: bool, yaml: bool) -> String {
    let objects = if objects {
        Objects::ALL
    } else {
        Objects::default()
    };
    let mut out = String::new();
    let mut at = 0;
    for span in every_value_spans(text.as_bytes(), Named { objects, yaml }) {
        out.push_str(&text[at..span.range.start]);
        let placeholder = octocode_engine::security::redaction_placeholder(span.form.pattern());
        for line in text[span.range.clone()].split_inclusive('\n') {
            let body = line.trim_end_matches('\n');
            let lead = body.len() - body.trim_start_matches([' ', '\t', '\r']).len();
            out.push_str(&line[..lead]);
            if lead < body.len() {
                out.push_str(&placeholder);
            }
            out.push_str(&line[body.len()..]);
        }
        at = span.range.end;
    }
    out.push_str(&text[at..]);
    out
}

/// Past the budget, no value can be told from another: names and structure stay.
#[test]
fn the_fallback_hides_every_json_string_but_names() {
    assert_eq!(
        every_value(
            "{\"a\": \"plain words\", \"b\" : [\"c\", 1], \"n\": 7,\n  \"esc\": \"x\\\"y\"\n}\n",
            true,
            false
        ),
        format!(
            "{{\"a\": \"{JSON}\", \"b\" : [\"{JSON}\", 1], \"n\": 7,\n  \"esc\": \"{JSON}\"\n}}\n"
        )
    );
    // A value held escaped goes with the string that holds it.
    assert_eq!(
        every_value("\"a\": \"{\\\"data\\\":\\\"x\\\"}\"\n", true, false),
        format!("\"a\": \"{JSON}\"\n")
    );
    // A string left open by the window's end, or by its line's.
    assert_eq!(
        every_value("\"a\": \"open\n\"b\": \"cut", true, false),
        format!("\"a\": \"{JSON}\n\"b\": \"{JSON}")
    );
}

#[test]
fn the_fallback_hides_every_yaml_value_but_names() {
    assert_eq!(
        every_value(
            "---\nstringData:\n  blob: plain words\n  block: |\n    more plain words\n- item: x\n\n",
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
        objects: Objects::ALL,
        yaml: false,
    };
    let digest = hex::encode(Sha256::digest(text.as_bytes()));
    let judged = judge(&opened(file.path()), named, &digest, 1, &NeverCancel).expect("judged");
    assert!(matches!(judged, Judged::EveryValue));
    super::super::set_walk_budget(Some(1));
    let windows = stream(&opened(file.path()), &[Want::lines(2, 3)], &NeverCancel);
    super::super::set_walk_budget(None);
    let windows = windows.expect("stream");
    let window = String::from_utf8(windows[0].window.clone()).expect("utf8");
    assert!(!window.contains(VALUE), "{window}");
    assert!(windows[0].every_value, "the page says why");
    assert!(
        window.contains(&format!("{{\"data\":{{{JSON}}},\"kind\":\"{JSON}\"}},")),
        "{window}"
    );
}

/// Bytes that are no longer the pass's fail the read, whose windows came from it.
#[test]
fn a_walk_of_other_bytes_fails_the_read() {
    let file = temp(secret_source().as_bytes());
    let named = Named {
        objects: Objects::ALL,
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
    let shown = window;
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
    let shown = |index: usize| String::from_utf8(windows[index].window.clone()).expect("utf8");
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
    let source = tempfile::tempfile().expect("temp file");
    let edits = super::super::edit_map::EditMap::Listed(edits);
    let map = SealMap {
        edits: edits.clone(),
        key_lines: vec![(10..50, 7)].into(),
        file: std::sync::Arc::new(source.try_clone().expect("handle")),
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
        .seal(&edits, &source, &Judged::Exact(Vec::new()))
        .expect_err("the offset is not placed")
        .to_string();
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
        assert!(pages.len() < 4096, "the walk ends");
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

/// A page of the every-value fallback that hides a value names the memory
/// budget as a terminal limit: the reason, whose meaning the contract states,
/// is the page's own fact.
fn assert_every_value_disclosed(page: &LocalFetchResult) {
    let hidden = page
        .content
        .as_deref()
        .is_some_and(|content| content.contains("[REDACTED"));
    if hidden {
        let reason = PartialReason::SecurityWalkMemoryBudget;
        assert!(page.partial_reasons.contains(&reason), "{page:?}");
        assert_eq!(page.terminal_limit, Some(true), "{page:?}");
        assert!(
            !page
                .partial_reasons
                .contains(&PartialReason::SecuritySelectedView),
            "the scan bound is not the cause: {:?}",
            page.partial_reasons
        );
    }
}

/// Every page of `ranges` of a source past the walk budget, following `next` to
/// its end, with every page's content.
fn walked(file: &tempfile::NamedTempFile, ranges: serde_json::Value) -> Vec<LocalFetchResult> {
    let pages = walk_ranges_every_value(file, ranges);
    for page in &pages {
        assert_every_value_disclosed(page);
        assert!(
            !page
                .warnings
                .iter()
                .any(|warning| warning.contains("is not read past")),
            "no page withholds a line's tail: {:?}",
            page.warnings
        );
        assert!(
            !page.content.clone().unwrap_or_default().contains("\"v0"),
            "no value shows"
        );
    }
    pages
}

/// A line the fallback hides every value of is paged by bytes through to its end,
/// and the pages rejoin the one view the whole-file scan gives it: windows that
/// each hid values on their own would end the line's pages, gap or repeat
/// (B48). Its values sit across every window's edge.
#[test]
fn an_every_value_long_line_pages_by_bytes_to_its_end() {
    let line = every_value_line();
    let tail = "{\"note\":\"after\"}\n";
    let file = temp(format!("{line}{tail}").as_bytes());
    // The scanner's edits first, then every value, as the whole-file read shows it.
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
    let expected = format!(
        "{}{}",
        every_value(&scanned(&line), true, false),
        every_value(&scanned(tail), true, false)
    );
    let pages = walked(&file, serde_json::json!([{"startLine":1,"endLine":2}]));
    assert!(
        pages.len() > 2,
        "the line takes several byte pages: {}",
        pages.len()
    );
    let shown: String = pages
        .iter()
        .filter_map(|page| page.content.clone())
        .collect();
    let differs = shown
        .bytes()
        .zip(expected.bytes())
        .position(|(shown, view)| shown != view)
        .unwrap_or(shown.len().min(expected.len()));
    assert!(
        shown == expected,
        "the pages rejoin the whole-file view: {differs} of {} bytes agree",
        expected.len()
    );
}

/// Ranges around a line the fallback hides read in order, each once.
#[test]
fn ranges_either_side_of_an_every_value_line_read_once() {
    let text = format!(
        "{{\"a\":1}}\n{{\"b\":2}}\n{}{{\"c\":3}}\n",
        every_value_line()
    );
    let file = temp(text.as_bytes());
    let pages = walked(
        &file,
        serde_json::json!([{"startLine":1,"endLine":1},{"startLine":3,"endLine":4}]),
    );
    let shown: String = pages
        .iter()
        .filter_map(|page| page.content.clone())
        .collect();
    assert_eq!(shown.matches("{\"a\":1}").count(), 1, "{shown:.200}");
    assert_eq!(shown.matches("{\"c\":3}").count(), 1, "{shown:.200}");
    assert!(!shown.contains("{\"b\":2}"));
}
