use super::*;

/// A fake token: no real credential.
fn token() -> String {
    format!("ghp_{}", "abcdefghijklmnopqrstuvwxyz0123456789")
}

/// A kept pass shows exactly what its scan showed, and hides what it
/// hid: the same text at the same path is scanned once, and another
/// text or another path is another judgment.
#[test]
fn a_kept_pass_judges_as_its_scan_did() {
    let security = ContentSecurity::new();
    let root = tempfile::tempdir().expect("fixture");
    let path = root.path().join("src/config.ts");
    let secret = token();
    let source = format!("const a = 1;\nconst key = \"{secret}\";\nconst b = 2;\n");
    let scanned = octocode_runtime_base::security::scanned_on_this_thread;
    let before = scanned();
    let first = judged_text(&source, &path, &security);
    assert_eq!(scanned() - before, source.len(), "scanned once");
    let again = judged_text(&source, &path, &security);
    assert_eq!(
        scanned() - before,
        source.len(),
        "the kept pass scans nothing"
    );
    assert!(first.is_redacted() && again.is_redacted());
    assert_eq!(first.redaction(), again.redaction());
    for judged in [&first, &again] {
        let read = judged.lines(1, 3).expect("the read");
        assert!(!read.contains(&secret), "{read}");
        assert!(
            read.contains("const a = 1;") && read.contains("const b = 2;"),
            "{read}"
        );
        let line = judged.line_span(2, 1).expect("line 2");
        assert!(!judged.cut(line).contains(&secret));
    }
    assert_eq!(first.lines(1, 3), again.lines(1, 3));

    // A clean text keeps nothing and still scans once.
    let clean = "const a = 1;\n";
    let before = scanned();
    for _ in 0..3 {
        assert!(!judged_text(clean, &path, &security).is_redacted());
    }
    assert_eq!(scanned() - before, clean.len());

    // The path is part of what a pass reads; so is the text.
    let before = scanned();
    let other = judged_text(&source, &root.path().join("src/other.ts"), &security);
    assert!(other.is_redacted());
    let longer = format!("{source}const c = 3;\n");
    let changed = judged_text(&longer, &path, &security);
    assert!(!changed.lines(1, 4).expect("the read").contains(&secret));
    assert_eq!(scanned() - before, source.len() + longer.len());
}
