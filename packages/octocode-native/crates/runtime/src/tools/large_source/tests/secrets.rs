//! A streamed source whose secrets its windows cannot show: what makes a value
//! secret lies in its object, outside the window, on either side of it. Every
//! value below is fake.
use super::*;

const VALUE: &str = "RmFrZTBWYWx1ZTlOb3Q0UmVhbDhCbG9iN0xvbmc2RW5vdWdo";
const WORDS: &str = "plain words hunter2 not real";
const OTHER_R: &str = "Fake0Other9Prime4Factor8Not7Real6Value5Ab";
const OTHER_T: &str = "Fake0Other9Prime4Coefficient8Not7Real6Cd";
const PUBLIC: &str = "Public0Coordinate9Stays8Readable7Always6Ab";

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

fn kind_before_data(name: &str, value: &str, members: &str) -> String {
    list(&format!(
        "    {{\n      \"kind\": \"Secret\",\n      \"metadata\": {{ \"name\": \"a\" }},\n      \"{members}\": {{\n        \"{name}\": \"{value}\"\n      }}\n    }}"
    ))
}

/// The 1-based line of `text` that holds `value`.
fn line_of(text: &str, value: &str) -> usize {
    text.lines()
        .position(|line| line.contains(value))
        .expect("the fixture holds it")
        + 1
}

/// `file`'s whole text served by every read that can reach `line`: its range, a
/// `matchString` on `needle`, the first chunk.
fn every_read(
    file: &tempfile::NamedTempFile,
    line: usize,
    needle: &str,
) -> Vec<(&'static str, LocalFetchResult)> {
    let path = file.path();
    vec![
        (
            "range",
            fetch(
                file,
                &query(
                    serde_json::json!({"path":path,"ranges":[{"startLine":line,"endLine":line}]}),
                ),
                None,
            ),
        ),
        (
            "matchString",
            fetch(
                file,
                &query(serde_json::json!({"path":path,"matchString":[needle]})),
                None,
            ),
        ),
        (
            "whole",
            fetch(file, &query(serde_json::json!({"path":path})), None),
        ),
    ]
}

fn hides(file: &tempfile::NamedTempFile, line: usize, needle: &str, value: &str) {
    for (read, result) in every_read(file, line, needle) {
        assert_ne!(result.status, ReadStatus::Error, "{read}: {result:?}");
        let shown = result.content.as_deref().unwrap_or_default();
        assert!(!shown.contains(value), "{read} shows the value");
        assert!(!shown.contains(&value[..12]), "{read} shows part of it");
    }
}

fn suffixed(suffix: &str, content: &[u8]) -> tempfile::NamedTempFile {
    let mut file = tempfile::Builder::new()
        .suffix(suffix)
        .tempfile()
        .expect("temp");
    std::io::Write::write_all(&mut file, content).expect("write");
    file
}

/// A window of `data`, its `kind` on either side of the window.
#[test]
fn a_secret_value_stays_hidden_with_its_kind_out_of_the_window() {
    for text in [
        data_before_kind("blob", VALUE, "data"),
        kind_before_data("blob", VALUE, "data"),
    ] {
        let line = line_of(&text, VALUE);
        let file = suffixed(".json", text.as_bytes());
        hides(&file, line, "blob", VALUE);
    }
}

/// `stringData` holds plain words, which no value shape tells from any other.
#[test]
fn plain_words_in_string_data_stay_hidden_with_the_kind_out_of_the_window() {
    for text in [
        data_before_kind("db", WORDS, "stringData"),
        kind_before_data("db", WORDS, "stringData"),
    ] {
        let line = line_of(&text, WORDS);
        let file = suffixed(".json", text.as_bytes());
        hides(&file, line, "\"db\"", WORDS);
    }
}

/// A `kind` further from the window than anything a read holds at once.
#[test]
fn a_kind_many_windows_away_still_hides_the_value() {
    let labels = (0..400_000)
        .map(|at| format!("        \"label{at}\": \"public {at}\",\n"))
        .collect::<String>();
    for text in [
        format!(
            "{{\n  \"data\": {{\n    \"blob\": \"{VALUE}\"\n  }},\n  \"metadata\": {{\n{labels}        \"end\": \"x\"\n  }},\n  \"kind\": \"Secret\"\n}}\n"
        ),
        format!(
            "{{\n  \"kind\": \"Secret\",\n  \"metadata\": {{\n{labels}        \"end\": \"x\"\n  }},\n  \"data\": {{\n    \"blob\": \"{VALUE}\"\n  }}\n}}\n"
        ),
    ] {
        let line = line_of(&text, VALUE);
        let file = suffixed(".json", text.as_bytes());
        hides(&file, line, "\"blob\"", VALUE);
        let read = fetch(
            &file,
            &query(
                serde_json::json!({"path":file.path(),"ranges":[{"startLine":line,"endLine":line + 2}]}),
            ),
            None,
        );
        assert!(
            read.warnings
                .iter()
                .any(|warning| warning.starts_with("redactedContent")),
            "a masked page says so: {read:?}"
        );
    }
}

/// YAML `stringData` holds plain words too, whichever side its `kind` is on.
#[test]
fn yaml_string_data_stays_hidden_with_the_kind_out_of_the_window() {
    let filler = "apiVersion: v1\nkind: ConfigMap\ndata:\n  key: public value\n---\n"
        .repeat(past_whole() / 50);
    for text in [
        format!(
            "apiVersion: v1\nstringData:\n  blob: {WORDS}\n  other: {VALUE}\nkind: Secret\n---\n{filler}"
        ),
        format!(
            "kind: Secret\napiVersion: v1\nstringData:\n  blob: {WORDS}\n  other: {VALUE}\n---\n{filler}"
        ),
    ] {
        let line = line_of(&text, WORDS);
        let file = suffixed(".yaml", text.as_bytes());
        hides(&file, line, "blob:", WORDS);
        hides(&file, line + 1, "other:", VALUE);
    }
}

/// RFC 7518's further primes: `r` and `t` are as private as `d`, though no
/// name-based rule knows them, and the public members stay.
#[test]
fn a_jwk_other_prime_stays_hidden_with_its_kty_out_of_the_window() {
    let filler = "  {\"id\": 1, \"payload\": \"public value\"},\n".repeat(past_whole() / 40);
    for text in [
        format!(
            "{{\n \"kty\": \"RSA\",\n \"n\": \"{PUBLIC}\",\n \"oth\": [\n  {{\n   \"r\": \"{OTHER_R}\",\n   \"t\": \"{OTHER_T}\"\n  }}\n ],\n \"rows\": [\n{filler}  {{}}\n ]\n}}\n"
        ),
        format!(
            "{{\n \"oth\": [\n  {{\n   \"r\": \"{OTHER_R}\",\n   \"t\": \"{OTHER_T}\"\n  }}\n ],\n \"rows\": [\n{filler}  {{}}\n ],\n \"n\": \"{PUBLIC}\",\n \"kty\": \"RSA\"\n}}\n"
        ),
    ] {
        let line = line_of(&text, OTHER_R);
        let file = suffixed(".json", text.as_bytes());
        hides(&file, line, "\"r\"", OTHER_R);
        hides(&file, line + 1, "\"t\"", OTHER_T);
        let public = line_of(&text, PUBLIC);
        let read = fetch(
            &file,
            &query(
                serde_json::json!({"path":file.path(),"ranges":[{"startLine":public,"endLine":public}]}),
            ),
            None,
        );
        assert!(
            read.content
                .as_deref()
                .is_some_and(|text| text.contains(PUBLIC)),
            "a public member stays readable: {read:?}"
        );
    }
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

/// One minified line: the windows are byte pages of it, and every page keeps its
/// place in the source's bytes while the value stays hidden on all of them.
#[test]
fn byte_pages_of_one_line_stay_contiguous_around_a_hidden_value() {
    let items = (0..past_whole() / 40)
        .map(|at| format!("{{\"data\":{{\"k{at}\":\"public {at}\"}},\"kind\":\"ConfigMap\"}},"))
        .collect::<String>();
    let secret = format!("{{\"data\":{{\"blob\":\"{WORDS}\"}},\"kind\":\"Secret\"}}");
    let text = format!("{{\"items\":[{secret},{items}{{}}],\"kind\":\"List\"}}");
    assert!(text.len() > LIMITS.whole_source, "a streamed source");
    let file = suffixed(".json", text.as_bytes());
    let mut next =
        query(serde_json::json!({"path":file.path(),"ranges":[{"startLine":1,"endLine":1}]}));
    let mut shown = String::new();
    let mut raw_at = 0;
    for _ in 0..6 {
        let page = fetch(&file, &next, None);
        assert_eq!(page.status, ReadStatus::Success, "{page:?}");
        let content = page.content.expect("a page");
        assert!(!content.contains("hunter2"), "{content}");
        let bytes = page.pagination.expect("a byte page");
        assert_eq!(bytes.offset, raw_at, "pages are contiguous");
        raw_at += bytes.length;
        shown.push_str(&content);
        next = page
            .next
            .and_then(|next| next.continue_window)
            .expect("more pages")
            .query;
    }
    let raw = text.replace(
        &format!("\"blob\":\"{WORDS}\""),
        "[REDACTED-KUBERNETESSECRETJSON]",
    );
    assert!(raw.starts_with(&shown), "{shown:.120}");
}

/// A later page reads its window from the first page's pass: no second walk.
#[test]
fn a_later_page_stays_hidden_from_the_kept_pass() {
    let text = data_before_kind("blob", VALUE, "data");
    let file = suffixed(".json", text.as_bytes());
    let first = fetch(
        &file,
        &query(serde_json::json!({"path":file.path(),"matchString":["public value 8\""]})),
        None,
    );
    let digest = first.source_sha256.clone().expect("the source's digest");
    let line = line_of(&text, VALUE);
    let later = fetch(
        &file,
        &query(
            serde_json::json!({"path":file.path(),"ranges":[{"startLine":line,"endLine":line}],"snapshot":digest}),
        ),
        None,
    );
    let shown = later.content.as_deref().unwrap_or_default();
    assert!(!shown.contains(VALUE), "{later:?}");
    assert_eq!(later.source_sha256.as_deref(), Some(digest.as_str()));
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
