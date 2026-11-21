//! How the parts of one search page are stated and sized: search
//! `pagination` counts the walk's files (shown before, and left), the rest of
//! a page is the response's continuation, a part reads at most twice the rows it shows, each file is
//! read from GitHub once, a page's shift is stated once, a
//! page that fits the window is shown whole, and a cursor page states the
//! repository its rows share once, on its `data`.
use super::{Call, Index, files, hit, read_path, requests, walk, walk_with};
use crate::support::{self, Run as _, Workspace};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use octocode_test_support::json_fixture;
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex};
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

/// Files whose text holds `needles[path]` needle lines each: a row lists more
/// hit lines (and takes more of the window) the more needles its file holds.
struct Sized(HashMap<String, usize>);

impl Respond for Sized {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let file = read_path(request);
        let mut text = format!("fn needle() {{ /* {file} */ }}\nlet other = 1;\n");
        for line in 0..self.0.get(&file).copied().unwrap_or(1) {
            text.push_str(&format!(
                "// needle {line}: a note about {file} with some padding text to make the line long enough\n"
            ));
        }
        ResponseTemplate::new(200).set_body_json(json!({
            "type": "file", "encoding": "base64", "content": STANDARD.encode(text)
        }))
    }
}

/// A code index whose hits can change between page reads.
struct Moving(Arc<Mutex<Vec<Value>>>);

impl Respond for Moving {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let items = self.0.lock().expect("index").clone();
        Index(items).respond(request)
    }
}

/// A repo-scoped search of `names`, every 7th file holding 40 needle lines
/// (a large row) and the rest one to three.
async fn mount_mixed(server: &MockServer, names: &[String]) {
    let items = names
        .iter()
        .enumerate()
        .map(|(n, file)| hit(file, n + 1))
        .collect::<Vec<_>>();
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(Index(items))
        .mount(server)
        .await;
    let needles = names
        .iter()
        .enumerate()
        .map(|(n, file)| (file.clone(), if n % 7 == 3 { 40 } else { 1 + n % 3 }))
        .collect();
    for file in names {
        octocode_test_support::mock_http::list_in_trees(server, "o", "r", file).await;
    }
    Mock::given(method("GET"))
        .and(path_regex(r"^/api/v3/repos/o/r/contents/"))
        .respond_with(Sized(needles))
        .mount(server)
        .await;
}

fn start(limit: Option<usize>, chars: usize) -> Value {
    let mut row = json_fixture(r#"{"owner": "o", "repo": "r", "keywords": ["needle"]}"#);
    if let Some(limit) = limit {
        row["limit"] = json!(limit);
    }
    json!({"queries": [row], "responseChars": chars})
}

/// A window holding about half of a 20-file search page of `names`,
/// measured on a server of its own: it shows such a page in parts whatever
/// a published row takes.
async fn half_a_page(names: &[String]) -> usize {
    let server = MockServer::builder().start().await;
    super::mount(&server, names, &[]).await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&support::github_api(&server));
    let page = runtime
        .run(
            "measure".into(),
            "ghSearchCode".into(),
            start(Some(20), 1_000_000),
        )
        .await
        .expect("measured page")
        .structured_content
        .into_value();
    runtime.close().await;
    page.to_string().chars().count() / 2
}

fn shown(calls: &[Call]) -> BTreeSet<String> {
    calls.iter().flat_map(|call| call.listed.clone()).collect()
}

/// A search of one page (`limit` 100) and of three (`limit` 20), shown in
/// parts: every response's `pagination` starts after the files the earlier
/// responses showed and states the exact total; the reply's `next` is
/// present exactly while a file is left, on another search page or in the
/// rest of this one, and no row carries a page walk of its own.
#[tokio::test]
async fn a_part_continues_as_the_response_and_pagination_counts_search_pages() {
    for (limit, pages) in [(100, 1), (20, 3)] {
        let server = MockServer::builder().start().await;
        let names = files(50, |n| format!("src/f{n:03}.rs"));
        super::mount(&server, &names, &[]).await;
        let workspace = Workspace::new();
        let runtime = workspace.runtime(&support::github_api(&server));
        let window = half_a_page(&names).await;
        let calls = walk(&server, &runtime, start(Some(limit), window)).await;
        assert!(calls.len() > pages, "the window shows pages in parts");
        assert_eq!(shown(&calls), names.iter().cloned().collect());
        let mut before = 0;
        for (at, call) in calls.iter().enumerate() {
            let data = &call.output["results"][0]["data"];
            assert!(
                data.pointer("/next/nextPage").is_none(),
                "call {at}: the reply's next is the one walk: {data}"
            );
            if let Some(pagination) = data.get("pagination") {
                assert_eq!(pagination["offset"], before, "call {at}: {pagination}");
                assert_eq!(pagination["total"], names.len(), "call {at}: {pagination}");
                assert!(
                    pagination.get("hasMore").is_none() && pagination.get("countScope").is_none(),
                    "call {at}: {pagination}"
                );
            }
            before += call.listed.len();
            assert_eq!(
                call.output["next"].is_object(),
                at + 1 < calls.len(),
                "limit {limit} call {at}: next is present exactly while a file is left"
            );
        }
        runtime.close().await;
    }
}

/// A part reserves no room for anything it does not serve: wherever a
/// search page goes on in another part, the row that opens that part would
/// not have fit this one. (A page's first row once reserved room for a read
/// call no reply carried, so parts ended a row early.)
#[tokio::test]
async fn a_part_ends_only_where_its_next_row_does_not_fit() {
    let server = MockServer::builder().start().await;
    let names = files(40, |n| format!("src/f{n:03}.rs"));
    super::mount(&server, &names, &[]).await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&support::github_api(&server));
    let window = half_a_page(&names).await;
    let calls = walk(&server, &runtime, start(Some(100), window)).await;
    assert_eq!(shown(&calls), names.iter().cloned().collect());
    let chars = |value: &Value| value.to_string().chars().count();
    let mut continued = 0;
    for pair in calls.windows(2) {
        if !pair[0].output["next"].is_object() {
            continue;
        }
        continued += 1;
        let next_row = &pair[1].output["results"][0]["data"]["files"][0];
        // A part reserves the reply's continuation at its widest: beside
        // `next`, the expiry and replay facts a saved page states, and the
        // call's `kind`, which a served call drops. A fresh part states
        // neither, so that much room may stay unused; nothing else may.
        let widest = r#","expiresAt":"2026-01-01T00:00:00.000Z","replayed":true"#.len()
            + r#""kind":"continue","#.len();
        assert!(
            chars(&pair[0].output) + chars(next_row) + widest > window,
            "a part of {} chars left room for a row of {} in a window of {window}",
            chars(&pair[0].output),
            chars(next_row)
        );
    }
    assert!(continued > 0, "the window shows the page in parts");
    runtime.close().await;
}

/// Mixed row sizes: every part reads at most three times the rows it shows
/// (a batch holds at most twice the rows already shown), and the walk reaches
/// every hit once.
#[tokio::test]
async fn a_part_reads_at_most_three_times_the_rows_it_shows() {
    let server = MockServer::builder().start().await;
    let names = files(130, |n| format!("src/f{n:03}.rs"));
    mount_mixed(&server, &names).await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&support::github_api(&server));
    let calls = walk(&server, &runtime, start(None, 5000)).await;
    assert_eq!(shown(&calls), names.iter().cloned().collect());
    for (at, call) in calls.iter().enumerate() {
        assert!(
            call.reads.len() <= 3 * call.listed.len(),
            "call {at} read {} files to show {}: {:?}",
            call.reads.len(),
            call.listed.len(),
            call.reads
        );
    }
    runtime.close().await;
}

/// A walk reads each file from GitHub once: a file an earlier part read is
/// answered from the walk's evidence. No row states a read count: what the
/// page cost is no evidence for the agent.
#[tokio::test]
async fn a_walk_reads_each_file_once_and_states_no_read_count() {
    let server = MockServer::builder().start().await;
    let names = files(130, |n| format!("src/f{n:03}.rs"));
    mount_mixed(&server, &names).await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&support::github_api(&server));
    let calls = walk(&server, &runtime, start(None, 5000)).await;
    for (at, call) in calls.iter().enumerate() {
        let data = &call.output["results"][0]["data"];
        assert!(data.get("contentReads").is_none(), "call {at}: {data}");
    }
    let (_, reads) = requests(&server).await;
    assert_eq!(reads.len(), names.len(), "each file is read once");
    runtime.close().await;
}

/// The index moves after the first call, so page 2 lists a page-1 file
/// again and states a new match count: shown in parts, that page states the
/// shift once, on its first part.
#[tokio::test]
async fn a_page_shown_in_parts_states_its_shift_once() {
    let server = MockServer::builder().start().await;
    let names = files(50, |n| format!("src/f{n:03}.rs"));
    let index = Arc::new(Mutex::new(
        names
            .iter()
            .enumerate()
            .map(|(n, file)| hit(file, n + 1))
            .collect::<Vec<_>>(),
    ));
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(Moving(Arc::clone(&index)))
        .mount(&server)
        .await;
    for file in &names {
        octocode_test_support::mock_http::list_in_trees(&server, "o", "r", file).await;
    }
    Mock::given(method("GET"))
        .and(path_regex(r"^/api/v3/repos/o/r/contents/"))
        .respond_with(super::Files {
            failing: Vec::new(),
        })
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&support::github_api(&server));
    let mut moved = false;
    let window = half_a_page(&names).await;
    let calls = walk_with(&server, &runtime, start(Some(20), window), |_| {
        if !moved {
            index
                .lock()
                .expect("index")
                .insert(0, hit("src/inserted.rs", 999));
            moved = true;
        }
    })
    .await;
    let stated = |text: &str| {
        calls
            .iter()
            .map(|call| call.output.to_string().matches(text).count())
            .sum::<usize>()
    };
    assert!(calls.len() > 3, "pages are shown in parts");
    assert_eq!(stated("GitHub listed again"), 1, "the dropped file once");
    assert_eq!(stated("match count changed"), 1, "the new count once");
    runtime.close().await;
}

/// A page whose whole answer fits the window is shown whole, with no
/// continuation, although less than a fixed row skeleton is left over; a
/// window just short of it still reaches every hit once.
#[tokio::test]
async fn a_page_that_fits_the_window_is_shown_whole() {
    let server = MockServer::builder().start().await;
    let names = files(50, |n| format!("src/f{n:03}.rs"));
    super::mount(&server, &names, &[]).await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&support::github_api(&server));
    let whole = runtime
        .run(
            "whole".into(),
            "ghSearchCode".into(),
            start(None, 1_000_000),
        )
        .await
        .expect("whole page")
        .structured_content
        .into_value();
    let chars = whole.to_string().chars().count();
    let fitted = runtime
        .run(
            "fitted".into(),
            "ghSearchCode".into(),
            start(None, chars + 300),
        )
        .await
        .expect("fitted page")
        .structured_content
        .into_value();
    assert_eq!(super::listed(&fitted).len(), names.len(), "{fitted}");
    assert!(fitted.get("next").is_none(), "{fitted}");
    assert!(
        fitted["results"][0]["data"].get("next").is_none(),
        "{fitted}"
    );
    let short = walk(&server, &runtime, start(None, chars - 50)).await;
    assert!(short.len() > 1, "a window short of the page continues");
    assert_eq!(shown(&short), names.iter().cloned().collect());
    runtime.close().await;
}

/// The rows of one repository state `owner` and `repo` once, on the page.
/// A fresh page drops them as echoes of its query; a cursor page keeps
/// them (its query is only the cursor), so each page alone names its
/// rows' repository, and no row repeats it.
#[tokio::test]
async fn a_cursor_page_states_its_rows_repository_once() {
    let server = MockServer::builder().start().await;
    let names = files(50, |n| format!("src/f{n:03}.rs"));
    super::mount(&server, &names, &[]).await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&support::github_api(&server));
    let window = half_a_page(&names).await;
    let query = start(Some(20), window);
    let calls = walk(&server, &runtime, query.clone()).await;
    assert!(calls.len() > 1, "the walk reaches cursor pages");
    assert_eq!(shown(&calls), names.iter().cloned().collect());
    for (at, call) in calls.iter().enumerate() {
        let data = &call.output["results"][0]["data"];
        for file in data["files"].as_array().expect("files") {
            for field in ["owner", "repo"] {
                assert!(
                    file.get(field).is_none(),
                    "call {at}: a row repeats {field}: {file}"
                );
            }
        }
        for field in ["owner", "repo"] {
            let stated = data.get(field);
            if at == 0 {
                assert!(
                    stated.is_none(),
                    "the fresh page drops the {field} echo: {data}"
                );
            } else {
                assert_eq!(
                    stated,
                    Some(&query["queries"][0][field]),
                    "call {at}: a cursor page states {field}: {data}"
                );
            }
        }
    }
    runtime.close().await;
}
