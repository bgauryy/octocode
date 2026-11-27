//! `output.mcpResult` end to end on a real runtime: the same calls an MCP
//! session runs (`Channel::Mcp`), answered through [`reply`] in each format.
//! Unset, the reply is the JSON projection; `text` holds the same evidence
//! once, and its continuation lines run unchanged.
use super::*;
use octocode_native::runtime::{Channel, HostOptions, ToolRuntime};
use octocode_runtime_base::response::wire::wire_json;
use serde_json::json;
use std::collections::BTreeMap;

/// Pages a walk may take before it counts as not ending.
const WALK_BOUND: usize = 400;
/// Lines of the paged file: several pages at [`PAGE_CHARS`].
const FILE_LINES: usize = 150;
/// A response size that pages the fixture's file and its search.
const PAGE_CHARS: u64 = 700;
/// Files of the search fixture and hits in each.
const SEARCH_FILES: [&str; 3] = ["src/a/one.rs", "src/a/two.rs", "src/b/three.rs"];
const HITS_PER_FILE: usize = 24;

fn file_line(number: usize) -> String {
    format!("row {number:03} of the fixture")
}

/// A token in a format the output gate redacts, built here so the source
/// holds none.
fn secret() -> String {
    format!("ghp_{}", "a1B2".repeat(9))
}

/// One MCP session: a workspace, a private home and the runtime of
/// `output.mcpResult` = `format` (`None`: unset).
struct Session {
    _home: tempfile::TempDir,
    _repo: tempfile::TempDir,
    runtime: ToolRuntime,
    calls: u64,
}

/// One reply, and the JSON projection of the outcome it was made from.
struct Answer {
    text: String,
    is_error: bool,
    json: String,
}

impl Session {
    fn new(format: Option<&str>) -> Self {
        let home = tempfile::tempdir().expect("home");
        let repo = tempfile::tempdir().expect("repo");
        let root = repo.path();
        std::fs::create_dir_all(root.join(".git")).expect("git");
        for dir in ["src/a", "src/b"] {
            std::fs::create_dir_all(root.join(dir)).expect("dir");
        }
        let lines: Vec<String> = (1..=FILE_LINES).map(file_line).collect();
        std::fs::write(root.join("src/big.txt"), lines.join("\n") + "\n").expect("file");
        for path in SEARCH_FILES {
            std::fs::write(root.join(path), search_source(path)).expect("source");
        }
        std::fs::write(
            root.join("src/secret.txt"),
            format!("before\ntoken = \"{}\"\nafter\n", secret()),
        )
        .expect("secret");
        let mut env = BTreeMap::from([
            (
                "OCTOCODE_HOME".to_owned(),
                home.path().to_string_lossy().into_owned(),
            ),
            ("OCTOCODE_ENABLE_LOCAL".to_owned(), "true".to_owned()),
            ("PATH".to_owned(), "/usr/bin:/bin".to_owned()),
        ]);
        if let Some(format) = format {
            env.insert("OCTOCODE_MCP_RESULT".to_owned(), format.to_owned());
        }
        let runtime = ToolRuntime::from_host(HostOptions {
            cwd: Some(root.to_path_buf()),
            env: Some(env),
            ..crate::cli::mcp_server::mcp_host()
        })
        .expect("runtime");
        Self {
            _home: home,
            _repo: repo,
            runtime,
            calls: 0,
        }
    }

    fn format(&self) -> OutputMcpResult {
        self.runtime.config().resolved.output.mcp_result
    }

    /// One `tools/call` as the handler runs it in process.
    async fn call(&mut self, tool: &str, input: Value) -> Answer {
        self.calls += 1;
        let result = match self.runtime.admit_with(format!("mcp-{}", self.calls), None) {
            Ok(admission) => {
                self.runtime
                    .execute(admission, tool.to_owned(), input.clone(), Channel::Mcp)
                    .await
            }
            Err(error) => Err(error),
        };
        let json = match &result {
            Ok(outcome) => wire_json(&outcome.structured_content),
            Err(error) => wire_json(&tool_error_result(tool, error).envelope),
        };
        let McpResult { text, is_error } =
            reply(tool, result, self.format(), &input).expect("an answer");
        Answer {
            text,
            is_error,
            json,
        }
    }

    /// Every reply of `tool` from `input` to the last page, each next call
    /// read from the reply itself by `next`.
    async fn walk(
        &mut self,
        tool: &str,
        input: Value,
        next: impl Fn(&str) -> Option<Value>,
    ) -> Vec<String> {
        let mut pages = Vec::new();
        let mut input = Some(input);
        while let Some(arguments) = input.take() {
            assert!(pages.len() < WALK_BOUND, "the walk of {tool} does not end");
            let answer = self.call(tool, arguments).await;
            assert!(!answer.is_error, "{}", answer.text);
            if let Some(call) = next(&answer.text) {
                assert_eq!(call["tool"], json!(tool), "{}", answer.text);
                input = Some(call["input"].clone());
            }
            pages.push(answer.text);
        }
        pages
    }

    async fn close(self) {
        self.runtime.close().await;
    }
}

/// A source file of two functions, [`HITS_PER_FILE`] `needle` lines in all.
fn search_source(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path).replace(".rs", "");
    let half = HITS_PER_FILE / 2;
    let mut source = String::new();
    for (function, from) in [("first", 0), ("second", half)] {
        source.push_str(&format!("fn {function}_{name}() {{\n"));
        for hit in from..from + half {
            source.push_str(&format!("    needle({hit}, \"{name}\");\n"));
        }
        source.push_str("}\n");
    }
    source
}

/// The next call a JSON reply holds: the response's, else the row's.
fn next_in_json(text: &str) -> Option<Value> {
    let reply: Value = serde_json::from_str(text).expect("a JSON reply");
    reply
        .pointer("/next")
        .or_else(|| reply.pointer("/next"))
        .cloned()
}

/// The value of the `# label: value` line of a text reply.
fn field<'a>(text: &'a str, label: &str) -> Option<&'a str> {
    text.lines()
        .filter_map(|line| line.strip_prefix("# "))
        .find_map(|line| line.strip_prefix(label)?.strip_prefix(": "))
}

/// The next call a text reply holds: its `# next` line, the reply's one
/// continuation. Its value is the call, as JSON.
fn next_in_text(text: &str) -> Option<Value> {
    let value = field(text, "next")?;
    Some(serde_json::from_str(value).unwrap_or_else(|error| panic!("{error}: {value}")))
}

/// `N<TAB>text` as `(N, text)`.
fn numbered(line: &str) -> Option<(usize, String)> {
    let (number, text) = line.split_once('\t')?;
    Some((number.parse().ok()?, text.to_owned()))
}

/// The source lines of a JSON `localFetch` reply.
fn fetched_in_json(text: &str) -> Vec<(usize, String)> {
    let reply: Value = serde_json::from_str(text).expect("a JSON reply");
    let content = reply["results"][0]["data"]["content"]
        .as_str()
        .expect("content");
    content
        .lines()
        .map(|line| numbered(line).expect("a numbered line"))
        .collect()
}

/// The source lines of a text `localFetch` reply: the lines under
/// `content: |` up to the first field after them.
fn fetched_in_text(text: &str) -> Vec<(usize, String)> {
    text.lines()
        .skip_while(|line| *line != "content: |")
        .skip(1)
        .map_while(numbered)
        .collect()
}

/// A search hit: its file, its line and the line's text.
type Hit = (String, usize, String);

/// The hits of a JSON `localSearch` reply.
fn hits_in_json(text: &str) -> Vec<Hit> {
    let reply: Value = serde_json::from_str(text).expect("a JSON reply");
    let files = reply["results"][0]["data"]["files"].as_array();
    let mut hits = Vec::new();
    for file in files.into_iter().flatten() {
        let path = file["path"].as_str().expect("path");
        for hit in file["matches"].as_array().into_iter().flatten() {
            let line = usize::try_from(hit["line"].as_u64().expect("line")).expect("line");
            let value = hit["value"].as_str().expect("value");
            hits.push((path.to_owned(), line, value.to_owned()));
        }
    }
    hits
}

/// The hits of a text `localSearch` reply, read by the rules of the text:
/// `# root: dir` names the directory the paths are relative to; a line that
/// is a path opens a file whose hits follow as `N:text`; a lone hit is
/// `path:N:text`; a `  [ … ]` note after a hit is not part of its text.
fn hits_in_text(text: &str) -> Vec<Hit> {
    let root = field(text, "root");
    let full = |path: &str| match root {
        Some(root) => format!("{root}/{path}"),
        None => path.to_owned(),
    };
    let without_note = |text: &str| match text.rsplit_once("  [") {
        Some((text, note)) if note.ends_with(']') => text.to_owned(),
        _ => text.to_owned(),
    };
    let line_first = |line: &str| {
        let (number, text) = line.split_once(':')?;
        Some((number.parse::<usize>().ok()?, without_note(text)))
    };
    let mut hits = Vec::new();
    let mut file = None;
    for line in text.lines() {
        if line.is_empty() || line.starts_with("# ") {
            continue;
        }
        if let Some((number, text)) = line_first(line) {
            let path: &String = file.as_ref().expect("a hit follows its file");
            hits.push((path.clone(), number, text));
        } else if let Some((path, hit)) = line.split_once(".rs:") {
            let (number, text) = line_first(hit).expect("a lone hit");
            hits.push((full(&format!("{path}.rs")), number, text));
        } else {
            file = Some(full(&without_note(line)));
        }
    }
    hits
}

/// V3: with no setting the format is `json`, the default, and the reply is
/// the JSON projection of the outcome byte for byte, a failed row's and a
/// rejected call's included.
#[tokio::test]
async fn the_default_reply_is_the_json_projection() {
    assert_eq!(OutputMcpResult::default(), OutputMcpResult::Json);
    let mut session = Session::new(None);
    assert_eq!(session.format(), OutputMcpResult::default());
    let calls = [
        json!({"queries": [{"path": "src/big.txt"}], "responseChars": PAGE_CHARS}),
        json!({"queries": [{"path": "src/absent.txt"}]}),
        json!({"queries": [{"path": "src/big.txt", "notAField": 1}]}),
    ];
    for input in calls {
        let answer = session.call("localFetch", input).await;
        assert_eq!(answer.text.as_bytes(), answer.json.as_bytes());
    }
    session.close().await;
}

/// V2: the value on the `# next` line of a paged text reply is the
/// call: it parses as JSON, and its `input` sent as the next call's
/// arguments returns the page that follows.
#[tokio::test]
async fn a_text_continuation_runs_unchanged() {
    let mut session = Session::new(Some("text"));
    assert_eq!(session.format(), OutputMcpResult::Text);
    let input = json!({"queries": [{"path": "src/big.txt"}], "responseChars": PAGE_CHARS});
    let first = session.call("localFetch", input).await;
    let value =
        field(&first.text, "next").unwrap_or_else(|| panic!("a next page in\n{}", first.text));
    let call: Value = serde_json::from_str(value).expect("the call is JSON");
    // The same call the JSON projection of this outcome holds.
    let held: Value = serde_json::from_str(&first.json).expect("JSON");
    assert_eq!(Some(&call), held.pointer("/next"));
    assert_eq!(call["tool"], "localFetch");
    let second = session.call("localFetch", call["input"].clone()).await;
    assert!(!second.is_error, "{}", second.text);
    let (before, after) = (fetched_in_text(&first.text), fetched_in_text(&second.text));
    let last = before.last().expect("a first page").0;
    assert!(last < FILE_LINES, "the file takes more than one page");
    assert_eq!(after.first().map(|(number, _)| *number), Some(last + 1));
    session.close().await;
}

/// V1, `localFetch`: every page walked in `json` and in `text` yields the
/// file's lines with their numbers, each once and in order.
#[tokio::test]
async fn a_file_walks_to_the_same_lines_in_both_formats() {
    let input = || json!({"queries": [{"path": "src/big.txt"}], "responseChars": PAGE_CHARS});
    let expected: Vec<(usize, String)> = (1..=FILE_LINES).map(|n| (n, file_line(n))).collect();
    let mut json_session = Session::new(None);
    let pages = json_session.walk("localFetch", input(), next_in_json).await;
    assert!(pages.len() > 1, "the file takes more than one page");
    let lines: Vec<_> = pages
        .iter()
        .flat_map(|page| fetched_in_json(page))
        .collect();
    assert_eq!(lines, expected);
    json_session.close().await;

    let mut text_session = Session::new(Some("text"));
    let text_pages = text_session.walk("localFetch", input(), next_in_text).await;
    assert_eq!(text_pages.len(), pages.len(), "the same pages");
    let lines: Vec<_> = text_pages
        .iter()
        .flat_map(|page| fetched_in_text(page))
        .collect();
    assert_eq!(lines, expected);
    // The file's size is stated in both.
    let total = format!("totalLines: {FILE_LINES}");
    assert!(text_pages[0].contains(&total), "{}", text_pages[0]);
    text_session.close().await;
}

/// V1, `localSearch`: every page walked in `json` and in `text` yields the
/// same hits (file, line, text), each once: every `needle` line of the
/// fixture.
#[tokio::test]
async fn a_search_walks_to_the_same_hits_in_both_formats() {
    let input = || {
        json!({"queries": [{"path": "src", "matchString": ["needle"]}],
            "responseChars": PAGE_CHARS})
    };
    let mut json_session = Session::new(None);
    let pages = json_session
        .walk("localSearch", input(), next_in_json)
        .await;
    assert!(pages.len() > 1, "the search takes more than one page");
    let mut expected: Vec<Hit> = pages.iter().flat_map(|page| hits_in_json(page)).collect();
    json_session.close().await;
    assert_eq!(expected.len(), SEARCH_FILES.len() * HITS_PER_FILE);

    let mut text_session = Session::new(Some("text"));
    let text_pages = text_session
        .walk("localSearch", input(), next_in_text)
        .await;
    let mut hits: Vec<Hit> = text_pages
        .iter()
        .flat_map(|page| hits_in_text(page))
        .collect();
    text_session.close().await;
    // Each once in each format, and the same ones: order within the walk is
    // each format's own page order, which the sort removes.
    expected.sort();
    hits.sort();
    assert_eq!(hits, expected);
    hits.dedup();
    assert_eq!(hits.len(), expected.len(), "no hit twice");
    for path in SEARCH_FILES {
        let of_file = hits.iter().filter(|(file, _, _)| file == path).count();
        assert_eq!(of_file, HITS_PER_FILE, "{path}");
    }
}

/// What the output gate redacts stays redacted in the text: the renderer
/// reads the sealed outcome, never the file.
#[tokio::test]
async fn redacted_content_stays_redacted_in_text() {
    for format in [None, Some("text")] {
        let mut session = Session::new(format);
        let input = json!({"queries": [{"path": "src/secret.txt"}]});
        let answer = session.call("localFetch", input).await;
        assert!(!answer.text.contains(&secret()), "{}", answer.text);
        assert!(!answer.text.contains("a1B2a1B2"), "{}", answer.text);
        // The lines around it are read, and the reply says it redacted.
        assert!(answer.text.contains("before") && answer.text.contains("after"));
        assert!(answer.text.contains("redacted"), "{}", answer.text);
        session.close().await;
    }
}

/// astSearch on inline `content` answers in both formats: a snippet's tree and
/// its matches without a file, a credential in it hidden where a rule's own
/// text would be refused, and `path` beside `content` flagged with its fix.
#[tokio::test]
async fn an_inline_snippet_answers_in_both_formats() {
    let source = format!("foo(1); bar(\"{}\")", secret());
    let row = |fields: Value| {
        let mut row = json!({"content": source, "parser": "typescript"});
        for (field, value) in fields.as_object().expect("fields") {
            row[field] = value.clone();
        }
        json!({"queries": [row]})
    };
    for format in [None, Some("text")] {
        let mut session = Session::new(format);
        let tree = session
            .call("astSearch", row(json!({"operation": "syntaxTree"})))
            .await;
        assert!(!tree.is_error, "{}", tree.text);
        assert!(tree.text.contains("call_expression"), "{}", tree.text);
        let matched = session
            .call(
                "astSearch",
                row(json!({"operation": "match", "rule": {"pattern": "foo($A)"}})),
            )
            .await;
        assert!(!matched.is_error, "{}", matched.text);
        assert!(matched.text.contains("foo(1)"), "{}", matched.text);
        let hidden = session
            .call(
                "astSearch",
                row(json!({
                    "operation": "match",
                    "rule": {"pattern": "bar($A)"},
                    "includeCaptures": true
                })),
            )
            .await;
        assert!(!hidden.is_error, "{}", hidden.text);
        assert!(hidden.text.contains("bar("), "{}", hidden.text);
        for answer in [&tree, &matched, &hidden] {
            assert!(!answer.text.contains(&secret()), "{}", answer.text);
            assert!(!answer.text.contains("a1B2a1B2"), "{}", answer.text);
            assert!(!answer.json.contains("a1B2a1B2"), "{}", answer.json);
        }
        let refused = session
            .call(
                "astSearch",
                row(json!({
                    "operation": "match",
                    "rule": {"pattern": "foo($A)"},
                    "path": "src"
                })),
            )
            .await;
        assert!(refused.is_error, "{}", refused.text);
        assert!(
            refused.text.contains("content replaces path"),
            "{}",
            refused.text
        );
        session.close().await;
    }
}

/// `isError` is the outcome's in both formats: a failed row, a rejected
/// call and a good call each flag alike, and the text names the failure as
/// a failed row prints.
#[tokio::test]
async fn the_error_flag_is_the_same_in_both_formats() {
    let calls = [
        (json!({"queries": [{"path": "src/absent.txt"}]}), true),
        (
            json!({"queries": [{"path": "src/big.txt", "notAField": 1}]}),
            true,
        ),
        (
            json!({"queries": [{"path": "src/big.txt", "ranges": [{"startLine": 1, "endLine": 2}]}]}),
            false,
        ),
    ];
    let mut json_session = Session::new(None);
    let mut text_session = Session::new(Some("text"));
    for (input, failed) in calls {
        let as_json = json_session.call("localFetch", input.clone()).await;
        let as_text = text_session.call("localFetch", input).await;
        assert_eq!(as_json.is_error, failed, "{}", as_json.text);
        assert_eq!(as_text.is_error, as_json.is_error, "{}", as_text.text);
        assert_eq!(
            as_text.text.contains("# status: error"),
            failed,
            "{}",
            as_text.text
        );
        let code = serde_json::from_str::<Value>(&as_json.text)
            .ok()
            .and_then(|reply| {
                let code = reply
                    .get("errorCode")
                    .or_else(|| reply.pointer("/results/0/data/errorCode"))?;
                Some(code.as_str()?.to_owned())
            });
        if let Some(code) = code {
            assert!(
                as_text.text.contains(&format!("errorCode: {code}")),
                "{}",
                as_text.text
            );
        }
    }
    json_session.close().await;
    text_session.close().await;
}

/// A result without a text view stays the JSON projection in `text`, and a
/// call that is a protocol error stays one.
#[test]
fn text_falls_back_to_json_and_keeps_protocol_errors() {
    use octocode_contract_types::ErrorCode;
    use octocode_runtime_base::security::Sanitized;
    let outcome = || ToolOutcome {
        structured_content: Sanitized::scrubbed(json!({"notResults": true})),
        failure: None,
        all_failed: false,
        continued: None,
    };
    let text = reply(
        "localFetch",
        Ok(outcome()),
        OutputMcpResult::Text,
        &Value::Null,
    )
    .expect("an answer");
    let json = mcp_result("localFetch", Ok(outcome())).expect("an answer");
    assert_eq!(text.text, json.text);
    assert_eq!(text.is_error, json.is_error);
    for code in [ErrorCode::Cancelled, ErrorCode::Closed] {
        let error = || RuntimeError {
            code,
            message: "gone".to_owned(),
            payload: None,
            validation_issues: None,
        };
        for format in [OutputMcpResult::Json, OutputMcpResult::Text] {
            let ended = reply("localFetch", Err(error()), format, &Value::Null);
            assert_eq!(ended.err().map(|error| error.code), Some(code));
        }
    }
}
