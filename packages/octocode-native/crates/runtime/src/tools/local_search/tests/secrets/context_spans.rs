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
        for pattern in [
            "blob.*ghp_Z9Q8",
            "blob.*ghp_",
            "Qm.*ghp_Z",
            "Qm[A-Za-z0-9]+",
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
            assert!(!shown.is_empty(), "{name} {pattern}: a row is shown");
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
