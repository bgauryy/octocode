//! One redaction decision per source, served by every tool: a value whose
//! secrecy comes from the key that names it (on its line, on an earlier
//! line, across a comment or a wrap) is hidden in every response that shows
//! text of its source, and no tool hides more than a read of the same lines.
//! All secrets are fake.

use crate::support::{Workspace, call, query_path};
use serde_json::{Value, json};
use std::collections::BTreeMap;

const TS: &str = r#"export const config = {
  awsSecretAccessKey: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYzQ8vT3kLm2",
  password: "Tr0ub4dor&3xK9!zQ",
};
const apiKey =
  // the key
  "Bq1Zx9Lm2Vp4Kd8Rt7Yb6Nc3";
const dbPassword =

  "Eq1Zx9Lm2Vp4Kd8Rt7Yb6Nc3";
const split = "ghp_" + // joined below
  "x7Kq2Lm9Zp4Vw8Rt1Yb6Nc3Hd5Jf0GsAeUoI";
export function connect(token: string = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08") { return token; }
"#;

const PY: &str = "AWS_SECRET_ACCESS_KEY = (\n    # wrapped\n    \"wJalrXUtnFEMI/K7MDENG/bPxRfiCYzQ9vT3kLm2\"\n)\n";

const RS: &str =
    "fn f() {\n    let password = \"Tq0ub4dor&3xK9!zQw\".to_owned();\n    let _ = password;\n}\n";

/// Ordinary code a fragment judged alone reads as configuration.
const CODE: &str = "const log = console;\nfunction getGitHubToken(l: unknown) { return String(l); }\nexport const options = {\n  getToken: () => getGitHubToken(log),\n  manualToken: undefined as string | undefined,\n};\n";

/// Every value planted, by the file that holds it.
const PLANTED: [(&str, &str); 8] = [
    ("s.ts", "wJalrXUtnFEMI/K7MDENG/bPxRfiCYzQ8vT3kLm2"),
    ("s.ts", "Tr0ub4dor&3xK9!zQ"),
    ("s.ts", "Bq1Zx9Lm2Vp4Kd8Rt7Yb6Nc3"),
    ("s.ts", "Eq1Zx9Lm2Vp4Kd8Rt7Yb6Nc3"),
    ("s.ts", "x7Kq2Lm9Zp4Vw8Rt1Yb6Nc3Hd5Jf0GsAeUoI"),
    (
        "s.ts",
        "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08",
    ),
    ("s.py", "wJalrXUtnFEMI/K7MDENG/bPxRfiCYzQ9vT3kLm2"),
    ("g.rs", "Tq0ub4dor&3xK9!zQw"),
];

/// A language server that answers a definition with the declaration's whole
/// range, as a real one does for a declaration continued over lines.
const SERVER: &str = r#"
let buffer = Buffer.alloc(0);
function send(id, result) {
  const body = JSON.stringify({jsonrpc:'2.0', id, result});
  process.stdout.write('Content-Length: ' + Buffer.byteLength(body) + '\r\n\r\n' + body);
}
function handle(msg) {
  if (msg.method === 'initialize') return send(msg.id, {capabilities:{definitionProvider:true,referencesProvider:true,hoverProvider:true,textDocumentSync:1}});
  if (msg.method === 'exit') return process.exit(0);
  if (!msg.id) return;
  const uri = msg.params?.textDocument?.uri;
  const span = (from, to) => ({uri,range:{start:{line:from,character:0},end:{line:to,character:3}}});
  if (msg.method === 'textDocument/definition') return send(msg.id, [span(4, 6)]);
  if (msg.method === 'textDocument/references') return send(msg.id, [span(4, 6), span(7, 9), span(1, 1)]);
  send(msg.id, null);
}
process.stdin.on('data', chunk => {
  buffer = Buffer.concat([buffer,chunk]);
  for (;;) {
    const split = buffer.indexOf('\r\n\r\n');
    if (split < 0) return;
    const length = Number(/Content-Length: (\d+)/i.exec(buffer.subarray(0,split).toString())[1]);
    if (buffer.length < split + 4 + length) return;
    const msg = JSON.parse(buffer.subarray(split+4,split+4+length));
    buffer = buffer.subarray(split+4+length);
    handle(msg);
  }
});
"#;

/// The lines of a numbered text (`<line>\t<text>`), by line.
fn numbered(text: &str) -> BTreeMap<u64, String> {
    text.lines()
        .filter_map(|record| {
            let (number, line) = record.split_once('\t')?;
            Some((number.trim().parse().ok()?, line.to_owned()))
        })
        .collect()
}

/// Every `(line, text)` a response holds under `key` in an object that
/// states its `line`.
fn rows(value: &Value, key: &str, out: &mut Vec<(u64, String)>) {
    match value {
        Value::Array(items) => items.iter().for_each(|item| rows(item, key, out)),
        Value::Object(fields) => {
            if let (Some(line), Some(text)) = (
                fields.get("line").and_then(Value::as_u64),
                fields.get(key).and_then(Value::as_str),
            ) {
                out.push((line, text.to_owned()));
            }
            fields.values().for_each(|field| rows(field, key, out));
        }
        _ => {}
    }
}

/// Whether `text` holds a redaction placeholder.
fn redacted(text: &str) -> bool {
    text.contains("[REDACTED")
}

#[tokio::test]
#[expect(clippy::too_many_lines, reason = "one walk over every tool and view")]
async fn every_tool_serves_a_source_as_its_whole_file_judgment() {
    let workspace = Workspace::new();
    let root = workspace
        .write("fx/s.ts", TS)
        .parent()
        .expect("dir")
        .to_owned();
    workspace.write("fx/s.py", PY);
    workspace.write("fx/g.rs", RS);
    workspace.write("fx/code.ts", CODE);
    let script = workspace.write("server.cjs", SERVER);
    std::fs::write(
        workspace.home.join("lsp-servers.json"),
        json!({"languageServers":{".ts":{
            "command":"node","args":[script],"languageId":"plaintext"
        }}})
        .to_string(),
    )
    .expect("lsp config");
    let runtime = workspace.runtime(&[]);
    let file = |name: &str| root.join(name);
    let run = |tool: &'static str, query: Value| {
        let runtime = &runtime;
        async move {
            let outcome = call(runtime, tool, query.clone())
                .await
                .unwrap_or_else(|error| panic!("{tool} {query}: {error:?}"));
            let output = outcome.structured_content.into_value();
            assert!(
                output.pointer("/results/0/data/errorCode").is_none(),
                "{tool} {query}: {output}"
            );
            (format!("{tool} {query}"), output)
        }
    };

    // Where a tool shows other text than a read of the same lines.
    let mut differs = Vec::new();
    // The reference: what a read of each file shows.
    let mut read = BTreeMap::new();
    let mut responses = Vec::new();
    for name in ["s.ts", "s.py", "g.rs", "code.ts"] {
        let (call, output) = run("localFetch", query_path(&file(name), json!({}))).await;
        let content = output["results"][0]["data"]["content"]
            .as_str()
            .unwrap_or_else(|| panic!("{call}: {output}"));
        read.insert(name, numbered(content));
        responses.push((call, output));
        for view in ["compact", "symbols"] {
            responses.push(run("localFetch", query_path(&file(name), json!({"view": view}))).await);
        }
    }
    let shown = |name: &str| {
        read[name]
            .values()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join("\n")
    };
    // A read hides every planted value: the whole-file judgment this test
    // holds every other tool to.
    for (name, value) in PLANTED {
        assert!(!shown(name).contains(value), "{name} reads {value}");
    }
    assert_eq!(shown("code.ts"), CODE.trim_end(), "code reads verbatim");

    // localSearch: every view, hits on the value lines.
    for (name, needle) in [("s.ts", "Zx9Lm2"), ("s.ts", "K7MDENG"), ("s.py", "wJalr")] {
        for view in [
            "content",
            "matchOnly",
            "files",
            "countLines",
            "countMatches",
        ] {
            let query = json!({"matchString": needle, "view": view});
            responses.push(run("localSearch", query_path(&file(name), query)).await);
        }
        let context = json!({"matchString": needle, "contextLines": {"before": 1, "after": 1}});
        responses.push(run("localSearch", query_path(&file(name), context)).await);
    }
    for (name, needle) in [("s.ts", "Zx9Lm2"), ("s.py", "wJalr"), ("code.ts", "Token")] {
        let (call, output) = run(
            "localSearch",
            query_path(&file(name), json!({"matchString": needle})),
        )
        .await;
        let mut hits = Vec::new();
        rows(&output, "value", &mut hits);
        assert!(!hits.is_empty(), "{call}: {output}");
        for (line, value) in hits {
            if value != read[name][&line] {
                differs.push(format!("{call}\n  line {line} shows {value:?}"));
            }
        }
    }

    // astSearch: node text, captures, names, the tree.
    for name in ["s.ts", "s.py", "g.rs", "code.ts"] {
        let kind = if name.ends_with(".rs") {
            "string_literal"
        } else {
            "string"
        };
        for query in [
            json!({"operation": "match", "rule": {"kind": kind}, "includeCaptures": true}),
            json!({"operation": "match", "rule": {"kind": kind}}),
            json!({"operation": "symbols"}),
            json!({"operation": "syntaxTree"}),
        ] {
            responses.push(run("astSearch", query_path(&file(name), query)).await);
        }
    }
    for (name, rule) in [
        ("s.ts", json!({"pattern": "const $NAME = $VALUE"})),
        ("s.ts", json!({"kind": "string"})),
        ("code.ts", json!({"kind": "pair"})),
    ] {
        let query = json!({"operation": "match", "rule": rule, "includeCaptures": true});
        let (call, output) = run("astSearch", query_path(&file(name), query)).await;
        let mut nodes = Vec::new();
        rows(&output, "value", &mut nodes);
        rows(&output, "text", &mut nodes);
        assert!(!nodes.is_empty(), "{call}: {output}");
        for (line, value) in nodes {
            // A node is a slice of the read: each of its lines is part of
            // the read's line, with the read's placeholders and no other.
            for (at, piece) in (line..).zip(value.split('\n')) {
                let whole = &read[name][&at];
                if !whole.contains(piece) {
                    differs.push(format!("{call}\n  line {line} shows {piece:?}"));
                }
            }
        }
        responses.push((call, output));
    }

    // lspSearch: a multi-line declaration quoted at the server's range.
    for operation in ["definition", "references"] {
        let query = json!({"symbolName": "apiKey", "line": 5, "operation": operation});
        let (call, output) = run("lspSearch", query_path(&file("s.ts"), query)).await;
        let mut quoted = Vec::new();
        fn contents(value: &Value, out: &mut Vec<String>) {
            match value {
                Value::Array(items) => items.iter().for_each(|item| contents(item, out)),
                Value::Object(fields) => {
                    if let Some(text) = fields.get("content").and_then(Value::as_str) {
                        out.push(text.to_owned());
                    }
                    fields.values().for_each(|field| contents(field, out));
                }
                _ => {}
            }
        }
        contents(&output, &mut quoted);
        assert!(!quoted.is_empty(), "{call}: {output}");
        for content in quoted {
            for (line, text) in numbered(&content) {
                if text != read["s.ts"][&line] {
                    differs.push(format!("{call}\n  line {line} shows {text:?}"));
                }
            }
        }
        responses.push((call, output));
    }

    // The graph: facts stored at build time are the judged source's.
    responses.push(run("graphBuild", query_path(&root, json!({}))).await);
    for query in [
        json!({"operation": "find"}),
        json!({"operation": "symbols", "target": "g.rs"}),
        json!({"operation": "symbols", "target": "s.ts"}),
        json!({"operation": "unresolved"}),
        json!({"operation": "unresolved", "edges": ["calls", "uses", "passes", "imports"]}),
        json!({"operation": "witnesses", "target": "g.rs"}),
        json!({"operation": "callers", "target": "f"}),
        json!({"operation": "callees", "target": "f"}),
    ] {
        let outcome = call(&runtime, "graphQuery", query_path(&root, query.clone()))
            .await
            .unwrap_or_else(|error| panic!("graphQuery {query}: {error:?}"));
        responses.push((
            format!("graphQuery {query}"),
            outcome.structured_content.into_value(),
        ));
    }

    // No planted value in any response of any tool.
    let mut leaks = Vec::new();
    for (call, output) in &responses {
        let text = output.to_string();
        for (name, value) in PLANTED {
            // JSON escapes nothing these values hold but `&` stays itself.
            if text.contains(value) {
                leaks.push(format!("{call}\n  shows {name}'s {value}"));
            }
        }
    }
    // And the code file is code in every response: nothing was hidden in it.
    for (call, output) in &responses {
        if call.contains("code.ts") && redacted(&output.to_string()) {
            differs.push(format!("{call}\n  hides code: {output}"));
        }
    }
    assert!(
        leaks.is_empty() && differs.is_empty(),
        "{} leak(s):\n{}\n{} fragment(s) differ from the read:\n{}",
        leaks.len(),
        leaks.join("\n"),
        differs.len(),
        differs.join("\n")
    );
}

/// Text no detector reads as a word: `len` characters of a fixed sequence
/// seeded by `seed`. All of it is fake.
fn multiline_body(seed: u64, len: usize) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut state = seed;
    (0..len)
        .map(|_| {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let at = usize::try_from(state >> 33).unwrap_or(0) % ALPHABET.len();
            char::from(ALPHABET[at])
        })
        .collect()
}

/// One declaration of a generated source: its text, the names it declares
/// and the values planted in it.
struct MultilinePiece {
    text: String,
    names: Vec<&'static str>,
    planted: Vec<String>,
}

/// The declarations of one language: plain ones, a key block in a string
/// that spans lines, a named value continued over lines, and a one-line
/// function whose own name's letter also occurs in the value it holds.
fn multiline_pieces(extension: &str, seed: u64) -> Vec<MultilinePiece> {
    let piece = |text: String, names: &[&'static str], planted: &[&String]| MultilinePiece {
        text,
        names: names.to_vec(),
        planted: planted.iter().map(|value| (*value).clone()).collect(),
    };
    let (b1, b2) = (multiline_body(seed, 64), multiline_body(seed + 1, 64));
    let value = multiline_body(seed + 2, 24);
    let inline = format!("{}f", multiline_body(seed + 3, 24));
    let tail = multiline_body(seed + 4, 36);
    match extension {
        "ts" => vec![
            piece("export const label = \"plain\";\n".into(), &["label"], &[]),
            piece(
                format!(
                    "const pem = `-----BEGIN PRIVATE KEY-----\n{b1}\n{b2}\n-----END PRIVATE KEY-----`;\n"
                ),
                &["pem"],
                &[&b1, &b2],
            ),
            piece(
                format!("const apiKey =\n  // the key\n  \"{value}\";\n"),
                &["apiKey"],
                &[&value],
            ),
            piece(
                format!("function f(token: string = \"{inline}\") {{ return token; }}\n"),
                &["f"],
                &[&inline],
            ),
            piece(
                format!("const split = \"ghp_\" + // joined below\n  \"{tail}\";\n"),
                &["split"],
                &[&tail],
            ),
            piece("type Receipt = { id: number };\n".into(), &["Receipt"], &[]),
        ],
        "py" => vec![
            piece("label = \"plain\"\n".into(), &["label"], &[]),
            piece(
                format!(
                    "KEY = \"\"\"-----BEGIN OPENSSH PRIVATE KEY-----\n{b1}\n{b2}\n-----END OPENSSH PRIVATE KEY-----\"\"\"\n"
                ),
                &["KEY"],
                &[&b1, &b2],
            ),
            piece(
                format!("AWS_SECRET_ACCESS_KEY = (\n    # wrapped\n    \"{value}\"\n)\n"),
                &["AWS_SECRET_ACCESS_KEY"],
                &[&value],
            ),
            piece(
                format!("def f(api_key=\"{inline}\"):\n    return api_key\n"),
                &["f"],
                &[&inline],
            ),
            piece(
                format!("token = \"ghp_\" \\\n    \"{tail}\"\n"),
                &["token"],
                &[&tail],
            ),
            piece("count = 3\n".into(), &["count"], &[]),
        ],
        _ => vec![
            piece("var label = \"plain\"\n".into(), &["label"], &[]),
            piece(
                format!(
                    "var pemKey = `-----BEGIN RSA PRIVATE KEY-----\n{b1}\n{b2}\n-----END RSA PRIVATE KEY-----`\n"
                ),
                &["pemKey"],
                &[&b1, &b2],
            ),
            piece(
                format!("var password = \"{value}\"\n"),
                &["password"],
                &[&value],
            ),
            piece(
                format!("func f() {{ secret := \"{inline}\"; _ = secret }}\n"),
                &["f"],
                &[&inline],
            ),
            piece("type C struct{ Token string }\n".into(), &["C"], &[]),
        ],
    }
}

/// Every string a response holds under `key`.
fn multiline_strings(value: &Value, key: &str, out: &mut Vec<String>) {
    match value {
        Value::Array(items) => items
            .iter()
            .for_each(|item| multiline_strings(item, key, out)),
        Value::Object(fields) => {
            for (name, field) in fields {
                match field {
                    Value::String(text) if name == key => out.push(text.clone()),
                    field => multiline_strings(field, key, out),
                }
            }
        }
        _ => {}
    }
}

/// The redaction placeholders `text` holds.
fn multiline_placeholders(text: &str) -> Vec<&str> {
    let mut found = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find("[REDACTED") {
        let end = rest[at..].find(']').map_or(rest.len(), |end| at + end + 1);
        found.push(&rest[at..end]);
        rest = &rest[end..];
    }
    found
}

/// Sources whose redaction spans lines (a key block in a template literal, a
/// triple-quoted string and a raw string; a named value continued over
/// lines), each declaration at every position of its file: a declaration
/// name is the source's identifier, a range of a read shows what the whole
/// read shows of those lines, and an AST piece shows its lines as the read
/// does, each hidden value under the read's own placeholder.
#[tokio::test]
#[expect(clippy::too_many_lines, reason = "one walk over every layout and view")]
async fn multiline_redactions_keep_names_and_one_placeholder_per_value() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    let run = |tool: &'static str, query: Value| {
        let runtime = &runtime;
        async move {
            let outcome = call(runtime, tool, query.clone())
                .await
                .unwrap_or_else(|error| panic!("{tool} {query}: {error:?}"));
            let output = outcome.structured_content.into_value();
            assert!(
                output.pointer("/results/0/data/errorCode").is_none(),
                "{tool} {query}: {output}"
            );
            (format!("{tool} {query}"), output)
        }
    };
    let mut differs = Vec::new();
    let mut leaks = Vec::new();
    let mut layouts = 0;
    for (extension, kinds, binding) in [
        (
            "ts",
            &["string", "template_string"][..],
            "const $NAME = $VALUE",
        ),
        ("py", &["string"][..], "$NAME = $VALUE"),
        (
            "go",
            &["interpreted_string_literal", "raw_string_literal"][..],
            "var $NAME = $VALUE",
        ),
    ] {
        let count = multiline_pieces(extension, 0).len();
        for turn in 0..count {
            let mut pieces = multiline_pieces(extension, 7 + u64::try_from(turn).unwrap_or(0) * 11);
            pieces.rotate_left(turn);
            let head = if extension == "go" {
                "package main\n\n"
            } else {
                ""
            };
            let source = head.to_owned()
                + &pieces
                    .iter()
                    .map(|piece| piece.text.as_str())
                    .collect::<String>();
            let file = workspace.write(&format!("ml/{extension}{turn}/m.{extension}"), &source);
            let raw: Vec<&str> = source.lines().collect();
            layouts += 1;

            // The reference: the whole read.
            let (call, output) = run("localFetch", query_path(&file, json!({}))).await;
            let read = numbered(
                output["results"][0]["data"]["content"]
                    .as_str()
                    .unwrap_or_else(|| panic!("{call}: {output}")),
            );
            assert_eq!(read.len(), raw.len(), "{call}: {output}");
            let whole = read
                .values()
                .map(String::as_str)
                .collect::<Vec<_>>()
                .join("\n");
            let kinds_read = multiline_placeholders(&whole);
            // The values this read hides; the key block's always are.
            let hidden: Vec<&String> = pieces
                .iter()
                .flat_map(|piece| &piece.planted)
                .filter(|value| !whole.contains(value.as_str()))
                .collect();
            for body in pieces
                .iter()
                .filter(|piece| piece.text.contains("-----BEGIN"))
                .flat_map(|piece| &piece.planted)
            {
                assert!(hidden.contains(&body), "{call} reads a key body: {whole}");
            }
            let mut responses = vec![(call, output)];

            // A read of ranges shows each line as the whole read does,
            // wherever its windows start.
            let total = u64::try_from(raw.len()).unwrap_or(0);
            for (width, phase) in [(1, 0), (1, 1), (2, 0), (2, 1), (2, 2), (3, 1)] {
                let ranges: Vec<Value> = (1..=total)
                    .filter(|line| line % (width + 1) == phase % (width + 1))
                    .map(
                        |line| json!({"startLine": line, "endLine": (line + width - 1).min(total)}),
                    )
                    .collect();
                if ranges.is_empty() {
                    continue;
                }
                let (call, output) =
                    run("localFetch", query_path(&file, json!({"ranges": ranges}))).await;
                let content = output["results"][0]["data"]["content"]
                    .as_str()
                    .unwrap_or_else(|| panic!("{call}: {output}"));
                let shown = numbered(content);
                assert!(!shown.is_empty(), "{call}: {output}");
                for (line, text) in shown {
                    if read.get(&line) != Some(&text) {
                        differs.push(format!(
                            "{call}\n  line {line} reads {text:?}, whole read {:?}",
                            read.get(&line)
                        ));
                    }
                }
                responses.push((call, output));
            }

            // Declaration names are the source's identifiers.
            let (call, output) = run(
                "astSearch",
                query_path(&file, json!({"operation": "symbols"})),
            )
            .await;
            let mut names = Vec::new();
            multiline_strings(&output, "symbolName", &mut names);
            for piece in &pieces {
                for name in &piece.names {
                    if !names.iter().any(|shown| shown == name) {
                        differs.push(format!("{call}\n  no declaration named {name}: {names:?}"));
                    }
                }
            }
            for name in &names {
                if redacted(name) || name.contains(char::is_whitespace) {
                    differs.push(format!("{call}\n  names a declaration {name:?}"));
                }
            }
            responses.push((call, output));

            // AST pieces: each line of a node is part of the read's line. A
            // node that starts inside a value hidden from an earlier line on
            // shows that value's placeholder, where the read shows the
            // placeholder on the earlier line.
            let mut queries: Vec<Value> = kinds
                .iter()
                .map(|kind| json!({"operation": "match", "rule": {"kind": kind}, "includeCaptures": true}))
                .collect();
            queries.push(json!({"operation": "match", "rule": {"pattern": binding}, "includeCaptures": true}));
            queries.push(json!({"operation": "match", "rule": {"pattern": binding}}));
            for query in queries {
                let (call, output) = run("astSearch", query_path(&file, query)).await;
                let mut nodes = Vec::new();
                rows(&output, "value", &mut nodes);
                rows(&output, "text", &mut nodes);
                for (line, value) in nodes {
                    let value = value.strip_suffix(" …").unwrap_or(&value);
                    for (at, shown) in (line..).zip(value.split('\n')) {
                        let Some(whole_line) = read.get(&at) else {
                            differs
                                .push(format!("{call}\n  line {at} is past the file: {shown:?}"));
                            continue;
                        };
                        if whole_line.contains(shown) {
                            continue;
                        }
                        let placeholders = multiline_placeholders(shown);
                        let continued = at == line
                            && !placeholders.is_empty()
                            && placeholders
                                .iter()
                                .all(|placeholder| kinds_read.contains(placeholder))
                            && placeholders
                                .iter()
                                .fold(shown.to_owned(), |rest, placeholder| {
                                    rest.replacen(placeholder, "\n", 1)
                                })
                                .split('\n')
                                .all(|kept| {
                                    usize::try_from(at).is_ok_and(|at| raw[at - 1].contains(kept))
                                });
                        if !continued {
                            differs.push(format!(
                                "{call}\n  line {at} shows {shown:?}, the read {whole_line:?}"
                            ));
                        }
                    }
                }
                let mut bound = Vec::new();
                fn captured(value: &Value, out: &mut Vec<String>) {
                    match value {
                        Value::Array(items) => items.iter().for_each(|item| captured(item, out)),
                        Value::Object(fields) => {
                            if let Some(name) = fields
                                .get("metavars")
                                .and_then(|captures| captures.get("NAME"))
                                .and_then(Value::as_str)
                            {
                                out.push(name.to_owned());
                            }
                            fields.values().for_each(|field| captured(field, out));
                        }
                        _ => {}
                    }
                }
                captured(&output, &mut bound);
                for name in bound {
                    if redacted(&name) || name.contains(char::is_whitespace) {
                        differs.push(format!("{call}\n  captures the name {name:?}"));
                    }
                }
                responses.push((call, output));
            }

            for (call, output) in &responses {
                let text = output.to_string();
                if text.contains("[REDACTED]") {
                    differs.push(format!("{call}\n  shows a placeholder of no kind: {text}"));
                }
                for value in &hidden {
                    if text.contains(value.as_str()) {
                        leaks.push(format!("{call}\n  shows {value}"));
                    }
                }
            }
        }
    }
    assert!(layouts >= 15, "{layouts} layouts");
    assert!(
        leaks.is_empty() && differs.is_empty(),
        "{} leak(s):\n{}\n{} view(s) differ from the read:\n{}",
        leaks.len(),
        leaks.join("\n"),
        differs.len(),
        differs.join("\n")
    );
}

/// A name only the snapshot's stored facts hold, standing for a value the
/// analysis version that built them left unjudged. Fake, and no secret
/// format: the current analysis serves it, so only the refusal hides it.
const STALE_GRAPH_VALUE: &str = "stale_graph_unjudged_value_zq7";

/// The first row's data of a response.
fn stale_graph_data(output: &Value) -> &Value {
    &output["results"][0]["data"]
}

/// Rewrites the manifest of snapshot `id` as the analysis version before the
/// one that built it wrote it.
fn stale_graph_as_previous_analysis(workspace: &Workspace, id: &str) {
    let path = workspace
        .workspace
        .join(".octocode/graph")
        .join(id)
        .join("manifest.json");
    let mut manifest: Value =
        serde_json::from_slice(&std::fs::read(&path).expect("manifest")).expect("manifest JSON");
    let current = manifest["analysisVersion"]
        .as_u64()
        .expect("analysis version");
    manifest["analysisVersion"] = json!(current - 1);
    std::fs::write(&path, manifest.to_string()).expect("manifest write");
}

/// A snapshot holds facts as the analysis version that built it judged them.
/// One built by another version is not evidence: no graphQuery operation,
/// by path or by ID, as the queried snapshot or as a baseline, shows a fact
/// of it. Each refuses with the build that replaces it.
#[tokio::test]
async fn stale_graph_snapshot_of_another_analysis_version_serves_no_fact() {
    let workspace = Workspace::new();
    let root = workspace
        .write(
            "old/g.rs",
            format!("fn f() {{\n    {STALE_GRAPH_VALUE}();\n}}\n"),
        )
        .parent()
        .expect("dir")
        .to_owned();
    let runtime = workspace.runtime(&[]);
    let run = |tool: &'static str, query: Value| {
        let runtime = &runtime;
        async move {
            call(runtime, tool, query.clone())
                .await
                .unwrap_or_else(|error| panic!("{tool} {query}: {error:?}"))
                .structured_content
                .into_value()
        }
    };
    let unresolved = || query_path(&root, json!({"operation": "unresolved"}));

    let built = run("graphBuild", query_path(&root, json!({}))).await;
    let id = stale_graph_data(&built)["id"]
        .as_str()
        .unwrap_or_else(|| panic!("{built}"))
        .to_owned();
    // The snapshot holds the value: while it is current, a query shows it.
    let current = run("graphQuery", unresolved()).await;
    assert!(
        current.to_string().contains(STALE_GRAPH_VALUE),
        "the fixture's fact is in the snapshot: {current}"
    );

    stale_graph_as_previous_analysis(&workspace, &id);
    let mut refusals = Vec::new();
    for query in [
        json!({"operation": "stats"}),
        json!({"operation": "find"}),
        json!({"operation": "find", "target": STALE_GRAPH_VALUE}),
        json!({"operation": "candidates", "target": STALE_GRAPH_VALUE}),
        json!({"operation": "node", "target": "g.rs"}),
        json!({"operation": "symbols", "target": "g.rs"}),
        json!({"operation": "deps", "target": "g.rs"}),
        json!({"operation": "dependents", "target": "g.rs"}),
        json!({"operation": "callers", "target": "f"}),
        json!({"operation": "callees", "target": "f", "confidence": "low"}),
        json!({"operation": "walk", "target": "g.rs"}),
        json!({"operation": "path", "target": "f", "to": "g.rs"}),
        json!({"operation": "cycles"}),
        json!({"operation": "hubs"}),
        json!({"operation": "diagnostics"}),
        json!({"operation": "witnesses", "target": "g.rs"}),
        json!({"operation": "unresolved"}),
        json!({"operation": "unresolved", "includeProvenance": true,
            "edges": ["calls", "uses", "passes", "imports"]}),
        json!({"operation": "stale"}),
        json!({"operation": "issues"}),
        json!({"operation": "issues", "baseline": id}),
        json!({"operation": "impact", "target": "f"}),
        json!({"operation": "drift", "baseline": id}),
    ] {
        for named in [false, true] {
            let mut query = query_path(&root, query.clone());
            if named {
                query["graph"] = json!(id);
            }
            let output = run("graphQuery", query.clone()).await;
            let data = stale_graph_data(&output);
            assert!(
                !output.to_string().contains(STALE_GRAPH_VALUE),
                "graphQuery {query} shows a fact of the old snapshot: {output}"
            );
            assert_eq!(
                data["errorCode"], "notFound",
                "graphQuery {query}: {output}"
            );
            assert_eq!(
                data["related"][0]["tool"], "graphBuild",
                "graphQuery {query}: {output}"
            );
            assert!(data.get("rows").is_none(), "graphQuery {query}: {output}");
            refusals.push(data.clone());
        }
    }
    // One refusal, whatever was asked.
    refusals.dedup();
    assert_eq!(refusals.len(), 1, "{refusals:?}");

    // Its lead builds a snapshot of the current analysis, which answers.
    let lead = refusals[0]["related"][0]["input"].clone();
    let rebuilt = run("graphBuild", lead).await;
    assert_ne!(stale_graph_data(&rebuilt)["id"], json!(id), "{rebuilt}");
    let answered = run("graphQuery", unresolved()).await;
    assert!(
        stale_graph_data(&answered).get("errorCode").is_none(),
        "{answered}"
    );
    // The old snapshot stays refused as the baseline of the new one.
    let drift = run(
        "graphQuery",
        query_path(&root, json!({"operation": "drift", "baseline": id})),
    )
    .await;
    assert_eq!(stale_graph_data(&drift), &refusals[0], "{drift}");
}

/// A language server whose hover and diagnostics quote values of the files
/// they are about: `use.ts` hovers the constant `cfg.ts` declares (its
/// definition names that file), `far.ts` hovers one no location names, and
/// `x.ts` gets a diagnostic quoting its own value.
const LSP_TEXT_SERVER: &str = r#"
let buffer = Buffer.alloc(0);
function send(id, result) {
  const body = JSON.stringify({jsonrpc:'2.0', id, result});
  process.stdout.write('Content-Length: ' + Buffer.byteLength(body) + '\r\n\r\n' + body);
}
const at = {start:{line:1,character:13},end:{line:1,character:15}};
function handle(msg) {
  if (msg.method === 'initialize') return send(msg.id, {capabilities:{definitionProvider:true,typeDefinitionProvider:true,hoverProvider:true,diagnosticProvider:{interFileDependencies:false,workspaceDiagnostics:false},textDocumentSync:1}});
  if (msg.method === 'exit') return process.exit(0);
  if (!msg.id) return;
  const uri = msg.params?.textDocument?.uri ?? '';
  if (msg.method === 'textDocument/hover') return send(msg.id, {contents:{kind:'markdown',value:'```typescript\nconst pw: "Zr0ub4dor&3xK9!zQw"\n```'},range:at});
  if (msg.method === 'textDocument/definition') return send(msg.id, uri.endsWith('/use.ts') ? [{uri:uri.replace('/use.ts','/cfg.ts'),range:{start:{line:0,character:13},end:{line:0,character:16}}}] : []);
  if (msg.method === 'textDocument/typeDefinition') return send(msg.id, []);
  if (msg.method === 'textDocument/diagnostic') return send(msg.id, {kind:'full',items:[{range:at,severity:1,source:'ts',code:2345,message:"Argument of type '\"wrong-Value-9f86d081\"' is not assignable to parameter of type '\"Tr0ub4dor&3xK9!zQ\"'."}]});
  send(msg.id, null);
}
process.stdin.on('data', chunk => {
  buffer = Buffer.concat([buffer,chunk]);
  for (;;) {
    const split = buffer.indexOf('\r\n\r\n');
    if (split < 0) return;
    const length = Number(/Content-Length: (\d+)/i.exec(buffer.subarray(0,split).toString())[1]);
    if (buffer.length < split + 4 + length) return;
    const msg = JSON.parse(buffer.subarray(split+4,split+4+length));
    buffer = buffer.subarray(split+4+length);
    handle(msg);
  }
});
"#;

/// The one placeholder a text shows, `[REDACTED…]`.
fn lsp_text_placeholder(text: &str) -> &str {
    let from = text
        .find("[REDACTED")
        .unwrap_or_else(|| panic!("a placeholder in {text}"));
    let len = text[from..].find(']').expect("a closed placeholder");
    &text[from..=from + len]
}

/// What a language server writes about a file (a hover, a diagnostic
/// message) shows that file's values as a read of it does: the key that
/// names a value a secret is in the source, not in the server's text. A
/// value of a file no location names is withheld, with the reason.
#[tokio::test]
async fn lsp_server_text_shows_a_value_as_a_read_of_its_file_does() {
    const CFG_VALUE: &str = "Zr0ub4dor&3xK9!zQw";
    const X_VALUE: &str = "Tr0ub4dor&3xK9!zQ";
    let workspace = Workspace::new();
    let cfg = workspace.write(
        "lt/cfg.ts",
        "export const cfg = { user: \"svc\", password: \"Zr0ub4dor&3xK9!zQw\" } as const;\n",
    );
    let root = cfg.parent().expect("dir").to_owned();
    let uses = "import { cfg } from \"./cfg\";\nexport const pw = cfg.password;\n";
    workspace.write("lt/use.ts", uses);
    workspace.write("lt/far.ts", uses);
    workspace.write(
        "lt/x.ts",
        "export const password = \"Tr0ub4dor&3xK9!zQ\" as const;\nexport const pw = (p: typeof password) => p;\npw(\"wrong-Value-9f86d081\");\n",
    );
    let script = workspace.write("lsp-text-server.cjs", LSP_TEXT_SERVER);
    std::fs::write(
        workspace.home.join("lsp-servers.json"),
        json!({"languageServers":{".ts":{
            "command":"node","args":[script],"languageId":"plaintext"
        }}})
        .to_string(),
    )
    .expect("lsp config");
    let runtime = workspace.runtime(&[]);
    let run = |tool: &'static str, name: &'static str, query: Value| {
        let (runtime, path) = (&runtime, root.join(name));
        async move {
            let outcome = call(runtime, tool, query_path(&path, query.clone()))
                .await
                .unwrap_or_else(|error| panic!("{tool} {name} {query}: {error:?}"));
            let output = outcome.structured_content.into_value();
            assert!(
                output.pointer("/results/0/data/errorCode").is_none(),
                "{tool} {name} {query}: {output}"
            );
            output
        }
    };
    // The reference: the placeholder a read of each file shows for its value.
    let mut read = BTreeMap::new();
    for (name, value) in [("cfg.ts", CFG_VALUE), ("x.ts", X_VALUE)] {
        let output = run("localFetch", name, json!({})).await;
        let content = output["results"][0]["data"]["content"]
            .as_str()
            .unwrap_or_else(|| panic!("{name}: {output}"))
            .to_owned();
        assert!(!content.contains(value), "{name} reads {value}: {content}");
        read.insert(name, lsp_text_placeholder(&content).to_owned());
    }
    let hover = json!({"operation": "hover", "symbolName": "pw", "line": 2});

    // A hover of a value its definition's file declares.
    let named = run("lspSearch", "use.ts", hover.clone()).await;
    let text = named["results"][0]["data"]["hover"]["contents"]["value"]
        .as_str()
        .unwrap_or_else(|| panic!("{named}"));
    assert!(!named.to_string().contains(CFG_VALUE), "{named}");
    assert_eq!(
        text,
        format!("```typescript\nconst pw: \"{}\"\n```", read["cfg.ts"]),
        "{named}"
    );

    // A hover of a value no location names a file for: withheld, and said so.
    let unnamed = run("lspSearch", "far.ts", hover).await;
    assert!(!unnamed.to_string().contains(CFG_VALUE), "{unnamed}");
    assert_eq!(
        unnamed["results"][0]["data"]["hover"]["contents"]["value"],
        "```typescript\nconst pw: \"[REDACTED]\"\n```",
        "{unnamed}"
    );
    assert!(
        unnamed["results"][0]["data"]["warnings"]
            .as_array()
            .is_some_and(|warnings| warnings
                .iter()
                .filter_map(Value::as_str)
                .any(|warning| warning.starts_with("serverTextWithheld:"))),
        "{unnamed}"
    );

    // A diagnostic that quotes its file's value, and one the file shows.
    let diagnostics = run("lspSearch", "x.ts", json!({"operation": "diagnostic"})).await;
    let shown = diagnostics.to_string();
    assert!(!shown.contains(X_VALUE), "{diagnostics}");
    let message = diagnostics["results"][0]["data"]["matches"][0]["message"]
        .as_str()
        .unwrap_or_else(|| panic!("{diagnostics}"));
    assert_eq!(
        message,
        format!(
            "Argument of type '\"wrong-Value-9f86d081\"' is not assignable to parameter of type '\"{}\"'.",
            read["x.ts"]
        ),
        "{diagnostics}"
    );
}

/// A source past the whole-file ceiling, with a token written in pieces (a
/// `+` join over two lines, an array, a template) at its start and its end.
fn over_ceiling_source(tail: &str) -> String {
    let pieces = |name: &str| {
        format!(
            "const {name} = \"ghp_\" +\n  \"{tail}\";\nconst {name}Parts = [\"ghp_\", \"{tail}\"].join(\"\");\nconst {name}Tpl = `${{\"ghp_\"}}${{\"{tail}\"}}`;\n"
        )
    };
    let ceiling = octocode_engine::security::MAX_CONTENT_SIZE;
    let mut text = pieces("early");
    let mut line = 0;
    while text.len() < ceiling + ceiling / 64 {
        text.push_str(&format!("export const filler{line} = {line}; // plain\n"));
        line += 1;
    }
    text.push_str(&pieces("late"));
    text
}

/// A search row of a source larger than a whole-file read shows what a read
/// of the same lines serves, in every view: the pieces of a split token are
/// hidden at the file's start and at its end.
#[tokio::test]
async fn over_ceiling_source_rows_are_the_reads_lines() {
    const TAIL: &str = "H7Kq2Lm9Zp4Vw8Rt1Yb6Nc3Hd5Jf0GsAeUoI";
    let workspace = Workspace::new();
    let source = over_ceiling_source(TAIL);
    assert!(source.len() > octocode_engine::security::MAX_CONTENT_SIZE);
    let total = source.lines().count() as u64;
    let path = workspace.write("big/over.ts", &source);
    let runtime = workspace.runtime(&[]);
    let run = |tool: &'static str, query: Value| {
        let runtime = &runtime;
        let path = &path;
        async move {
            let outcome = call(runtime, tool, query_path(path, query.clone()))
                .await
                .unwrap_or_else(|error| panic!("{tool} {query}: {error:?}"));
            let output = outcome.structured_content.into_value();
            assert!(
                !output.to_string().contains(TAIL),
                "{tool} {query}: {output}"
            );
            output
        }
    };
    let mut read = BTreeMap::new();
    for (first, last) in [(1, 6), (total - 5, total)] {
        let ranges = json!({"ranges": [{"startLine": first, "endLine": last}]});
        let output = run("localFetch", ranges).await;
        let content = output["results"][0]["data"]["content"]
            .as_str()
            .unwrap_or_else(|| panic!("{output}"));
        read.extend(numbered(content));
    }
    // The read hides both pieces of the join: the prefix on its own line too.
    for line in [1, 2, total - 3, total - 2] {
        assert!(redacted(&read[&line]), "line {line}: {:?}", read[&line]);
    }
    for query in [
        json!({"matchString": "early|late", "regexEngine": "rust"}),
        json!({"matchString": "Parts", "contextLines": {"before": 2, "after": 1}}),
    ] {
        let output = run("localSearch", query.clone()).await;
        let mut hits = Vec::new();
        rows(&output, "value", &mut hits);
        assert_eq!(
            hits.len(),
            if query["contextLines"].is_null() {
                6
            } else {
                2
            },
            "{output}"
        );
        for (line, value) in hits {
            let shown = numbered(&value);
            if shown.is_empty() {
                assert_eq!(value, read[&line], "{query}: line {line}");
            }
            for (at, text) in shown {
                assert_eq!(text, read[&at], "{query}: line {at}");
            }
        }
    }
    for needle in ["[\"`][^\"`]*[\"`]", "H7Kq2Lm9Zp4V"] {
        let query =
            json!({"matchString": needle, "regexEngine": "rust", "view": "matchOnly", "limit": 40});
        let output = run("localSearch", query).await;
        let mut spans = Vec::new();
        rows(&output, "value", &mut spans);
        assert!(spans.iter().any(|(_, value)| redacted(value)), "{output}");
        for (line, value) in spans {
            assert!(
                redacted(&value) || read[&line].contains(&value),
                "line {line}: {value:?} is no cut of {:?}",
                read[&line]
            );
        }
    }
}
