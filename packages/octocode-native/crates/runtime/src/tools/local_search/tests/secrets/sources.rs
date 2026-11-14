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
