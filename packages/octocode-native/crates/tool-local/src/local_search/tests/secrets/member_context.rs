//! A value whose secrecy lies lines away in the same object: the row is judged
//! with the source, a search finds what a read's matchString finds there (a
//! hidden value is no hit), and the public members stay readable.
use super::*;
use octocode_test_support::json_fixture;

const SCALAR: &str = "Fake0Scalar9Not4Real8Value7Long6Enough5Xy";
const PUBLIC: &str = "Public0Coordinate9Stays8Readable7Always6Ab";

fn rows(name: &str, text: &str, fields: serde_json::Value) -> serde_json::Value {
    let root = tempfile::tempdir().expect("fixture directory");
    let path = root.path().join(name);
    fs::write(&path, text).expect("fixture");
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": path.to_string_lossy()}),
        Some(&ls_query(fields, None)),
    );
    search_in(&request, &policy, &security, None)
        .expect("search")
        .to_row()
        .expect("serialize")
}

/// Whether a read of `text` as `name` finds `needle` with its matchString:
/// a search of the same text finds it exactly then.
fn read_finds(name: &str, text: &str, needle: &str) -> bool {
    let root = tempfile::tempdir().expect("fixture directory");
    let path = root.path().join(name);
    fs::write(&path, text).expect("fixture");
    let (policy, security) = policy_for(root.path());
    let query: octocode_contract_types::LocalFetchQuery = serde_json::from_value(
        serde_json::json!({"path": path.to_string_lossy(), "matchString": [needle]}),
    )
    .expect("localFetch row");
    let read = crate::local_fetch::execute_local_fetch(
        &query,
        &policy,
        &security,
        &NeverCancel,
        None,
        false,
    );
    read.selected_match_count.unwrap_or(0) > 0
}

/// The rows a search found.
fn found_rows(found: &serde_json::Value) -> usize {
    found["files"][0]["matches"].as_array().map_or(0, Vec::len)
}

/// Whether `found`, a search of `fields` in `text` as `name`, finds what a
/// read's matchString finds.
fn finds_as_a_read(name: &str, text: &str, fields: &serde_json::Value, found: &serde_json::Value) {
    let needle = fields["matchString"][0].as_str().expect("needle");
    assert_eq!(
        found_rows(found) > 0,
        read_finds(name, text, needle),
        "{fields}: {found}"
    );
}

/// With no `kty` anywhere the same member is plain data.
#[test]
fn the_same_member_with_no_kty_is_shown() {
    let found = rows(
        "data.json",
        &format!("{{\n  \"d\": \"{SCALAR}\",\n  \"name\": \"x\"\n}}\n"),
        json_fixture(r#"{"matchString": ["\"d\""], "view": "content"}"#),
    );
    assert!(found.to_string().contains(SCALAR), "{found}");
}

/// A comment after the armor leaves the lines below as a read judges them: the
/// END still in the file says what they are.
#[test]
fn a_body_past_a_comment_in_its_armor_is_what_a_read_shows() {
    let body: Vec<String> = (0..6)
        .map(|at| format!("Zm9v{at}mFyYmF6cXV4Zm9vYmFyYmF6cXV4Zm9v"))
        .collect();
    let text = format!(
        "-----BEGIN PRIVATE KEY-----\nthis key is for staging\n{}\n-----END PRIVATE KEY-----\nplain words after it\n",
        body.join("\n")
    );
    for line in &body {
        let fields = serde_json::json!({"matchString":[&line[..14]],"view":"content"});
        let found = rows("k.txt", &text, fields.clone());
        assert!(!found.to_string().contains(&line[14..]), "{found}");
        // A body line a read hides is no hit.
        assert_eq!(found_rows(&found), 0, "{found}");
        finds_as_a_read("k.txt", &text, &fields, &found);
    }
    let after = rows(
        "k.txt",
        &text,
        json_fixture(r#"{"matchString": ["plain words"], "view": "content"}"#),
    );
    assert_eq!(
        after["files"][0]["matches"][0]["value"], "plain words after it",
        "{after}"
    );
}

/// A marker that opens no key: every other row of the file reads as it did.
#[test]
fn a_literal_marker_in_code_leaves_its_file_readable() {
    let text = "const MARKER: &str = \"-----BEGIN PRIVATE KEY-----\";\nfn check(line: &str) -> bool {\n    let tail = line.len();\n    line.contains(MARKER)\n}\n";
    let found = rows(
        "marker.rs",
        text,
        json_fixture(r#"{"matchString": ["let tail"], "view": "content"}"#),
    );
    assert_eq!(
        found["files"][0]["matches"][0]["value"], "    let tail = line.len();",
        "{found}"
    );
}

/// Only a row showing such a member reads its source whole: an ordinary row of
/// the same file, or of any other, is settled from its window.
#[test]
fn a_row_with_no_such_member_keeps_its_window_check() {
    let text = format!("const a = 1;\nconst b = \"plain\";\n// kty mention: \"kty\": {PUBLIC}\n");
    let found = rows(
        "a.ts",
        &text,
        json_fixture(r#"{"matchString": ["plain"], "view": "content"}"#),
    );
    assert_eq!(
        found["files"][0]["matches"][0]["value"], "const b = \"plain\";",
        "{found}"
    );
}

/// A key's own line says nothing: its comment is the line above. A read of
/// both lines hides the key, so it is no hit; the comment stays readable.
#[test]
fn a_row_of_a_minisign_secret_key_is_judged_with_its_comment() {
    let value = format!("{SCALAR}{SCALAR}");
    let text = format!("untrusted comment: minisign encrypted secret key\n{value}\n");
    for fields in [
        serde_json::json!({"matchString":[&SCALAR[..12]],"view":"content"}),
        serde_json::json!({"matchString":[&SCALAR[..12]],"view":"matchOnly"}),
    ] {
        let found = rows("key.txt", &text, fields.clone());
        let shown = found["files"][0]["matches"].to_string();
        assert!(!shown.contains(&SCALAR[12..24]), "{fields}: {found}");
        // The hidden key is no hit.
        assert_eq!(found_rows(&found), 0, "{fields}: {found}");
        finds_as_a_read("key.txt", &text, &fields, &found);
    }
    let found = rows(
        "key.txt",
        &text,
        json_fixture(r#"{"matchString": ["untrusted"], "view": "content"}"#),
    );
    assert_eq!(
        found["files"][0]["matches"][0]["value"],
        "untrusted comment: minisign encrypted secret key",
        "{found}"
    );
}
