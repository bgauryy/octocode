//! graphQuery `drift` answers within the published output contract: it
//! names the baseline it compares with as it names its own snapshot.
use super::*;

/// A drift between two builds of one scope answers, every row within the
/// contract ([`call`] checks each against it): the baseline by its id, the
/// change it counts, and the changed file's row.
#[test]
fn drift_answers_within_the_output_contract() {
    let dir = Fixture::new();
    dir.write("app/a.ts", "export const a = 1;\n");
    let base = call(
        &dir.0,
        ToolId::GraphBuild,
        json_fixture(r#"{"path": "app"}"#),
    );
    let base_id = base["id"].as_str().expect("id").to_owned();
    dir.write("app/a.ts", "export const a = 2;\n");
    let head = call(
        &dir.0,
        ToolId::GraphBuild,
        json_fixture(r#"{"path": "app"}"#),
    );
    assert_ne!(head["id"], json!(base_id), "{head}");
    let drift = call(
        &dir.0,
        ToolId::GraphQuery,
        json!({"operation": "drift", "path": "app", "baseline": base_id}),
    );
    assert_eq!(drift["baseline"]["id"], json!(base_id), "{drift}");
    assert_eq!(drift["summary"]["filesChanged"], 1, "{drift}");
    let changed = json_fixture(r#"{"category": "file", "change": "changed", "path": "app/a.ts"}"#);
    assert!(
        drift["rows"]
            .as_array()
            .is_some_and(|rows| rows.contains(&changed)),
        "{drift}"
    );
}

/// A drift whose head was never rebuilt after an edit compares the baseline
/// with itself and finds nothing: the empty answer states the head is stale,
/// says why it may be empty, and leads to the graphBuild that sees the edit.
#[test]
fn an_empty_drift_on_a_stale_head_leads_to_a_rebuild() {
    let dir = Fixture::new();
    dir.write("app/a.ts", "export const a = 1;\n");
    let base = call(
        &dir.0,
        ToolId::GraphBuild,
        json_fixture(r#"{"path": "app"}"#),
    );
    let base_id = base["id"].as_str().expect("id").to_owned();
    let fresh = call(
        &dir.0,
        ToolId::GraphQuery,
        json!({"operation": "drift", "path": "app", "baseline": base_id}),
    );
    assert!(fresh.get("freshness").is_none(), "{fresh}");
    assert!(
        fresh.get("hints").is_none(),
        "a fresh empty drift is the answer: {fresh}"
    );
    dir.write("app/a.ts", "export const a = 2;\n");
    let drift = call(
        &dir.0,
        ToolId::GraphQuery,
        json!({"operation": "drift", "path": "app", "baseline": base_id}),
    );
    assert_eq!(drift["freshness"]["fresh"], false, "{drift}");
    assert!(drift["hints"].to_string().contains("graphBuild"), "{drift}");
    let rebuild = drift["related"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|call| call["tool"] == "graphBuild")
        .unwrap_or_else(|| panic!("no rebuild lead: {drift}"));
    let query = &rebuild["input"]["queries"][0];
    assert_eq!(query["path"], "app", "{rebuild}");
    octocode_runtime_base::contracts::validate_built_query("graphBuild", query.clone())
        .unwrap_or_else(|error| panic!("the rebuild is not runnable: {error:?}"));
}
