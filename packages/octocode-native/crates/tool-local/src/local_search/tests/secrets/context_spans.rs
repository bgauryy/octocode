//! A matchOnly span over a value that is secret only by its document's context
//! (a Kubernetes Secret's data value) and a token its line alone shows: the span
//! is judged by the whole source's scan, so the value goes with the token, as a
//! read of the file hides both. Every value below is fake.
use super::*;

const TOKEN: &str = "ghp_Z9Q8W7E6R5T4Y3U2I1O0P9L8K7J6H5G4F3D2";

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
