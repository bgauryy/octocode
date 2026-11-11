//! A localFetch page cut from a source judged whole is not judged again as
//! an isolated fragment (B165 D1), and a secret in that source stays
//! redacted in every range.
use crate::support::{Run as _, Workspace};
use serde_json::{Value, json};

const SOURCE: &str = "use serde::Serialize;\n\n#[derive(Serialize)]\nstruct SavingWalk<'a> {\n    pending: &'a [Ref],\n    token: Option<&'a str>,\n    read: &'a [Value],\n    import: &'a Option<String>,\n}\n\nfn main() {}\n";

async fn content(
    runtime: &octocode_native::runtime::ToolRuntime,
    path: &std::path::Path,
    lines: (usize, usize),
) -> String {
    let output: Value = runtime
        .run(
            "judged".into(),
            "localFetch".into(),
            json!({"queries":[{"path": path, "ranges":[{"startLine": lines.0, "endLine": lines.1}]}]}),
        )
        .await
        .expect("read")
        .structured_content
        .into_value();
    output["results"][0]["data"]["content"]
        .as_str()
        .unwrap_or_else(|| panic!("no content: {output}"))
        .to_owned()
}

#[tokio::test]
async fn ranged_reads_keep_the_whole_source_judgement() {
    let workspace = Workspace::new();
    let path = workspace.write("walk.rs", SOURCE);
    let runtime = workspace.runtime(&[]);
    for lines in [(5, 8), (1, 11), (6, 6)] {
        let text = content(&runtime, &path, lines).await;
        assert!(
            text.contains("token: Option<&'a str>,"),
            "{lines:?}: {text}"
        );
        assert!(!text.contains("[REDACTED"), "{lines:?}: {text}");
    }
    runtime.close().await;
}

/// The first string under a `cursor` key anywhere in `value`.
fn cursor_in(value: &Value) -> Option<String> {
    match value {
        Value::Object(map) => map
            .get("cursor")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| map.values().find_map(cursor_in)),
        Value::Array(items) => items.iter().find_map(cursor_in),
        _ => None,
    }
}

/// The `nextPage` continuation anywhere in `value`.
fn next_page(value: &Value) -> Option<&Value> {
    match value {
        Value::Object(map) => map
            .get("nextPage")
            .or_else(|| map.values().find_map(next_page)),
        Value::Array(items) => items.iter().find_map(next_page),
        _ => None,
    }
}

/// Every page of a whole-file read, in order, following `nextPage`.
async fn whole_read_pages(
    runtime: &octocode_native::runtime::ToolRuntime,
    path: &std::path::Path,
    limit: usize,
) -> Vec<String> {
    let mut pages = Vec::new();
    let mut input = json!({"queries":[{"path": path, "limit": limit}]});
    loop {
        let output: Value = runtime
            .run("judged".into(), "localFetch".into(), input)
            .await
            .expect("read")
            .structured_content
            .into_value();
        pages.push(
            output["results"][0]["data"]["content"]
                .as_str()
                .unwrap_or_else(|| panic!("no content: {output}"))
                .to_owned(),
        );
        let Some(cursor) = next_page(&output).and_then(cursor_in) else {
            return pages;
        };
        input = json!({ "cursor": cursor });
    }
}

/// Every page of a whole-file read is cut from the same sealed source as the
/// first, so none is judged again as a fragment (B165 round 8, M1).
#[tokio::test]
async fn every_page_of_a_whole_read_keeps_the_whole_source_judgement() {
    let workspace = Workspace::new();
    let path = workspace.write("walk.rs", SOURCE);
    let runtime = workspace.runtime(&[]);
    let pages = whole_read_pages(&runtime, &path, 5).await;
    assert!(pages.len() >= 2, "{pages:?}");
    assert!(
        pages.concat().contains("token: Option<&'a str>,"),
        "{pages:?}"
    );
    for page in &pages {
        assert!(!page.contains("[REDACTED"), "{page}");
    }
    runtime.close().await;
}

#[tokio::test]
async fn a_secret_in_the_source_stays_redacted_in_every_page_of_a_whole_read() {
    let workspace = Workspace::new();
    let planted = SOURCE.replace(
        "fn main() {}",
        "const PASSWORD: &str = \"hunter2-Xq9!zP4#wL7\";",
    );
    let path = workspace.write("walk.rs", planted);
    let runtime = workspace.runtime(&[]);
    for page in whole_read_pages(&runtime, &path, 5).await {
        assert!(!page.contains("hunter2-Xq9"), "{page}");
    }
    runtime.close().await;
}

#[tokio::test]
async fn a_secret_in_the_source_stays_redacted_in_a_ranged_read() {
    let workspace = Workspace::new();
    let token = format!("ghp_{}", "a1B2".repeat(9));
    let planted = SOURCE.replace(
        "fn main() {}",
        &format!(
            "const TOKEN: &str = \"{token}\";\nconst PASSWORD: &str = \"hunter2-Xq9!zP4#wL7\";"
        ),
    );
    let path = workspace.write("walk.rs", planted);
    let runtime = workspace.runtime(&[]);
    for lines in [(11, 12), (5, 12), (12, 12)] {
        let text = content(&runtime, &path, lines).await;
        assert!(!text.contains(&token), "{lines:?}: {text}");
        assert!(!text.contains("hunter2-Xq9"), "{lines:?}: {text}");
    }
    runtime.close().await;
}

/// One whole read of `path` in `view`: its numbered `content`, by source line.
async fn view_lines(
    runtime: &octocode_native::runtime::ToolRuntime,
    path: &std::path::Path,
    query: Value,
) -> std::collections::BTreeMap<usize, String> {
    let mut query = query;
    query["path"] = json!(path);
    let output: Value = runtime
        .run(
            "judged".into(),
            "localFetch".into(),
            json!({ "queries": [query] }),
        )
        .await
        .expect("read")
        .structured_content
        .into_value();
    output["results"][0]["data"]["content"]
        .as_str()
        .unwrap_or_else(|| panic!("no content: {output}"))
        .lines()
        .filter_map(|line| {
            let (number, text) = line.split_once('\t')?;
            Some((number.parse().ok()?, text.to_owned()))
        })
        .collect()
}

/// The redaction placeholders of `text`, in order.
fn placeholders(text: &str) -> Vec<&str> {
    text.match_indices("[REDACTED")
        .map(|(at, _)| {
            let rest = &text[at..];
            &rest[..=rest.find(']').unwrap_or(rest.len() - 1)]
        })
        .collect()
}

const TYPES: &str = "type Receipt = { id: number; dispatchToken: string };\nexport const x: Receipt = { id: 1, dispatchToken: \"a\" };\n\ninterface Session {\n  accessToken: string;\n  apiKey: string; // the caller's\n  password: string;\n}\n";

/// The sources the two views are compared on: code whose names read like
/// credentials, the same code with planted secrets, and data files
/// (YAML, `NAME=value` lines) with planted secrets. Each with the planted values no view may show.
fn view_fixtures() -> Vec<(&'static str, String, Vec<String>)> {
    let token = format!("ghp_{}", "a1B2".repeat(9));
    let password = "hunter2-Xq9!zP4#wL7";
    vec![
        ("types.ts", TYPES.to_owned(), vec![]),
        (
            "planted.ts",
            format!(
                "{TYPES}\nexport const session = {{\n  accessToken: \"{token}\",\n  password: \"{password}\",\n}};\n"
            ),
            vec![token.clone(), password.to_owned()],
        ),
        (
            "planted.rs",
            format!(
                "{SOURCE}\nconst TOKEN: &str = \"{token}\";\n\n// spare\nconst PASSWORD: &str = \"{password}\";\n"
            ),
            vec![token.clone(), password.to_owned()],
        ),
        (
            "deploy.yaml",
            format!(
                "service:\n  name: api\n\n  api_token: {token}\n  password: \"{password}\"\n  replicas: 2\n"
            ),
            vec![token.clone(), password.to_owned()],
        ),
        (
            "service.conf",
            format!("PORT=8080\n\nGITHUB_TOKEN={token}\nDB_PASSWORD={password}\n"),
            vec![token, password.to_owned()],
        ),
    ]
}

/// One redaction decision, made on the source, is what every view shows: on
/// each source line the compact view keeps, it shows the placeholders the
/// content view shows there and no other, whole or in a range, and neither
/// view shows a planted secret.
#[tokio::test]
async fn compact_and_content_views_redact_the_same_values() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    for (name, source, secrets) in view_fixtures() {
        let path = workspace.write(name, &source);
        let last = source.lines().count();
        for range in [None, Some((2, last.saturating_sub(1).max(2)))] {
            let read = |view: &str| {
                let mut query = json!({ "view": view });
                if let Some((start, end)) = range {
                    query["ranges"] = json!([{ "startLine": start, "endLine": end }]);
                }
                query
            };
            let content = view_lines(&runtime, &path, read("content")).await;
            let compact = view_lines(&runtime, &path, read("compact")).await;
            assert!(!compact.is_empty(), "{name} {range:?}: no compact lines");
            for (line, text) in &compact {
                let verbatim = content
                    .get(line)
                    .unwrap_or_else(|| panic!("{name} {range:?}: line {line} not in content"));
                assert_eq!(
                    placeholders(text),
                    placeholders(verbatim),
                    "{name} {range:?} line {line}: compact `{text}`, content `{verbatim}`"
                );
            }
            for view in [&content, &compact] {
                for (line, text) in view {
                    for secret in &secrets {
                        assert!(
                            !text.contains(secret),
                            "{name} {range:?} line {line}: {text}"
                        );
                    }
                }
            }
            if secrets.is_empty() {
                assert!(
                    content.values().all(|text| placeholders(text).is_empty()),
                    "{name} {range:?}: {content:?}"
                );
            } else {
                assert!(
                    content.values().any(|text| !placeholders(text).is_empty()),
                    "{name} {range:?}: nothing redacted: {content:?}"
                );
            }
        }
    }
    runtime.close().await;
}

/// Every page of a compact read is cut from the one compact view, so no page
/// is judged again as a fragment.
#[tokio::test]
async fn every_page_of_a_compact_read_keeps_the_whole_source_judgement() {
    let workspace = Workspace::new();
    let source = TYPES.repeat(40);
    let path = workspace.write("types.ts", &source);
    let runtime = workspace.runtime(&[]);
    let mut pages = Vec::new();
    let mut input = json!({"queries":[{"path": path, "view": "compact", "limit": 7}]});
    loop {
        let output: Value = runtime
            .run("judged".into(), "localFetch".into(), input)
            .await
            .expect("read")
            .structured_content
            .into_value();
        pages.push(
            output["results"][0]["data"]["content"]
                .as_str()
                .unwrap_or_else(|| panic!("no content: {output}"))
                .to_owned(),
        );
        let Some(cursor) = next_page(&output).and_then(cursor_in) else {
            break;
        };
        input = json!({ "cursor": cursor });
    }
    assert!(pages.len() >= 2, "{pages:?}");
    for page in &pages {
        assert!(!page.contains("[REDACTED"), "{page}");
    }
    assert!(
        pages.concat().contains("dispatchToken: string"),
        "{pages:?}"
    );
    runtime.close().await;
}

/// Sources that lay a credential assignment out as code does: a comment
/// between the name, the operator and the value, the value on the lines after
/// its name, a numbered name. Each with the planted values, every one written
/// in code, so a view that keeps the statement shows a placeholder for it.
fn layout_fixtures() -> Vec<(&'static str, String, Vec<&'static str>)> {
    let token = "Zq7xKp2mVw9tLb4nRc8dYf3hJs6g";
    let other = "Hd5Jf0GsAeUoIx7Kq2Lm9Zp4Vw8R";
    let password = "hunter2-Xq9!zP4#wL7";
    vec![
        (
            "commented.ts",
            format!(
                "export const config = {{\n  /* block */ dbPassword /* c1 */ : /* c2 */ \"{password}\" /* c3 */,\n}};\nconst apiKey =\n  // the key\n  \"{token}\";\nconst authToken2 = \"{other}\";\n"
            ),
            vec![password, token, other],
        ),
        (
            "commented.rs",
            format!(
                "const SERVICE_API_KEY: &str = /* inline */ \"{token}\";\n\nfn connect() -> String {{\n    let db_password = String::from(\n        // key\n        \"{password}\",\n    );\n    db_password\n}}\n"
            ),
            vec![token, password],
        ),
        (
            "wrapped.py",
            format!(
                "SERVICE_API_KEY = (\n    # wrapped\n    \"{token}\"\n)\ndb_password = \\\n    \"{password}\"\n"
            ),
            vec![token, password],
        ),
        (
            "nested.yaml",
            format!(
                "service:\n  api_key:\n    {token}\n  password:   # rotated\n    {password}\n  replicas: 2\n"
            ),
            vec![token, password],
        ),
    ]
}

/// One read of `path` with `query`: the whole response as text, and its
/// `content` again as the reader sees it, so a value with a quote or a
/// backslash compares as written. A refused read (a view the file has none
/// of) is its error's text: a refusal shows no value either.
async fn response_text(
    runtime: &octocode_native::runtime::ToolRuntime,
    path: &std::path::Path,
    query: Value,
) -> String {
    let mut query = query;
    query["path"] = json!(path);
    let output: Value = match runtime
        .run(
            "judged".into(),
            "localFetch".into(),
            json!({ "queries": [query] }),
        )
        .await
    {
        Ok(output) => output.structured_content.into_value(),
        Err(error) => return format!("{error:?}"),
    };
    let mut text = output.to_string();
    if let Some(content) = output["results"][0]["data"]["content"].as_str() {
        text.push('\n');
        text.push_str(content);
    }
    text
}

/// The assignment is judged once, on the source, and through the comments
/// and line breaks inside it: no view shows a planted value anywhere in its
/// response, whole or in a range, and the compact view, which drops those
/// comments, redacts every value the content view does.
#[tokio::test]
async fn no_view_shows_a_value_assigned_through_comments_or_line_breaks() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    let planted = view_fixtures()
        .into_iter()
        .map(|(name, source, secrets)| (name, source, secrets.len(), secrets));
    let layouts = layout_fixtures()
        .into_iter()
        .map(|(name, source, secrets)| {
            let count = secrets.len();
            let secrets = secrets.into_iter().map(str::to_owned).collect();
            (name, source, count, secrets)
        });
    for (name, source, count, secrets) in layouts.chain(planted) {
        let path = workspace.write(name, &source);
        let last = source.lines().count();
        for range in [None, Some((2, last.saturating_sub(1).max(2)))] {
            let read = |view: &str| {
                let mut query = json!({ "view": view });
                if let Some((start, end)) = range {
                    query["ranges"] = json!([{ "startLine": start, "endLine": end }]);
                }
                query
            };
            let content = response_text(&runtime, &path, read("content")).await;
            let compact = response_text(&runtime, &path, read("compact")).await;
            let symbols = response_text(&runtime, &path, read("symbols")).await;
            for (view, text) in [
                ("content", &content),
                ("compact", &compact),
                ("symbols", &symbols),
            ] {
                for secret in &secrets {
                    assert!(
                        !text.contains(secret.as_str()),
                        "{name} {range:?} {view} shows {secret}: {text}"
                    );
                }
            }
            if range.is_none() {
                let redacted = |text: &str| placeholders(text).len();
                assert!(
                    redacted(&content) >= count,
                    "{name}: content redacts {} of {count}: {content}",
                    redacted(&content)
                );
                assert!(
                    redacted(&compact) >= redacted(&content),
                    "{name}: compact redacts {}, content {}: {compact}",
                    redacted(&compact),
                    redacted(&content)
                );
            }
        }
    }
    runtime.close().await;
}
