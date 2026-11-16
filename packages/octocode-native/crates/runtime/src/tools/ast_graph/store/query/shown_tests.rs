//! Byte provenance of a source a read hides any of: no row states a digest,
//! span, origin or occurrence id computed over its own text (each would test
//! a guess of the hidden value), while a source that hides nothing keeps
//! them, and a file changed since the build is still reported stale.
use super::*;
use crate::execution::cancel::NeverCancel;
use crate::security::ContentSecurity;
use crate::tools::ast_graph::store::{IngestOptions, ingest};
use crate::tools::test_support::{Fixture, workspace_policy};

/// A fabricated GitHub token: secret-shaped, never a real credential.
const TOKEN: &str = "ghp_Fake0123456789abcdefFAKE0123456789ab";

fn sources() -> (String, String) {
    let secret = format!(
        "const token = \"{TOKEN}\";\nexport function leaky() {{ return helper(token); }}\nfunction helper(value: string) {{ return missingSink(value); }}\n"
    );
    let clean = "export function clean() { return absentSink(1); }\n".to_owned();
    (secret, clean)
}

fn built() -> Fixture {
    let dir = Fixture::new();
    let (secret, clean) = sources();
    dir.write("app/secret.ts", &secret);
    dir.write("app/clean.ts", &clean);
    let out = ingest(
        &IngestOptions {
            path: dir.0.join("app"),
            workspace: Some(dir.0.to_path_buf()),
            ..Default::default()
        },
        &workspace_policy(&dir.0),
        &ContentSecurity::new(),
        &NeverCancel,
        None,
    );
    assert_eq!(out.class, ExitClass::Success, "{}", out.value);
    dir
}

fn ask(dir: &Fixture, op: &str, target: Option<&str>, provenance: bool) -> Value {
    let options = QueryOptions {
        op: op.into(),
        target: target.map(str::to_owned),
        workspace: Some(dir.0.to_path_buf()),
        check_freshness: true,
        include_provenance: provenance,
        ..Default::default()
    };
    query_cancellable(&options, &workspace_policy(&dir.0), &NeverCancel).value
}

/// Rows of `output` (its `rows`, and a point answer's `target` or `node`).
fn rows_of<'a>(output: &'a Value, file: &str) -> Vec<&'a Value> {
    output["rows"]
        .as_array()
        .into_iter()
        .flatten()
        .chain(output.get("target"))
        .chain(output.get("node"))
        .filter(|row| {
            row["path"]
                .as_str()
                .is_some_and(|path| path.ends_with(file))
                || row["id"].as_str().is_some_and(|id| id.contains(file))
        })
        .collect()
}

const PROVENANCE: [&str; 9] = [
    "sourceDigest",
    "sourceRange",
    "sourceOrigin",
    "selectionByteRange",
    "selectionOrigin",
    "bindingSourceRange",
    "bindingOrigin",
    "recipient",
    "occurrenceId",
];

#[test]
fn a_source_with_a_hidden_value_states_no_digest_or_span_of_its_own_text() {
    let dir = built();
    let (secret, clean) = sources();
    let raw = octocode_engine::digest::sha256(secret.as_bytes());
    let clean_digest = octocode_engine::digest::sha256(clean.as_bytes());
    for (op, target) in [
        ("unresolved", None),
        ("witnesses", None),
        ("node", Some("secret.ts#helper")),
        ("symbols", Some("secret.ts")),
    ] {
        for provenance in [true, false] {
            let out = ask(&dir, op, target, provenance);
            let text = out.to_string();
            assert!(!text.contains(TOKEN), "{op}: {text}");
            assert!(
                !text.contains(&raw),
                "{op}: the raw digest is shown: {text}"
            );
            assert!(
                !text.contains(&format!("\"byteLength\":{}", secret.len())),
                "{op}: the raw length is shown: {text}"
            );
            let rows = rows_of(&out, "secret.ts");
            assert!(!rows.is_empty(), "{op}: {text}");
            for row in rows {
                for field in PROVENANCE {
                    assert!(row.get(field).is_none(), "{op}: {field} on {row}");
                }
                if row.get("edge").is_some() {
                    assert!(row.get("id").is_none(), "{op}: an occurrence id: {row}");
                }
                if row["kind"] != "file" {
                    assert!(
                        row["line"].as_u64().is_some(),
                        "{op}: placed by line: {row}"
                    );
                }
            }
        }
    }
    // A source that hides nothing keeps its provenance.
    let out = ask(&dir, "unresolved", None, true);
    let rows = rows_of(&out, "clean.ts");
    assert!(!rows.is_empty(), "{out}");
    for row in rows {
        assert_eq!(row["sourceDigest"], clean_digest, "{row}");
        assert!(row["sourceRange"].is_object(), "{row}");
        assert!(row.get("id").is_some(), "{row}");
    }
}

/// What a snapshot states of a source is decided when it was built: an edit
/// is seen as staleness, never as a change of what rows show. The source that
/// held a hidden value still states nothing of it once edited or removed, and
/// the one that held none keeps its provenance.
#[test]
fn what_rows_state_is_fixed_at_build_and_an_edit_is_stale() {
    let dir = built();
    let (secret, clean) = sources();
    let out = ask(&dir, "unresolved", None, true);
    assert!(out.get("freshness").is_none(), "{out}");
    dir.write(
        "app/secret.ts",
        secret.replace(TOKEN, "ghp_Fake9876543210abcdefFAKE9876543210zz"),
    );
    std::fs::remove_file(dir.0.join("app/clean.ts")).expect("remove");
    let out = ask(&dir, "unresolved", None, true);
    assert_eq!(out["freshness"]["fresh"], false, "the edit is seen: {out}");
    let text = out.to_string();
    assert!(!text.contains(TOKEN), "{text}");
    assert!(
        !text.contains(&octocode_engine::digest::sha256(secret.as_bytes())),
        "{text}"
    );
    for row in rows_of(&out, "secret.ts") {
        for field in PROVENANCE {
            assert!(row.get(field).is_none(), "{field} on {row}");
        }
    }
    let rows = rows_of(&out, "clean.ts");
    assert!(!rows.is_empty(), "{out}");
    for row in rows {
        assert_eq!(
            row["sourceDigest"],
            octocode_engine::digest::sha256(clean.as_bytes()),
            "{row}"
        );
    }
}
