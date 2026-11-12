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
        // D1 (round 13): a block scalar's body is its key's value in every
        // view, its indentation and header staying.
        (
            "db.yaml",
            "db:\n  host: localhost\n\n  password: |\n    Fz8kQ2mL9xW4\n  api_key: >-\n    Qm7vR2xL8pZ3\n  port: 5432\n"
                .to_owned(),
            vec!["Fz8kQ2mL9xW4".to_owned(), "Qm7vR2xL8pZ3".to_owned()],
        ),
        // D3: code whose literal under `password` is no secret, and a
        // parameter default beside it, holds no placeholder in any view.
        (
            "b.py",
            "password = 'abc'\n\ndef f(x=\"abc\"):\n    return x\n".to_owned(),
            vec![],
        ),
        // D4: a template literal whose text outside its holes is a scheme
        // word is code; the holes' own strings are code too.
        (
            "transport.ts",
            "export function auth(h: Headers, u: string, p: string) {\n  const scheme = 'Basic';\n  h.Authorization = `Basic ${Buffer.from(`${u}:${p}`).toString('base64')}`;\n  return h;\n}\n"
                .to_owned(),
            vec![],
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
        // Final round, item 1: a credential-named literal its line ends
        // inside, at a real line break or a continuing backslash, is one
        // value with the lines it goes on over.
        (
            "open.py",
            format!(
                "import os\n\ndef f(api_key=\"{token}\"):\n    return api_key\ntoken = \"{other}\n\"\nsecret = \"{password}\\\nmore\"\n"
            ),
            vec![token, other, password],
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

/// Round 12 fixtures: a named value in an expression, a literal joined to
/// text, a plain scalar with text after its token, prose that states a
/// value, a type annotation and the handshake nonce. Each with the values no
/// view may show and the text every view keeps.
/// One round 12 fixture: file name, source, a search needle, the values no
/// view may show and the text every view keeps.
type BoundFixture = (
    &'static str,
    String,
    &'static str,
    Vec<&'static str>,
    Vec<&'static str>,
);

fn bound_fixtures() -> Vec<BoundFixture> {
    const KEY: &str = "Zq7Xk2Pm9Vt4Lw8Rb3Nc6Hy1Jd5Fg0Ts7Qe2Ua4";
    vec![
        (
            "expr.ts",
            format!(
                "const apiKey = cond ? \"{KEY}\" : \"otherVisible3\";\nconst apiKey3 = \"{KEY}\" + \"tail-visible1\";\nconst k2 = 1; // my password was Tr0ub4dor&3xK9!zQ\n"
            ),
            "apiKey",
            vec![KEY, "Tr0ub4dor&3xK9!zQ"],
            vec!["otherVisible3", "tail-visible1", "my password was "],
        ),
        (
            "conf.py",
            format!("api_key = \"{KEY}\" + suffix_visible5\nother = \"x\"\n"),
            "api_key",
            vec![KEY],
            vec!["suffix_visible5"],
        ),
        (
            "notes.txt",
            "Sec-Fake-Api-Key: ghp_x7Kq2Lm9Zp4Vw8Rt1Yb6Nc3Hd5Jf0GsAeUoI  tailJ\npassword: str\ndispatchToken: string\nurl = postgres://user:password@localhost\n"
                .to_owned(),
            "Key",
            vec!["ghp_x7Kq2Lm9Zp4Vw8Rt1Yb6Nc3Hd5Jf0GsAeUoI"],
            vec!["tailJ", "password: str", "dispatchToken: string", "user:password@"],
        ),
        (
            "ws.ts",
            "const headers = { \"Sec-WebSocket-Key\": \"dGhlIHNhbXBsZSBub25jZQ==\" };\n".to_owned(),
            "Sec",
            vec![],
            vec!["dGhlIHNhbXBsZSBub25jZQ=="],
        ),
        // Round 12 retest: an expression over lines, a chosen password in
        // an arm, the Bearer token in every layout.
        (
            "r12.ts",
            format!(
                "const apiKey5 =\n  cond\n    ? \"{KEY}\"\n    : \"otherVisible17\";\nconst password = isProd ? \"Tr0ub4dor&3xK9!zQ\" : \"x\";\nconst dbPassword = c ? \"x\" : \"hunter2-Sup3rS3cret\";\nheaders[\"Authorization\"] = \"Bearer abcdefghijklmnopqrstuvwx\";\nconst h = {{ Authorization: `Bearer abcdefghijklmnopqrstuvwx` }};\n"
            ),
            "assword",
            vec![KEY, "Tr0ub4dor&3xK9!zQ", "hunter2-Sup3rS3cret", "abcdefghijklmnopqrstuvwx"],
            vec!["otherVisible17", "Authorization\"] = \"Bearer ", "{ Authorization: `Bearer "],
        ),
        (
            "r12.go",
            format!(
                "package a\n\nvar apiKey = map[bool]string{{true: \"{KEY}\", false: \"otherVisible11\"}}[cond]\n"
            ),
            "apiKey",
            vec![KEY],
            vec!["otherVisible11", "map[bool]string{true: \""],
        ),
        (
            "r12.sh",
            format!("API_KEY=\"${{API_KEY:-{KEY}}}\"\n"),
            "API_KEY",
            vec![KEY],
            vec!["API_KEY=\"${API_KEY:-"],
        ),
        // Prose that states a password, a value and the word after it, code
        // a text file quotes.
        (
            "r12.txt",
            "my password was Pl4net-Orb1t9\nthe password is: hunter2-Sup3rS3cret\nthen password = hunter2-Sup3rS3cret\nthe password is: s3cretPassw0rd\nthe password is required\nSIGNING_KEY=x-9f8.6d0.818 84c.7d6.59a\nAUTH_TOKEN=q7.x2.k9 m4.p8.z1\ntoken: T | null\n  token: this.token,\n  password: undefined,\n  apiKey: null,\n"
                .to_owned(),
            "assword",
            vec!["Pl4net-Orb1t9", "hunter2-Sup3rS3cret", "s3cretPassw0rd", "x-9f8.6d0.818"],
            vec![
                // D3 (bin16 retest): an unquoted value ends at its blank.
                "] 84c.7d6.59a",
                "my password was ",
                "the password is required",
                "AUTH_TOKEN=q7.x2.k9 m4.p8.z1",
                "token: T | null",
                "token: this.token,",
                "password: undefined,",
                "apiKey: null,",
            ],
        ),
        // Round 14, B: a diff's hunks are judged as the text they change: a
        // block scalar's body on `+` and context lines, and a Secret's data in
        // its own document, go; the ConfigMap's value and every marker stay.
        (
            "x.txt",
            "diff --git a/app.yaml b/app.yaml\n--- a/app.yaml\n+++ b/app.yaml\n@@ -1,3 +1,11 @@\n config:\n   token: |-\n     Ab3Cd9Ef2Gh7Jk5Lm1\n+  token: |-\n+    Hk3Lm9Pq2Rs7Tv5Wx1\n+---\n+kind: ConfigMap\n+data:\n+  cacheKeyPrefix: app-cache\n+---\n+kind: Secret\n+data:\n+  username: YWRtaW4=\n"
                .to_owned(),
            "token",
            vec!["Ab3Cd9Ef2Gh7Jk5Lm1", "Hk3Lm9Pq2Rs7Tv5Wx1", "YWRtaW4="],
            vec!["+  token: |-", "+    [REDACTED-", "+  cacheKeyPrefix: app-cache", "+  username: [REDACTED-"],
        ),
        // C: a Secret's rule holds within its own document, in a text
        // indented as a whole too.
        (
            "manifest.txt",
            "  kind: ConfigMap\n  data:\n    cacheKeyPrefix: app-cache\n  ---\n  kind: Secret\n  data:\n    username: YWRtaW4=\n"
                .to_owned(),
            "cacheKeyPrefix",
            vec!["YWRtaW4="],
            vec!["cacheKeyPrefix: app-cache", "username: [REDACTED-"],
        ),
        // F: a literal's escaped line ends its value, and a format argument
        // is no value: templates and sentences show whole.
        (
            "fields.rs",
            "fn f(symbols: &str) -> Vec<String> {\n    let npm_token = format!(\"//registry.npmjs.org/:_authToken={TOKEN}\\n\");\n    let prose = [\n        \"the password is required\\n\",\n        \"the password is <your-password>\\n\",\n    ];\n    vec![format!(\"the password is {symbols}\\n\"), npm_token]\n}\n"
                .to_owned(),
            "password",
            vec![],
            vec![
                "format!(\"//registry.npmjs.org/:_authToken={TOKEN}\\n\");",
                "\"the password is required\\n\",",
                "\"the password is <your-password>\\n\",",
                "format!(\"the password is {symbols}\\n\")",
            ],
        ),
        // Final round, item 2: a literal's config entry loses its value
        // alone; the registry, the key and the escape stay.
        (
            "npm.py",
            format!("x_token = \"//registry.npmjs.org/:_authToken={KEY}\\n\"\n"),
            "x_token",
            vec![KEY],
            vec!["x_token = \"//registry.npmjs.org/:_authToken=[REDACTED-", "]\\n\""],
        ),
        // D and H: a dunder name's camelCase word, and a sentence about a
        // value under a credential's name, read as written.
        (
            "vite.ts",
            "export const ssrImportKey = `__vite_ssr_import__`\nexport const ssrExportAllKey = `__vite_ssr_exportAll__`\n"
                .to_owned(),
            "Key",
            vec![],
            vec!["`__vite_ssr_exportAll__`", "`__vite_ssr_import__`"],
        ),
        (
            "rules.txt",
            "password: must be at least 8 characters\npassword: |\n  must be set by the operator\n"
                .to_owned(),
            "password",
            vec![],
            vec!["password: must be at least 8 characters", "  must be set by the operator"],
        ),
    ]
}

/// One redaction decision per line, whatever view shows it: a read, the
/// compact view, a search's content and matchOnly rows and an AST match show
/// the same placeholders on a line, none shows a planted value, and each
/// keeps the text around the value.
#[tokio::test]
async fn every_view_bounds_a_value_as_a_read_does() {
    use crate::support::{call, query_path};
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    for (name, source, needle, secrets, kept) in bound_fixtures() {
        let path = workspace.write(name, &source);
        let content = view_lines(&runtime, &path, json!({"view": "content"})).await;
        let read = content.values().cloned().collect::<Vec<_>>().join("\n");
        for secret in &secrets {
            assert!(!read.contains(secret), "{name}: {read}");
        }
        for text in &kept {
            assert!(read.contains(text), "{name} lost {text:?}: {read}");
        }
        let compact = view_lines(&runtime, &path, json!({"view": "compact"})).await;
        // The compact view drops comments: a line it keeps whole shows the
        // read's placeholders, and one it cut shows the first of them only.
        for (line, text) in &compact {
            let (shown, read) = (placeholders(text), placeholders(&content[line]));
            if text.trim() == content[line].trim() {
                assert_eq!(shown, read, "{name} compact line {line}");
            } else {
                assert!(
                    read.starts_with(&shown),
                    "{name} compact line {line}: {text:?}"
                );
            }
        }
        let mut responses = Vec::new();
        for view in ["content", "matchOnly"] {
            let query = query_path(&path, json!({"matchString": needle, "view": view}));
            responses.push(call(&runtime, "localSearch", query).await.expect("search"));
        }
        let ast =
            json!({"operation": "match", "rule": {"kind": "string"}, "includeCaptures": true});
        if !name.ends_with(".txt") {
            responses.push(
                call(&runtime, "astSearch", query_path(&path, ast))
                    .await
                    .expect("ast"),
            );
        }
        for outcome in responses {
            let output = outcome.structured_content.into_value();
            let text = output.to_string();
            for secret in &secrets {
                assert!(!text.contains(secret), "{name}: {output}");
            }
            let mut rows = Vec::new();
            collect_rows(&output, &mut rows);
            for (line, value) in rows {
                let Some(whole) = content.get(&line) else {
                    continue;
                };
                for piece in value.split('\n') {
                    assert!(
                        whole.contains(piece) || piece.is_empty(),
                        "{name} line {line}: a view shows {piece:?}, the read {whole:?}"
                    );
                }
            }
        }
    }
    runtime.close().await;
}

/// Every `(line, value)` a response holds in an object that states its
/// `line` beside a `value` or `text`.
fn collect_rows(value: &Value, out: &mut Vec<(usize, String)>) {
    match value {
        Value::Array(items) => items.iter().for_each(|item| collect_rows(item, out)),
        Value::Object(fields) => {
            let line = fields
                .get("line")
                .and_then(Value::as_u64)
                .and_then(|line| usize::try_from(line).ok());
            for key in ["value", "text"] {
                if let (Some(line), Some(text)) = (line, fields.get(key).and_then(Value::as_str)) {
                    out.push((line, text.to_owned()));
                }
            }
            fields.values().for_each(|field| collect_rows(field, out));
        }
        _ => {}
    }
}

/// A minified source of one long line, its pages cut by bytes: the object
/// literal jQuery passes to `ce.each` names `altKey`, `ctrlKey`, `metaKey`
/// and `key` before code that is no value (`!0`, a call). Round 13: the
/// operator-stated word after `altKey:` took the rest of the line, and page
/// 3's continuation lost its snapshot (`outputContractViolation`), so the
/// bytes past it were unreachable. Every page reads, each continuation is
/// valid output, and the pages together hold every byte of the source.
#[tokio::test]
async fn a_minified_source_walks_to_its_end() {
    const EVENT_KEYS: &str = "ce.each({altKey:!0,bubbles:!0,cancelable:!0,changedTouches:!0,ctrlKey:!0,detail:!0,eventPhase:!0,metaKey:!0,pageX:!0,pageY:!0,shiftKey:!0,view:!0,\"char\":!0,code:!0,charCode:!0,key:!0,keyCode:!0,button:!0,buttons:!0,which:!0},ce.event.addProp),ce.each({focus:\"focusin\",blur:\"focusout\"},function(r,i){function o(e){var t=_.get(this,\"handle\"),n=ce.event.fix(e);n.type=\"focusin\"===e.type?\"focus\":\"blur\",t(e)}});";
    let mut source = String::new();
    let mut part = 0;
    while source.len() < 120_000 {
        source.push_str(&format!("var a{part}=function(e){{return e.x{part}+1}};"));
        if part % 300 == 0 {
            source.push_str(EVENT_KEYS);
        }
        part += 1;
    }
    let workspace = Workspace::new();
    let path = workspace.write("vendor/lib.min.js", &source);
    let runtime = workspace.runtime(&[]);
    let mut input = json!({"queries":[{"path": path}]});
    let mut read = String::new();
    let mut pages = 0;
    loop {
        assert!(pages < 200, "the walk ends");
        let output: Value = runtime
            .run(format!("minified-{pages}"), "localFetch".into(), input)
            .await
            .unwrap_or_else(|error| panic!("page {pages}: {error:?}"))
            .structured_content
            .into_value();
        assert!(
            output["results"][0]["data"].get("error").is_none(),
            "page {pages}: {output}"
        );
        let content = output["results"][0]["data"]["content"]
            .as_str()
            .unwrap_or_else(|| panic!("page {pages}: no content: {output}"));
        assert!(!content.contains("[REDACTED"), "page {pages}: {content}");
        read.push_str(content);
        pages += 1;
        let Some(cursor) = next_page(&output).and_then(cursor_in) else {
            break;
        };
        input = json!({ "cursor": cursor });
    }
    assert!(pages >= 3, "{pages} pages");
    assert_eq!(read, source, "the pages hold every byte of the source");
    runtime.close().await;
}

/// H1: the detector's own source, where comments name credentials around
/// backticks and code mentions `secret` beside other words, reads as written:
/// no value runs from a backtick in a comment into the code lines below it,
/// and `secret == "~"` compares with a mark, no secret.
#[tokio::test]
async fn code_that_names_credentials_in_comments_reads_as_written() {
    const SOURCE: &str = "}\n\n/// Whether `secret` after `key` is a sentence: after `name:` several words\n/// are prose (`password: must be at least 8 characters`); after `NAME=` they\n/// are a value.\npub(super) fn is_sentence(key: &impl KeyFacts, secret: &str) -> bool {\n    key.ends_with_colon() && secret.contains(char::is_whitespace)\n}\n\n/// Characters of a comment's first word after `name:` that make it a secret\n/// whatever words follow it.\nconst COMMENT_OPAQUE_WORD_CHARS: usize = 8;\n\nfn is_non_string_scalar(secret: &str) -> bool {\n    secret == \"~\"\n        || [\"true\", \"false\", \"null\"]\n            .iter()\n            .any(|keyword| secret.eq_ignore_ascii_case(keyword))\n}\n";
    let workspace = Workspace::new();
    let path = workspace.write("src/credential_values.rs", SOURCE);
    let runtime = workspace.runtime(&[]);
    let lines = SOURCE.lines().count();
    let text = content(&runtime, &path, (1, lines)).await;
    assert!(!text.contains("[REDACTED"), "{text}");
    for line in SOURCE.lines().filter(|line| !line.is_empty()) {
        assert!(text.contains(line), "{line:?} lost: {text}");
    }
    runtime.close().await;
}

/// D3 (round 13): a view's own warnings are the tool's text, never a
/// file's: compact and symbols reads state them verbatim, and a read that
/// replaced nothing states no redaction.
#[tokio::test]
async fn a_views_own_warnings_read_as_written() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    let plain = workspace.write("b.py", "password = 'abc'\ndef f(x=\"abc\"):\n");
    let planted = workspace.write(
        "s.py",
        "import os\nGITHUB_TOKEN = \"ghp_a1B2a1B2a1B2a1B2a1B2a1B2a1B2a1B2a1B2\"  # pat\ndef f(api_key=\"Zq7Xk2Pm9Vt4Lw8Rb3Nc6Hy1Jd5Fg0Ts7Qe2Ua4\"):\n    return api_key\ntoken = \"Zq7Xk2Pm9Vt4Lw8Rb3Nc6Hy1Jd5Fg0Ts7Qe2Ua4\n\"\n",
    );
    for (path, view) in [
        (&plain, "compact"),
        (&planted, "compact"),
        (&planted, "symbols"),
    ] {
        let output: Value = runtime
            .run(
                "warnings".into(),
                "localFetch".into(),
                json!({"queries":[{"path": path, "view": view}]}),
            )
            .await
            .expect("read")
            .structured_content
            .into_value();
        let data = &output["results"][0]["data"];
        let warnings: Vec<&str> = data["warnings"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        for warning in &warnings {
            assert!(!warning.contains("[REDACTED-"), "{view}: {warning}");
        }
        let content = data["content"].as_str().unwrap_or_default();
        let redacted = warnings
            .iter()
            .any(|warning| warning.starts_with("redactedContent"));
        assert_eq!(
            redacted,
            content.contains("[REDACTED-"),
            "{view} {path:?}: {warnings:?} {content}"
        );
    }
    runtime.close().await;
}

/// Every page of the localFetch read `query` starts, in order: each page's
/// row, until no `nextPage` remains. A page that fails (round 13: the gate
/// rewrote the continuation's `snapshot`, `outputContractViolation`) fails
/// the walk.
async fn walk_read(
    runtime: &octocode_native::runtime::ToolRuntime,
    label: &str,
    query: Value,
) -> Vec<Value> {
    let mut input = json!({"queries":[query]});
    let mut rows = Vec::new();
    loop {
        assert!(rows.len() < 2_000, "{label}: the walk ends");
        let output: Value = runtime
            .run(
                format!("{label}-{}", rows.len()),
                "localFetch".into(),
                input,
            )
            .await
            .unwrap_or_else(|error| panic!("{label} page {}: {error:?}", rows.len()))
            .structured_content
            .into_value();
        assert!(
            output.get("error").is_none() && output["results"][0]["data"].get("error").is_none(),
            "{label} page {}: {output}",
            rows.len()
        );
        let next = next_page(&output).and_then(cursor_in);
        rows.push(output["results"][0]["data"].clone());
        let Some(cursor) = next else {
            return rows;
        };
        input = json!({ "cursor": cursor });
    }
}

/// The source line numbers the pages of a line walk show, in order.
fn walked_lines(rows: &[Value]) -> Vec<usize> {
    rows.iter()
        .filter_map(|row| row["content"].as_str())
        .flat_map(str::lines)
        .filter_map(|line| line.split_once('\t')?.0.parse().ok())
        .collect()
}

/// Round 13, D1: a page with a continuation whose text the detector reads
/// as a credential's context (an assignment, then a JSON object line) had
/// its continuation's `snapshot` hidden. Each walk reaches its end, and the
/// pages show every line exactly once, in order.
#[tokio::test]
async fn a_credential_shaped_page_keeps_its_continuation() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    let small = workspace.write("t.md", "TOKEN=abc\n{\"a\":\"b\"}\nx\n");
    let constructors = workspace.write(
        "c.rs",
        "let a = Token::new(\"\");\nlet b = Secret::new(\"\");\nlet c = Password::from(\"\");\nlet d = ApiKeys::new(\"x\");\nfn e() {}\nfn f() {}\n",
    );
    for (label, path, lines) in [("small", &small, 3), ("constructors", &constructors, 6)] {
        for limit in [1, 2] {
            let rows = walk_read(&runtime, label, json!({"path": path, "limit": limit})).await;
            assert_eq!(
                walked_lines(&rows),
                (1..=lines).collect::<Vec<_>>(),
                "{label} limit {limit}: {rows:?}"
            );
        }
    }
    runtime.close().await;
}

/// The two real files the round-13 retest failed on, walked whole: plain
/// line pages, `ranges` with `limit`, and `matchString` pages.
#[tokio::test]
async fn real_sources_the_retest_failed_on_walk_to_their_end() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..");
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    for (name, source, term) in [
        ("CONFIGURATION.md", "docs/CONFIGURATION.md", "token"),
        (
            "credential_values.rs",
            "packages/octocode-native/crates/engine/src/security/detector/credential_values.rs",
            "secret",
        ),
    ] {
        let text = std::fs::read_to_string(root.join(source)).expect("repository source");
        let path = workspace.write(name, &text);
        let total = text.lines().count();
        let rows = walk_read(&runtime, name, json!({"path": path, "limit": 40})).await;
        assert_eq!(
            walked_lines(&rows),
            (1..=total).collect::<Vec<_>>(),
            "{name}"
        );
        let end = total.min(400);
        let ranged = walk_read(
            &runtime,
            name,
            json!({"path": path, "ranges": [{"startLine": 1, "endLine": end}], "limit": 25}),
        )
        .await;
        assert_eq!(
            walked_lines(&ranged),
            (1..=end).collect::<Vec<_>>(),
            "{name} ranges"
        );
        let matched = walk_read(
            &runtime,
            name,
            json!({"path": path, "matchString": term, "limit": 10}),
        )
        .await;
        let lines = walked_lines(&matched);
        assert!(
            !lines.is_empty() && lines.windows(2).all(|pair| pair[0] < pair[1]),
            "{name} matchString: {lines:?}"
        );
    }
    runtime.close().await;
}

/// Every response `tool` gives for `query`, its rows' text included.
async fn tool_text(
    runtime: &octocode_native::runtime::ToolRuntime,
    tool: &str,
    query: Value,
) -> String {
    let output: Value = runtime
        .run("armor".into(), tool.into(), json!({ "queries": [query] }))
        .await
        .unwrap_or_else(|error| panic!("{tool}: {error:?}"))
        .structured_content
        .into_value();
    output.to_string()
}

/// Owner rule (round 14, item 3): a key block's armor names the block and
/// holds none of it, so every view shows the armor lines as written, a
/// literal's closing quote too, and no view shows a byte of the body: a
/// read, the compact view, a search's content and matchOnly rows, the
/// symbols outline, and a byte page of a source over the whole-file
/// ceiling. A block with no END still hides its body.
#[tokio::test]
async fn every_view_shows_a_key_blocks_armor_and_none_of_its_body() {
    const BEGIN: &str = "-----BEGIN RSA PRIVATE KEY-----";
    const END: &str = "-----END RSA PRIVATE KEY-----";
    let bodies = [
        "MIIEowIBAAKCAQEAfakeArmorBodyLineOneAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        "Zq7Xk2Pm9Vt4Lw8Rb3Nc6Hy1Jd5Fg0Ts7Qe2Ua4fakeArmorBodyLineTwoBBBBB",
    ];
    let block = format!("{BEGIN}\n{}\n{}\n{END}\n", bodies[0], bodies[1]);
    let quoted = format!("private_key: \"{BEGIN}\"");
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    let text = workspace.write("keys.txt", format!("{quoted}\n{block}tail\n"));
    let open = workspace.write(
        "open.txt",
        format!("head\n{BEGIN}\n{}\n{}\n", bodies[0], bodies[1]),
    );
    let code = workspace.write(
        "pem.ts",
        format!(
            "export const pem = `{}`;\nexport function tail() {{ return 1; }}\n",
            block.trim_end()
        ),
    );
    let mut large = "let filler_value = 0;\n".repeat(11 * 1024 * 1024 / 22);
    let large_lines = large.lines().count();
    large.push_str(&block);
    large.push_str("tail\n");
    let large = workspace.write("large.txt", large);
    // Each read, and the armor text it must show: a matchOnly row shows its
    // span alone, and a search for body text shows no armor at all.
    let reads = [
        ("localFetch", json!({"path": text}), Some(BEGIN)),
        (
            "localFetch",
            json!({"path": text, "view": "compact"}),
            Some(BEGIN),
        ),
        (
            "localFetch",
            json!({"path": text, "ranges": [{"startLine": 2, "endLine": 5}]}),
            Some(BEGIN),
        ),
        ("localFetch", json!({"path": open}), Some(BEGIN)),
        ("localFetch", json!({"path": code}), Some(BEGIN)),
        (
            "localFetch",
            json!({"path": code, "view": "compact"}),
            Some(BEGIN),
        ),
        ("localFetch", json!({"path": code, "view": "symbols"}), None),
        (
            "localSearch",
            json!({"path": workspace.workspace, "matchString": ["PRIVATE KEY"], "view": "content"}),
            Some(BEGIN),
        ),
        (
            "localSearch",
            json!({"path": workspace.workspace, "matchString": ["PRIVATE KEY"], "view": "matchOnly"}),
            Some("PRIVATE KEY"),
        ),
        (
            "localSearch",
            json!({"path": workspace.workspace, "matchString": ["BEGIN RSA PRIVATE KEY-----\\n.*"], "multiline": "on", "regexEngine": "rust", "view": "matchOnly"}),
            None,
        ),
        (
            "localSearch",
            json!({"path": workspace.workspace, "matchString": ["fakeArmorBody"], "view": "content"}),
            None,
        ),
        (
            "localSearch",
            json!({"path": workspace.workspace, "matchString": ["fakeArmorBody"], "view": "matchOnly"}),
            None,
        ),
        (
            "astSearch",
            json!({"operation": "symbols", "path": code}),
            None,
        ),
        (
            "localFetch",
            json!({"path": large, "ranges": [{"startLine": large_lines - 1, "endLine": large_lines + 6}]}),
            Some(BEGIN),
        ),
    ];
    for (tool, query, armor) in reads {
        let shown = tool_text(&runtime, tool, query.clone()).await;
        for body in bodies {
            assert!(!shown.contains(&body[..24]), "{tool} {query}: {shown}");
        }
        if let Some(armor) = armor {
            assert!(
                shown.contains(armor),
                "{tool} {query}: the armor is shown: {shown}"
            );
        }
    }
    let read = tool_text(&runtime, "localFetch", json!({"path": text})).await;
    assert!(
        read.contains(&quoted.replace('"', "\\\"")),
        "the quoted armor reads whole: {read}"
    );
    assert!(read.contains(END), "{read}");
    runtime.close().await;
}

/// The detector's own source states an armor in a comment, a quote and code
/// after it (`` `-----BEGIN RSA PRIVATE KEY-----`): ``): the line reads whole.
#[tokio::test]
async fn a_comment_that_quotes_an_armor_reads_whole() {
    let line = "/// Whether `secret` is one PEM armor line (`-----BEGIN RSA PRIVATE KEY-----`):";
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    let path = workspace.write("armor.rs", format!("{line}\nfn is_armor_line() {{}}\n"));
    let text = content(&runtime, &path, (1, 2)).await;
    assert!(text.contains(line), "{text}");
    runtime.close().await;
}
