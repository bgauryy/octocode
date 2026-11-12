//! A matchOnly span over a value that is secret only by its document's context
//! (a Kubernetes Secret's data value) and a token its line alone shows: the span
//! is judged by the whole source's scan, so the value goes with the token, as a
//! read of the file hides both. Every value below is fake.
use super::*;

const TOKEN: &str = "ghp_Z9Q8W7E6R5T4Y3U2I1O0P9L8K7J6H5G4F3D2";
const VALUE: &str = "QmFzZTY0VmFsdWVIZXJlWFla";

fn documents() -> [(&'static str, String); 2] {
    [
        (
            "k8s.yaml",
            format!(
                "apiVersion: v1\nkind: Secret\nmetadata:\n  name: demo\ntype: Opaque\ndata:\n  blob: {VALUE} # {TOKEN}\n"
            ),
        ),
        (
            "k8s.json",
            format!(
                "{{\n  \"apiVersion\": \"v1\",\n  \"kind\": \"Secret\",\n  \"data\": {{\n    \"blob\": \"{VALUE}\", \"note\": \"{TOKEN}\"\n  }}\n}}\n"
            ),
        ),
    ]
}

/// Spans that end inside the token show no byte of the context-only value.
#[test]
fn a_span_ending_in_a_token_hides_a_value_secret_by_its_document() {
    for (name, text) in documents() {
        let root = tempfile::tempdir().expect("fixture directory");
        fs::write(root.path().join(name), &text).expect("fixture");
        // A pattern that reaches into a hidden value matches nothing, as a
        // read's matchString does; one on the text a read shows is a row.
        for (pattern, shows) in [
            ("blob.*ghp_Z9Q8", false),
            ("blob.*ghp_", false),
            ("Qm.*ghp_Z", false),
            ("Qm[A-Za-z0-9]+", false),
            ("kind.*", true),
        ] {
            let (_, pages) = walk_search(
                root.path(),
                serde_json::json!({
                    "path": root.path().join(name), "matchString": [pattern],
                    "regexEngine": "rust", "view": "matchOnly"
                }),
                None,
            );
            let shown: Vec<String> = pages
                .iter()
                .flat_map(|page| page["files"].as_array().cloned().unwrap_or_default())
                .flat_map(|file| file["matches"].as_array().cloned().unwrap_or_default())
                .map(|row| row["value"].as_str().expect("value").to_owned())
                .collect();
            assert_eq!(!shown.is_empty(), shows, "{name} {pattern}: {shown:?}");
            for value in &shown {
                for at in 0..VALUE.len() - 3 {
                    let fragment = &VALUE[at..at + 4];
                    assert!(
                        !value.contains(fragment),
                        "{name} {pattern}: value fragment {fragment:?} shown in {value:?}"
                    );
                }
                assert!(
                    !value.contains("Z9Q8"),
                    "{name} {pattern}: token shown in {value:?}"
                );
            }
        }
    }
}

/// A content row whose line holds a key secret only by the line above it (a
/// signify secret key's comment) and a token: the row shows its line as the
/// window's scan left it, so neither the key nor the token is shown.
#[test]
fn a_content_row_hides_a_value_secret_by_its_neighbor() {
    const KEY: &str = "q8Zr4LmT2vXw9KpN7sYd3HfJ6gBc1QeRu5WaVo0MiExDzlUt";
    let root = tempfile::tempdir().expect("fixture directory");
    let text = format!("untrusted comment: signify secret key\n{KEY} # {TOKEN}\n");
    fs::write(root.path().join("sig.txt"), text).expect("fixture");
    // Hidden text matches nothing; the line a read shows is a row.
    for (pattern, shows) in [
        ("q8Zr", false),
        ("ghp_", false),
        ("Dzl", false),
        (" # ", true),
    ] {
        let (_, pages) = walk_search(
            root.path(),
            serde_json::json!({
                "path": root.path().join("sig.txt"), "matchString": [pattern],
                "regexEngine": "rust", "view": "content"
            }),
            None,
        );
        let shown: Vec<String> = pages
            .iter()
            .flat_map(|page| page["files"].as_array().cloned().unwrap_or_default())
            .flat_map(|file| file["matches"].as_array().cloned().unwrap_or_default())
            .map(|row| row["value"].as_str().expect("value").to_owned())
            .collect();
        assert_eq!(!shown.is_empty(), shows, "{pattern}: {shown:?}");
        for value in &shown {
            assert!(!value.contains("LmT2"), "{pattern}: key shown in {value:?}");
            assert!(
                !value.contains("Z9Q8"),
                "{pattern}: token shown in {value:?}"
            );
        }
    }
}

/// A content row with context lines shows, line for line, the source lines it
/// starts at (its first context line), as a scan of their document leaves them:
/// a before-context line that is secret only by its neighbors is hidden, and no
/// line shows under another's place.
#[test]
fn a_context_row_shows_its_own_lines_as_their_document_leaves_them() {
    const KEY: &str = "q8Zr4LmT2vXw9KpN7sYd3HfJ6gBc1QeRu5WaVo0MiExDzlUt";
    const VALUE: &str = "QmFzZTY0VmFsdWVIZXJlWFla";
    let files = [
        (
            "ctx_sig.txt",
            format!("untrusted comment: signify secret key\n{KEY}\nfoo marker line\nbar\n"),
            "marker",
        ),
        (
            "ctx_k8s.yaml",
            format!(
                "apiVersion: v1\nkind: Secret\nmetadata:\n  name: demo\ntype: Opaque\ndata:\n  blob: {VALUE}\n# trailer marker line\n"
            ),
            "trailer",
        ),
        (
            "ctx_k8s2.yaml",
            format!(
                "apiVersion: v1\nkind: Secret\ndata:\n  first: {VALUE}\n  second: {VALUE}Zz\n# end marker\n"
            ),
            "second",
        ),
    ];
    let root = tempfile::tempdir().expect("fixture directory");
    for (name, text, pattern) in &files {
        let path = root.path().join(name);
        fs::write(&path, text).expect("fixture");
        let clean = ContentSecurity::new()
            .sanitize_text(text, Some(&path))
            .content
            .into_owned();
        let clean: Vec<&str> = clean.lines().collect();
        let hit = text.lines().position(|l| l.contains(pattern)).expect("hit");
        for (before, after) in [(1usize, 0usize), (0, 1), (1, 1), (2, 1)] {
            let (_, pages) = walk_search(
                root.path(),
                serde_json::json!({
                    "path": path, "matchString": [pattern], "view": "content",
                    "contextLines": {"before": before, "after": after}
                }),
                None,
            );
            let rows: Vec<serde_json::Value> = pages
                .iter()
                .flat_map(|page| page["files"].as_array().cloned().unwrap_or_default())
                .flat_map(|file| file["matches"].as_array().cloned().unwrap_or_default())
                .collect();
            assert_eq!(rows.len(), 1, "{name} {before}/{after}: {rows:?}");
            let value = rows[0]["value"].as_str().expect("value");
            let first = hit.saturating_sub(before);
            let last = (hit + after).min(clean.len() - 1);
            let shown: Vec<&str> = value.split('\n').collect();
            assert_eq!(
                shown.len(),
                last - first + 1,
                "{name} {before}/{after}: {value:?}"
            );
            let source: Vec<&str> = text.lines().collect();
            for (at, row) in (first..=last).zip(&shown) {
                let exact = format!("{}\t{}", at + 1, clean[at]);
                // A redaction that reaches past the row's lines leaves its line
                // the bare placeholder.
                let bare = format!("{}\t[REDACTED]", at + 1);
                assert!(
                    *row == exact || (clean[at] != source[at] && *row == bare),
                    "{name} {before}/{after}: line {} shows {row:?}, not {exact:?}",
                    at + 1
                );
            }
            for at in 0..VALUE.len() - 3 {
                assert!(!value.contains(&VALUE[at..at + 4]), "{name}: {value:?}");
            }
            assert!(!value.contains("LmT2"), "{name}: {value:?}");
        }
    }
}

/// Every row of a content search of `name` (holding `text`) under `query`,
/// checked line for line: consecutive source numbers, each line as the whole
/// source's scan leaves it (or the bare placeholder where that scan redacted
/// it), and no fragment of `secret`.
fn assert_rows_exact(name: &str, text: &str, query: serde_json::Value, secret: &str) {
    let root = tempfile::tempdir().expect("fixture directory");
    let path = root.path().join(name);
    fs::write(&path, text).expect("fixture");
    let mut query = query;
    query["path"] = serde_json::json!(path);
    let (_, pages) = walk_search(root.path(), query.clone(), None);
    let clean = ContentSecurity::new()
        .sanitize_text(text, Some(&path))
        .content
        .into_owned();
    let clean: Vec<&str> = clean.lines().collect();
    let source: Vec<&str> = text.lines().collect();
    let rows: Vec<String> = pages
        .iter()
        .flat_map(|page| page["files"].as_array().cloned().unwrap_or_default())
        .flat_map(|file| file["matches"].as_array().cloned().unwrap_or_default())
        .map(|row| row["value"].as_str().expect("value").to_owned())
        .collect();
    assert!(!rows.is_empty(), "{name} {query}: no row");
    for value in &rows {
        for at in 0..secret.len() - 3 {
            assert!(
                !value.contains(&secret[at..at + 4]),
                "{name} {query}: {value:?}"
            );
        }
        let mut previous = None::<usize>;
        for row in value.split('\n') {
            let (number, shown) = row.split_once('\t').expect("a numbered line");
            let number: usize = number.parse().expect("a line number");
            if let Some(previous) = previous {
                assert_eq!(number, previous + 1, "{name} {query}: {value:?}");
            }
            previous = Some(number);
            let at = number - 1;
            assert!(
                shown == clean[at] || (clean[at] != source[at] && shown == "[REDACTED]"),
                "{name} {query}: line {number} shows {shown:?}, not {:?}",
                clean[at]
            );
        }
    }
}

/// A row whose context stops at a matched test-code line the search left out
/// (`testCode: exclude`) is still placed at the line it starts at: a Secret's
/// value among its lines is hidden, and every line keeps its own number, on
/// either side of the gap, in CRLF text, at the file's edges and in merged
/// windows.
#[test]
fn a_row_beside_a_left_out_test_line_is_placed_at_its_own_lines() {
    const VALUE: &str = "QmFzZTY0VmFsdWVIZXJlWFla";
    let head = "#[cfg(test)]\nmod tests {\n    // marker t\n}\n// marker p\n";
    let tail_test = "\n#[cfg(test)]\nmod tail {\n    // marker t\n}\n";
    let secret_a = format!(
        "{{\n\"data\":\n{{\"blob\": \"{VALUE}\"}},\n\"note\": \"x\",\n\"kind\": \"Secret\"\n}}\n"
    );
    let secret_b = format!(
        "{{\n\"data\": {{\"blob\": \"{VALUE}\"}},\n\"note\": \"x\",\n\"kind\": \"Secret\"\n}}\n"
    );
    let fixtures = [
        ("e.rs", format!("{head}{secret_a}")),
        ("f.rs", format!("{head}{secret_b}")),
        // The gap after the match: its after-context stops at a test line.
        ("g.rs", format!("{secret_b}// marker p{tail_test}")),
        // Two rows whose windows merge.
        ("h.rs", format!("{head}// marker q\n{secret_b}")),
        // The file's first line is a row.
        ("i.rs", format!("// marker p\n{secret_b}{tail_test}")),
    ];
    for (name, text) in &fixtures {
        for crlf in [false, true] {
            let text = if crlf {
                text.replace('\n', "\r\n")
            } else {
                text.clone()
            };
            for (before, after) in [(2, 3), (2, 2), (1, 1), (0, 3), (3, 0)] {
                for test_code in ["exclude", "include", "only"] {
                    assert_rows_exact(
                        name,
                        &text,
                        serde_json::json!({
                            "matchString": ["marker"], "view": "content", "testCode": test_code,
                            "contextLines": {"before": before, "after": after}
                        }),
                        VALUE,
                    );
                }
            }
        }
    }
}

/// A matchOnly span of a whole line that holds key material after a label:
/// the scan hides the key and keeps the label, and the span shows that same
/// text, as the content row of the line and a read of it do, not a bare
/// placeholder.
#[test]
fn a_span_of_a_labelled_key_line_keeps_its_label() {
    // Fake base64: the alphabet in a fixed walk.
    let alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let body = |seed: usize| {
        (0..60)
            .map(|at| char::from(alphabet.as_bytes()[(seed * 7 + at * 13) % alphabet.len()]))
            .collect::<String>()
    };
    let block = (1..=6).map(body).collect::<Vec<_>>().join("\n");
    let text = format!(
        "plain words\n-----BEGIN OPENSSH PRIVATE KEY-----\n{block}\nordinary tail text\nlogo2: {}\nlast plain line\n",
        body(9)
    );
    let root = tempfile::tempdir().expect("fixture directory");
    let path = root.path().join("ku.txt");
    fs::write(&path, &text).expect("fixture");
    let value_of = |view: &str| {
        let found = search_root(
            root.path(),
            serde_json::json!({"matchString": ["^logo2.*$"], "regexEngine": "rust", "view": view}),
        )
        .to_row()
        .expect("serialize");
        found["files"][0]["matches"][0]["value"]
            .as_str()
            .unwrap_or_else(|| panic!("{view}: {found}"))
            .to_owned()
    };
    let content = value_of("content");
    assert!(
        content.starts_with("logo2: [REDACTED"),
        "the fixture's labelled line is hidden as key material: {content}"
    );
    assert_eq!(value_of("matchOnly"), content);
}
