//! The one loop on both transports (CLI structured JSON and the MCP text
//! payload): at tiny windows, with Unicode, escapes and long values, every
//! walk reads its evidence exactly once and equal to the unpaged answer, and
//! no page says `hasMore:false` while a walk remains.
use super::the_next_calls;
use crate::support::{Run as _, Workspace, mcp_payload};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use octocode_native::runtime::ToolRuntime;
use octocode_test_support::json_fixture;
use serde_json::{Value, json};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";
const TINY: [usize; 3] = [450, 1200, 4000];

#[derive(Clone, Copy, Debug)]
enum Transport {
    Cli,
    Mcp,
}

async fn page(runtime: &ToolRuntime, on: Transport, tool: &str, input: Value) -> Value {
    let id = format!("one-walk-{on:?}");
    let output = match on {
        Transport::Cli => (*runtime
            .run(id, tool.into(), input.clone())
            .await
            .unwrap_or_else(|error| panic!("{tool} {input}: {error:?}"))
            .structured_content)
            .clone(),
        Transport::Mcp => mcp_payload(
            &runtime
                .run_mcp(id, tool.into(), input.clone())
                .await
                .unwrap_or_else(|error| panic!("{tool} {input}: {error:?}")),
        ),
    };
    octocode_runtime_base::contracts::validate_output(tool, &output).expect("output contract");
    output
}

/// Every page of the one loop from `start` on `on`.
async fn walk_on(runtime: &ToolRuntime, on: Transport, tool: &str, start: Value) -> Vec<Value> {
    let mut pages = Vec::new();
    let mut queue = std::collections::VecDeque::from([start]);
    while let Some(input) = queue.pop_front() {
        assert!(pages.len() < 400, "the walk terminates");
        let output = page(runtime, on, tool, input).await;
        queue.extend(the_next_calls(&output));
        pages.push(output);
    }
    pages
}

/// The source text a read walk delivered, byte for byte: a row part
/// continues its previous part's `content`; each window's lines lose their
/// number and keep their own terminator, so a lost or doubled trailing
/// newline changes the text.
fn read_text(pages: &[Value]) -> String {
    read_windows(pages)
        .iter()
        .flat_map(|window| window.split_inclusive('\n'))
        .map(|line| line.split_once('\t').expect("numbered line").1)
        .collect()
}

/// The `content` of each window of a read walk, parts joined.
fn read_windows(pages: &[Value]) -> Vec<String> {
    let mut windows: Vec<String> = Vec::new();
    for row in pages
        .iter()
        .flat_map(|page| page["results"].as_array().into_iter().flatten())
    {
        let Some(content) = row["data"]["content"].as_str() else {
            continue;
        };
        match row["rowPart"]["part"].as_u64() {
            Some(part) if part > 1 => windows.last_mut().expect("first part").push_str(content),
            _ => windows.push(content.to_owned()),
        }
    }
    windows
}

/// Each `(row index, path, line)` hit of a search walk, in walk order. A
/// part whose `rowPart.text` continues a hit's value repeats that hit's
/// identity; the continuation is the same hit, counted once.
fn hits(pages: &[Value]) -> Vec<(u64, String, u64)> {
    let mut found = Vec::new();
    for row in pages
        .iter()
        .flat_map(|page| page["results"].as_array().into_iter().flatten())
    {
        let index = row["index"].as_u64().expect("index");
        let mut continued = row["rowPart"].get("text").is_some();
        for file in row["data"]["files"].as_array().into_iter().flatten() {
            for hit in file["matches"].as_array().into_iter().flatten() {
                if std::mem::take(&mut continued) {
                    continue;
                }
                let line = hit["line"].as_u64().expect("hit line");
                found.push((
                    index,
                    file["path"].as_str().unwrap_or_default().to_owned(),
                    line,
                ));
            }
        }
    }
    found
}

fn assert_once<T: Ord + Clone + std::fmt::Debug>(walked: &[T], expected: &[T], label: &str) {
    let mut sorted = walked.to_vec();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), walked.len(), "{label}: evidence read twice");
    let mut want = expected.to_vec();
    want.sort();
    assert_eq!(
        sorted, want,
        "{label}: evidence differs from the unpaged answer"
    );
}

/// Lines with escapes, Unicode, a few long values, a `\r\n` terminator and a
/// lone `\r`.
fn body() -> String {
    let mut text: String = (0..240)
        .map(|line| {
            let long = if line % 37 == 0 {
                "long ".repeat(300)
            } else {
                String::new()
            };
            format!(
                "evidence_{line:04} \"q\" \\ \t界😀é {long}{}\n",
                "w ".repeat(line % 9)
            )
        })
        .collect();
    text.push_str("evidence_crlf \"q\" end\r\n");
    text.push_str("evidence_lone\rcr 界\r tail\n");
    text
}

/// Newline-terminated lines of `body`; a `\r` is content, not a terminator.
fn body_lines() -> u64 {
    body().matches('\n').count() as u64
}

/// A read row walked at every tiny window on both transports reads `body`
/// exactly once.
async fn assert_read_walks(runtime: &ToolRuntime, tool: &str, row: Value, body: &str) {
    for on in [Transport::Cli, Transport::Mcp] {
        for budget in TINY {
            let pages = walk_on(
                runtime,
                on,
                tool,
                json!({"queries":[row.clone()],"responseChars":budget}),
            )
            .await;
            assert!(pages.len() > 1, "{tool} {on:?} {budget}: pages");
            assert_eq!(read_text(&pages), body, "{tool} {on:?} {budget}");
        }
    }
}

#[tokio::test]
async fn local_reads_walk_exactly_once_on_both_transports() {
    let workspace = Workspace::new();
    let file = workspace.write("src/evidence.txt", body());
    let runtime = workspace.runtime(&[]);
    assert_read_walks(&runtime, "localFetch", json!({"path":file}), &body()).await;
    runtime.close().await;
}

#[tokio::test]
async fn github_reads_walk_exactly_once_on_both_transports() {
    let server = MockServer::builder().start().await;
    let body = body();
    octocode_test_support::mock_http::list_in_trees(&server, "a", "b", "src/evidence.txt").await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/src%2Fevidence.txt"))
        .and(query_param("ref", SHA))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "type":"file","encoding":"base64","content":STANDARD.encode(&body),
            "size":body.len(),"sha":"f".repeat(40),"path":"src/evidence.txt"
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    assert_read_walks(
        &runtime,
        "ghGetFileContent",
        json!({"owner":"a","repo":"b","ref":SHA,"path":"src/evidence.txt"}),
        &body,
    )
    .await;
    runtime.close().await;
}

#[tokio::test]
async fn local_searches_walk_exactly_once_on_both_transports() {
    let workspace = Workspace::new();
    for file in 0..5 {
        workspace.write(&format!("docs/f{file}.txt"), body());
    }
    let runtime = workspace.runtime(&[]);
    let row = json_fixture(
        r#"{
            "path": "docs",
            "matchString": ["evidence_"],
            "view": "content",
            "limit": 60
        }"#,
    );
    let baseline = hits(
        &walk_on(
            &runtime,
            Transport::Cli,
            "localSearch",
            json!({"queries":[row.clone()]}),
        )
        .await,
    );
    assert_eq!(
        baseline.len(),
        5 * body_lines() as usize,
        "every line of every file"
    );
    for on in [Transport::Cli, Transport::Mcp] {
        for budget in TINY {
            let pages = walk_on(
                &runtime,
                on,
                "localSearch",
                json!({"queries":[row.clone()],"responseChars":budget}),
            )
            .await;
            assert_once(
                &hits(&pages),
                &baseline,
                &format!("localSearch {on:?} {budget}"),
            );
        }
    }
    runtime.close().await;
}

#[tokio::test]
async fn a_mixed_batch_walks_exactly_once_on_both_transports() {
    let workspace = Workspace::new();
    workspace.write("a/one.txt", body());
    workspace.write("b/two.txt", "evidence_only 界😀\n");
    workspace.write("c/three.txt", body());
    let runtime = workspace.runtime(&[]);
    let row = |path: &str, limit: u64| json!({"path":path,"matchString":["evidence_"],"view":"content","limit":limit});
    let queries = json!([row("a", 50), {"path":"a","bogus":true}, row("b", 5), row("c", 70)]);
    let mut expected = Vec::new();
    for (index, file, lines) in [
        (0, "a/one.txt", body_lines()),
        (2, "b/two.txt", 1),
        (3, "c/three.txt", body_lines()),
    ] {
        for line in 1..=lines {
            expected.push((index, file.to_owned(), line));
        }
    }
    for on in [Transport::Cli, Transport::Mcp] {
        for budget in [None, Some(1200), Some(4000)] {
            let mut start = json!({"queries":queries});
            if let Some(budget) = budget {
                start["responseChars"] = json!(budget);
            }
            let pages = walk_on(&runtime, on, "localSearch", start).await;
            let walked = hits(&pages)
                .into_iter()
                .map(|(index, path, line)| {
                    let file = expected
                        .iter()
                        .find(|(_, file, _)| path.ends_with(file.as_str()))
                        .map_or(path, |(_, file, _)| file.clone());
                    (index, file, line)
                })
                .collect::<Vec<_>>();
            assert_once(&walked, &expected, &format!("batch {on:?} {budget:?}"));
            let rejected = pages
                .iter()
                .flat_map(|page| page["results"].as_array().into_iter().flatten())
                .filter(|row| row["status"] == "error")
                .count();
            assert_eq!(rejected, 1, "the rejected row shows once");
        }
    }
    runtime.close().await;
}

/// A stub language server: `references` answers `count` locations.
fn lsp_workspace(count: usize) -> (Workspace, std::path::PathBuf) {
    let workspace = Workspace::new();
    let source = format!(
        "function needle() {{}}\n{}",
        "needle(\"界😀\\\\\");\n".repeat(count)
    );
    let path = workspace.write("src/lib.ts", &source);
    let script = workspace.write("server.cjs", format!(r#"
let buffer = Buffer.alloc(0);
function send(id, result) {{
  const body = JSON.stringify({{jsonrpc:'2.0', id, result}});
  process.stdout.write('Content-Length: ' + Buffer.byteLength(body) + '\r\n\r\n' + body);
}}
function handle(msg) {{
  if (msg.method === 'initialize') return send(msg.id, {{capabilities:{{definitionProvider:true,referencesProvider:true,textDocumentSync:1}}}});
  if (msg.method === 'exit') return process.exit(0);
  if (!msg.id) return;
  const uri = msg.params?.textDocument?.uri;
  const location = line => ({{uri,range:{{start:{{line,character:line ? 0 : 9}},end:{{line,character:line ? 6 : 15}}}}}});
  if (msg.method === 'textDocument/definition') return send(msg.id, [location(0)]);
  if (msg.method === 'textDocument/references') return send(msg.id, Array.from({{length:{}}}, (_,line)=>location(line)));
  send(msg.id, null);
}}
process.stdin.on('data', chunk => {{
  buffer = Buffer.concat([buffer,chunk]);
  for (;;) {{
    const split = buffer.indexOf('\r\n\r\n');
    if (split < 0) return;
    const length = Number(/Content-Length: (\d+)/i.exec(buffer.subarray(0,split).toString())[1]);
    if (buffer.length < split + 4 + length) return;
    const msg = JSON.parse(buffer.subarray(split+4,split+4+length));
    buffer = buffer.subarray(split+4+length);
    handle(msg);
  }}
}});
"#, count + 1));
    std::fs::write(
        workspace.home.join("lsp-servers.json"),
        json!({"languageServers":{".ts":{"command":"node","args":[script],"languageId":"plaintext"}}})
            .to_string(),
    )
    .expect("lsp config");
    (workspace, path)
}

#[tokio::test]
async fn lsp_references_walk_exactly_once_on_both_transports() {
    assert!(
        std::process::Command::new("node")
            .arg("--version")
            .output()
            .is_ok_and(|run| run.status.success()),
        "Node is required for the LSP process fixture"
    );
    let (workspace, path) = lsp_workspace(60);
    let runtime = workspace.runtime(&[]);
    let row = json!({"path":path,"symbolName":"needle","line":1,"operation":"references"});
    let baseline = hits(
        &walk_on(
            &runtime,
            Transport::Cli,
            "lspSearch",
            json!({"queries":[row.clone()]}),
        )
        .await,
    );
    assert!(baseline.len() >= 60, "every reference: {}", baseline.len());
    for on in [Transport::Cli, Transport::Mcp] {
        for budget in [700, 1500, 4000] {
            let pages = walk_on(
                &runtime,
                on,
                "lspSearch",
                json!({"queries":[row.clone()],"responseChars":budget}),
            )
            .await;
            assert_once(
                &hits(&pages),
                &baseline,
                &format!("lspSearch {on:?} {budget}"),
            );
        }
    }
    runtime.close().await;
}

/// SHR-1: an answer that fits one page at a tiny budget is complete, so it
/// states no `responsePagination` block (formerly `hasMore:false` with
/// `oversized:true`, a page 1 of 1 that read as the end of a walk). Its
/// text is free text a page can window, so it is over the budget by its
/// fixed parts only and states no `oversized` either.
#[tokio::test]
async fn a_complete_answer_wider_than_a_tiny_budget_states_no_page_block() {
    let workspace = Workspace::new();
    let file = workspace.write("tiny.txt", "one line\n");
    let runtime = workspace.runtime(&[]);
    for on in [Transport::Cli, Transport::Mcp] {
        // A budget under the whole answer's size, whatever fields it carries.
        let whole = page(
            &runtime,
            on,
            "localFetch",
            json!({"queries":[{"path":file}]}),
        )
        .await;
        let budget = whole.to_string().len() / 2;
        let output = page(
            &runtime,
            on,
            "localFetch",
            json!({"queries":[{"path":file}],"responseChars":budget}),
        )
        .await;
        assert!(output.get("next").is_none(), "{on:?}: {output}");
        assert!(output.get("oversized").is_none(), "{on:?}: {output}");
        assert_eq!(read_text(&[output]), "one line\n");
    }
    runtime.close().await;
}

/// Whole reads over the automatic page (20,000 chars), escape-heavy so raw
/// and JSON-escaped sizes differ: every page reads on or ends a walk it
/// began; none is a fresh `hasMore:false` page.
#[tokio::test]
async fn whole_reads_over_the_automatic_page_never_state_a_fresh_end() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    for size in [19_000, 21_000, 26_000, 40_000] {
        let line = "say \"hi\" \\ path\\to\t\"x\" 界😀\n";
        let mut body = line.repeat(size / line.len() + 1);
        body.truncate(
            body.char_indices()
                .take_while(|(at, _)| *at < size)
                .last()
                .map_or(0, |(at, c)| at + c.len_utf8()),
        );
        if !body.ends_with('\n') {
            body.push('\n');
        }
        let file = workspace.write(&format!("whole{size}.txt"), &body);
        for on in [Transport::Cli, Transport::Mcp] {
            let pages = walk_on(
                &runtime,
                on,
                "localFetch",
                json!({"queries":[{"path":file}]}),
            )
            .await;
            // Every reply but the last carries the one continuation; the
            // last states none.
            for (at, page) in pages.iter().enumerate() {
                assert_eq!(
                    page["next"].is_object(),
                    at + 1 < pages.len(),
                    "{size} {on:?}: page {at}: {page}"
                );
            }
            assert_eq!(read_text(&pages), body, "{size} {on:?}");
        }
    }
    runtime.close().await;
}

/// The documented rule: a walk is stated once, as the reply's `next`, and
/// the row keeps no copy, whether the response pages or not.
#[tokio::test]
async fn a_single_walk_rides_its_row_unpaged_and_the_response_paged() {
    let workspace = Workspace::new();
    workspace.write("docs/a.txt", body());
    let runtime = workspace.runtime(&[]);
    let row = json_fixture(r#"{"path": "docs", "matchString": ["evidence_"], "limit": 5}"#);
    for on in [Transport::Cli, Transport::Mcp] {
        let unpaged = page(
            &runtime,
            on,
            "localSearch",
            json!({"queries":[row.clone()]}),
        )
        .await;
        assert!(unpaged["next"].is_object(), "{on:?}: {unpaged}");
        assert!(
            unpaged["results"][0]["data"]
                .pointer("/next/nextPage")
                .is_none(),
            "{on:?}: {unpaged}"
        );
        let paged = page(
            &runtime,
            on,
            "localSearch",
            json!({"queries":[row.clone()],"responseChars":450}),
        )
        .await;
        assert!(paged["next"].is_object(), "{on:?}: {paged}");
        assert!(!the_next_calls(&paged).is_empty());
    }
    runtime.close().await;
}
