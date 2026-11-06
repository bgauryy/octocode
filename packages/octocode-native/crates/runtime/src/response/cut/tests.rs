use super::*;

const MARKER: &str = "[REDACTED-GITHUBTOKENS]";

fn samples() -> Vec<String> {
    let long_line = "x".repeat(5_000);
    vec![
        String::new(),
        "a\nb\nc\n".into(),
        format!("head\n{long_line}\ntail"),
        "crlf line one\r\ncrlf line two\r\n".repeat(40),
        "日本語のテキスト🦀😀 \"quoted\" \\ tab\t\u{1} end\n".repeat(30),
        format!("token = {MARKER}; other = {MARKER}\n").repeat(50),
        format!("{}{MARKER}{}", "y".repeat(97), "z".repeat(300)),
        "[REDACTED-literal never closed on its line\nnext".repeat(20),
        "a[b]c[REDACTED: source unreadable for secret check]d".repeat(25),
    ]
}

fn markers(text: &str) -> Vec<Range<usize>> {
    let mut found = Vec::new();
    for prefix in [REDACTION_PREFIX, CONTENT_REDACTION_PREFIX] {
        for (at, _) in text.match_indices(prefix) {
            if let Some(close) = text[at..].find(']')
                && !text[at..at + close].contains('\n')
            {
                found.push(at..at + close + 1);
            }
        }
    }
    found
}

/// The windows of `text` a walk of [`window_end`] at `room` cuts, in order.
fn cuts(text: &str, room: usize) -> Vec<Range<usize>> {
    let mut windows = Vec::new();
    let mut start = 0;
    while start < text.len() {
        let end = window_end(text, start, room);
        windows.push(start..end);
        start = end;
    }
    windows
}

#[test]
fn windows_join_to_the_text_fit_their_room_and_never_split_a_marker() {
    for text in samples() {
        for room in [1, 2, 7, 23, 64, 333, 4_096, 100_000] {
            let windows = cuts(&text, room);
            let joined: String = windows.iter().map(|w| &text[w.clone()]).collect();
            assert_eq!(joined, text, "room {room}");
            for window in &windows {
                assert!(!window.is_empty(), "room {room}: a walk always advances");
                let shown = &text[window.clone()];
                let whole_marker = markers(&text).iter().any(|m| m == window);
                assert!(
                    escaped_len(shown) <= room
                        || shown.chars().count() == 1
                        || shown == "\r\n"
                        || whole_marker,
                    "room {room}: window {window:?} is {} units",
                    escaped_len(shown)
                );
                for marker in markers(&text) {
                    let inside = |at: usize| marker.start < at && at < marker.end;
                    assert!(
                        !inside(window.start) && !inside(window.end),
                        "room {room}: cut {window:?} splits marker {marker:?}"
                    );
                }
                assert!(
                    !(shown.ends_with('\r') && text[window.end..].starts_with('\n')),
                    "room {room}: a CRLF pair is never split"
                );
            }
        }
    }
}

#[test]
fn a_window_ends_at_a_line_end_unless_its_line_outgrows_the_room() {
    let text = "short\nanother short\n".repeat(10);
    for window in cuts(&text, 40) {
        assert!(text[window].ends_with('\n'));
    }
    let long = format!("{}\nrest\n", "q".repeat(100));
    let windows = cuts(&long, 30);
    assert_eq!(&long[windows[0].clone()], "q".repeat(30));
    assert_eq!(
        windows[0].len() + windows[1].len(),
        60,
        "mid-line windows fill the room"
    );
}

#[test]
fn each_window_fits_the_room_it_is_given() {
    let text = "z".repeat(1_000);
    assert_eq!(window_end(&text, 0, 10), 10);
    assert_eq!(window_end(&text, 10, 100), 110);
}

/// The one cut rule for any unit: a cut moves back to a character's first
/// byte, before a `\r\n` pair, and to the start of a redaction marker it
/// would fall inside; every other byte is already a clean cut.
#[test]
fn a_clean_cut_never_splits_a_character_a_crlf_or_a_marker() {
    for text in samples() {
        let markers = markers(&text);
        for at in 0..=text.len() {
            let cut = clean_cut(&text, at);
            assert!(cut <= at, "{at}: a cut moves back, never forward");
            assert!(text.is_char_boundary(cut), "{at}");
            assert!(
                !(text[..cut].ends_with('\r') && text[cut..].starts_with('\n')),
                "{at}: inside a CRLF"
            );
            assert!(
                markers
                    .iter()
                    .all(|marker| cut <= marker.start || cut >= marker.end),
                "{at}: inside a marker"
            );
            let clean = text.is_char_boundary(at)
                && !(text[..at].ends_with('\r') && text[at..].starts_with('\n'))
                && markers
                    .iter()
                    .all(|marker| at <= marker.start || at >= marker.end);
            assert_eq!(is_clean_cut(&text, at), clean, "{at}");
            assert_eq!(cut == at, clean, "{at}");
        }
        assert!(!is_clean_cut(&text, text.len() + 1), "past the end");
    }
}

/// A window at any byte starts at that byte or just before it (never
/// inside a character, CRLF pair or marker), both its edges are clean cuts,
/// and walking on from its end with [`window_end`] joins to the rest of the
/// text exactly once.
#[test]
fn a_window_at_a_byte_starts_at_its_clean_cut_and_walks_on_to_the_end() {
    for text in samples() {
        for at in (0..=text.len()).step_by(7) {
            for room in [1, 13, 200] {
                let window = window_at(&text, at, room);
                assert_eq!(window.start, clean_cut(&text, at), "{at} {room}");
                assert!(window.start <= at, "{at} {room}");
                assert!(is_clean_cut(&text, window.start), "{at} {room}");
                assert!(is_clean_cut(&text, window.end), "{at} {room}");
                assert!(window.end > window.start || window.start == text.len());
                let mut joined = text[window.clone()].to_owned();
                let mut start = window.end;
                while start < text.len() {
                    let end = window_end(&text, start, room);
                    joined.push_str(&text[start..end]);
                    start = end;
                }
                assert_eq!(joined, text[window.start..], "{at} {room}");
            }
        }
    }
}

#[test]
fn escaped_units_match_serialized_json() {
    let text = "a\"\\\n\r\t\u{8}\u{c}\u{1}\u{1f}é日🦀\u{7f}";
    let json = serde_json::to_string(text).expect("json");
    assert_eq!(escaped_len(text) + 2, json.encode_utf16().count());
}
