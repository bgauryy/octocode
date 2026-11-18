//! Patch windows of a commit read: a file's own window only where it says
//! more than the page's, every patch rebuilt whole from the walk, and a
//! credential file's withheld patch named as withheld.
use super::{Session, data, served};
use octocode_test_support::json_fixture;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SHA: &str = "2222222222222222222222222222222222222222";
const PARENT: &str = "1111111111111111111111111111111111111111";

async fn mount_commit(server: &MockServer, files: Value) {
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/a/b/commits/{SHA}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "sha": SHA,
            "commit": {"message": "m", "author": {"name": "a", "date": "2026-01-01T00:00:00Z"}},
            "parents": [{"sha": PARENT}],
            "files": files
        })))
        .mount(server)
        .await;
}

/// A changed file of `lines` added lines (`<tag> line <n>`) and its raw patch.
fn added_file(name: &str, tag: &str, lines: usize) -> (Value, String) {
    let body = (0..lines)
        .map(|n| format!("+{tag} line {n}"))
        .collect::<Vec<_>>()
        .join("\n");
    let patch = format!("@@ -0,0 +1,{lines} @@\n{body}");
    let file = json!({"filename": name, "status": "added", "additions": lines, "deletions": 0,
        "changes": lines, "sha": "4".repeat(40), "patch": patch});
    (file, patch)
}

fn patches_read() -> Value {
    json!({"operation": "commit", "owner": "a", "repo": "b", "ref": SHA, "sections": ["patches"]})
}

/// Every page of a patch walk from [`patches_read`], following each page's
/// continuation, as served.
async fn walk(server: &MockServer) -> Vec<Value> {
    let session = Session::new(server);
    let mut pages = Vec::new();
    let mut input = patches_read();
    for _ in 0..100 {
        let page = session.served(input).await;
        let next = data(&page)
            .pointer("/next/patches/input")
            .or_else(|| page.pointer("/responsePagination/next/input"))
            .cloned();
        pages.push(page);
        let Some(next) = next else {
            session.close().await;
            return pages;
        };
        input = next;
    }
    panic!("the patch walk did not end");
}

/// Each file's patch text across `pages`, joined in page order, with the
/// view's new-side line gutter (`<n>\t`) taken off each line.
fn rebuilt(pages: &[Value]) -> BTreeMap<String, String> {
    let mut joined = BTreeMap::<String, String>::new();
    for page in pages {
        for file in data(page)["files"].as_array().into_iter().flatten() {
            if let (Some(name), Some(text)) = (file["path"].as_str(), file["patch"].as_str()) {
                joined.entry(name.to_owned()).or_default().push_str(text);
            }
        }
    }
    joined
        .into_iter()
        .map(|(name, view)| {
            let raw = view
                .lines()
                .map(|line| match line.split_once('\t') {
                    Some((gutter, rest)) if gutter.bytes().all(|b| b.is_ascii_digit()) => rest,
                    _ => line,
                })
                .collect::<Vec<_>>()
                .join("\n");
            (name, raw)
        })
        .collect()
}

/// One file whose patch spans several pages: each page states its window
/// once, in `sectionPagination.patches`, and the file row repeats none of it;
/// the pages rebuild the patch exactly.
#[tokio::test]
async fn a_single_file_walk_states_its_window_once_and_rebuilds_the_patch() {
    let server = MockServer::builder().start().await;
    let (file, patch) = added_file("src/a.rs", "a", 6000);
    mount_commit(&server, json!([file])).await;
    let pages = walk(&server).await;
    assert!(pages.len() > 1, "the patch spans pages");
    for page in &pages {
        let data = data(page);
        assert!(
            data.pointer("/sectionPagination/patches").is_some(),
            "the page states its window: {page}"
        );
        assert!(
            data["files"][0].get("patchPagination").is_none(),
            "the window is stated by sectionPagination.patches: {page}"
        );
    }
    assert_eq!(rebuilt(&pages).get("src/a.rs"), Some(&patch));
}

/// A small file whole, then a large one cut: the large file's own window
/// counts its own characters (it differs from the page's), so it stays on
/// every page of it, and the walk rebuilds both patches exactly.
#[tokio::test]
async fn a_file_cut_after_another_keeps_its_own_window() {
    let server = MockServer::builder().start().await;
    let (small, small_patch) = added_file("src/a.rs", "a", 40);
    let (large, large_patch) = added_file("src/b.rs", "b", 6000);
    mount_commit(&server, json!([small, large])).await;
    let pages = walk(&server).await;
    assert!(pages.len() > 1, "the large patch spans pages");
    let mut file_offset = 0;
    for page in &pages {
        let data = data(page);
        let stream = &data["sectionPagination"]["patches"];
        let large_row = data["files"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|file| file["path"] == "src/b.rs")
            .expect("each page continues src/b.rs");
        let own = &large_row["patchPagination"];
        assert_eq!(own["offset"], file_offset, "file-local offset: {page}");
        assert_ne!(own["total"], stream["total"], "{page}");
        file_offset += own["length"].as_u64().expect("length");
    }
    let rebuilt = rebuilt(&pages);
    assert_eq!(rebuilt.get("src/a.rs"), Some(&small_patch));
    assert_eq!(rebuilt.get("src/b.rs"), Some(&large_patch));
}

/// A credential file's patch is withheld by the path policy: its row says
/// so (`withheld`), not that GitHub sent no patch, and the read is not
/// partial for it; the inventory names the same reason.
#[tokio::test]
async fn a_withheld_patch_says_withheld() {
    let server = MockServer::builder().start().await;
    let (secret, _) = added_file(".env", "SECRET_KEY=hunter2", 5);
    let (ordinary, _) = added_file("src/app.rs", "let ordinary = 1;", 1);
    mount_commit(&server, json!([secret, ordinary])).await;
    let patches = served(&server, patches_read()).await;
    assert!(!patches.to_string().contains("hunter2"), "{patches}");
    let data = data(&patches);
    let row = data["files"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|file| file["path"] == ".env")
        .expect(".env row");
    assert_eq!(row["patchUnavailable"], "withheld", "{patches}");
    assert!(row.get("patch").is_none(), "{patches}");
    for partial in ["isPartial", "terminalLimit"] {
        assert!(row.get(partial).is_none(), "{partial}: {patches}");
        assert!(data.get(partial).is_none(), "{partial}: {patches}");
    }
    assert!(!patches.to_string().contains("tooLarge"), "{patches}");
    let mut list = patches_read();
    list["sections"] = json_fixture(r#"["files"]"#);
    let inventory = served(&server, list).await;
    let rows = super::data(&inventory)["files"].to_string();
    assert!(rows.contains("!withheld .env"), "{inventory}");
    assert!(!rows.contains("tooLarge"), "{inventory}");
}
