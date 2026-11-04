//! Every private-key format through `localSearch`: a body line searched by
//! its own text never comes back, in content or matchOnly.

use super::*;

use crate::security::key_formats as formats;

#[test]
fn a_body_line_searched_by_its_text_never_comes_back() {
    for format in formats::formats() {
        for secret in &format.secrets {
            // A whole body line, and a short span of one.
            let span = &secret[..secret.len().min(12)];
            for (needle, view) in [
                (secret.as_str(), "content"),
                (secret.as_str(), "matchOnly"),
                (span, "content"),
                (span, "matchOnly"),
            ] {
                let found = search_fixture(
                    &[("notes.txt", format.text.as_str())],
                    ls_query(
                        serde_json::json!({"matchString":[needle],"view":view}),
                        None,
                    ),
                );
                let values = all_values(&found);
                for hidden in &format.secrets {
                    assert!(
                        !values.contains(hidden.as_str()) && !values.contains(span),
                        "{} {view}: {found}",
                        format.name
                    );
                }
            }
        }
    }
}

#[test]
fn a_late_rfc4716_body_in_a_large_source_never_comes_back() {
    let all = formats::formats();
    let format = all
        .iter()
        .find(|format| format.name == "rfc4716 encrypted")
        .expect("format");
    let filler = "let filler_value = 0;\n".repeat(560_000);
    let content = format!("{filler}{}", format.text);
    for view in ["matchOnly", "content"] {
        let found = search_fixture(
            &[("big.txt", content.as_str())],
            ls_query(
                serde_json::json!({"matchString":[format.secrets[1]],"view":view}),
                None,
            ),
        );
        assert!(!all_values(&found).contains(&format.secrets[1]), "{found}");
    }
}

/// Lines around a key stay readable when matched, in the file that holds it.
#[test]
fn a_near_miss_marker_leaves_its_lines_searchable() {
    let run = "QUJDREVGR0hJSktMTU5PUFFSU1RVVldYWVowMTIzNDU2Nzg5QUJDREVGR0hJSktM";
    for miss in formats::NOT_MARKERS {
        let file = format!("{miss}\n{run}\n");
        let found = search_fixture(
            &[("notes.txt", file.as_str())],
            ls_query(
                serde_json::json!({"matchString":[run],"view":"content"}),
                None,
            ),
        );
        assert!(all_values(&found).contains(run), "{miss:?}: {found}");
    }
}
