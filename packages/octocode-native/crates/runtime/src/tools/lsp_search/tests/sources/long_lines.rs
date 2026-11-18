use octocode_runtime_base::test_support::checks::location_rows;
use octocode_runtime_base::test_support::temp_workspace;
use octocode_test_support::json_fixture;
use octocode_tool_lsp::lsp_search::source::SourceCache;
use octocode_tool_lsp::test_support::{huge_line, query};

/// Every page of a localFetch walk from `call` (a built continuation), joined.
fn walk_local_fetch(
    call: &serde_json::Value,
    paths: &octocode_runtime_base::policy::path::PathPolicy,
) -> String {
    let row = call
        .pointer("/input/queries/0")
        .cloned()
        .expect("query row");
    let mut query: octocode_contract_types::LocalFetchQuery =
        serde_json::from_value(row).expect("localFetch query");
    let security = octocode_runtime_base::security::ContentSecurity::new();
    let mut joined = String::new();
    for _ in 0..1_000 {
        let page = octocode_tool_local::local_fetch::execute_local_fetch(
            &query,
            paths,
            &security,
            &octocode_runtime_base::execution::cancel::NeverCancel,
            None,
            None,
        );
        assert_eq!(page.error_code, None, "{page:?}");
        joined.push_str(page.content.as_deref().expect("content"));
        match page.next.and_then(|next| next.continue_window) {
            Some(next) => query = next.query,
            None => return joined,
        }
    }
    panic!("the walk does not end");
}

/// A declaration on a line too long for its page is a byte window from the
/// declaration's start, cut from the redacted view localFetch pages, sized
/// from the page budget. Its readDeclaration walk (byte offset plus
/// snapshot) reaches the rest of the line exactly once, and the related
/// read of the line reaches the disclosed prefix.
#[tokio::test(flavor = "current_thread")]
async fn a_declaration_on_a_huge_line_is_a_byte_window_its_reads_complete() {
    huge_line_window(serde_json::json!({}), "function target(").await;
}

/// A contextLines window on the same line is bounded the same way, from its
/// anchor (the name the server located).
#[tokio::test(flavor = "current_thread")]
async fn a_context_window_on_a_huge_line_is_a_byte_window_its_reads_complete() {
    huge_line_window(
        json_fixture(r#"{"contextLines": {"before": 0, "after": 0}}"#),
        "target(a)",
    )
    .await;
}

/// The definition row of `function target` on [`huge_line`], asked with
/// `extra` query fields: its window starts with `anchor`, and its reads
/// reach the rest of the line and its prefix exactly once.
async fn huge_line_window(extra: serde_json::Value, anchor: &str) {
    let (_dir, root, paths) = temp_workspace();
    let file = root.join("bundle.js");
    let (line, column) = huge_line();
    std::fs::write(&file, format!("{line}\n")).expect("file");
    let uri = format!("file://{}", file.to_string_lossy());
    let budget = 20_000;
    let mut fields = serde_json::json!({
        "operation": "definition", "path": file.to_string_lossy(), "symbolName": "target", "line": 1
    });
    for (key, value) in extra.as_object().expect("fields") {
        fields[key] = value.clone();
    }
    let q = query(fields);
    let mut sources = SourceCache::new(&paths);
    let row = octocode_tool_lsp::lsp_search::locations::locations(
        &q,
        &mut sources,
        "definition",
        "definitionProvider",
        vec![serde_json::json!({
            "uri": uri,
            "range": {"start": {"line": 0, "character": column + 9}, "end": {"line": 0, "character": column + 15}},
            "content": line,
            "displayRange": {"startLine": 1, "endLine": 1, "startCharacter": column + 1}
        })],
        budget,
        &octocode_runtime_base::security::ContentSecurity::new(),
    )
    .await;
    // The page cost of a location: its public row with its path
    // (`location_sizes`).
    let rows = location_rows(&row);
    let location = rows.first().expect("one location");
    let units = octocode_engine::json_size::utf16_units(location).expect("size");
    assert!(units <= budget, "the row fits its page budget: {units}");
    let content = location["content"].as_str().expect("content");
    let window = content
        .split('\n')
        .find_map(|record| record.strip_prefix("1\t"))
        .expect("numbered window");
    use octocode_runtime_base::security::scan::ContentScan as _;
    let view = octocode_runtime_base::security::ContentSecurity::new()
        .redact_source_lines(&format!("{line}\n"), &file)
        .expect("scan")
        .0
        .into_owned();
    assert_ne!(view, format!("{line}\n"), "the fixture holds a secret");
    let start = view.find("function target(").expect("declaration in view")
        + "function target(a)"
            .find(anchor)
            .expect("anchor in the declaration");
    assert!(window.starts_with(anchor), "{content}");
    assert_eq!(row["isPartial"], true, "{row}");
    assert_eq!(
        row["partialReasons"],
        json_fixture(r#"["declarationClipped"]"#),
        "{row}"
    );
    let read = &row["next"]["readDeclaration"];
    assert_eq!(read["tool"], "localFetch", "{row}");
    let read_row = &read["input"]["queries"][0];
    assert_eq!(read_row["readProgress"]["unit"], "bytes", "{read}");
    assert!(read_row["snapshot"].is_string(), "{read}");
    let rest = walk_local_fetch(read, &paths);
    assert_eq!(
        format!("{window}{rest}"),
        view[start..],
        "window and walk are the declaration to the line end exactly once"
    );
    let related = row["related"]
        .as_array()
        .and_then(|calls| calls.iter().find(|call| call["tool"] == "localFetch"))
        .expect("related line read");
    let whole = walk_local_fetch(related, &paths);
    assert_eq!(whole, view, "the related read reaches the disclosed prefix");
    assert!(
        content.contains(&format!("{start} bytes omitted before the window")),
        "{content}"
    );
}

/// Without contextLines (a request the contract did not default), a
/// declaration longer than a row shows is its first lines, and its
/// readDeclaration reads the rest exactly once.
#[tokio::test(flavor = "current_thread")]
async fn a_long_declaration_shows_its_first_lines_and_reads_the_rest() {
    let (_dir, root, paths) = temp_workspace();
    let file = root.join("long.js");
    let body = (0..80).map(|i| format!("  step{i}();")).collect::<Vec<_>>();
    let declaration = format!("function target() {{\n{}\n}}", body.join("\n"));
    std::fs::write(&file, format!("{declaration}\n")).expect("file");
    let lines = declaration.split('\n').count();
    let q = query(serde_json::json!({
        "operation": "definition", "path": file.to_string_lossy(), "symbolName": "target", "line": 1
    }));
    let mut sources = SourceCache::new(&paths);
    let row = octocode_tool_lsp::lsp_search::locations::locations(
        &q,
        &mut sources,
        "definition",
        "definitionProvider",
        vec![serde_json::json!({
            "uri": format!("file://{}", file.to_string_lossy()),
            "range": {"start": {"line": 0, "character": 9}, "end": {"line": 0, "character": 15}},
            "content": declaration,
            "displayRange": {"startLine": 1, "endLine": lines}
        })],
        600,
        &octocode_runtime_base::security::ContentSecurity::new(),
    )
    .await;
    let rows = location_rows(&row);
    let content = rows[0]["content"].as_str().expect("content");
    let shown = content
        .split('\n')
        .filter_map(|record| record.split_once('\t').map(|(_, text)| text))
        .collect::<Vec<_>>();
    assert!(shown.len() < lines, "the declaration is capped: {content}");
    let read = &row["next"]["readDeclaration"];
    assert_eq!(read["tool"], "localFetch", "{row}");
    let rest = walk_local_fetch(read, &paths);
    assert_eq!(
        format!("{}\n{rest}", shown.join("\n")),
        format!("{declaration}\n"),
        "shown lines and the read are the declaration exactly once"
    );
}
