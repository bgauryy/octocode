//! A value whose secrecy lies lines away in the same object: the row is judged
//! with the source, a search finds what a read's matchString finds there (a
//! hidden value is no hit), and the public members stay readable.
use super::*;

const SCALAR: &str = "Fake0Scalar9Not4Real8Value7Long6Enough5Xy";
const PUBLIC: &str = "Public0Coordinate9Stays8Readable7Always6Ab";
const VALUE: &str = "QUJDREVGR0hJSktMTU5PUFFSU1RVVldYWVowMTIz";

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
    let read = crate::tools::local_fetch::execute_local_fetch(
        &query,
        &policy,
        &security,
        &NeverCancel,
        None,
        None,
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

/// `kty` either side of the value, further from it than a row's window.
fn jwks() -> [String; 2] {
    let padding = (0..12)
        .map(|at| format!("  \"m{at}\": \"v{at}\",\n"))
        .collect::<String>();
    [
        format!(
            "{{\n  \"kty\": \"EC\",\n{padding}  \"x\": \"{PUBLIC}\",\n  \"d\": \"{SCALAR}\"\n}}\n"
        ),
        format!(
            "{{\n  \"d\": \"{SCALAR}\",\n  \"x\": \"{PUBLIC}\",\n{padding}  \"kty\": \"EC\"\n}}\n"
        ),
    ]
}

#[test]
fn a_row_of_a_private_jwk_member_is_judged_with_its_object() {
    for text in jwks() {
        for fields in [
            serde_json::json!({"matchString":["\"d\""],"view":"content"}),
            serde_json::json!({"matchString":[&SCALAR[..12]],"view":"content"}),
            serde_json::json!({"matchString":[&SCALAR[..12]],"view":"matchOnly"}),
        ] {
            let found = rows("key.json", &text, fields.clone());
            assert!(
                !found.to_string().contains(&SCALAR[..12])
                    || fields["matchString"][0] == SCALAR[..12],
                "{fields}: {found}"
            );
            let shown = found["files"][0]["matches"].to_string();
            assert!(!shown.contains(&SCALAR[12..24]), "{fields}: {found}");
            // The member's name is a hit; its hidden value is none.
            let expected = usize::from(fields["matchString"][0] == "\"d\"");
            assert_eq!(found_rows(&found), expected, "{fields}: {found}");
            finds_as_a_read("key.json", &text, &fields, &found);
        }
        let public = rows(
            "key.json",
            &text,
            serde_json::json!({"matchString":["\"x\""],"view":"content"}),
        );
        assert!(public.to_string().contains(PUBLIC), "{public}");
    }
}

/// With no `kty` anywhere the same member is plain data.
#[test]
fn the_same_member_with_no_kty_is_shown() {
    let found = rows(
        "data.json",
        &format!("{{\n  \"d\": \"{SCALAR}\",\n  \"name\": \"x\"\n}}\n"),
        serde_json::json!({"matchString":["\"d\""],"view":"content"}),
    );
    assert!(found.to_string().contains(SCALAR), "{found}");
}

#[test]
fn a_row_of_a_kubernetes_secret_in_json_is_judged_with_the_manifest() {
    let labels = (0..40)
        .map(|at| format!("      \"l{at}\": \"v{at}\",\n"))
        .collect::<String>();
    for text in [
        format!(
            "{{\n  \"apiVersion\": \"v1\",\n  \"data\": {{\n    \"tls.crt\": \"{VALUE}\"\n  }},\n  \"kind\": \"Secret\",\n  \"metadata\": {{ \"name\": \"x\" }}\n}}\n"
        ),
        format!(
            "{{\n  \"kind\": \"Secret\",\n  \"metadata\": {{\n    \"labels\": {{\n{labels}      \"a\": \"b\"\n    }}\n  }},\n  \"data\": {{\n    \"tls.crt\": \"{VALUE}\"\n  }}\n}}\n"
        ),
    ] {
        let found = rows(
            "secret.json",
            &text,
            serde_json::json!({"matchString":["tls.crt"],"view":"content"}),
        );
        assert!(!found.to_string().contains(&VALUE[..12]), "{found}");
        let fields = serde_json::json!({"matchString":["tls.crt"]});
        finds_as_a_read("secret.json", &text, &fields, &found);
    }
}

/// Real API-server order: deep `metadata` between `kind` and `data`, either side.
#[test]
fn a_row_of_a_secret_past_deep_metadata_is_judged_with_the_manifest() {
    let managed = r#""metadata": {"managedFields": [{"fieldsV1": {"f:data": {".": {}, "f:p": {"f:q": {}}}}}]}"#;
    for text in [
        format!(
            "{{\n  \"kind\": \"Secret\",\n  {managed},\n  \"stringData\": {{\n    \"a\": \"{VALUE}\"\n  }},\n  \"data\": {{\n    \"tls.crt\": \"{SCALAR}\"\n  }}\n}}\n"
        ),
        format!(
            "{{\n  \"data\": {{\n    \"tls.crt\": \"{SCALAR}\"\n  }},\n  {managed},\n  \"kind\": \"Secret\"\n}}\n"
        ),
    ] {
        let found = rows(
            "secret.json",
            &text,
            serde_json::json!({"matchString":["tls.crt"],"view":"content"}),
        );
        assert!(!found.to_string().contains(&SCALAR[..12]), "{found}");
        let fields = serde_json::json!({"matchString":["tls.crt"]});
        finds_as_a_read("secret.json", &text, &fields, &found);
    }
}

/// A plain `stringData` value is what a read hides, whatever it looks like, and
/// the row says so.
#[test]
fn a_row_of_a_plain_stringdata_value_is_what_a_read_shows() {
    let plain = "correct horse battery staple";
    let text = format!(
        "{{\n  \"apiVersion\": \"v1\",\n  \"kind\": \"Secret\",\n  \"stringData\": {{\n    \"dbpass\": \"{plain}\",\n    \"short\": \"aGVsbG8=\"\n  }},\n  \"name\": \"visible\"\n}}\n"
    );
    for fields in [
        serde_json::json!({"matchString":["correct horse"],"view":"content"}),
        serde_json::json!({"matchString":["short"],"view":"content"}),
        serde_json::json!({"matchString":["correct horse"],"view":"matchOnly"}),
    ] {
        let found = rows("secret.json", &text, fields.clone());
        let shown = found["files"][0]["matches"].to_string();
        assert!(!shown.contains("horse battery"), "{fields}: {found}");
        assert!(!shown.contains("aGVsbG8="), "{fields}: {found}");
        finds_as_a_read("secret.json", &text, &fields, &found);
    }
    // The manifest's other members stay, and the same member in another kind.
    let name = rows(
        "secret.json",
        &text,
        serde_json::json!({"matchString":["visible"],"view":"content"}),
    );
    assert!(name.to_string().contains("\\\"visible\\\"") || name.to_string().contains("visible"));
    assert_eq!(
        name["files"][0]["matches"][0]["value"], "  \"name\": \"visible\"",
        "{name}"
    );
    let other = text.replace("\"Secret\"", "\"ConfigMap\"");
    let found = rows(
        "other.json",
        &other,
        serde_json::json!({"matchString":["correct horse"],"view":"content"}),
    );
    assert!(found.to_string().contains(plain), "{found}");
}

#[test]
fn a_row_of_a_nested_jwk_is_judged_with_its_object() {
    let text = format!(
        "{{\n  \"kty\": \"EC\",\n  \"meta\": {{\n    \"o\": \"x\"\n  }},\n  \"x\": \"{PUBLIC}\",\n  \"d\": \"{SCALAR}\"\n}}\n"
    );
    let found = rows(
        "key.json",
        &text,
        serde_json::json!({"matchString":[&SCALAR[..12]],"view":"content"}),
    );
    assert!(
        !found["files"][0]["matches"]
            .to_string()
            .contains(&SCALAR[12..24]),
        "{found}"
    );
    let public = rows(
        "key.json",
        &text,
        serde_json::json!({"matchString":["\"x\""],"view":"content"}),
    );
    assert!(public.to_string().contains(PUBLIC), "{public}");
}

/// Further from `data` than any fixed gap: the row and a matchOnly span alike.
#[test]
fn a_row_of_a_yaml_secret_past_long_annotations_is_judged_with_the_file() {
    let annotations = (0..3_000)
        .map(|at| format!("    a{at}: value number {at}\n"))
        .collect::<String>();
    let text = format!(
        "apiVersion: v1\nkind: Secret\nmetadata:\n  annotations:\n{annotations}data:\n  tls.crt: {VALUE}\n"
    );
    for fields in [
        serde_json::json!({"matchString":["tls.crt"],"view":"content"}),
        serde_json::json!({"matchString":[&VALUE[..12]],"view":"matchOnly"}),
    ] {
        let found = rows("secret.yaml", &text, fields.clone());
        let shown = found["files"][0]["matches"].to_string();
        assert!(!shown.contains(&VALUE[12..24]), "{fields}: {found}");
        finds_as_a_read("secret.yaml", &text, &fields, &found);
    }
    let annotation = rows(
        "secret.yaml",
        &text,
        serde_json::json!({"matchString":["value number 7$"],"regexEngine":"rust","view":"content"}),
    );
    assert!(
        annotation.to_string().contains("value number 7"),
        "{annotation}"
    );
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
        serde_json::json!({"matchString":["plain words"],"view":"content"}),
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
        serde_json::json!({"matchString":["let tail"],"view":"content"}),
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
        serde_json::json!({"matchString":["plain"],"view":"content"}),
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
        serde_json::json!({"matchString":["untrusted"],"view":"content"}),
    );
    assert_eq!(
        found["files"][0]["matches"][0]["value"],
        "untrusted comment: minisign encrypted secret key",
        "{found}"
    );
}

/// A path a file-context pattern applies to does not take this check away.
#[test]
fn a_private_member_in_yaml_is_judged_with_its_mapping() {
    let text = format!(
        "keys:\n  - kty: EC\n    a: 1\n    b: 2\n    c: 3\n    x: {PUBLIC}\n    d: {SCALAR}\n"
    );
    let fields = serde_json::json!({"matchString":[&SCALAR[..12]],"view":"content"});
    let found = rows("jwks.yaml", &text, fields.clone());
    let shown = found["files"][0]["matches"].to_string();
    assert!(!shown.contains(&SCALAR[12..24]), "{found}");
    // The hidden member is no hit.
    assert_eq!(found_rows(&found), 0, "{found}");
    finds_as_a_read("jwks.yaml", &text, &fields, &found);
}
