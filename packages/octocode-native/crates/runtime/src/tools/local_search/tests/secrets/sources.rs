//! Values whose secrecy their own lines do not show: a key in an encoding the
//! raw bytes hide, and a credential a whole-file pattern recognizes.

use super::*;

fn search_bytes(name: &str, bytes: &[u8], fields: serde_json::Value) -> serde_json::Value {
    let root = tempfile::tempdir().expect("fixture directory");
    let path = root.path().join(name);
    fs::write(&path, bytes).expect("fixture");
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

fn utf16(text: &str, big_endian: bool) -> Vec<u8> {
    let units = text.encode_utf16();
    if big_endian {
        [0xfe, 0xff]
            .into_iter()
            .chain(units.flat_map(u16::to_be_bytes))
            .collect()
    } else {
        [0xff, 0xfe]
            .into_iter()
            .chain(units.flat_map(u16::to_le_bytes))
            .collect()
    }
}

#[test]
fn a_utf16_key_is_redacted_as_its_utf8_copy_is() {
    for format in crate::security::key_formats::formats() {
        for big_endian in [false, true] {
            let bytes = utf16(&format.text, big_endian);
            for view in ["content", "matchOnly"] {
                for secret in &format.secrets {
                    let span = &secret[..secret.len().min(12)];
                    let found = search_bytes(
                        "notes.txt",
                        &bytes,
                        serde_json::json!({"matchString":[span],"view":view}),
                    );
                    let values = all_values(&found);
                    for hidden in &format.secrets {
                        assert!(
                            !values.contains(hidden.as_str()) && !values.contains(span),
                            "{} {view} big={big_endian}: {found}",
                            format.name
                        );
                    }
                }
            }
        }
    }
}

/// Line numbers and the lines beside a key are the decoded text's.
#[test]
fn a_utf16_rows_context_stays_readable() {
    let key = "intro é\n-----BEGIN RSA PRIVATE KEY-----\nMIIEpQIBAAKCAQEAsplitKeyBodyAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\n-----END RSA PRIVATE KEY-----\ntail\n";
    for big_endian in [false, true] {
        let found = search_bytes(
            "notes.txt",
            &utf16(key, big_endian),
            serde_json::json!({"matchString":["intro|tail"],"regexEngine":"rust","view":"content"}),
        );
        let row = found["files"][0]["matches"].clone();
        assert_eq!(row[0]["value"], "intro é", "{found}");
        assert_eq!(row[1]["line"], 5, "{found}");
        assert_eq!(row[1]["value"], "tail", "{found}");
    }
}

const VALUE: &str = "QUJDREVGR0hJSktMTU5PUFFSU1RVVldYWVowMTIz";

/// Both orders a Secret manifest is written in, with its kind further than any
/// row's window from its values.
fn manifests() -> [String; 2] {
    let labels = (0..40)
        .map(|at| format!("    l{at}: v{at}\n"))
        .collect::<String>();
    [
        format!(
            "apiVersion: v1\nkind: Secret\nmetadata:\n  labels:\n{labels}data:\n  tls.crt: {VALUE}\n  tls.key: {VALUE}2\n"
        ),
        format!(
            "apiVersion: v1\ndata:\n  tls.crt: {VALUE}\n  tls.key: {VALUE}2\nkind: Secret\nmetadata:\n  labels:\n{labels}"
        ),
    ]
}

#[test]
fn a_row_of_a_kubernetes_secret_is_judged_with_the_whole_file() {
    for manifest in manifests() {
        for fields in [
            serde_json::json!({"matchString":["tls.crt"],"view":"content"}),
            serde_json::json!({"matchString":["tls.key"],"view":"content"}),
            serde_json::json!({"matchString":["QUJDREVGR0hJ"],"view":"matchOnly"}),
            serde_json::json!({"matchString":["tls.crt"],"view":"content","contextLines":{"before":2,"after":2}}),
        ] {
            let found = search_bytes("secret.yaml", manifest.as_bytes(), fields.clone());
            assert!(
                !found.to_string().contains(&VALUE[..12]),
                "{fields}: {found}"
            );
            assert!(found["files"][0]["matches"].is_array(), "{found}");
        }
        // The file's other lines stay as they are.
        let found = search_bytes(
            "secret.yaml",
            manifest.as_bytes(),
            serde_json::json!({"matchString":["l7:"],"view":"content"}),
        );
        assert_eq!(
            found["files"][0]["matches"][0]["value"], "    l7: v7",
            "{found}"
        );
        // The pattern is the path's: the same text elsewhere is plain text.
        let found = search_bytes(
            "secret.txt",
            manifest.as_bytes(),
            serde_json::json!({"matchString":["tls.crt"],"view":"content"}),
        );
        assert!(found.to_string().contains(VALUE), "{found}");
    }
}

/// A key in the same manifest does not take the check away from its neighbors.
#[test]
fn a_row_beside_a_key_block_is_still_judged_with_the_whole_file() {
    let manifest = format!(
        "kind: Secret\nstringData:\n  tls.crt: {VALUE}\n  tls.key: |\n    -----BEGIN RSA PRIVATE KEY-----\n    MIIEpQIBAAKCAQEAsplitKeyBodyAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\n    -----END RSA PRIVATE KEY-----\n"
    );
    let found = search_bytes(
        "secret.yaml",
        manifest.as_bytes(),
        serde_json::json!({"matchString":["tls."],"view":"content"}),
    );
    let shown = found.to_string();
    assert!(
        !shown.contains(&VALUE[..12]) && !shown.contains("MIIEpQ"),
        "{found}"
    );
}

/// A YAML file nothing in which starts such a pattern keeps every row, however
/// large; one too large to scan whole that does fails closed.
#[test]
fn a_large_yaml_is_refused_only_when_it_could_hold_such_a_secret() {
    let filler = format!("# {}\n", "p".repeat(78)).repeat(220_000);
    let plain = format!("{filler}needle: {VALUE}\n");
    let found = search_bytes(
        "large.yaml",
        plain.as_bytes(),
        serde_json::json!({"matchString":["needle"],"view":"content"}),
    );
    assert_eq!(
        found["files"][0]["matches"][0]["value"],
        format!("needle: {VALUE}"),
        "{}",
        &found.to_string()[..200]
    );
    let secret = format!("kind: Secret\ndata:\n  tls.crt: {VALUE}\n{filler}");
    let found = search_bytes(
        "large.yaml",
        secret.as_bytes(),
        serde_json::json!({"matchString":["tls.crt"],"view":"content"}),
    );
    assert!(!found.to_string().contains(VALUE), "{found}");
}
