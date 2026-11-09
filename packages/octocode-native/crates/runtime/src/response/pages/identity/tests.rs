//! C3eK B-R1: a page whose rows all share one repository states it once.
use super::*;
use serde_json::json;

/// A contract that declares `owner`, `repo` and `commitSha` on `data` and
/// lets a `files` or `pullRequests` row omit them; a `commits` row requires
/// its `commitSha`.
fn declared(pointer: &str) -> Option<PageContext> {
    let field = pointer.rsplit('/').next()?;
    match pointer.split('/').nth(2) {
        Some("owner" | "repo" | "commitSha") => Some(PageContext::Optional),
        Some("commits") if field == "commitSha" => Some(PageContext::Required),
        Some(_) if ["owner", "repo", "commitSha"].contains(&field) => {
            Some(PageContext::Optional)
        }
        _ => None,
    }
}

fn file(owner: &str, repo: &str, sha: &str, path: &str) -> Value {
    json!({"owner":owner,"repo":repo,"path":path,"commitSha":sha,"lines":["1\tx"]})
}

/// Each row's repository as an agent rebuilds it: its own fields, else the
/// page's.
fn rebuilt(data: &Map<String, Value>, collection: &str) -> Vec<(Value, Value, Value)> {
    data[collection]
        .as_array()
        .expect("rows")
        .iter()
        .map(|item| {
            let of = |field: &str| item.get(field).or_else(|| data.get(field)).cloned().unwrap_or(Value::Null);
            (of("owner"), of("repo"), of("commitSha"))
        })
        .collect()
}

#[test]
fn an_all_same_page_states_its_repository_once_and_no_row_repeats_it() {
    let original = json!({"files":[file("o","r","s1","a.rs"),file("o","r","s1","b.rs"),file("o","r","s1","c.rs")]});
    let mut data = original.as_object().expect("data").clone();
    let before = rebuilt(&data, "files");
    hoist(&mut data, &declared);
    assert_eq!(data["owner"], "o");
    assert_eq!(data["repo"], "r");
    assert_eq!(data["commitSha"], "s1");
    for item in data["files"].as_array().expect("files") {
        for field in ["owner", "repo", "commitSha"] {
            assert!(item.get(field).is_none(), "{field} repeated: {item}");
        }
        assert!(item.get("path").is_some(), "{item}");
    }
    assert_eq!(rebuilt(&data, "files"), before, "every row's repository rebuilds");
    assert!(
        Value::Object(data).to_string().len() < original.to_string().len(),
        "the page is smaller"
    );
}

#[test]
fn a_mixed_page_keeps_each_rows_own_repository() {
    for files in [
        json!([file("o","r","s1","a.rs"), file("o","q","s2","b.rs")]),
        json!([file("o","r","s1","a.rs"), file("p","r","s1","b.rs")]),
    ] {
        let mut data = json!({"files": files}).as_object().expect("data").clone();
        let before = data.clone();
        hoist(&mut data, &declared);
        assert_eq!(data, before, "a mixed page is unchanged");
    }
    // One repository read at two commits: the repository moves, each
    // row keeps its commit.
    let mut data = json!({"files":[file("o","r","s1","a.rs"), file("o","r","s2","b.rs")]})
        .as_object()
        .expect("data")
        .clone();
    let before = rebuilt(&data, "files");
    hoist(&mut data, &declared);
    assert_eq!((data["owner"].clone(), data["repo"].clone()), (json!("o"), json!("r")));
    assert!(data.get("commitSha").is_none(), "{data:?}");
    assert_eq!(data["files"][0]["commitSha"], "s1");
    assert_eq!(rebuilt(&data, "files"), before);
}

#[test]
fn a_required_identity_and_a_differing_page_value_stay_on_the_rows() {
    let commit = |sha: &str| json!({"owner":"o","repo":"r","commitSha":sha,"date":"d"});
    let mut data = json!({"commits":[commit("c1")]}).as_object().expect("data").clone();
    hoist(&mut data, &declared);
    assert_eq!(data["commits"][0]["commitSha"], "c1", "a commit row keeps its sha");
    assert!(data.get("commitSha").is_none(), "{data:?}");
    assert_eq!(data["owner"], "o");
    let mut data = json!({"commitSha":"other","files":[file("o","r","s1","a.rs")]})
        .as_object()
        .expect("data")
        .clone();
    hoist(&mut data, &declared);
    assert_eq!(data["commitSha"], "other");
    assert_eq!(data["files"][0]["commitSha"], "s1", "{data:?}");
}

#[test]
fn a_field_the_contract_does_not_declare_on_data_stays_on_the_rows() {
    let only_commit = |pointer: &str| match declared(pointer) {
        Some(_) if pointer == "/data/owner" || pointer == "/data/repo" => None,
        kind => kind,
    };
    let mut data = json!({"files":[file("o","r","s1","a.rs"), file("o","r","s1","b.rs")]})
        .as_object()
        .expect("data")
        .clone();
    let before = data.clone();
    hoist(&mut data, &only_commit);
    assert_eq!(data, before, "the repository stays on the rows, and its commit with it");
}

/// The embedded contract drives it: a ghGetHistoryItem pull request states
/// its repository on `data` (declared there), and a ghSearchRepo row, whose
/// `owner` and `repo` are its required identity, keeps them.
#[test]
fn the_embedded_contract_decides_what_moves() {
    let mut history = json!({"results":[{"index":0,"data":{
        "pullRequests":[{"number":1,"title":"t","owner":"o","repo":"r"}]}}]});
    state_shared_identity(ToolId::GhGetHistoryItem, &mut history);
    let data = &history["results"][0]["data"];
    assert_eq!((data["owner"].clone(), data["repo"].clone()), (json!("o"), json!("r")), "{history}");
    assert!(data["pullRequests"][0].get("owner").is_none(), "{history}");
    let schema = &crate::contracts::tool_contract(ToolId::GhGetHistoryItem).expect("contract")["outputSchema"];
    assert_eq!(
        crate::contracts::undeclared::undeclared_paths(schema, &history),
        Vec::<String>::new()
    );
    let mut repos = json!({"results":[{"index":0,"data":{
        "repositories":[{"owner":"o","repo":"r"},{"owner":"o","repo":"r"}]}}]});
    let before = repos.clone();
    state_shared_identity(ToolId::GhSearchRepo, &mut repos);
    assert_eq!(repos, before);
}
