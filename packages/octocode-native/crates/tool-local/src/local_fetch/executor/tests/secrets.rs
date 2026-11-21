//! Secrets stay redacted in bounded windows and match probes; innocent
//! base64 passes through.

use super::*;
use octocode_test_support::json_fixture;

// A bounded read of an interior body line of a private key must not
// leak the key, even when the file is NOT key-named and the selected window
// contains no BEGIN/END marker (so the anchored full-block patterns cannot
// fire). The `Passthrough` scan, proving the full-file block guard
// — not the window sanitizer — closes the leak.
#[test]
fn interior_private_key_window_does_not_leak_in_non_key_named_file() {
    let temp = Fixture::new();
    let dir = temp.0.clone();
    let path = dir.join("notes.txt"); // deliberately NOT a *.pem/id_rsa path
    let body = "MIIEpQIBAAKCAQEAsplitKeyBodyLineOneAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    let body2 = "c3BsaXRLZXlCb2R5TGluZVR3b0JBQkFCQUJBQkFCQUJBQkFCQUJBQkFCQUJBQkFC";
    let file = format!(
        "fn main() {{}}\nlet config = load();\n-----BEGIN RSA PRIVATE KEY-----\n{body}\n{body2}\n-----END RSA PRIVATE KEY-----\nlet done = true;\n"
    );
    fs::write(&path, &file).expect("write file");

    // Select only the interior body lines (4..=5) — no BEGIN/END in view.
    let req = qj(
        &path,
        json_fixture(r#"{"ranges": [{"startLine": 4, "endLine": 5}]}"#),
    );
    let result = fetch(&req, &Paths(dir.clone()));

    let content = result.content.clone().unwrap_or_default();
    assert!(
        !content.contains(body) && !content.contains(body2),
        "private key body leaked from an interior window: {content}"
    );
}

// matchString must run on redacted text: a line-level secret that the
// window sanitizer redacts must not be confirmable by probing matchString
// (an exact-prefix oracle). Matching the true secret and a wrong guess must
// be indistinguishable, and line numbers must stay source-accurate.
#[test]
fn match_string_cannot_probe_line_level_secrets() {
    use octocode_runtime_base::security::ContentSecurity;
    let temp = Fixture::new();
    let dir = temp.0.clone();
    let path = dir.join("config.txt");
    let secret = "AKIAIOSFODNN7EXAMPLE";
    fs::write(
        &path,
        format!("header\naws_access_key_id = {secret}\nneedle after\n"),
    )
    .expect("write file");
    let security = ContentSecurity::new();
    let probe = |needle: &str| {
        let req = qj(&path, serde_json::json!({"matchString":[needle]}));
        execute_local_fetch(
            &req,
            &Paths(dir.clone()),
            &security,
            &NeverCancel,
            None,
            false,
        )
    };
    let right = probe("AKIAIOSFODNN7EXAMPL");
    let wrong = probe("AKIAIOSFODNN7EXAMPQ");
    assert_eq!(right.selected_match_count, wrong.selected_match_count);
    assert_eq!(right.error_code, wrong.error_code);
    assert_eq!(right.selected_match_count, Some(0), "{right:?}");
    // Same hints, and the same leads but for the guess they repeat,
    // either way (no oracle); the hints explain the miss.
    assert_eq!(right.hints, wrong.hints);
    assert_eq!(
        serde_json::to_string(&right.next)
            .expect("json")
            .replace("AKIAIOSFODNN7EXAMPL", "AKIAIOSFODNN7EXAMPQ"),
        serde_json::to_string(&wrong.next).expect("json")
    );
    assert_eq!(right.hints.len(), 1, "{right:?}");
    let clean = dir.join("clean.txt");
    fs::write(&clean, "nothing secret\n").expect("write clean");
    let miss = execute_local_fetch(
        &qj(&clean, json_fixture(r#"{"matchString": ["absent"]}"#)),
        &Paths(dir.clone()),
        &security,
        &NeverCancel,
        None,
        false,
    );
    // A file without a secret gets the same recovery; only the file that
    // holds a masked value says it was not searched (`maskedSpans` states
    // its count either way).
    assert_eq!(
        serde_json::to_value(&miss.next).expect("json")["findText"]["tool"],
        serde_json::to_value(&right.next).expect("json")["findText"]["tool"],
        "{miss:?}"
    );
    assert!(!miss.hints[0].contains("masked"), "{miss:?}");
    assert!(right.hints[0].contains("maskedSpans"), "{right:?}");
    let after = probe("needle");
    assert_eq!(after.match_ranges, vec![LineRange { start: 3, end: 3 }]);
    assert_eq!(after.total_lines, Some(3));
}

// Over-redaction guard: an ordinary long base64 line (config blob,
// hash, minified asset) with NO private-key markers anywhere in the file
// must pass through byte-identical — the guard keys off BEGIN/END markers,
// never bare base64.
#[test]
fn innocent_base64_window_is_returned_byte_identical() {
    let temp = Fixture::new();
    let dir = temp.0.clone();
    let path = dir.join("data.txt");
    let blob = "aGVsbG8gd29ybGQgdGhpcyBpcyBqdXN0IGEgbG9uZyBiYXNlNjQgYmxvYg==";
    let file = format!("header\n{blob}\nfooter\n");
    fs::write(&path, &file).expect("write file");

    let req = qj(
        &path,
        json_fixture(r#"{"ranges": [{"startLine": 2, "endLine": 2}]}"#),
    );
    let result = fetch(&req, &Paths(dir.clone()));

    let content = result.content.clone().unwrap_or_default();
    assert!(
        content.contains(blob),
        "innocent base64 was wrongly redacted: {content}"
    );
}

/// A miss in a file that holds a masked secret value claims absence only for
/// the text it searched: its one hint says the masked values are not
/// searched, whether the term sits inside one or nowhere (the same sentence,
/// so a miss tells nothing of the secret), and keeps its one call.
#[test]
fn a_miss_over_masked_values_claims_no_absence_inside_them() {
    let tail = "H7Kq2Lm9Zp4Vw8Rt1Yb6Nc3Hd5Jf0GsAeUoI";
    let t = Fixture::new();
    let security = octocode_runtime_base::security::ContentSecurity::new();
    let read = |path: &std::path::Path, fields: serde_json::Value| {
        execute_local_fetch(
            &qj(path, fields),
            &Paths(t.0.clone()),
            &security,
            &NeverCancel,
            None,
            false,
        )
    };
    let hints = ["a.py", "a.txt"].map(|name| {
        let p = t.0.join(name);
        fs::write(&p, format!("other = 5\ntoken = \"ghp_{tail}\"\n")).expect("fixture");
        let mut hints = Vec::new();
        for pattern in [&tail[..12], "zz-not-anywhere"] {
            let result = read(
                &p,
                serde_json::json!({"matchString": [pattern], "caseMode": "sensitive"}),
            );
            assert!(
                result.masked_spans.is_some_and(|spans| spans > 0),
                "{result:?}"
            );
            assert_eq!(result.hints.len(), 1, "{result:?}");
            let hint = result.hints[0].clone();
            assert!(hint.contains("maskedSpans"), "{name} {pattern}: {hint}");
            assert!(
                hint.contains("never searched") && !hint.contains("of this file holds it"),
                "{name} {pattern}: {hint}"
            );
            let next = serde_json::to_value(result.next.as_ref().expect("next")).expect("json");
            assert_eq!(
                octocode_runtime_base::test_support::billing::offered_calls(&next).len(),
                1,
                "{next}"
            );
            hints.push(hint);
        }
        assert_eq!(hints[0], hints[1], "a right and a wrong guess read alike");
        hints.remove(0)
    });
    // One masked value reads as one: "is", not "are".
    for hint in &hints {
        assert!(
            hint.contains("1 masked secret value (maskedSpans) is never"),
            "{hint}"
        );
    }
    // A file without masked values keeps the plain sentence.
    let p = t.0.join("b.txt");
    fs::write(&p, "other = 5\n").expect("fixture");
    let plain = read(&p, json_fixture(r#"{"matchString": ["zz-not-anywhere"]}"#));
    assert!(!plain.hints[0].contains("masked"), "{plain:?}");
}
