use super::*;
use serde_json::json;

fn local_row(lines: usize, width: usize) -> Value {
    let text: String = (1..=lines)
        .map(|line| format!("{line:05}{}\n", "x".repeat(width)))
        .collect();
    json!({"path":"a.txt","content":text,"totalLines":lines})
}

fn query(extra: Value) -> Value {
    let mut query = json!({"path":"a.txt"});
    for (key, value) in extra.as_object().into_iter().flatten() {
        query[key] = value.clone();
    }
    query
}

#[test]
fn source_rows_share_the_page_less_the_envelope_and_others_split_it() {
    assert_eq!(
        row_window(ToolId::LocalFetch, 20_000, 3),
        (20_000 - ENVELOPE_CHARS) / 3
    );
    assert_eq!(
        row_window(ToolId::GhGetFileContent, 20_000, 1),
        20_000 - ENVELOPE_CHARS
    );
    assert_eq!(row_window(ToolId::LocalSearch, 20_000, 4), 5_000);
    assert_eq!(row_window(ToolId::LocalFetch, 100, 0), 1);
}

#[test]
fn a_row_within_its_share_keeps_its_page() {
    assert_eq!(
        fitted_lines(&local_row(10, 20), &query(json!({})), 10_000),
        None
    );
    let explicit = local_row(7, 60);
    assert_eq!(
        fitted_lines(&explicit, &query(json!({"length":7})), 20_000),
        None
    );
}

#[test]
fn a_row_over_its_share_keeps_the_lines_that_fit() {
    let share = row_window(ToolId::LocalFetch, 20_000, 3);
    let lines = fitted_lines(&local_row(300, 60), &query(json!({})), share).expect("cut");
    assert!((50..300).contains(&lines), "{lines}");
    let row = local_row(lines, 60);
    let continued = row_chars(&row)
        + octocode_engine::json_size::utf16_units(&query(json!({}))).unwrap_or(0)
        + CONTINUATION_EXTRA_CHARS;
    assert!(continued <= share, "{continued} > {share}");
}

#[test]
fn byte_pages_are_not_reshaped() {
    for asked in [
        json!({"unit": "bytes", "length": 300}),
        json!({"fullContent": true, "offset": 4096}),
        json!({"unit": "bytes", "offset": 10}),
    ] {
        assert_eq!(
            fitted_lines(&local_row(300, 60), &query(asked.clone()), 6_000),
            None,
            "{asked}"
        );
    }
    let mut paged = local_row(300, 60);
    paged["pagination"] = json!({"unit": "bytes"});
    assert_eq!(fitted_lines(&paged, &query(json!({})), 6_000), None);
}

/// A ranked row its continuation cannot replay, and one line, are
/// indivisible.
#[test]
fn indivisible_rows_keep_their_page() {
    let mut ranked = local_row(900, 60);
    ranked["selection"] =
        json!({"status":"complete","scope":"selectedView","selectedItems":2,"totalItems":2});
    assert_eq!(fitted_lines(&ranked, &query(json!({})), 20_000), None);
    ranked["next"] = json!({"continueWindow":{"tool":"localFetch","query":{"queries":[{
        "path":"a.txt", "offset":900, "selection":"native-plan"
    }]}}});
    assert!(fitted_lines(&ranked, &query(json!({})), 20_000).is_some());
    assert_eq!(
        fitted_lines(&local_row(1, 60_000), &query(json!({})), 20_000),
        None
    );
    let mut failed = local_row(900, 60);
    failed["error"] = json!("gone");
    assert_eq!(fitted_lines(&failed, &query(json!({})), 20_000), None);
}

fn read_local(
    query: &Value,
    policy: &crate::policy::path::PathPolicy,
    security: &crate::security::ContentSecurity,
    window: Option<usize>,
) -> Value {
    let prepared =
        crate::contracts::prepare_many_and_validate("localFetch", json!({"queries":[query]}))
            .expect("canonical query");
    let request = serde_json::from_value(prepared[0].clone()).expect("query");
    let result = crate::tools::local_fetch::execute_local_fetch(
        &request,
        policy,
        security,
        &crate::execution::cancel::NeverCancel,
        window,
        None,
    );
    assert_eq!(
        result.status,
        crate::tools::source_view::ReadStatus::Success,
        "{result:?}"
    );
    serde_json::to_value(result).expect("result")
}

/// The lines of every page of a walk from `query`, each read with its
/// share of `window` (`None`: the reader's own page), and the pages.
fn walk_local(
    mut query: Value,
    policy: &crate::policy::path::PathPolicy,
    window: Option<usize>,
) -> (Vec<String>, usize) {
    let security = crate::security::ContentSecurity::new();
    let share = window.map(|window| row_window(ToolId::LocalFetch, window, 1));
    let mut records = Vec::new();
    for pages in 1..=200 {
        let row = read_local(&query, policy, &security, share);
        let mut wire = crate::tools::test_support::published(ToolId::LocalFetch, &query, &row);
        crate::response::numbered::number_read_rows(ToolId::LocalFetch, &mut wire);
        crate::tools::test_support::checks::assert_output_contract("localFetch", &wire);
        if let Some(window) = window {
            let size = octocode_engine::json_size::utf16_units(&wire).unwrap_or(0);
            assert!(size <= window, "{size} > {window}");
        }
        let data = &wire["results"][0]["data"];
        records.extend(
            data["content"]
                .as_str()
                .expect("content")
                .lines()
                .filter(|line| *line != "--")
                .map(str::to_owned),
        );
        let Some(next) = data["next"].get("continueWindow") else {
            return (records, pages);
        };
        let prepared =
            crate::contracts::prepare_many_and_validate("localFetch", next["query"].clone())
                .expect("canonical continuation");
        query = prepared[0].clone();
    }
    panic!("read failed to terminate");
}

#[test]
fn single_short_line_reads_fit_the_numbered_json_window_and_copy_every_page() {
    let root = crate::tools::test_support::Fixture::new();
    let path = root.write("short.txt", "x\n".repeat(12_000));
    let policy = crate::tools::test_support::workspace_policy(&root.0);
    for window in [20_000, 50_000] {
        for extra in [
            json!({"fullContent":true}),
            json!({"unit":"lines","length":50_000}),
            json!({"ranges":["1-3000","9001-12000"]}),
        ] {
            let mut query = json!({"path":path});
            query
                .as_object_mut()
                .expect("object")
                .extend(extra.as_object().expect("extra").clone());
            let (expected, _) = walk_local(query.clone(), &policy, None);
            let (actual, _) = walk_local(query, &policy, Some(window));
            assert_eq!(actual, expected);
            assert_eq!(
                actual
                    .iter()
                    .collect::<std::collections::BTreeSet<_>>()
                    .len(),
                actual.len()
            );
        }
    }
}

/// Every view of a late-match source pages identically walked at the
/// reader's own page size and walked through shares of each of
/// `windows`. Its escape-heavy lines put a byte-sized page over its
/// share, so every share is met by the reader's fit.
fn late_and_transformed_pages_keep_source_coordinates(windows: &[usize]) {
    let root = crate::tools::test_support::Fixture::new();
    let literal = format!("\"{}🙂\"", "\\".repeat(16));
    let source: String = (1..=5_000)
        .map(|line| {
            if line <= 1_000 {
                format!("// omitted {line}\n")
            } else if line % 3 == 0 {
                format!("const needle_{line} = {literal};\n")
            } else {
                format!("const value_{line} = {literal};\n")
            }
        })
        .collect();
    let path = root.write("views.ts", source);
    let policy = crate::tools::test_support::workspace_policy(&root.0);
    for &window in windows {
        for extra in [
            json!({"matchString":"needle", "contextLines":1}),
            json!({"minify":"standard", "unit":"lines"}),
            json!({"minify":"standard", "ranges":["1100-2800","3500-5000"]}),
        ] {
            let mut query = json!({"path":path});
            query
                .as_object_mut()
                .expect("object")
                .extend(extra.as_object().expect("extra").clone());
            let (expected, _) = walk_local(query.clone(), &policy, None);
            let (actual, _) = walk_local(query, &policy, Some(window));
            assert!(!actual.is_empty());
            assert_eq!(actual, expected);
            assert_eq!(
                actual
                    .iter()
                    .collect::<std::collections::BTreeSet<_>>()
                    .len(),
                actual.len()
            );
        }
    }
}

/// The default run walks one window; the full matrix (ignored: `cargo
/// test -- --ignored full_matrix`) walks both.
#[test]
fn late_match_and_transformed_line_pages_keep_their_source_coordinates() {
    late_and_transformed_pages_keep_source_coordinates(&[20_000]);
}

#[test]
#[ignore = "full matrix: cargo test -- --ignored full_matrix"]
fn late_match_and_transformed_line_pages_keep_their_source_coordinates_full_matrix() {
    late_and_transformed_pages_keep_source_coordinates(&[20_000, 50_000]);
}
