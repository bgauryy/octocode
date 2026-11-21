//! astSearch on inline source (`code` with `parser`) through the real
//! admission, on the CLI and MCP channels: the rows a file with that text
//! gives, the refusals that name their fix, a credential redacted in place
//! instead of refused, and pages that continue from the saved row alone.
use crate::support::{self, Run as _, Workspace};
use octocode_contract_types::ErrorCode;
use octocode_native::runtime::ToolRuntime;
use octocode_runtime_base::id::ToolId;
use serde_json::{Value, json};

/// A credential-shaped token: what a read of a file redacts and the input
/// gate refuses in a caller's own words.
fn token() -> String {
    format!("ghp_{}", "a1B2c3D4e5F6g7H8i9J0k1L2m3N4o5P6q7R8")
}

/// The published bound of `content`, in characters.
fn code_limit() -> usize {
    let limit = octocode_runtime_base::contracts::query_schema_number(
        ToolId::AstSearch,
        None,
        "content",
        "maxLength",
    )
    .expect("the contract bounds content");
    usize::try_from(limit).expect("limit")
}

fn snippet(content: &str, row: Value) -> Value {
    let mut row = row;
    row["content"] = json!(content);
    row["parser"] = json!("typescript");
    row
}

/// The first row's data of `row` asked on the CLI channel and on the MCP
/// channel, which must agree.
async fn data(runtime: &ToolRuntime, row: Value) -> Value {
    let cli = support::run(runtime, "astSearch", row.clone()).await;
    let mcp = runtime
        .run_mcp("mcp".into(), "astSearch".into(), support::envelope(row))
        .await
        .expect("mcp call");
    assert_ne!(mcp["isError"], true, "{mcp}");
    let mcp = support::mcp_payload(&mcp);
    assert_eq!(mcp["results"][0]["data"], *cli.data(), "CLI and MCP agree");
    cli.data().clone()
}

/// What refuses `row`, on both channels: the CLI error's text, which the MCP
/// result repeats in band.
async fn refusal(runtime: &ToolRuntime, row: Value) -> String {
    let input = support::envelope(row);
    let error = runtime
        .run("refused".into(), "astSearch".into(), input.clone())
        .await
        .expect_err("the row must not run");
    assert_eq!(error.code, ErrorCode::InvalidInput, "{error:?}");
    let text = error.payload.as_deref().expect("payload")["error"]
        .as_str()
        .expect("error text")
        .to_owned();
    let mcp = runtime
        .run_mcp("refused-mcp".into(), "astSearch".into(), input)
        .await
        .expect("MCP reports the refusal in band");
    assert_eq!(mcp["isError"], true, "{mcp}");
    // The MCP text block is the same payload as JSON text.
    let shown = mcp["content"][0]["text"].as_str().expect("text");
    let quoted = json!(text).to_string();
    assert!(
        shown.contains(quoted.trim_matches('"')) || shown.contains(&text),
        "{shown}\n{text}"
    );
    text
}

/// Every file under `dir`, with its bytes.
fn saved_files(dir: &std::path::Path) -> Vec<(std::path::PathBuf, Vec<u8>)> {
    let mut found = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if let Ok(bytes) = std::fs::read(&path)
                && !bytes.is_empty()
            {
                found.push((path, bytes));
            }
        }
    }
    found
}

const WALK_FACTS: [&str; 5] = [
    "path",
    "filesScanned",
    "skippedPaths",
    "skippedGrammars",
    "skippedFormats",
];

#[tokio::test]
async fn a_snippet_answers_as_a_file_with_that_text_without_walk_facts() {
    let workspace = Workspace::new();
    let source = "foo(1); bar(2)\n";
    let file = workspace.write("src/a.ts", source);
    let runtime = workspace.runtime(&[]);
    let tree = json!({"operation":"syntaxTree"});
    let of_code = data(&runtime, snippet(source, tree.clone())).await;
    let of_file = data(&runtime, support::query_path(&file, tree)).await;
    assert!(of_code["nodes"].as_array().is_some_and(|n| !n.is_empty()));
    assert_eq!(of_code["nodes"], of_file["nodes"]);
    for captures in [false, true] {
        let rule =
            json!({"operation":"match","rule":{"pattern":"foo($A)"},"includeCaptures":captures});
        let of_code = data(&runtime, snippet(source, rule.clone())).await;
        let of_file = data(&runtime, support::query_path(&file, rule)).await;
        assert_eq!(of_code["matchCount"], 1, "{of_code}");
        assert_eq!(
            of_code["files"][0]["matches"], of_file["files"][0]["matches"],
            "{of_code}"
        );
        for fact in WALK_FACTS {
            assert!(of_code.get(fact).is_none(), "{fact}: {of_code}");
        }
    }
    // An empty answer names no walk either.
    let empty = data(
        &runtime,
        snippet(
            source,
            json!({"operation":"match","rule":{"pattern":"zzz($A)"}}),
        ),
    )
    .await;
    for fact in WALK_FACTS {
        assert!(empty.get(fact).is_none(), "{fact}: {empty}");
    }
    runtime.close().await;
}

#[tokio::test]
async fn a_snippet_row_that_cannot_run_is_refused_naming_its_fix() {
    let workspace = Workspace::new();
    workspace.write("src/a.ts", "foo(1)\n");
    let runtime = workspace.runtime(&[]);
    let rule = json!({"operation":"match","rule":{"pattern":"foo($A)"}});
    let row = || snippet("foo(1); bar(2)", rule.clone());

    let mut no_parser = row();
    no_parser.as_object_mut().expect("row").remove("parser");
    let text = refusal(&runtime, no_parser).await;
    assert!(text.contains("content needs parser"), "{text}");

    let mut with_path = row();
    with_path["path"] = json!("src");
    let text = refusal(&runtime, with_path).await;
    assert!(text.contains("content replaces path"), "{text}");

    for (field, value) in [
        ("exclude", json!(["a"])),
        ("include", json!(["*.ts"])),
        ("defaultExcludes", json!(false)),
        ("includeIgnored", json!(true)),
        ("includeHidden", json!(true)),
        ("testCode", json!("exclude")),
        ("parserGlobs", json!({"typescript": ["*.x"]})),
        ("maxDepth", json!(1)),
        ("sort", json!({"by":"matchCount","order":"desc"})),
    ] {
        let mut walked = row();
        walked[field] = value;
        let text = refusal(&runtime, walked).await;
        assert!(text.contains("not a walk"), "{field}: {text}");
        assert!(text.contains(field), "{field}: {text}");
    }

    let symbols = snippet("foo(1)", json!({"operation":"symbols"}));
    let text = refusal(&runtime, symbols).await;
    assert!(text.contains("content"), "{text}");
    assert!(
        text.contains("match") && text.contains("syntaxTree"),
        "the refusal names the operations that take content: {text}"
    );

    // A tree needs one source; a match that names none walks the workspace.
    let neither = refusal(&runtime, json!({"operation":"syntaxTree"})).await;
    assert!(neither.contains("or content"), "{neither}");
    runtime.close().await;
}

#[tokio::test]
async fn a_snippet_is_bounded_by_its_published_limit_not_a_query_strings() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    let limit = code_limit();
    let line = "call(alpha, beta);\n";
    let sized = |chars: usize| line.repeat(chars / line.len() + 1)[..chars].to_owned();
    // Longer than any other caller string may be, and within the bound.
    let within = data(
        &runtime,
        snippet(
            &sized(limit),
            json!({"operation":"match","rule":{"pattern":"call($$$A)"},"view":"countMatches"}),
        ),
    )
    .await;
    assert!(
        within["matchCount"].as_u64().is_some_and(|n| n > 500),
        "{within}"
    );
    let text = refusal(
        &runtime,
        snippet(&sized(limit + 1), json!({"operation":"syntaxTree"})),
    )
    .await;
    assert!(text.contains("content"), "{text}");
    assert!(text.contains(&limit.to_string()), "names the limit: {text}");
    runtime.close().await;
}

/// A credential in a snippet does not refuse the row, as it would in a
/// pattern: it is redacted before the parse, so the rows are those of a file
/// with that text as a read shows it, and no channel repeats the credential.
#[tokio::test]
async fn a_credential_in_a_snippet_is_redacted_not_refused() {
    let workspace = Workspace::new();
    let token = token();
    let source = format!("const key = \"{token}\";\nuse(key, \"{token}\");\n");
    let file = workspace.write("src/a.ts", &source);
    let runtime = workspace.runtime(&[]);
    for row in [
        json!({"operation":"syntaxTree"}),
        json!({"operation":"match","rule":{"pattern":"const key = $V"}}),
        json!({"operation":"match","rule":{"kind":"string"},"includeCaptures":true}),
    ] {
        let of_code = data(&runtime, snippet(&source, row.clone())).await;
        let of_file = data(&runtime, support::query_path(&file, row.clone())).await;
        assert!(!of_code.to_string().contains(&token), "{of_code}");
        for rows in ["nodes", "files"] {
            let rows_of = |data: &Value| match rows {
                "files" => data["files"][0]["matches"].clone(),
                _ => data[rows].clone(),
            };
            assert_eq!(rows_of(&of_code), rows_of(&of_file), "{row}");
        }
        let mcp = runtime
            .run_mcp(
                "secret".into(),
                "astSearch".into(),
                support::envelope(snippet(&source, row)),
            )
            .await
            .expect("mcp");
        assert!(!mcp.to_string().contains(&token), "{mcp}");
    }
    // The same token as a rule's own text is still refused.
    let text = refusal(
        &runtime,
        snippet(
            "foo(1)",
            json!({"operation":"match","rule":{"pattern":token}}),
        ),
    )
    .await;
    assert!(text.contains("credential"), "{text}");
    assert!(!text.contains(&token), "{text}");
    runtime.close().await;
}

/// A snippet answer over one page continues by cursor alone: the saved row
/// holds the snippet (as shown), so no page re-sends it, every row is listed
/// once, and the rows are the paged file's.
#[tokio::test]
async fn a_paged_snippet_continues_by_cursor_and_lists_the_file_rows() {
    let workspace = Workspace::new();
    let token = token();
    let source = (0..250)
        .map(|index| format!("call{index}(alpha, \"{token}\");\n"))
        .collect::<String>();
    assert!(source.chars().count() <= code_limit());
    let file = workspace.write("src/a.ts", &source);
    let runtime = workspace.runtime(&[]);
    let nodes = |output: &Value| {
        output["results"][0]["data"]["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .map(Value::to_string)
            .collect::<Vec<_>>()
    };
    let match_rows = |output: &Value| {
        output["results"][0]["data"]["files"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|file| file["matches"].as_array().cloned().unwrap_or_default())
            .map(|row| row.to_string())
            .collect::<Vec<_>>()
    };
    for (row, rows_of) in [
        (
            json!({"operation":"syntaxTree"}),
            &nodes as &dyn Fn(&Value) -> Vec<String>,
        ),
        (
            json!({"operation":"match","rule":{"pattern":"$F(alpha, $S)"},"includeCaptures":true}),
            &match_rows,
        ),
    ] {
        let of_code = support::walk(
            &runtime,
            "astSearch",
            json!({"queries":[snippet(&source, row.clone())]}),
            rows_of,
        )
        .await;
        // What the walk saved to continue from holds the snippet as shown.
        let saved = saved_files(&workspace.home);
        assert!(!saved.is_empty(), "{row}: the walk saved its continuations");
        for (path, bytes) in &saved {
            assert!(
                !bytes
                    .windows(token.len())
                    .any(|part| part == token.as_bytes()),
                "{row}: {} holds the credential",
                path.display()
            );
        }
        let of_file = support::walk(
            &runtime,
            "astSearch",
            json!({"queries":[support::query_path(&file, row.clone())]}),
            rows_of,
        )
        .await;
        assert!(
            of_code.queries.len() > 2,
            "{row}: {} calls",
            of_code.queries.len()
        );
        for call in &of_code.queries[1..] {
            support::assert_cursor_only(call);
        }
        assert_eq!(of_code.rows, of_file.rows, "{row}");
        assert!(
            of_code.rows.iter().all(|shown| !shown.contains(&token)),
            "{row}"
        );
    }
    runtime.close().await;
}
