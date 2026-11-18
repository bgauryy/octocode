//! Context windows around hit lines: numbering, merging and redaction.
use super::*;

fn windowed(text: &str, window: Window, security: &impl ContentScan) -> (Vec<String>, usize) {
    match scan(
        text.as_bytes(),
        "a.txt",
        &["needle".into()],
        "",
        window,
        security,
    ) {
        FileHits::Lines { lines, total, .. } => (lines, total),
        other => panic!("hit: {other:?}"),
    }
}

/// Context lines surround each hit, numbered like hits; `total` still counts hits.
#[test]
fn context_lines_list_the_neighbours_of_each_hit() {
    let text = "a\nb\nneedle\nc\nd\ne\nneedle\n";
    let (lines, total) = windowed(
        text,
        Window {
            before: 1,
            after: 1,
        },
        &Passthrough,
    );
    assert_eq!(
        lines,
        ["2\tb", "3\tneedle", "4\tc", "6\te", "7\tneedle"],
        "the window clamps at the file end"
    );
    assert_eq!(total, 2);
}

/// Overlapping and adjacent windows merge: every source line lists at most once, in order.
#[test]
fn overlapping_context_windows_merge() {
    let text = "a\nneedle\nneedle\nb\nc\nneedle\nd\n";
    let (lines, total) = windowed(
        text,
        Window {
            before: 2,
            after: 2,
        },
        &Passthrough,
    );
    let numbers: Vec<u32> = lines
        .iter()
        .map(|line| {
            line.split('\t')
                .next()
                .and_then(|n| n.parse().ok())
                .expect("number")
        })
        .collect();
    assert_eq!(numbers, [1, 2, 3, 4, 5, 6, 7], "{lines:?}");
    assert_eq!(total, 3);
    let (lines, _) = windowed(
        text,
        Window {
            before: 1,
            after: 0,
        },
        &Passthrough,
    );
    assert_eq!(
        lines,
        ["1\ta", "2\tneedle", "3\tneedle", "5\tc", "6\tneedle"]
    );
}

/// Context lines pass the same redaction as hit lines.
#[test]
fn context_lines_are_redacted_like_hits() {
    let secret = format!("ghp_{}", "Zq7".repeat(12));
    let text = format!("const t = \"{secret}\";\nneedle\nconst u = \"{secret}\";\n");
    let security = octocode_runtime_base::security::ContentSecurity::new();
    let (lines, _) = windowed(
        &text,
        Window {
            before: 1,
            after: 1,
        },
        &security,
    );
    assert_eq!(lines.len(), 3, "{lines:?}");
    assert!(!lines.join("\n").contains(&secret[8..28]), "{lines:?}");
}

/// The hit lines are named exactly once each, ascending, however the
/// context windows merge; without context `lines` is the hits alone.
#[test]
fn matched_lines_name_each_hit_once_whatever_the_windows() {
    let text = "a\nneedle\nneedle\nb\nc\nneedle\nd\n";
    for (window, listed) in [
        (Window::default(), 3),
        (
            Window {
                before: 1,
                after: 1,
            },
            7,
        ),
        (
            Window {
                before: 9,
                after: 9,
            },
            7,
        ),
    ] {
        let FileHits::Lines {
            lines,
            matched,
            total,
            ..
        } = scan(
            text.as_bytes(),
            "a.txt",
            &["needle".into()],
            "",
            window,
            &Passthrough,
        )
        else {
            panic!("hit");
        };
        assert_eq!(matched, [2, 3, 6], "{window:?}");
        assert_eq!(total, matched.len());
        assert_eq!(lines.len(), listed, "{window:?}: {lines:?}");
    }
}
