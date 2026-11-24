use super::*;
use octocode_contract_types::{ErrorCode, LocalFetchQuery};
use octocode_runtime_base::execution::cancel::NeverCancel;
use octocode_runtime_base::security::ContentSecurity;
use octocode_test_support::json_fixture;
use serde_json::json;
use std::path::Path;

fn query(fields: serde_json::Value) -> LocalFetchQuery {
    let mut row = json_fixture(r#"{"path": "/fixture/app.ts"}"#);
    row.as_object_mut()
        .expect("query")
        .extend(fields.as_object().expect("fields").clone());
    serde_json::from_value(row).expect("canonical reader query")
}

fn fetch_window(source: &str, q: &LocalFetchQuery, window: usize) -> LocalFetchResult {
    process_fetched_content(
        q,
        source.as_bytes(),
        Path::new("/fixture/app.ts"),
        &SourceFacts {
            window: Some(window),
            ..SourceFacts::default()
        },
        &ContentSecurity::new(),
        &NeverCancel,
    )
}

fn filler(n: usize) -> String {
    (1..=n)
        .map(|i| format!("const filler_{i} = {i};\n"))
        .collect()
}

fn walk(source: &str, mut q: LocalFetchQuery, window: usize) -> (String, usize) {
    let mut joined = String::new();
    let mut pages = 0;
    loop {
        let result = fetch_window(source, &q, window);
        assert!(result.error.is_none(), "{result:?}");
        joined.push_str(result.content.as_deref().expect("content"));
        pages += 1;
        assert!(pages < source.len() + 2, "progress must terminate");
        let Some(next) = result.next.and_then(|next| next.continue_window) else {
            break;
        };
        assert_ne!(next.query, q, "continuation must advance");
        if local_fetch_query::unit(&next.query) == local_fetch_query::unit(&q) {
            assert!(
                local_fetch_query::offset(&next.query).unwrap_or(0)
                    > local_fetch_query::offset(&q).unwrap_or(0)
            );
        }
        q = next.query;
    }
    (joined, pages)
}

#[test]
fn default_read_and_explicit_ranges_reconstruct_complete_source() {
    let source = filler(1_200);
    for fields in [
        json!({}),
        json_fixture(r#"{"ranges": [{"startLine": 1, "endLine": 1200}]}"#),
    ] {
        let (actual, pages) = walk(&source, query(fields), 1200);
        assert_eq!(actual, source);
        assert!(pages > 1);
    }
}

#[test]
fn an_unanchored_read_uses_the_response_budget_and_small_files_finish() {
    let source = filler(1_200);
    let first = fetch_window(&source, &query(json!({})), 50_000);
    assert_eq!(first.content.as_deref(), Some(source.as_str()));
    assert!(first.next.is_none());
    assert_eq!(first.returned_lines, Some(1200));
    let small = filler(10);
    let out = fetch_window(&small, &query(json!({})), 1200);
    assert_eq!(out.content.as_deref(), Some(small.as_str()));
    assert!(out.next.is_none());
}

/// A misremembered declaration head is served as the file's one
/// declaration of its name, and the warning names only the name that line
/// declares: a pattern that is no head is not called a declaration.
#[test]
fn a_misremembered_head_names_only_the_declared_name_it_served() {
    let source = "export function parseEnv(text: string) {\n  return text;\n}\nparseEnv('x');\n";
    let read = fetch_window(
        source,
        &query(json!({"matchString": ["const parseEnv(", "parseEnv(zz"]})),
        50_000,
    );
    assert_eq!(
        read.warnings.first().map(String::as_str),
        Some(
            "No line holds `const parseEnv(`, `parseEnv(zz`: served the declaration of `parseEnv` at line 1 instead."
        ),
        "{read:?}"
    );
}

/// The name of a misremembered head counts whole identifiers only: `emitt`
/// inside `EventEmitter` is another name, so the hint counts, and its read
/// selects, only the line that uses `emitt` itself.
#[test]
fn a_misremembered_heads_name_counts_whole_identifiers_only() {
    let source = "use(emitt);\nuse(EventEmitter);\n";
    let read = fetch_window(
        source,
        &query(json!({"matchString": ["function emitt("]})),
        50_000,
    );
    assert_eq!(
        read.hints,
        ["No line holds it as written; 1 line matches `emitt`: the related[] call reads them."],
        "{read:?}"
    );
    let named = read
        .next
        .as_ref()
        .and_then(|next| next.read.as_ref())
        .expect("the name's read");
    let served = fetch_window(source, &named.query, 50_000);
    let text = served.content.as_deref().unwrap_or_default();
    assert!(
        served.selected_match_count == Some(1)
            && text.contains("use(emitt);")
            && !text.contains("EventEmitter"),
        "{served:?}"
    );
    // A name the file holds only inside longer identifiers offers no read of it.
    let inside = fetch_window(
        "use(EventEmitter);\n",
        &query(json!({"matchString": ["function emitt("]})),
        50_000,
    );
    assert!(
        inside.hints.iter().all(|hint| !hint.contains("`emitt`")),
        "{inside:?}"
    );
}

mod fit;
mod pages;
mod scan_memo;
mod scan_refusal;
