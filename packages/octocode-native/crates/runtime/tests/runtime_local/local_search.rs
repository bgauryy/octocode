//! Search semantics and executable handoffs through the runtime.
use crate::support::{Run as _, Workspace, call, checks, row_data, row_status};
use serde_json::json;

#[tokio::test]
async fn literal_and_regex_matchers_have_explicit_distinct_semantics() {
    let workspace = Workspace::new();
    workspace.write("src/example.txt", "a.c\nabc\nx|y\nx\ny\n");
    let runtime = workspace.runtime(&[]);
    for (engine, expected) in [("literal", vec![1, 3]), ("rust", vec![1, 2, 3, 4, 5])] {
        let found=call(&runtime,"localSearch",json!({"path":workspace.workspace,"matchString":["a.c","x|y"],"regexEngine":engine,"contextLines":{"before":0,"after":0}})).await.expect("search");
        assert_eq!(row_status(&found), "success");
        let lines = row_data(&found)["files"][0]["matches"]
            .as_array()
            .expect("matches")
            .iter()
            .map(|row| row["line"].as_u64().expect("line"))
            .collect::<Vec<_>>();
        assert_eq!(lines, expected, "{engine}: {}", found.structured_content);
    }
    runtime.close().await;
}

#[tokio::test]
async fn excluded_source_is_disclosed_and_reachable_by_an_explicit_request() {
    let workspace = Workspace::new();
    for name in [
        "src/core.c",
        "drivers/target/transport.c",
        "dist/bundle.js",
        "Cargo.lock",
        "vendor/lib.min.js",
    ] {
        workspace.write(name, "transport_free_cmd\n");
    }
    let runtime = workspace.runtime(&[]);
    let found = call(
        &runtime,
        "localSearch",
        json!({"path":workspace.workspace,"matchString":["transport_free_cmd"],"view":"files"}),
    )
    .await
    .expect("search");
    let paths = |data: &serde_json::Value| {
        data["files"]
            .as_array()
            .expect("files")
            .iter()
            .map(|file| file["path"].as_str().expect("path").to_owned())
            .collect::<std::collections::BTreeSet<_>>()
    };
    assert_eq!(
        paths(row_data(&found)),
        ["Cargo.lock".into(), "src/core.c".into()].into()
    );
    assert!(
        !row_data(&found)["skippedPaths"]["entries"]
            .as_array()
            .expect("disclosure")
            .is_empty()
    );
    // A page with hits answers: its skipped paths are disclosed, not
    // re-offered as a lead; the request that lifts them is the caller's.
    assert!(
        !row_data(&found)["related"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|call| call["input"]["queries"][0]["defaultExcludes"] == false),
        "{}",
        row_data(&found)
    );
    let all = call(
        &runtime,
        "localSearch",
        json!({"queries":[{"path":workspace.workspace,"matchString":["transport_free_cmd"],"view":"files","defaultExcludes":false}]}),
    )
    .await
    .expect("the explicit request executes");
    assert_eq!(
        paths(row_data(&all)),
        [
            "src/core.c",
            "drivers/target/transport.c",
            "dist/bundle.js",
            "Cargo.lock",
            "vendor/lib.min.js"
        ]
        .map(str::to_owned)
        .into()
    );
    runtime.close().await;
}

/// A hit in a Unicode line too long for a row: on both transports the row is
/// one whole-character window of the line around the hit, placed by its
/// 1-based character column, on one page; the line whole is the ready
/// `localFetch` read of that line, which runs.
#[tokio::test]
async fn long_unicode_lines_show_a_window_around_the_hit_and_lead_to_the_whole_line() {
    let workspace = Workspace::new();
    let line = format!(
        "{}needleHere{} END_OF_SOURCE",
        "é ".repeat(30_000),
        " tail".repeat(1_000)
    );
    workspace.write("src/minified.txt", format!("head\n{line}\ntail\n"));
    let runtime = workspace.runtime(&[]);
    let hit = line.find("needleHere").expect("hit");
    let column = line[..hit].chars().count() + 1;
    let mut values = Vec::new();
    for mcp in [false, true] {
        let input = json!({"queries":[{
            "path":workspace.workspace,"matchString":["needleHere"]
        }]});
        let request = format!("unicode-{mcp}");
        let output = if mcp {
            let result = runtime
                .run_mcp(request, "localSearch".into(), input)
                .await
                .expect("MCP search");
            crate::support::mcp_payload(&result)
        } else {
            runtime
                .run(request, "localSearch".into(), input)
                .await
                .expect("CLI search")
                .structured_content
                .into_value()
        };
        checks::assert_output_contract("localSearch", &output);
        assert!(
            output.get("responsePagination").is_none(),
            "MCP={mcp}: one page"
        );
        let rows = output["results"].as_array().expect("rows");
        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert!(
            row.get("rowPart").is_none(),
            "MCP={mcp}: a window, not parts"
        );
        let data = &row["data"];
        let matches = data["files"][0]["matches"].as_array().expect("matches");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0]["line"], 2);
        assert_eq!(matches[0]["column"], column, "MCP={mcp}");
        let value = matches[0]["value"].as_str().expect("text window");
        let start = line.find(value).expect("the window is the line's own text");
        assert!(start < hit && hit + "needleHere".len() < start + value.len());
        assert!(
            value.len() < line.len() / 10,
            "MCP={mcp}: a window of the line"
        );
        let lead = checks::related_call(data, "localFetch", |input| {
            input["queries"][0]["ranges"] == json!([{"startLine":2,"endLine":2}])
        });
        let read = call(&runtime, "localFetch", lead["input"].clone())
            .await
            .expect("the line read runs");
        assert_ne!(row_status(&read), "error", "{}", read.structured_content);
        values.push(value.to_owned());
    }
    assert_eq!(values[0], values[1], "CLI and MCP show the same window");
    runtime.close().await;
}

/// A source under the read limit whose Latin-1 decodes past it, naming a Secret
/// or not.
fn grown_source(names_a_secret: bool) -> Vec<u8> {
    let kind = if names_a_secret { "Secret" } else { "Config" };
    let mut text = format!(
        "{{\"kind\":\"{kind}\",\"apiVersion\":\"v1\",\"metadata\":{{\"name\":\"f\"}},\"data\":{{\"a\":\"QUJD\"}}}}\n\"needle here\"\n"
    )
    .into_bytes();
    let mut latin1 = vec![0xe9_u8; 79];
    latin1.push(b'\n');
    while text.len() + latin1.len() < 10 * 1024 * 1024 - 1024 {
        text.extend_from_slice(&latin1);
    }
    text
}

/// The read lead of `found`, when it offers one.
fn read_leads(found: &serde_json::Value) -> Vec<serde_json::Value> {
    found["related"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|lead| lead["tool"] == "localFetch")
        .map(|lead| lead["input"].clone())
        .collect()
}

/// A source that decodes past the read limit is read, whatever it names: a
/// search offers the read, and that read shows the hit with only a Secret's
/// value hidden.
#[tokio::test]
async fn a_read_is_offered_of_a_source_decoding_past_the_limit_and_hides_only_its_secret() {
    let workspace = Workspace::new();
    let secret = workspace.write("a-secret.json", grown_source(true));
    let plain = workspace.write("b-plain.json", grown_source(false));
    let runtime = workspace.runtime(&[]);
    for (path, value_shown) in [(&secret, false), (&plain, true)] {
        let found = call(
            &runtime,
            "localSearch",
            json!({"path":path,"matchString":["needle"]}),
        )
        .await
        .expect("search");
        assert_eq!(
            row_status(&found),
            "success",
            "{:?}",
            found.structured_content
        );
        assert_eq!(row_data(&found)["matchCount"], 1);
        let leads = read_leads(row_data(&found));
        assert_eq!(leads.len(), 1, "{:?}", found.structured_content);
        let read = runtime
            .run("read".into(), "localFetch".into(), leads[0].clone())
            .await
            .expect("read")
            .structured_content;
        assert_ne!(read["results"][0]["status"], "error", "{read}");
        let lines = read["results"][0]["data"].to_string();
        assert!(lines.contains("needle here"), "{read}");
        assert_eq!(lines.contains("QUJD"), value_shown, "{read}");
    }
    runtime.close().await;
}

/// UTF-16 past the whole-file read limit cannot be decoded in windows, so every
/// read of it is refused: its hit stays, no read is offered, and beside it in a
/// search of both the readable file is the one offered.
#[tokio::test]
async fn no_read_is_offered_of_utf16_past_the_read_limit() {
    let workspace = Workspace::new();
    let text = format!(
        "first\nneedle here\n{}",
        "padding of the fixture\n".repeat(300_000)
    );
    let utf16 = |text: &str| {
        let mut bytes = vec![0xff, 0xfe];
        bytes.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
        bytes
    };
    let over = workspace.write("a-over.txt", utf16(&text));
    let under = workspace.write("b-under.txt", utf16("first\nneedle here\n"));
    let runtime = workspace.runtime(&[]);
    let search = |path: &std::path::Path| json!({"path":path,"matchString":["needle"]});
    let found = call(&runtime, "localSearch", search(&over))
        .await
        .expect("search");
    assert_eq!(row_data(&found)["matchCount"], 1);
    assert_eq!(
        read_leads(row_data(&found)),
        Vec::<serde_json::Value>::new()
    );
    let read = call(
        &runtime,
        "localFetch",
        json!({"path":over,"ranges":[{"startLine":1,"endLine":8}]}),
    )
    .await
    .expect("read");
    assert_eq!(row_data(&read)["errorCode"], "fileTooLarge");
    let found = call(&runtime, "localSearch", search(&under))
        .await
        .expect("search");
    assert_eq!(read_leads(row_data(&found)).len(), 1, "a small one reads");
    let both = call(&runtime, "localSearch", search(&workspace.workspace))
        .await
        .expect("search");
    let leads = read_leads(row_data(&both));
    assert_eq!(leads.len(), 1, "{:?}", both.structured_content);
    assert!(
        leads[0]["queries"][0]["path"]
            .as_str()
            .is_some_and(|path| path.ends_with("b-under.txt")),
        "{leads:?}"
    );
    runtime.close().await;
}
