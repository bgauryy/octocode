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
    lines.mask(&judged);
    let window = String::from_utf8(lines.held[0].window.clone()).expect("utf8");
    assert!(!window.contains(VALUE), "{window}");
    assert!(lines.held[0].masked);
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
    assert!(windows[0].masked);
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
    let pass = stream_pass(&handle, file.path(), &[], &NeverCancel)
        .expect("pass")
        .pass;
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
    assert!(!windows[0].masked);
    assert_eq!(windows[0].window, b"  {\"id\": 1, \"note\": \"public\"},\n");
}

/// A source that names one takes one more read, and no third.
#[test]
fn a_source_that_names_one_is_read_at_most_twice() {
    let text = secret_source();
    let file = temp(text.as_bytes());
    let handle = opened(file.path());
    let before = source_bytes_read();
    let pass = stream_pass(&handle, file.path(), &[], &NeverCancel)
        .expect("pass")
        .pass;
    assert_eq!(source_bytes_read() - before, 2 * text.len());
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
