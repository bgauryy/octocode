//! A saved page whose response is no longer retained re-executes its rows
//! and answers only when the sources reproduce the content its earlier pages
//! were cut from; a lost handle never executes.

use crate::support::{Run as _, Workspace};
use octocode_contract_types::ErrorCode;
use serde_json::{Value, json};
use std::path::Path;

const STORAGE: [(&str, &str); 1] = [("OCTOCODE_STORAGE_MODE", "persistent")];

fn env() -> Vec<(&'static str, String)> {
    STORAGE
        .iter()
        .map(|(key, value)| (*key, (*value).to_owned()))
        .collect()
}

/// Lines with multi-byte text, so every page boundary is Unicode-checked.
fn source() -> String {
    (0..120)
        .map(|index| {
            format!(
                "needle ünïcødé 😀 {index} {}\n",
                "long value ".repeat(index % 7)
            )
        })
        .collect()
}

/// Make every entry under `dir` look unused for longer than any lifetime.
fn age(dir: &Path) {
    for entry in std::fs::read_dir(dir).expect("store").flatten() {
        let file = std::fs::File::options()
            .write(true)
            .open(entry.path())
            .expect("entry");
        file.set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1))
            .expect("age entry");
    }
}

/// Evict every saved response: what a store under memory pressure does.
fn evict(dir: &Path) {
    for entry in std::fs::read_dir(dir).expect("store").flatten() {
        std::fs::remove_file(entry.path()).expect("evict entry");
    }
}

/// Every hit (line and value) of a page, in order.
fn hits(output: &Value) -> Vec<(u64, String)> {
    let mut hits = Vec::new();
    for row in output["results"].as_array().into_iter().flatten() {
        for file in row["data"]["files"].as_array().into_iter().flatten() {
            for hit in file["matches"].as_array().into_iter().flatten() {
                hits.push((
                    hit["line"].as_u64().expect("line"),
                    hit["value"].as_str().unwrap_or_default().to_owned(),
                ));
            }
        }
    }
    hits
}

/// Rows of `output` a batch rejected before execution.
fn rejected(output: &Value) -> usize {
    output["results"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|row| row["data"]["errorCode"] == "invalidInput")
        .count()
}

/// One walk: its first page, every hit in order, and the rejected rows
/// over every page.
type Walked = (Value, Vec<(u64, String)>, usize);

/// Walk every response page from `start`; before each later page, `before`
/// runs (it may age the stores or restart the runtime).
async fn walk(workspace: &Workspace, start: Value, mut before: impl FnMut()) -> Walked {
    let mut runtime = workspace.runtime(&env());
    let first = runtime
        .run("first".into(), "localSearch".into(), start)
        .await
        .expect("first page")
        .structured_content
        .into_value();
    let mut seen = hits(&first);
    let mut rejected_rows = rejected(&first);
    let mut next = first.pointer("/responsePagination/next/input").cloned();
    let mut pages = 1;
    while let Some(input) = next {
        before();
        runtime.close().await;
        runtime = workspace.runtime(&env());
        pages += 1;
        let output = runtime
            .run(format!("page-{pages}"), "localSearch".into(), input)
            .await
            .unwrap_or_else(|error| panic!("page {pages}: {error:?}"))
            .structured_content;
        assert_eq!(output["responsePagination"]["replayed"], true, "{output}");
        seen.extend(hits(&output));
        rejected_rows += rejected(&output);
        next = output.pointer("/responsePagination/next/input").cloned();
    }
    runtime.close().await;
    assert!(pages > 2, "the fixture spans several pages");
    (first, seen, rejected_rows)
}

#[tokio::test]
async fn an_unretained_replay_reexecutes_and_every_page_is_read_exactly_once() {
    let workspace = Workspace::new();
    let file = workspace.write("saved.txt", source());
    let pages = workspace.home.join("tmp/response-pages");
    let start = |chars: usize| json!({"queries":[{"path":file,"matchString":["needle"]}],"responseChars":chars});
    for chars in [600, 3000] {
        let (_, retained, _) = walk(&workspace, start(chars), || {}).await;
        let (_, reexecuted, _) = walk(&workspace, start(chars), || evict(&pages)).await;
        let lines: Vec<u64> = reexecuted.iter().map(|(line, _)| *line).collect();
        assert_eq!(
            lines,
            (1..=120).collect::<Vec<_>>(),
            "every hit exactly once at {chars}"
        );
        assert_eq!(
            reexecuted, retained,
            "a re-executed page is the page the replay held"
        );
    }
}

/// A rejected row rides the first page whole, outside the page plan: as
/// metadata of the call it never makes that page `oversized`, and every
/// admitted hit is read exactly once, whether later pages replay or
/// re-execute, and no later page carries the rejected row again.
#[tokio::test]
async fn a_rejected_row_rides_page_one_without_oversizing_it() {
    let workspace = Workspace::new();
    let file = workspace.write("saved.txt", source());
    let pages = workspace.home.join("tmp/response-pages");
    let start = |chars: usize| {
        json!({"queries":[
            {"path":file,"matchString":["needle"]},
            {"path":file,"matchString":["needle"],"bogusField":1}
        ],"responseChars":chars})
    };
    for chars in [600, 3000] {
        let (first, retained, replayed_rejections) = walk(&workspace, start(chars), || {}).await;
        assert_eq!(
            rejected(&first),
            1,
            "the rejected row rides page 1: {first}"
        );
        assert_eq!(
            replayed_rejections, 1,
            "once over replayed pages at {chars}"
        );
        assert!(
            first.get("oversized").is_none(),
            "a rejected row never makes page 1 oversized at {chars}: {first}"
        );
        let (first, reexecuted, reexecuted_rejections) =
            walk(&workspace, start(chars), || evict(&pages)).await;
        assert_eq!(
            rejected(&first),
            1,
            "the rejected row rides page 1: {first}"
        );
        assert_eq!(
            reexecuted_rejections, 1,
            "once over re-executed pages at {chars}"
        );
        let lines: Vec<u64> = reexecuted.iter().map(|(line, _)| *line).collect();
        assert_eq!(
            lines,
            (1..=120).collect::<Vec<_>>(),
            "every hit exactly once at {chars}"
        );
        assert_eq!(reexecuted, retained, "replay and re-execution agree");
    }
}

/// A search index whose descriptions make a three-repository row span pages.
async fn search_server() -> wiremock::MockServer {
    let server = wiremock::MockServer::builder().start().await;
    let items: Vec<Value> = (0..3)
        .map(|index| {
            json!({
                "full_name": format!("o/r{index}"), "name": format!("r{index}"),
                "html_url": "https://x", "default_branch": "main",
                "description": format!("{index} {}", "words ".repeat(150)),
            })
        })
        .collect();
    wiremock::Mock::given(wiremock::matchers::path("/api/v3/search/repositories"))
        .respond_with(octocode_github::mock_http::SearchIndex(items))
        .mount(&server)
        .await;
    server
}

/// Each row's repositories and `observedAt` values on one page.
fn reuse_facts(output: &Value, seen: &mut [(Vec<String>, Vec<Value>); 2]) {
    for row in output["results"].as_array().into_iter().flatten() {
        let index = usize::try_from(row["index"].as_u64().expect("index")).expect("row");
        let data = &row["data"];
        for repository in data["repositories"].as_array().into_iter().flatten() {
            seen[index].0.push(repository["repo"].to_string());
        }
        seen[index].1.extend(data.get("observedAt").cloned());
    }
}

/// B105: a re-executed page whose run reused saved answers at other moments
/// than the first run (row 0 was live, now reused) still answers: the moment
/// is call metadata (`observedAt`) outside the snapshot. Each row's evidence
/// arrives once, and row 1's moment once, on its first part.
#[tokio::test]
async fn a_reexecution_with_another_reuse_moment_is_not_stale() {
    let server = search_server().await;
    let workspace = Workspace::new();
    let mut settings = env();
    settings.push(("GITHUB_API_URL", format!("{}/api/v3", server.uri())));
    let query = |keyword: &str| json!({"operation":"search","keywords":[keyword],"limit":3});
    let runtime = workspace.runtime(&settings);
    runtime
        .run(
            "warm".into(),
            "ghSearchRepo".into(),
            json!({"queries":[query("warm")]}),
        )
        .await
        .expect("the warm answer is saved");
    runtime.close().await;
    let start = json!({"queries":[query("cold"), query("warm")],"responseChars":1500});
    let mut runtime = workspace.runtime(&settings);
    let first = runtime
        .run("first".into(), "ghSearchRepo".into(), start)
        .await
        .expect("first page")
        .structured_content
        .into_value();
    assert!(
        first["results"]
            .as_array()
            .expect("rows")
            .iter()
            .all(|row| row["index"] == 0),
        "row 1 starts on a re-executed page: {first}"
    );
    let mut seen: [(Vec<String>, Vec<Value>); 2] = Default::default();
    reuse_facts(&first, &mut seen);
    let mut next = first.pointer("/responsePagination/next/input").cloned();
    while let Some(input) = next {
        runtime.close().await;
        evict(&workspace.home.join("tmp/response-pages"));
        runtime = workspace.runtime(&settings);
        let output = runtime
            .run("page".into(), "ghSearchRepo".into(), input)
            .await
            .unwrap_or_else(|error| panic!("a re-executed page answers: {error:?}"))
            .structured_content;
        reuse_facts(&output, &mut seen);
        next = output.pointer("/responsePagination/next/input").cloned();
    }
    runtime.close().await;
    let repositories = ["\"r0\"", "\"r1\"", "\"r2\""].map(str::to_owned).to_vec();
    assert_eq!(seen[0].0, repositories, "row 0 once");
    assert_eq!(seen[1].0, repositories, "row 1 once");
    assert_eq!(
        seen[0].1,
        Vec::<Value>::new(),
        "row 0's first part was live"
    );
    assert_eq!(seen[1].1.len(), 1, "row 1's moment once: {:?}", seen[1].1);
    assert!(
        seen[1].1[0]
            .as_str()
            .is_some_and(|moment| moment.ends_with('Z'))
    );
}

#[tokio::test]
async fn a_changed_source_or_a_lost_handle_never_answers_a_saved_page() {
    let workspace = Workspace::new();
    let file = workspace.write("saved.txt", source());
    let runtime = workspace.runtime(&env());
    let first = runtime
        .run(
            "first".into(),
            "localSearch".into(),
            json!({"queries":[{"path":file,"matchString":["needle"]}],"responseChars":600}),
        )
        .await
        .expect("first page")
        .structured_content;
    let next = first["responsePagination"]["next"]["input"].clone();
    runtime.close().await;
    evict(&workspace.home.join("tmp/response-pages"));
    std::fs::write(
        &file,
        source().replace("needle ünïcødé 😀 7 ", "needle changed 7 "),
    )
    .expect("edit source");
    let runtime = workspace.runtime(&env());
    let error = runtime
        .run("changed".into(), "localSearch".into(), next.clone())
        .await
        .expect_err("changed source");
    assert_eq!(error.code, ErrorCode::StaleSnapshot);
    runtime.close().await;
    std::fs::write(&file, source()).expect("restore source");
    // Unread for longer than the idle lifetime: the handle itself expired.
    age(&workspace.home.join("tmp/continuations"));
    let runtime = workspace.runtime(&env());
    let error = runtime
        .run("lost".into(), "localSearch".into(), next)
        .await
        .expect_err("lost handle");
    assert_eq!(error.code, ErrorCode::ResponseSnapshotExpired);
    assert!(error.message.contains("No source or provider was called"));
    runtime.close().await;
}

/// A saved page edited in place (still valid JSON, so the store keeps it)
/// no longer matches the snapshot its cursor names: the cursor answers as an
/// expired snapshot with a restart, never as a worker failure.
#[tokio::test]
async fn a_saved_page_edited_in_place_answers_a_restart() {
    let workspace = Workspace::new();
    let file = workspace.write("saved.txt", source());
    let runtime = workspace.runtime(&env());
    let first = runtime
        .run(
            "first".into(),
            "localSearch".into(),
            json!({"queries":[{"path":file,"matchString":["needle"]}],"responseChars":900}),
        )
        .await
        .expect("first page")
        .structured_content
        .into_value();
    let next = first
        .pointer("/responsePagination/next/input")
        .cloned()
        .expect("a later page");
    runtime.close().await;
    let mut edited = 0;
    for entry in std::fs::read_dir(workspace.home.join("tmp/response-pages"))
        .expect("store")
        .flatten()
    {
        let text = std::fs::read_to_string(entry.path()).expect("entry");
        if let Some(at) = text.find("\"line\":") {
            let end = at + "\"line\":".len();
            let mut changed = text.clone();
            changed.insert(end, '9');
            std::fs::write(entry.path(), changed).expect("edit entry");
            edited += 1;
        }
    }
    assert!(edited > 0, "a saved page held a line to edit");
    let runtime = workspace.runtime(&env());
    let outcome = runtime
        .run("later".into(), "localSearch".into(), next)
        .await;
    runtime.close().await;
    match outcome {
        Err(error) => {
            assert_eq!(error.code, ErrorCode::ResponseSnapshotExpired, "{error:?}");
        }
        Ok(output) => {
            let output = output.structured_content.into_value();
            assert_eq!(
                output["results"][0]["data"]["errorCode"], "responseSnapshotExpired",
                "{output}"
            );
            assert_eq!(
                output["results"][0]["data"]["errorClass"], "restart",
                "{output}"
            );
        }
    }
}
