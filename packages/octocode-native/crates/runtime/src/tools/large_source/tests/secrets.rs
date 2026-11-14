//! A streamed source whose secrets its windows cannot show: what makes a value
//! secret lies in its object, outside the window, on either side of it. Every
//! value below is fake.
use super::*;

const VALUE: &str = "RmFrZTBWYWx1ZTlOb3Q0UmVhbDhCbG9iN0xvbmc2RW5vdWdo";

/// Bytes that take a source past the whole-file ceiling: ConfigMaps, as a
/// `kubectl get -o json` list holds beside its Secrets.
fn config_maps() -> String {
    let mut text = String::new();
    let mut at = 0usize;
    while text.len() < past_whole() {
        text.push_str(&format!(
            "    {{\n      \"data\": {{\n        \"k{at}\": \"public value {at}\"\n      }},\n      \"kind\": \"ConfigMap\"\n    }},\n"
        ));
        at += 1;
    }
    text
}

fn list(secret: &str) -> String {
    format!(
        "{{\n  \"items\": [\n{secret},\n{}    {{}}\n  ],\n  \"kind\": \"List\"\n}}\n",
        config_maps()
    )
}

fn data_before_kind(name: &str, value: &str, members: &str) -> String {
    list(&format!(
        "    {{\n      \"{members}\": {{\n        \"{name}\": \"{value}\"\n      }},\n      \"kind\": \"Secret\",\n      \"metadata\": {{ \"name\": \"a\" }}\n    }}"
    ))
}

/// The 1-based line of `text` that holds `value`.
fn line_of(text: &str, value: &str) -> usize {
    text.lines()
        .position(|line| line.contains(value))
        .expect("the fixture holds it")
        + 1
}

fn suffixed(suffix: &str, content: &[u8]) -> tempfile::NamedTempFile {
    let mut file = tempfile::Builder::new()
        .suffix(suffix)
        .tempfile()
        .expect("temp");
    std::io::Write::write_all(&mut file, content).expect("write");
    file
}

/// The exact form: only the Secret's own values go; a ConfigMap beside it in
/// the same list keeps its data, and a file with no Secret is untouched.
#[test]
fn only_the_secret_s_own_values_are_hidden() {
    let text = data_before_kind("blob", VALUE, "data");
    let file = suffixed(".json", text.as_bytes());
    let line = line_of(&text, "public value 7\"");
    let read = fetch(
        &file,
        &query(
            serde_json::json!({"path":file.path(),"ranges":[{"startLine":line,"endLine":line + 2}]}),
        ),
        None,
    );
    assert_eq!(
        read.content.as_deref(),
        Some("        \"k7\": \"public value 7\"\n      },\n      \"kind\": \"ConfigMap\"\n"),
        "{read:?}"
    );
    assert!(
        !read
            .warnings
            .iter()
            .any(|warning| warning.starts_with("redactedContent")),
        "nothing on this page was hidden: {read:?}"
    );
}

/// A log that only refers to a Secret holds none of its values.
#[test]
fn a_reference_to_a_secret_hides_nothing() {
    let row = "{\"level\":\"info\",\"msg\":\"event\",\"ref\":{\"kind\":\"Secret\",\"name\":\"x\"},\"user\":\"svc\"}\n";
    let file = suffixed(".log", row.repeat(past_whole() / row.len()).as_bytes());
    let read = fetch(
        &file,
        &query(serde_json::json!({"path":file.path(),"ranges":[{"startLine":9,"endLine":9}]})),
        None,
    );
    assert_eq!(read.content.as_deref(), Some(row), "{read:?}");
}

/// Retest R6: a source past the whole-file ceiling (a long line, then key
/// blocks), walked by every continuation from its start to its end. The note
/// that key lines were replaced stands on the page that shows their
/// placeholders, and only there; and each hidden line of a block shows the
/// placeholder, never an empty line. A PGP armor's `Version:` and `Comment:`
/// headers read as written.
#[test]
fn a_key_blocks_note_and_placeholders_stand_on_the_page_that_shows_them() {
    const NOTE: &str = "inside private-key blocks were replaced";
    let body: Vec<String> = (0..19)
        .map(|at| {
            format!(
                "MIIEfake{at:02}{}",
                "QUJDREVGR0hJSktMTU5PUFFSU1RVVldYWVow".repeat(2)
            )[..64]
                .to_owned()
        })
        .collect();
    let mut text = "word ".repeat(past_whole() / 5 + 1);
    text.push('\n');
    text.push_str("plain filler line\n".repeat(40).as_str());
    text.push_str("-----BEGIN RSA PRIVATE KEY-----\n");
    for line in &body {
        text.push_str(line);
        text.push('\n');
    }
    text.push_str("-----END RSA PRIVATE KEY-----\nfiller between\n");
    text.push_str(
        "-----BEGIN PGP PRIVATE KEY BLOCK-----\nVersion: GnuPG v2\nComment: fabricated\n\n",
    );
    for line in &body[..5] {
        text.push_str(&line.replace("MIIE", "lQOY"));
        text.push('\n');
    }
    text.push_str("-----END PGP PRIVATE KEY BLOCK-----\ntail line\n");
    let file = temp(text.as_bytes());
    let placeholder = crate::security::key_fragment_placeholder();
    let mut next = query(serde_json::json!({"path": file.path()}));
    let mut shown = String::new();
    loop {
        let page = fetch(&file, &next, None);
        assert_eq!(page.status, ReadStatus::Success, "{page:?}");
        let content = page.content.clone().expect("a page");
        let noted = page.warnings.iter().any(|warning| warning.contains(NOTE));
        assert_eq!(
            noted,
            content.contains(&placeholder),
            "the note stands where the placeholders do: {:?} {:?}",
            page.warnings,
            content.get(..content.len().min(200))
        );
        shown.push_str(&content);
        let Some(call) = page.next.and_then(|next| next.continue_window) else {
            break;
        };
        next = call.query;
    }
    for line in &body {
        assert!(!shown.contains(&line[8..40]), "a body line shows");
    }
    let lines: Vec<&str> = shown.lines().collect();
    let begin = lines
        .iter()
        .position(|line| line.ends_with("-----BEGIN RSA PRIVATE KEY-----"))
        .expect("the RSA armor shows");
    for line in &lines[begin + 1..=begin + body.len()] {
        assert!(line.ends_with(&placeholder), "{line:?}");
    }
    let pgp = lines
        .iter()
        .position(|line| line.ends_with("-----BEGIN PGP PRIVATE KEY BLOCK-----"))
        .expect("the PGP armor shows");
    assert!(
        lines[pgp + 1].ends_with("Version: GnuPG v2"),
        "{:?}",
        &lines[pgp..]
    );
    assert!(
        lines[pgp + 2].ends_with("Comment: fabricated"),
        "{:?}",
        &lines[pgp..]
    );
    for line in &lines[pgp + 4..pgp + 9] {
        assert!(line.ends_with(&placeholder), "{:?}", &lines[pgp..]);
    }
}
