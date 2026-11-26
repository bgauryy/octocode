//! A page whose rows do not all fit the response window whole shares it:
//! the largest rows list a run of their hits, state `matchCount`, and their
//! `readHits` read lists the rest, so every row of the page is shown.
use super::*;
use octocode_runtime_base::response::numbered::SEPARATOR;
use octocode_runtime_base::response::read_share::published_chars;

/// `a/b`'s search of `files` (each `(name, body)`) at [`SHA`].
async fn mount_files(server: &MockServer, files: &[(&str, String)]) {
    let items = files
        .iter()
        .map(|(name, _)| {
            json!({"name":name,"path":format!("src/{name}"),"sha":"1","html_url":"https://x",
                "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"}})
        })
        .collect::<Vec<_>>();
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count":files.len(),"incomplete_results":false,"items":items
        })))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/HEAD"))
        .respond_with(ResponseTemplate::new(200).set_body_string(SHA))
        .mount(server)
        .await;
    for (name, body) in files {
        octocode_test_support::mock_http::list_in_trees(server, "a", "b", &format!("src/{name}"))
            .await;
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/a/b/contents/src%2F{name}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "type":"file","encoding":"base64","content":STANDARD.encode(body)
            })))
            .mount(server)
            .await;
    }
}

fn row<'a>(data: &'a serde_json::Value, name: &str) -> &'a serde_json::Value {
    data["files"]
        .as_array()
        .and_then(|rows| rows.iter().find(|row| row["path"] == format!("src/{name}")))
        .unwrap_or_else(|| panic!("src/{name} is listed: {data}"))
}

/// The source line a listed entry names.
fn line(entry: &serde_json::Value) -> u64 {
    entry
        .as_str()
        .and_then(|entry| entry.split_once(SEPARATOR))
        .and_then(|(number, _)| number.parse().ok())
        .expect("a numbered entry")
}

#[tokio::test]
async fn the_largest_row_lists_a_run_and_reads_the_rest() {
    let hits = 120_u64;
    let busy = (1..=hits)
        .map(|n| format!("let value_{n} = needle({n}); // a long enough line of padding text\n"))
        .collect::<String>();
    let files = [
        ("busy.rs", busy),
        ("calm.rs", "fn needle() {}\nlet x = needle();\n".to_owned()),
        ("quiet.rs", "fn needle() {}\n".to_owned()),
    ];
    let server = MockServer::builder().start().await;
    mount_files(&server, &files).await;
    let provider = one_shot_provider(&server);
    let query: GhSearchCodeQuery =
        serde_json::from_value(json!({"owner":"a","repo":"b","keywords":["needle"]}))
            .expect("query");
    let context = default_context();
    let withholds = &octocode_runtime_base::policy::discovery::is_sensitive_repository_file;
    let page = |window: Option<usize>| {
        super::super::run(
            &provider,
            &query,
            Ok(&context),
            &Passthrough,
            withholds,
            window,
        )
    };
    let whole = page(None).await.expect("whole page");
    let whole_chars = published_chars(&whole.data).row;
    assert_eq!(
        row(&whole.data, "busy.rs")["matches"]
            .as_array()
            .map(Vec::len),
        Some(120),
        "without a window every hit is listed"
    );
    let window = whole_chars / 2;
    let out = page(Some(window)).await.expect("shared page");
    let data = &out.data;
    assert!(!out.diagnostics.continues_page, "one part: {data}");
    assert!(data["next"].get("nextPage").is_none(), "{data}");
    for name in ["calm.rs", "quiet.rs"] {
        let small = row(data, name);
        assert_eq!(
            small["matches"],
            row(&whole.data, name)["matches"],
            "{name}"
        );
        assert!(small.get("matchCount").is_none(), "{name}: listed whole");
    }
    let busy = row(data, "busy.rs");
    let listed = busy["matches"].as_array().expect("a run of hits");
    assert!(!listed.is_empty() && listed.len() < 120, "{busy}");
    assert_eq!(busy["matchCount"], hits, "{busy}");
    let read = &data["next"]["readHits"];
    let input = &read["input"]["queries"][0];
    assert_eq!(read["tool"], "ghGetFileContent", "{read}");
    assert_eq!(input["path"], "src/busy.rs", "{read}");
    assert_eq!(input["ref"], SHA, "{read}");
    assert_eq!(input["matchString"], json!(["needle"]), "{read}");
    // The run lists the first hits; the read starts right after them and
    // runs to the file's end.
    let last = listed.last().map(line).expect("a listed line");
    assert_eq!(listed.first().map(line), Some(1), "{busy}");
    assert_eq!(
        input["ranges"],
        json!([{"startLine": last + 1, "endLine": hits}]),
        "{read}"
    );
    // As published: the rows' shared identity is stated once on the page.
    let mut published = data.as_object().cloned().expect("data");
    octocode_runtime_base::response::hoist::apply_on(
        octocode_runtime_base::id::ToolId::GhSearchCode,
        &mut published,
    );
    let chars = published_chars(&serde_json::Value::Object(published)).row;
    assert!(
        chars <= window,
        "the page fits its window: {chars} > {window}"
    );
}

/// A one-page search shown in parts states, on every part, how many files
/// the search found (`pagination.total`) and where it starts in the walk:
/// `offset` counts the files the earlier parts showed, and `hasMore` holds
/// while the page has files left (the part continues as the response).
#[tokio::test]
async fn every_part_of_a_page_states_the_files_found() {
    let files = [
        (
            "one.rs",
            "fn needle() { /* the first file */ }\n".to_owned(),
        ),
        (
            "two.rs",
            "fn needle() { /* the second file */ }\n".to_owned(),
        ),
        (
            "three.rs",
            "fn needle() { /* the third file */ }\n".to_owned(),
        ),
    ];
    let server = MockServer::builder().start().await;
    mount_files(&server, &files).await;
    let provider = one_shot_provider(&server);
    let query: GhSearchCodeQuery =
        serde_json::from_value(json!({"owner":"a","repo":"b","keywords":["needle"]}))
            .expect("query");
    let context = default_context();
    let withholds = &octocode_runtime_base::policy::discovery::is_sensitive_repository_file;
    let first = super::super::run(
        &provider,
        &query,
        Ok(&context),
        &Passthrough,
        withholds,
        Some(1),
    )
    .await
    .expect("first part");
    assert!(first.diagnostics.continues_page, "{}", first.data);
    let pagination = &first.data["pagination"];
    assert_eq!(pagination["total"], files.len(), "{}", first.data);
    assert_eq!(pagination["offset"], 0, "{}", first.data);
    assert_eq!(pagination["hasMore"], true, "{}", first.data);
    let shown = first.data["files"].as_array().map_or(0, Vec::len);
    assert!(shown > 0 && shown < files.len(), "{}", first.data);
    let rest = next_page(&first);
    let second = super::super::run(
        &provider,
        &rest,
        Ok(&context),
        &Passthrough,
        withholds,
        Some(1),
    )
    .await
    .expect("second part");
    assert_eq!(
        second.data["pagination"]["total"],
        files.len(),
        "{}",
        second.data
    );
    let pagination = &second.data["pagination"];
    assert_eq!(pagination["offset"], shown, "{}", second.data);
    let left = shown + second.data["files"].as_array().map_or(0, Vec::len) < files.len();
    assert_eq!(pagination["hasMore"], left, "{}", second.data);
    assert_eq!(second.diagnostics.continues_page, left, "{}", second.data);
}

/// A cut row lists the declarations of the hits it lists and of no other,
/// and a page whose rows state declarations still fits its window: a
/// declaration is priced as its hit's lines are.
#[tokio::test]
async fn a_cut_row_states_only_the_declarations_of_its_listed_hits() {
    let blocks = 80_u64;
    // Every hit line opens a three-line function: each declares a block.
    let busy = (1..=blocks)
        .map(|n| format!("fn needle_{n}(input: usize) -> usize {{\n    input + {n}\n}}\n"))
        .collect::<String>();
    let files = [
        ("busy.rs", busy),
        ("calm.rs", "fn needle() {\n    work();\n}\n".to_owned()),
    ];
    let server = MockServer::builder().start().await;
    mount_files(&server, &files).await;
    let provider = one_shot_provider(&server);
    let query: GhSearchCodeQuery =
        serde_json::from_value(json!({"owner":"a","repo":"b","keywords":["needle"]}))
            .expect("query");
    let context = default_context();
    let withholds = &octocode_runtime_base::policy::discovery::is_sensitive_repository_file;
    let page = |window: Option<usize>| {
        super::super::run(
            &provider,
            &query,
            Ok(&context),
            &Passthrough,
            withholds,
            window,
        )
    };
    let published = |data: &serde_json::Value| {
        let mut published = data.as_object().cloned().expect("data");
        octocode_runtime_base::response::hoist::apply_on(
            octocode_runtime_base::id::ToolId::GhSearchCode,
            &mut published,
        );
        serde_json::Value::Object(published)
    };
    let whole = page(None).await.expect("whole page");
    let whole = published(&whole.data);
    let all = row(&whole, "busy.rs")["declarations"]
        .as_array()
        .cloned()
        .expect("declarations");
    assert_eq!(all.len() as u64, blocks, "every hit declares a block");
    let window = published_chars(&whole).row / 2;
    let out = page(Some(window)).await.expect("shared page");
    let served = published(&out.data);
    let busy = row(&served, "busy.rs");
    let listed = busy["matches"]
        .as_array()
        .expect("a run of hits")
        .iter()
        .map(line)
        .collect::<Vec<_>>();
    assert!(
        !listed.is_empty() && (listed.len() as u64) < blocks,
        "{busy}"
    );
    // The declarations of the listed hits, exactly as the whole row states them.
    let expected = all
        .iter()
        .filter(|entry| listed.contains(&line(entry)))
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(busy["declarations"], json!(expected), "{busy}");
    assert_eq!(expected.len(), listed.len(), "{busy}");
    let chars = published_chars(&served).row;
    assert!(
        chars <= window,
        "the page fits its window: {chars} > {window}"
    );
}

/// A section several hits state leaves a cut row with the last of them, so
/// a cut is priced by the sections its own hits state: a Markdown row cut to
/// fit lists the largest run the window holds, not its smallest.
#[tokio::test]
async fn a_cut_markdown_row_lists_the_largest_run_that_fits() {
    let (sections, hits_each) = (30_u64, 4_u64);
    let busy = (1..=sections)
        .map(|n| {
            let body = (1..=hits_each)
                .map(|hit| format!("The needle of part {n}, line {hit}, with some padding text.\n"))
                .collect::<String>();
            format!("## Part {n} of the guide\n\n{body}\n")
        })
        .collect::<String>();
    let files = [
        ("busy.md", busy),
        ("calm.md", "## Calm\n\nOne needle.\n".to_owned()),
    ];
    let server = MockServer::builder().start().await;
    mount_files(&server, &files).await;
    let provider = one_shot_provider(&server);
    let query: GhSearchCodeQuery =
        serde_json::from_value(json!({"owner":"a","repo":"b","keywords":["needle"]}))
            .expect("query");
    let context = default_context();
    let withholds = &octocode_runtime_base::policy::discovery::is_sensitive_repository_file;
    let page = |window: Option<usize>| {
        super::super::run(
            &provider,
            &query,
            Ok(&context),
            &Passthrough,
            withholds,
            window,
        )
    };
    let published = |data: &serde_json::Value| {
        let mut published = data.as_object().cloned().expect("data");
        octocode_runtime_base::response::hoist::apply_on(
            octocode_runtime_base::id::ToolId::GhSearchCode,
            &mut published,
        );
        serde_json::Value::Object(published)
    };
    let whole = published(&page(None).await.expect("whole page").data);
    let all = row(&whole, "busy.md")["matches"]
        .as_array()
        .map(Vec::len)
        .expect("matches");
    assert_eq!(all as u64, sections * hits_each, "{whole}");
    let whole_chars = published_chars(&whole).row;
    let window = whole_chars / 2;
    let served = published(&page(Some(window)).await.expect("shared page").data);
    let chars = published_chars(&served).row;
    assert!(chars <= window, "the page fits: {chars} > {window}");
    let busy = row(&served, "busy.md");
    let listed = busy["matches"].as_array().map(Vec::len).expect("a run");
    assert!(listed < all, "{busy}");
    // One more section of hits does not fit the room the page leaves.
    let section_chars = whole_chars / usize::try_from(sections).expect("sections");
    assert!(
        window - chars < 2 * section_chars,
        "the row lists {listed} of {all} hits and leaves {} of {window} chars unused",
        window - chars
    );
}
