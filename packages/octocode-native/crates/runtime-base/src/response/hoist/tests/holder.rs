//! F1 holders. C3eK B-R1: a page whose rows all share one repository states
//! it once; a graph traversal page states its `from` once. Moved from
//! `pages/identity` with the stage; the behavior is unchanged.
use super::super::holder::state_on;
use super::super::plan;
use super::*;
use crate::test_support::assert_part_contract;
use octocode_test_support::json_fixture;
use serde_json::Map;

/// Today's holder groups of `tool` (each on `data`) over one row's `data`,
/// `declared` answering at row pointers (`/data/…`).
fn hoist(
    data: &mut Map<String, Value>,
    tool: ToolId,
    declared: &dyn Fn(&str) -> Option<PageContext>,
) {
    for holder in &plan::of(tool).holders {
        state_on(data, holder, &|member| declared(&format!("/data{member}")));
    }
}

/// The stage over a whole envelope.
fn state_shared_identity(tool: ToolId, envelope: &mut Value) {
    apply(tool, envelope);
}

/// A contract that declares `owner`, `repo` and `commitSha` on `data` and
/// lets a `files` or `pullRequests` row omit them; a `commits` row requires
/// its `commitSha`.
fn declared(pointer: &str) -> Option<PageContext> {
    let field = pointer.rsplit('/').next()?;
    match pointer.split('/').nth(2) {
        Some("owner" | "repo" | "commitSha") => Some(PageContext::Optional),
        Some("commits") if field == "commitSha" => Some(PageContext::Required),
        Some(_) if ["owner", "repo", "commitSha"].contains(&field) => Some(PageContext::Optional),
        _ => None,
    }
}

fn file(owner: &str, repo: &str, sha: &str, path: &str) -> Value {
    json!({"owner":owner,"repo":repo,"path":path,"commitSha":sha,"matches":[{"line":1,"value":"x"}]})
}

/// Each row's repository as an agent rebuilds it: its own fields, else the
/// page's.
fn rebuilt(data: &Map<String, Value>, collection: &str) -> Vec<(Value, Value, Value)> {
    data[collection]
        .as_array()
        .expect("rows")
        .iter()
        .map(|item| {
            let of = |field: &str| {
                item.get(field)
                    .or_else(|| data.get(field))
                    .cloned()
                    .unwrap_or(Value::Null)
            };
            (of("owner"), of("repo"), of("commitSha"))
        })
        .collect()
}

#[test]
fn an_all_same_page_states_its_repository_once_and_no_row_repeats_it() {
    let original = json!({"files":[file("o","r","s1","a.rs"),file("o","r","s1","b.rs"),file("o","r","s1","c.rs")]});
    let mut data = original.as_object().expect("data").clone();
    let before = rebuilt(&data, "files");
    hoist(&mut data, ToolId::GhSearchCode, &declared);
    assert_eq!(data["owner"], "o");
    assert_eq!(data["repo"], "r");
    assert_eq!(data["commitSha"], "s1");
    for item in data["files"].as_array().expect("files") {
        for field in ["owner", "repo", "commitSha"] {
            assert!(item.get(field).is_none(), "{field} repeated: {item}");
        }
        assert!(item.get("path").is_some(), "{item}");
    }
    assert_eq!(
        rebuilt(&data, "files"),
        before,
        "every row's repository rebuilds"
    );
    assert!(
        Value::Object(data).to_string().len() < original.to_string().len(),
        "the page is smaller"
    );
}

/// Hoist G-H3 (regen 4): a page of one owner's repositories states the
/// owner once; each row keeps its own repository and commit. A page of
/// several owners is unchanged.
#[test]
fn a_mixed_page_states_a_shared_owner_once_and_keeps_each_rows_own_repository() {
    let files = json!([file("o", "r", "s1", "a.rs"), file("o", "q", "s2", "b.rs")]);
    let mut data = json!({"files": files}).as_object().expect("data").clone();
    let before = rebuilt(&data, "files");
    hoist(&mut data, ToolId::GhSearchCode, &declared);
    assert_eq!(data["owner"], "o", "{data:?}");
    assert!(
        data.get("repo").is_none() && data.get("commitSha").is_none(),
        "{data:?}"
    );
    assert_eq!(data["files"][1]["repo"], "q");
    assert_eq!(
        rebuilt(&data, "files"),
        before,
        "every row's repository rebuilds"
    );
    let files = json!([file("o", "r", "s1", "a.rs"), file("p", "r", "s1", "b.rs")]);
    let mut data = json!({"files": files}).as_object().expect("data").clone();
    let before = data.clone();
    hoist(&mut data, ToolId::GhSearchCode, &declared);
    assert_eq!(data, before, "a page of two owners is unchanged");
    // One repository read at two commits: the repository moves, each
    // row keeps its commit.
    let mut data = json!({"files":[file("o","r","s1","a.rs"), file("o","r","s2","b.rs")]})
        .as_object()
        .expect("data")
        .clone();
    let before = rebuilt(&data, "files");
    hoist(&mut data, ToolId::GhSearchCode, &declared);
    assert_eq!(
        (data["owner"].clone(), data["repo"].clone()),
        (json!("o"), json!("r"))
    );
    assert!(data.get("commitSha").is_none(), "{data:?}");
    assert_eq!(data["files"][0]["commitSha"], "s1");
    assert_eq!(rebuilt(&data, "files"), before);
}

#[test]
fn a_required_identity_and_a_differing_page_value_stay_on_the_rows() {
    let commit = |sha: &str| json!({"owner":"o","repo":"r","commitSha":sha,"committedAt":"d"});
    let mut data = json!({"commits":[commit("c1")]})
        .as_object()
        .expect("data")
        .clone();
    hoist(&mut data, ToolId::GhSearchCode, &declared);
    assert_eq!(
        data["commits"][0]["commitSha"], "c1",
        "a commit row keeps its sha"
    );
    assert!(data.get("commitSha").is_none(), "{data:?}");
    assert_eq!(data["owner"], "o");
    let mut data = json!({"commitSha":"other","files":[file("o","r","s1","a.rs")]})
        .as_object()
        .expect("data")
        .clone();
    hoist(&mut data, ToolId::GhSearchCode, &declared);
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
    hoist(&mut data, ToolId::GhSearchCode, &only_commit);
    assert_eq!(
        data, before,
        "the repository stays on the rows, and its commit with it"
    );
}

/// The embedded contract drives it: a ghGetHistoryItem pull request states
/// its repository on `data`, and ghSearchRepo rows from the same owner
/// share that owner while keeping each repository's name.
#[test]
fn the_embedded_contract_decides_what_moves() {
    let mut history = json_fixture(
        r#"{
            "results": [
                {
                    "index": 0,
                    "data": {
                        "pullRequests": [{"number": 1, "title": "t", "owner": "o", "repo": "r"}]
                    }
                }
            ]
        }"#,
    );
    state_shared_identity(ToolId::GhGetHistoryItem, &mut history);
    let data = &history["results"][0]["data"];
    assert_eq!(
        (data["owner"].clone(), data["repo"].clone()),
        (json!("o"), json!("r")),
        "{history}"
    );
    assert!(data["pullRequests"][0].get("owner").is_none(), "{history}");
    let schema = &crate::contracts::tool_contract(ToolId::GhGetHistoryItem).expect("contract")["outputSchema"];
    assert_eq!(
        crate::contracts::undeclared::undeclared_paths(schema, &history),
        Vec::<String>::new()
    );
    let mut repos = json_fixture(
        r#"{
            "results": [
                {
                    "index": 0,
                    "data": {
                        "repositories": [{"owner": "o", "repo": "r"}, {"owner": "o", "repo": "r"}]
                    }
                }
            ]
        }"#,
    );
    state_shared_identity(ToolId::GhSearchRepo, &mut repos);
    let data = &repos["results"][0]["data"];
    assert_eq!(data["owner"], "o", "{repos}");
    for row in data["repositories"].as_array().expect("repositories") {
        assert!(row.get("owner").is_none(), "{repos}");
        assert_eq!(
            row["repo"], "r",
            "a row keeps its own required repo: {repos}"
        );
    }
}

/// A graphQuery `callers`/`callees` response of `rows` listed rows, as the
/// engine builds them: each row states the traversal node it was reached
/// `from` and its `edge`. `from` names that node for row `n`.
fn traversal(operation: &str, rows: usize, from: &dyn Fn(usize) -> String) -> Value {
    let rows = (0..rows)
        .map(|n| {
            json!({"id":format!("f{n}.ts#caller{n}"),"kind":"function","from":from(n),
                "edge":"calls","confidence":"high","edgeLine":n + 1,"via":"local","depth":1,
                "symbolName":format!("caller{n}"),"path":format!("f{n}.ts"),"line":n + 1,
                "endLine":n + 2})
        })
        .collect::<Vec<_>>();
    json!({"results":[{"index":0,"data":{
        "graph":{"id":"g","root":"/r","createdAt":"2026-11-09T00:00:00Z"},
        "operation":operation,"edges":["calls"],"rows":rows,
        "freshness":{"fresh":true,"analysisCurrent":true}}}]})
}

/// Each row's `from` as a reader of one page rebuilds it: the row's own,
/// else the page's.
fn rebuilt_traversal(data: &Value) -> Vec<Value> {
    data["rows"]
        .as_array()
        .expect("rows")
        .iter()
        .map(|row| {
            let mut full = row.clone();
            if full.get("from").is_none()
                && let Some(stated) = data.get("from")
            {
                full["from"] = stated.clone();
            }
            full
        })
        .collect()
}

/// A `callers` page states the `from` every row shares once, as the output
/// contract declares it, and the contract still accepts the page.
#[test]
fn a_callers_page_states_what_its_rows_share_once() {
    let original = traversal("callers", 6, &|_| "a.ts#target".into());
    let mut page = original.clone();
    state_shared_identity(ToolId::GraphQuery, &mut page);
    let data = &page["results"][0]["data"];
    let rows = original["results"][0]["data"]["rows"]
        .as_array()
        .expect("rows");
    assert_eq!(data["from"], rows[0]["from"], "from stated once: {page}");
    for row in data["rows"].as_array().expect("rows") {
        assert!(row.get("from").is_none(), "from repeated: {row}");
    }
    assert_eq!(rebuilt_traversal(data), rows.to_vec(), "every row rebuilds");
    crate::test_support::assert_row_contract(ToolId::GraphQuery, data);
    assert!(
        page.to_string().len() < original.to_string().len(),
        "the page is smaller"
    );
}

/// Rows reached from different nodes (a deeper walk) keep their own `from`,
/// and `edge` stays on every row.
#[test]
fn a_traversal_field_that_differs_between_rows_stays_on_the_rows() {
    let original = traversal("callees", 4, &|n| format!("a.ts#target{n}"));
    let mut page = original.clone();
    state_shared_identity(ToolId::GraphQuery, &mut page);
    let data = &page["results"][0]["data"];
    assert!(data.get("from").is_none(), "{page}");
    assert!(
        data.get("edge").is_none(),
        "an edge stays on its rows: {page}"
    );
    for row in data["rows"].as_array().expect("rows") {
        assert!(row["from"].is_string(), "{row}");
    }
    assert_eq!(
        rebuilt_traversal(data),
        original["results"][0]["data"]["rows"]
            .as_array()
            .expect("rows")
            .to_vec()
    );
    crate::test_support::assert_row_contract(ToolId::GraphQuery, data);
}

/// Every page of a walk at any response budget states the facts its rows
/// omit, so each page reads alone, and the pages deliver every row once.
#[test]
fn every_page_of_a_tiny_budget_walk_reads_alone() {
    use crate::response::page_model::ResponsePageOptions;
    let original = traversal("callers", 12, &|_| "a.ts#target".into());
    let expected = original["results"][0]["data"]["rows"]
        .as_array()
        .expect("rows")
        .to_vec();
    let mut page = original;
    state_shared_identity(ToolId::GraphQuery, &mut page);
    assert!(
        page["results"][0]["data"].get("from").is_some(),
        "the page states its traversal once: {page}"
    );
    let structured = page.as_object().expect("envelope").clone();
    let options = |budget: usize, offset: usize, snapshot: Option<String>| ResponsePageOptions {
        response_chars: Some(budget),
        response_offset: Some(offset),
        response_snapshot: snapshot,
        paging: true,
        saved: None,
        ..ResponsePageOptions::default()
    };
    for budget in [400, 600, 900, 1500, 2500] {
        let (_, first) = crate::response::row_pages::paginate_rows(
            structured.clone(),
            &options(budget, 0, None),
            Vec::new(),
            ToolId::GraphQuery,
        );
        assert!(
            first.total_pages > 1 || budget >= 1500,
            "budget {budget} splits the walk"
        );
        let mut delivered = Vec::new();
        for offset in 0..first.total_pages {
            let (envelope, _) = crate::response::row_pages::paginate_rows(
                structured.clone(),
                &options(budget, offset, Some(first.snapshot.clone())),
                Vec::new(),
                ToolId::GraphQuery,
            );
            for row in envelope["results"].as_array().expect("results") {
                let data = &row["data"];
                assert_part_contract(ToolId::GraphQuery, row);
                delivered.extend(rebuilt_traversal(data));
            }
        }
        assert_eq!(
            delivered, expected,
            "budget {budget}: each row once, readable alone"
        );
    }
}

/// F1 at any depth (hoist C-9c): the `source` every match of a file shares
/// is stated on the file, innermost holder first, and the file's `owner`,
/// which every file shares, then moves on to `data`. A file whose matches
/// differ keeps them, and every value reads back by the reading rule.
#[test]
fn a_holder_at_any_depth_states_what_its_items_share_innermost_first() {
    let plan = super::plan(
        Vec::new(),
        vec![
            super::holder("", &["owner"], &[]),
            super::holder("/files/*", &["source"], &[]),
        ],
        Default::default(),
    );
    let original = json!({"results": [{"index": 0, "data": {"files": [
        {"path": "a.ts", "owner": "o", "matches": [
            {"line": 2, "source": "recoveredImporter"},
            {"line": 7, "source": "recoveredImporter"},
        ]},
        {"path": "b.ts", "owner": "o", "matches": [
            {"line": 3, "source": "references"},
            {"line": 9, "source": "recoveredImporter"},
        ]},
    ], "next": {"nextPage": {"tool": "lspSearch", "input": {"cursor": "c"}}}}}]});
    let stated = super::hoisted(&plan, &original, &super::optional);
    let data = &stated["results"][0]["data"];
    assert_eq!(data["owner"], "o", "{stated}");
    assert_eq!(data["files"][0]["source"], "recoveredImporter", "{stated}");
    assert!(
        data["files"][0]["matches"][0].get("source").is_none(),
        "{stated}"
    );
    assert!(data["files"][1].get("source").is_none(), "{stated}");
    assert_eq!(data["files"][1]["matches"][0]["source"], "references");
    assert_eq!(data["next"], original["results"][0]["data"]["next"]);
    super::assert_recovers(&plan, &original, &stated);
}

/// A field the contract does not let an item omit, or does not declare on
/// the holder, never moves, at any depth.
#[test]
fn a_nested_holder_moves_only_what_the_contract_declares_there() {
    let plan = super::plan(
        Vec::new(),
        vec![super::holder("/files/*", &["source"], &[])],
        Default::default(),
    );
    let original = json!({"results": [{"index": 0, "data": {"files": [{"path": "a.ts", "matches": [
        {"line": 2, "source": "recoveredImporter"},
        {"line": 7, "source": "recoveredImporter"},
    ]}]}}]});
    let required =
        |pointer: &str| (!pointer.ends_with("/matches/0/source")).then_some(PageContext::Optional);
    assert_eq!(super::hoisted(&plan, &original, &required), original);
    let undeclared =
        |pointer: &str| (pointer != "/data/files/0/source").then_some(PageContext::Optional);
    assert_eq!(super::hoisted(&plan, &original, &undeclared), original);
}
