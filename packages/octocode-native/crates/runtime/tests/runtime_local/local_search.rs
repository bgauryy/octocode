//! Search semantics and executable handoffs through the runtime.
use crate::support::{Run as _, Workspace, call, checks, row_data, row_status};
use serde_json::{Value, json};

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
/// 1-based character column, on one page. Its path and line form a valid
/// explicit localFetch request for the whole line.
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
        assert!(output.get("next").is_none(), "MCP={mcp}: one page");
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
        // The column marks the window, which no warning restates; any lead is
        // a localFetch read of the hit's own line in the hit's own file.
        assert!(
            !data["warnings"]
                .to_string()
                .contains(data["files"][0]["path"].as_str().expect("path")),
            "MCP={mcp}: {data}"
        );
        for lead in data
            .get("related")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            assert_eq!(lead["tool"], "localFetch", "MCP={mcp}: {data}");
            let queries = lead["input"]["queries"].as_array().expect("queries");
            assert_eq!(queries.len(), 1, "MCP={mcp}: {data}");
            assert_eq!(queries[0]["path"], data["files"][0]["path"]);
            let ranges = queries[0]["ranges"].as_array().expect("ranges");
            assert_eq!(ranges.len(), 1, "MCP={mcp}: {data}");
            assert_eq!(ranges[0]["startLine"], matches[0]["line"]);
            assert_eq!(ranges[0]["endLine"], matches[0]["line"]);
        }
        let hit_line = matches[0]["line"].clone();
        let input = json!({"queries":[{"path":data["files"][0]["path"],"ranges":[{"startLine":hit_line,"endLine":hit_line}]}]});
        let read = call(&runtime, "localFetch", input)
            .await
            .expect("the identity-based line read runs");
        assert_ne!(row_status(&read), "error", "{}", read.structured_content);
        values.push(value.to_owned());
    }
    assert_eq!(values[0], values[1], "CLI and MCP show the same window");
    runtime.close().await;
}

/// A source under the read limit whose Latin-1 decodes past it, holding a
/// token (`QUJD…`) or a plain value of the same text.
fn grown_source(holds_a_token: bool) -> Vec<u8> {
    let value = if holds_a_token {
        "ghp_QUJDQUJDQUJDQUJDQUJDQUJDQUJDQUJDQUJD"
    } else {
        "plain QUJD"
    };
    let mut text = format!(
        "{{\"kind\":\"Config\",\"apiVersion\":\"v1\",\"metadata\":{{\"name\":\"f\"}},\"data\":{{\"a\":\"{value}\"}}}}\n\"needle here\"\n"
    )
    .into_bytes();
    let mut latin1 = vec![0xe9_u8; 79];
    latin1.push(b'\n');
    while text.len() + latin1.len() < 10 * 1024 * 1024 - 1024 {
        text.extend_from_slice(&latin1);
    }
    text
}

/// A source that decodes past the read limit is read, whatever it names: a
/// hit identity forms a read that shows the hit with only a Secret's value
/// hidden.
#[tokio::test]
async fn a_decoded_source_read_preserves_the_hit_and_hides_only_its_secret() {
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
        let data = row_data(&found);
        assert!(data.get("related").is_none(), "{data}");
        let hit = &data["files"][0];
        let end = hit["matches"][0]["line"].as_u64().expect("hit line");
        let input =
            json!({"queries":[{"path":hit["path"],"ranges":[{"startLine":1,"endLine":end}]}]});
        let read = runtime
            .run("read".into(), "localFetch".into(), input)
            .await
            .expect("identity-based read")
            .structured_content;
        assert_ne!(read["results"][0]["status"], "error", "{read}");
        let lines = read["results"][0]["data"].to_string();
        assert!(lines.contains("needle here"), "{read}");
        assert_eq!(lines.contains("QUJD"), value_shown, "{read}");
    }
    runtime.close().await;
}

/// UTF-16 past the whole-file read limit cannot be decoded in windows, so every
/// read of it is refused. Search keeps both hits, and an explicit read of
/// the smaller source still succeeds without generated read advice.
#[tokio::test]
async fn a_utf16_size_refusal_preserves_search_evidence_and_other_reads() {
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
    assert!(row_data(&found).get("related").is_none());
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
    assert_eq!(row_data(&found)["matchCount"], 1);
    assert!(row_data(&found).get("related").is_none());
    let readable_path = row_data(&found)["files"][0]["path"].clone();
    let readable = call(
        &runtime,
        "localFetch",
        json!({"path":readable_path,"ranges":[{"startLine":1,"endLine":2}]}),
    )
    .await
    .expect("small UTF-16 source reads");
    assert_eq!(row_status(&readable), "success");
    assert_eq!(
        row_data(&readable)["content"]
            .as_str()
            .expect("source")
            .lines()
            .collect::<Vec<_>>(),
        ["1\tfirst", "2\tneedle here"]
    );
    let both = call(&runtime, "localSearch", search(&workspace.workspace))
        .await
        .expect("search");
    assert_eq!(
        row_data(&both)["matchCount"],
        2,
        "{}",
        both.structured_content
    );
    assert!(row_data(&both).get("related").is_none());
    assert!(row_data(&both)["files"].to_string().contains("b-under.txt"));
    assert!(row_data(&both)["files"].to_string().contains("a-over.txt"));
    runtime.close().await;
}

/// A value in another form its schema gives one reading (one value for a
/// list, a number as JSON text) runs, and the response carries one warning
/// naming the field and the form to send. A field under another name is
/// never read: the row is refused, and the refusal names the declared field.
#[tokio::test]
async fn a_schema_derived_reading_runs_and_an_undeclared_name_is_refused() {
    let workspace = Workspace::new();
    workspace.write("src/a.txt", "defects-queue\nother\n");
    let runtime = workspace.runtime(&[]);
    let root = workspace.workspace.clone();
    let file = root.join("src/a.txt");
    for (tool, row, field) in [
        (
            "localSearch",
            json!({"path":root,"pattern":"defects-queue"}),
            "matchString",
        ),
        (
            "localSearch",
            json!({"path":root,"matchString":["defects"],"excludeDir":"x"}),
            "exclude",
        ),
        (
            "localFetch",
            json!({"path":file,"startLine":1,"endLine":1}),
            "ranges",
        ),
    ] {
        let refused = call(&runtime, tool, row.clone())
            .await
            .err()
            .unwrap_or_else(|| panic!("{tool} {row} runs"));
        let shown = format!("{:?}", refused.payload);
        assert!(
            shown.contains("invalidInput") && shown.contains(field),
            "{tool} {row}: {shown}"
        );
    }
    for (tool, row, field) in [
        (
            "localSearch",
            json!({"path":root,"matchString":["defects"],"exclude":"x"}),
            "exclude: [\"x\"]",
        ),
        ("localFetch", json!({"path":file,"limit":"1"}), "limit: 1"),
    ] {
        let found = call(&runtime, tool, row.clone()).await.expect("runs");
        assert_eq!(
            row_status(&found),
            "success",
            "{tool} {row}: {}",
            found.structured_content
        );
        let warnings = found.structured_content["warnings"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|warning| warning.as_str())
            .filter(|warning| warning.contains("next time"))
            .collect::<Vec<_>>();
        assert!(
            matches!(warnings.as_slice(), [one]
                if one.contains(&format!("send `{}", field.split(':').next().unwrap_or(field)))
                    && one.contains(field)),
            "{tool} {row}: {}",
            found.structured_content
        );
        // It ran: the hit's file (search views) or its line (reads) is shown.
        let shown = row_data(&found).to_string();
        assert!(
            shown.contains("a.txt") || shown.contains("defects-queue"),
            "{tool} {row}: {}",
            found.structured_content
        );
    }
    runtime.close().await;
}
