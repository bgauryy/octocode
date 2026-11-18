//! A classified row-listing page keeps only the leads that follow its shown
//! rows: a read of a row classification cut steers to excluded evidence.
use super::*;
use octocode_runtime_base::id::ToolId;
use octocode_test_support::json_fixture;
use serde_json::Value;
use serde_json::json;

fn lead(tool: &str, read: Value) -> Value {
    json!({"kind":"related","tool":tool,"input":{"queries":[read]}})
}

fn tools(data: &Value) -> Vec<Value> {
    data["related"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|lead| lead["input"]["queries"][0].clone())
        .collect()
}

fn classify(tool: ToolId, data: Value, query: &Value) -> Value {
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let runtime = runtime(FixJudge, &executor);
    let mut structured = json!({"results":[{"index":0,"data":data}]});
    let mut response_query = query.clone();
    apply(tool, &mut structured, query, &mut response_query, &runtime);
    structured["results"][0]["data"].take()
}

#[test]
fn a_listing_drops_the_read_of_an_entry_classification_cut() {
    let query = json_fixture(
        r#"{
            "path": "src",
            "operation": "files",
            "classification": {"predicates": ["Is this the fix?"]}
        }"#,
    );
    let listing = |read: &str| {
        json!({
            "files":[{"dir":"src","files":["fix.rs (10)","other.rs (20)"]}],
            "related":[
                lead("localFetch", json!({"path":read,"view":"symbols"})),
                lead("structureSearch", json_fixture(r#"{"path": "src", "operation": "tree"}"#)),
            ],
        })
    };
    let cut = classify(ToolId::StructureSearch, listing("src/other.rs"), &query);
    assert_eq!(
        cut["files"],
        json_fixture(r#"[{"dir": "src", "files": ["fix.rs (10)"]}]"#)
    );
    assert_eq!(
        tools(&cut),
        vec![json_fixture(r#"{"path": "src", "operation": "tree"}"#)],
        "{cut}"
    );
    let kept = classify(ToolId::StructureSearch, listing("src/fix.rs"), &query);
    assert_eq!(tools(&kept).len(), 2, "{kept}");
}

#[test]
fn a_history_page_drops_the_reads_of_rows_classification_cut() {
    let query = json_fixture(
        r#"{
            "owner": "o",
            "repo": "r",
            "operation": "pullRequest",
            "keywords": ["x"],
            "classification": {"predicates": ["Is this a bug fix?"]}
        }"#,
    );
    let read = |number: u64| {
        lead(
            "ghGetHistoryItem",
            json!({"operation":"pullRequest","owner":"o","repo":"r","number":number}),
        )
    };
    let broaden = lead(
        "ghSearchHistory",
        json_fixture(r#"{"operation": "pullRequest", "owner": "o", "repo": "r"}"#),
    );
    let data = json!({
        "pullRequests":[{"number":1,"title":"fix: crash"},{"number":2,"title":"docs: intro"}],
        "related":[read(2), read(1), broaden],
    });
    let data = classify(ToolId::GhSearchHistory, data, &query);
    let numbers = tools(&data)
        .iter()
        .map(|read| read["number"].clone())
        .collect::<Vec<_>>();
    assert_eq!(numbers, vec![json!(1), Value::Null], "{data}");
}

#[test]
fn a_commit_page_drops_the_reads_and_searches_that_follow_a_cut_commit() {
    let query = query();
    let data = json!({
        "commits":[
            {"commitSha":"aaa","messageHeadline":"fix: crash"},
            {"commitSha":"bbb","messageHeadline":"docs: intro (#7)"},
        ],
        "related":[
            lead("ghGetHistoryItem", json_fixture(r#"{
                "operation": "commit",
                "owner": "o",
                "repo": "r",
                "ref": "bbb"
            }"#)),
            lead("ghGetHistoryItem", json_fixture(r#"{
                "operation": "pullRequest",
                "owner": "o",
                "repo": "r",
                "number": 7
            }"#)),
            lead("ghSearchHistory", json_fixture(r#"{
                "operation": "pullRequest",
                "owner": "o",
                "repo": "r",
                "keywords": ["bbb"]
            }"#)),
            lead("ghGetHistoryItem", json_fixture(r#"{
                "operation": "commit",
                "owner": "o",
                "repo": "r",
                "ref": "aaa"
            }"#)),
        ],
    });
    let data = classify(ToolId::GhSearchHistory, data, &query);
    assert_eq!(
        tools(&data),
        vec![json_fixture(
            r#"{
                "operation": "commit",
                "owner": "o",
                "repo": "r",
                "ref": "aaa"
            }"#
        )],
        "{data}"
    );
}
