//! Secrets the document's shape names: `kind` and `kty` say what the members of
//! their own object are, however far apart and however deeply nested the rest.

use super::*;

const VALUE: &str = "QUJDREVGR0hJSktMTU5PUFFSU1RVVldYWVowMTIz";
const OTHER: &str = "Other1Scalar2Not3Real4Value5Long6Enough7Zz";
const SCALAR: &str = "Fake0Scalar9Not4Real8Value7Long6Enough5Xy";
const PUBLIC: &str = "Public0Coordinate9Stays8Readable7Always6Ab";

/// The metadata the API server writes between `kind` and `data`.
const MANAGED: &str = r#""metadata":{"name":"x","managedFields":[{"manager":"kubectl","fieldsV1":{"f:data":{".":{},"f:p":{"f:q":{"f:r":{}}}}}}]}"#;

fn hides(text: &str, path: Option<&str>, secrets: &[&str], kept: &[&str]) {
    for haystack in [text.to_owned(), format!("caf\u{e9}\n{text}\n\u{e9}")] {
        let result = detect(&haystack, path);
        for secret in secrets {
            assert!(
                !result.sanitized.contains(secret),
                "{secret} in {}",
                result.sanitized
            );
        }
        for piece in kept {
            assert!(
                result.sanitized.contains(piece),
                "{piece} lost: {}",
                result.sanitized
            );
        }
        assert_eq!(
            result.sanitized.split('\n').count(),
            haystack.split('\n').count(),
            "{}",
            result.sanitized
        );
        assert_no_pattern_matches(&result);
    }
}

fn unchanged(text: &str, path: &str) {
    assert_eq!(detect(text, Some(path)).sanitized, text);
}

#[test]
fn a_secret_in_json_is_hidden_past_any_nesting() {
    for text in [
        format!(r#"{{"kind":"Secret","apiVersion":"v1",{MANAGED},"data":{{"p":"{VALUE}"}}}}"#),
        format!(r#"{{"data":{{"p":"{VALUE}"}},{MANAGED},"kind":"Secret"}}"#),
        format!(
            "{{\n  \"kind\": \"Secret\",\n  {MANAGED},\n  \"data\": {{\n    \"p\": \"{VALUE}\"\n  }}\n}}\n"
        ),
    ] {
        hides(&text, Some("conf/secret.json"), &[VALUE], &["Secret"]);
        hides(&text, None, &[VALUE], &["Secret"]);
    }
}

/// JSONC and source code put comments and blank lines anywhere, also between a
/// member's name and its value: `data` and `kind` still say what their values
/// are, in a file and in the pathless net.
#[test]
fn a_comment_between_a_name_and_its_value_keeps_its_role() {
    for gap in [
        "\n// db creds\n",
        " /* c */ ",
        "\n\n# c\n",
        " // c\n\n/* a\n b */\n",
    ] {
        for text in [
            format!("{{\n\"data\":{gap}{{\"blob\": \"{VALUE}\"}},\n\"kind\": \"Secret\"\n}}\n"),
            format!(
                "{{\n\"kind\":{gap}\"Secret\",\n\"stringData\":{gap}{{\"blob\": \"{VALUE}\"}}\n}}\n"
            ),
        ] {
            for path in [Some("conf/secret.jsonc"), Some("src/fixture.rs"), None] {
                hides(&text, path, &[VALUE], &["Secret"]);
            }
        }
    }
}

#[test]
fn every_values_member_of_a_secret_is_hidden() {
    for text in [
        format!(r#"{{"kind":"Secret","stringData":{{"a":"{VALUE}"}},"data":{{"b":"{OTHER}"}}}}"#),
        format!(r#"{{"data":{{"b":"{OTHER}"}},"kind":"Secret","stringData":{{"a":"{VALUE}"}}}}"#),
    ] {
        hides(&text, None, &[VALUE, OTHER], &[]);
    }
    // Plain words are a Secret's values too: `stringData` holds them unencoded.
    hides(
        "{\"kind\":\"Secret\",\"stringData\":{\"p\":\"correct horse battery staple\"}}",
        None,
        &["correct horse"],
        &[],
    );
    // A value that is itself structured.
    hides(
        &format!(r#"{{"kind":"Secret","data":{{"a":{{"b":"{VALUE}"}}}}}}"#),
        None,
        &[VALUE],
        &[],
    );
}

/// Held escaped in another string, at any depth of escaping, with text around it.
#[test]
fn a_secret_held_in_a_string_is_hidden_past_nesting() {
    hides(
        &format!(
            r#"{{"note":"{{\"kind\":\"Secret\",\"metadata\":{{\"a\":{{\"b\":{{\"c\":{{}}}}}}}},\"data\":{{\"k\":\"{VALUE}\"}}}}"}}"#
        ),
        None,
        &[VALUE],
        &["note"],
    );
    hides(
        &format!(
            r#"{{"log":"applied {{\"kind\":\"Secret\",\"data\":{{\"k\":\"{VALUE}\"}}}} ok"}}"#
        ),
        None,
        &[VALUE],
        &["applied", "ok"],
    );
    hides(
        &format!(
            r#"{{"a":"{{\"b\":\"{{\\\"kind\\\":\\\"Secret\\\",\\\"data\\\":{{\\\"k\\\":\\\"{VALUE}\\\"}}}}\"}}"}}"#
        ),
        None,
        &[VALUE],
        &[],
    );
    hides(
        &format!("curl -d '{{\"kind\":\"Secret\",\"data\":{{\"k\":\"{VALUE}\"}}}}' $URL\n"),
        None,
        &[VALUE],
        &["curl", "$URL"],
    );
}

#[test]
fn only_a_secrets_own_values_are_hidden() {
    let path = "conf/a.json";
    // Named in prose, or in a reference beside unrelated data.
    unchanged(
        &format!(
            "Set \"kind\": \"Secret\" in the manifest.\n\nExample: {{\"data\":{{\"blob\":\"{VALUE}\"}}}}\n"
        ),
        path,
    );
    unchanged(
        &format!(r#"{{"secretRef":{{"kind":"Secret","name":"x"}},"data":{{"blob":"{VALUE}"}}}}"#),
        path,
    );
    unchanged(
        &format!(r#"{{"data":{{"blob":"{VALUE}"}},"child":{{"kind":"Secret"}}}}"#),
        path,
    );
    unchanged(
        &format!(r#"{{"description":"the kind: \"Secret\" one","data":{{"b":"{VALUE}"}}}}"#),
        path,
    );
    unchanged(
        &format!(r#"{{"kind":"SecretStore","data":{{"b":"{VALUE}"}}}}"#),
        path,
    );
    unchanged(&format!(r#"{{"kind":"Secret","data":"{VALUE}"}}"#), path);
    // Unquoted names are source code, not a manifest.
    unchanged(
        &format!("const x = {{ kind: \"Secret\", data: {{ blob: \"{VALUE}\" }} }};\n"),
        "src/a.conf",
    );
    // Braces inside a string do not move what the object holds.
    let braces = format!(r#"{{"a":{{"note":"}}}}{{","kind":"Secret"}},"data":{{"b":"{VALUE}"}}}}"#);
    unchanged(&braces, path);
    // Nothing to hide.
    unchanged(r#"{"kind":"Secret","data":{}}"#, path);
}

#[test]
fn a_private_key_member_is_hidden_past_a_nested_object() {
    for text in [
        format!(r#"{{"kty":"EC","meta":{{"o":"x"}},"x":"{PUBLIC}","d":"{SCALAR}"}}"#),
        format!(
            r#"{{"d":"{SCALAR}","meta":{{"o":{{"p":{{"q":{{}}}}}}}},"x":"{PUBLIC}","kty":"EC"}}"#
        ),
        format!(
            "{{\n  \"kty\": \"EC\",\n  \"meta\": {{\n    \"o\": \"x\"\n  }},\n  \"x\": \"{PUBLIC}\",\n  \"d\": \"{SCALAR}\"\n}}\n"
        ),
        format!("const k = {{ kty: 'EC', meta: {{ o: 'x' }}, x: '{PUBLIC}', d: '{SCALAR}' }};\n"),
        format!(
            r#"{{"e":"{{\"kty\":\"EC\",\"m\":{{\"o\":1}},\"x\":\"{PUBLIC}\",\"d\":\"{SCALAR}\"}}"}}"#
        ),
    ] {
        hides(&text, Some("conf/key.conf"), &[SCALAR], &[PUBLIC]);
    }
}

/// RFC 7518: `oth` holds further primes, each of its members private.
#[test]
fn a_keys_further_primes_are_hidden() {
    hides(
        &format!(
            r#"{{"kty":"RSA","n":"{PUBLIC}","e":"AQAB","oth":[{{"r":"{OTHER}","d":"{OTHER}","t":"{OTHER}"}}],"d":"{SCALAR}","p":"{SCALAR}"}}"#
        ),
        Some("conf/key.conf"),
        &[SCALAR, OTHER],
        &[PUBLIC, "AQAB"],
    );
    unchanged(
        &format!(r#"{{"oth":[{{"d":"{OTHER}"}}],"name":"x"}}"#),
        "conf/a.conf",
    );
}

#[test]
fn a_member_of_another_object_stays() {
    for text in [
        format!(r#"{{"kty":"EC","meta":{{"d":"{SCALAR}"}},"x":"{PUBLIC}"}}"#),
        format!(r#"{{"meta":{{"kty":"EC"}},"d":"{SCALAR}"}}"#),
        format!(r#"{{"kty":"EC"}}{{"d":"{SCALAR}"}}"#),
        format!(r#"{{"note":"kty: EC","d":"{SCALAR}"}}"#),
    ] {
        unchanged(&text, "conf/a.conf");
    }
}

/// A cut of a document: what lies outside every brace is one object, until a
/// closer shows it ended.
#[test]
fn a_cut_of_an_object_is_judged_as_one() {
    hides(
        &format!("\"kty\": \"EC\",\n  \"m\": {{\"o\": 1}},\n  \"d\": \"{SCALAR}\"\n"),
        None,
        &[SCALAR],
        &[],
    );
    hides(
        &format!("{{\"kty\": \"EC\",\n  \"d\": \"{SCALAR}\""),
        None,
        &[SCALAR],
        &[],
    );
    unchanged(
        &format!("\"kty\": \"EC\"\n}}\n{{\n\"d\": \"{SCALAR}\"\n"),
        "conf/a.conf",
    );
}

#[test]
fn a_secret_in_yaml_is_hidden_past_any_gap() {
    let annotations = (0..3_000)
        .map(|at| format!("    a{at}: value number {at}\n"))
        .collect::<String>();
    for text in [
        format!(
            "apiVersion: v1\nkind: Secret\nmetadata:\n  annotations:\n{annotations}data:\n  tls.crt: {VALUE}\n"
        ),
        format!(
            "apiVersion: v1\ndata:\n  tls.crt: {VALUE}\nmetadata:\n  annotations:\n{annotations}kind: Secret\n"
        ),
        format!("kind: \"Secret\"\r\nmetadata:\r\n{annotations}stringData:\r\n  a: {VALUE}\r\n"),
    ] {
        hides(&text, Some("k8s/secret.yaml"), &[VALUE], &["etadata:"]);
        assert!(
            detect_in(&text, Scope::Stored(None))
                .secrets_detected
                .contains(&"kubernetesSecrets".to_owned())
        );
    }
}

#[test]
fn every_block_of_a_yaml_secret_is_hidden() {
    let path = Some("k8s/a.yml");
    hides(
        &format!(
            "kind: Secret\nstringData:\n  config.yaml: |\n    line without a colon {VALUE}\n    other: {OTHER}\ndata:\n  b: {OTHER}\nkeep: yes\n"
        ),
        path,
        &[VALUE, OTHER],
        &["keep: yes", "kind: Secret"],
    );
    // A List's items, each with its own kind; a comment or a template line inside.
    hides(
        &format!(
            "kind: List\nitems:\n- apiVersion: v1\n  data:\n    keep: plainvalue\n  kind: ConfigMap\n- apiVersion: v1\n  kind: Secret\n  data:\n    # rotated\n    a: {VALUE}\n{{{{- if .x }}}}\n    b: {OTHER}\n{{{{- end }}}}\n- kind: ConfigMap\n  data:\n    other: stays\n"
        ),
        path,
        &[VALUE, OTHER],
        &["keep: plainvalue", "other: stays"],
    );
    // Flow style, and a kind with a comment.
    hides(
        &format!("kind: Secret # creds\ndata: {{a: {VALUE}}}\n"),
        path,
        &[VALUE],
        &["# creds"],
    );
    // One manifest held in a ConfigMap's value.
    hides(
        &format!(
            "kind: ConfigMap\ndata:\n  plain: stays\n  manifest: |\n    kind: Secret\n    data:\n      k: {VALUE}\n"
        ),
        path,
        &[VALUE],
        &["plain: stays"],
    );
}

#[test]
fn only_a_yaml_secrets_own_blocks_are_hidden() {
    let path = "k8s/a.yaml";
    for text in [
        format!("kind: ConfigMap\ndata:\n  a: {VALUE}\n"),
        // A reference to a Secret beside the data of its parent.
        format!("spec:\n  valueFrom:\n  - kind: Secret\n    name: x\ndata:\n  a: {VALUE}\n"),
        // Another document.
        format!("kind: Secret\nmetadata:\n  name: x\n---\nkind: ConfigMap\ndata:\n  a: {VALUE}\n"),
        // Another kind, or the kind in prose.
        format!("kind: SecretStore\ndata:\n  a: {VALUE}\n"),
        format!("x kind: Secret\ndata:\n  a: {VALUE}\n"),
        format!("description: the kind: Secret one\ndata:\n  a: {VALUE}\n"),
        // Nothing to hide.
        "kind: Secret\ndata: {}\nstringData: null\n".to_owned(),
        "kind: Secret\ndata:\nmetadata:\n  name: x\n".to_owned(),
    ] {
        unchanged(&text, path);
    }
}

/// An unquoted `kty` needs nothing before it in a walk of the text.
#[test]
fn a_key_in_yaml_may_open_the_file() {
    hides(
        &format!("kty: EC\nx: {PUBLIC}\nd: {SCALAR}\n"),
        Some("conf/key.yaml"),
        &[SCALAR],
        &[PUBLIC],
    );
}

/// A whole read cannot rely on its pages' margins when an object's members may
/// be any distance apart, and needs no whole scan when nothing names them.
#[test]
fn a_read_is_scanned_whole_only_when_an_object_says_so() {
    assert!(WholeSourcePatterns::outruns_a_page(
        "{\"kind\": \"Secret\",\n\"a\": 1}"
    ));
    assert!(WholeSourcePatterns::outruns_a_page("{\"kty\": \"EC\"}"));
    for text in [
        "{\"kind\": \"Deployment\", \"data\": {}}",
        "a Secret, and a kind, in prose",
        "\"Secret\": {\"kty_like\": 1}",
    ] {
        assert!(!WholeSourcePatterns::outruns_a_page(text), "{text}");
    }
}

/// A JSON member of any string value may be a Secret's, wherever its source
/// says so: only what cannot be one is settled by its window.
#[test]
fn a_row_of_any_string_member_asks_for_its_source() {
    for line in [
        r#"    "dbpass": "correct horse battery staple""#,
        r#"  "short": "aGVsbG8=","#,
        r#"\"dbpass\":\"correct horse\""#,
    ] {
        assert!(WholeSourcePatterns::shown_by(line).is_some(), "{line}");
    }
    for line in [
        "const b = \"plain\";",
        "  \"count\": 3,",
        "  \"data\": {",
        "a line of text",
    ] {
        assert!(WholeSourcePatterns::shown_by(line).is_none(), "{line}");
    }
}

/// A reader that looks only for [`SourceNames`] decides as the patterns do: it
/// counts every text they name, in any spelling they accept.
#[test]
fn every_named_object_holds_a_literal() {
    for text in [
        "\"kind\": \"Secret\"",
        "\\\"kind\\\":\\\"Secret\\\"",
        "\\\\\\\"kind\\\\\\\":\\\\\\\"Secret\\\\\\\"",
        "\"kty\": 1",
        "'kty': 1",
        "kty: 1",
        "\\\"kty\\\": 1",
    ] {
        assert!(
            WholeSourcePatterns::outruns_a_page(text),
            "the patterns must name this: {text}"
        );
        let mut names = SourceNames::for_path("any");
        names.feed(text.as_bytes());
        assert!(names.finish().objects, "{text}");
    }
}

/// A Kubernetes Secret block scalar with CRLF line ends: past the in-memory
/// limit the text hides what the in-memory pass hides and ends its lines the
/// same way, each hidden line keeping the `\r\n` it had, and a second pass
/// changes nothing.
#[test]
fn secret_block_scalars_keep_their_line_ends_on_the_long_path() {
    let content = "kind: Secret\r\nstringData:\r\n  config: |\r\n    lineOneFake\r\n    lineTwoFake\r\nnext: 1\r\n";
    let scope = Scope::File("app.yaml");
    let in_memory = detect_in(content, scope);
    let long = redact_within(content, scope, 1).expect("a secret is hidden");
    for piece in ["lineOneFake", "lineTwoFake"] {
        assert!(!long.sanitized.contains(piece), "{:?}", long.sanitized);
    }
    assert_eq!(long.sanitized, in_memory.sanitized);
    assert_eq!(
        long.sanitized.matches("\r\n").count(),
        long.sanitized.matches('\n').count()
    );
    assert_eq!(
        long.sanitized.matches('\n').count(),
        content.matches('\n').count()
    );
    assert!(redact_within(&long.sanitized, scope, 1).is_none());
}
