//! A batch of reads of a credential-named file larger than one response: the
//! saved response its pages are cut from follows the file's storage policy,
//! so it stays in this process (nothing on disk), a later runtime resumes by
//! re-executing the pinned page, and the pages rebuild every row once.

use super::{MARKER, RAW_FILE, SHA, files_holding, github, opted_in};
use crate::support::{Workspace, github_api};
use serde_json::{Value, json};
use wiremock::matchers::{method, path_regex};
use wiremock::{Mock, ResponseTemplate};

const LINES: usize = 300;
const READS: usize = 3;

/// One read per third of the file: each fits a tool page, the batch does
/// not fit one response, so the response pager saves it and cuts pages.
fn batch(file: &str) -> Value {
    let third = LINES / READS;
    let queries = (0..READS)
        .map(|read| {
            json!({"owner":"octo","repo":"demo","path":file,"ref":SHA,
                "ranges":[{"startLine":read * third + 1,"endLine":(read + 1) * third}]})
        })
        .collect::<Vec<_>>();
    json!({"queries":queries,"responseChars":3_000})
}

/// Each query's content, joined across pages: a part naming `rowPart.text`
/// appends its string to the previous part's; any other part of the same
/// query appends a new piece.
fn join(pages: &[Value]) -> Vec<String> {
    let mut rows = vec![String::new(); READS];
    for page in pages {
        for row in page["results"].as_array().into_iter().flatten() {
            let index = row["index"].as_u64().expect("row index");
            let content = row["data"]["content"].as_str().unwrap_or_default();
            rows[usize::try_from(index).expect("index")].push_str(content);
        }
    }
    rows
}

#[tokio::test]
async fn a_credential_file_paged_by_the_response_pager_leaves_no_copy_and_walks_across_runtimes() {
    let github = github().await;
    let body = (1..=LINES)
        .map(|n| format!("line {n:04} {MARKER}\n"))
        .collect::<String>();
    Mock::given(method("GET"))
        .and(path_regex("/contents/(\\.env|notes\\.txt)$"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("etag", "\"v1\"")
                .set_body_raw(body.clone(), RAW_FILE),
        )
        .with_priority(1)
        .mount(&github)
        .await;
    // The control under the default settings keeps its saved pages on disk;
    // the credential file is read under the opt-in, which keeps them in
    // memory, so its cursors end with the runtime (a walk across runtimes is
    // refused: `direct.rs`). The control also walks with a new runtime per
    // page, resuming from its saved pages on disk.
    let (kept, held) = (github_api(&github).to_vec(), opted_in(&github));
    let mut walks = Vec::new();
    for (file, restart_each_page) in [("notes.txt", false), ("notes.txt", true), (".env", false)] {
        let settings = if file == ".env" { &held } else { &kept };
        let workspace = Workspace::new();
        let mut runtime = workspace.runtime(settings);
        let mut queue = std::collections::VecDeque::from([batch(file)]);
        let mut pages = Vec::new();
        while let Some(input) = queue.pop_front() {
            assert!(pages.len() < 400, "{file}: the walk must end");
            let out = crate::support::Run::run(
                &runtime,
                "walk".into(),
                "ghGetFileContent".into(),
                input.clone(),
            )
            .await
            .expect("page")
            .structured_content
            .into_value();
            for row in out["results"].as_array().into_iter().flatten() {
                assert_ne!(row["status"], "error", "{file}: {out}");
            }
            assert!(
                out.pointer("/responsePagination/oversized").is_none(),
                "{file}: an oversized page: {out}"
            );
            queue.extend(crate::support::next_inputs(&out));
            pages.push(out);
            if restart_each_page {
                runtime.close().await;
                runtime = workspace.runtime(settings);
            }
        }
        runtime.close().await;
        assert!(pages.len() > 1, "{file}: the batch must page");
        let on_disk = files_holding(&workspace.home, MARKER);
        if file == ".env" {
            assert_eq!(
                on_disk,
                Vec::<String>::new(),
                "restart_each_page={restart_each_page}: a copy stayed on disk"
            );
        } else {
            assert!(
                on_disk.iter().any(|path| path.contains("/tmp/")),
                "the control keeps its saved pages, so this scan does see a copy: {on_disk:?}"
            );
        }
        walks.push(join(&pages));
    }
    assert_eq!(
        walks[1], walks[0],
        "a walk across runtimes reads what one runtime does"
    );
    assert_eq!(walks[2], walks[0], "a held walk reads what a kept one does");
    let lines = body.lines().collect::<Vec<_>>();
    let third = LINES / READS;
    for (read, content) in walks[0].iter().enumerate() {
        let expected = lines[read * third..(read + 1) * third].to_vec();
        let shown = content
            .lines()
            .map(|line| line.split_once('\t').map_or(line, |(_, text)| text))
            .collect::<Vec<_>>();
        assert_eq!(shown, expected, "read {read}: every line once, in order");
    }
}
