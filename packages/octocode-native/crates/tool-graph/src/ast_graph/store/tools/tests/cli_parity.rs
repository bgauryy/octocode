//! What only the retired `graph` CLI command answered, through graphQuery:
//! `issues` with its detectors, entries, tiers and baseline, and `find`
//! with no text listing every node.
use super::*;
use octocode_test_support::json_fixture;

fn fixture() -> (Fixture, String) {
    let dir = Fixture::new();
    dir.write("app/package.json", r#"{"name":"app","main":"main.ts"}"#);
    dir.write("app/main.ts", "import { a } from './a';\na();\n");
    dir.write(
        "app/a.ts",
        "import { b } from './b';\nexport function a(){ b(); }\n",
    );
    dir.write(
        "app/b.ts",
        "import { a } from './a';\nexport function b(){ a(); }\n",
    );
    dir.write("app/orphan.ts", "export function lonely(){}\n");
    let built = call(
        &dir.0,
        ToolId::GraphBuild,
        json_fixture(r#"{"path": "app"}"#),
    );
    let id = built["graph"].as_str().expect("id").to_owned();
    (dir, id)
}

fn rows(page: &Value) -> &Vec<Value> {
    page["rows"].as_array().expect("results")
}

#[test]
fn issues_runs_the_selected_detectors() {
    let (dir, _) = fixture();
    let cycles = call(
        &dir.0,
        ToolId::GraphQuery,
        json_fixture(
            r#"{
                "operation": "issues",
                "path": "app",
                "detectors": ["cycle"]
            }"#,
        ),
    );
    assert!(!rows(&cycles).is_empty(), "{cycles}");
    assert!(rows(&cycles).iter().all(|row| row["detector"] == "cycle"));
    assert!(
        rows(&cycles).iter().all(|row| row.get("verify").is_none()),
        "{cycles}"
    );
    assert_eq!(cycles["summary"]["minTier"], 90, "the contract default");
}

#[test]
fn issues_tiers_entries_and_baseline_reach_the_detectors() {
    let (dir, id) = fixture();
    let all = call(
        &dir.0,
        ToolId::GraphQuery,
        json_fixture(
            r#"{
                "operation": "issues",
                "path": "app",
                "minTier": 60,
                "limit": 1000
            }"#,
        ),
    );
    let default = call(
        &dir.0,
        ToolId::GraphQuery,
        json_fixture(r#"{"operation": "issues", "path": "app", "limit": 1000}"#),
    );
    assert!(rows(&all).len() >= rows(&default).len(), "{all}");
    assert_eq!(all["summary"]["minTier"], 60);
    let orphan = |page: &Value| {
        rows(page)
            .iter()
            .any(|row| row["detector"] == "unreachable-file" && row["subject"] == "app/orphan.ts")
    };
    assert!(orphan(&all), "{all}");
    let entered = call(
        &dir.0,
        ToolId::GraphQuery,
        json_fixture(
            r#"{
                "operation": "issues",
                "path": "app",
                "minTier": 60,
                "limit": 1000,
                "entries": ["orphan.ts"]
            }"#,
        ),
    );
    assert!(
        !orphan(&entered),
        "an explicit entry is reachable: {entered}"
    );
    let against = call(
        &dir.0,
        ToolId::GraphQuery,
        json!({"operation":"issues","path":"app","minTier":60,"limit":1000,"baseline":id}),
    );
    assert!(
        rows(&against).iter().all(|row| row["status"] == "existing"),
        "the same snapshot as baseline: {against}"
    );
}

#[test]
fn issues_detector_choices_are_the_engine_detectors() {
    let items = octocode_runtime_base::contracts::query_schema_value(
        ToolId::GraphQuery,
        Some("issues"),
        "detectors",
        "items",
    )
    .expect("detectors items");
    let contract = octocode_runtime_base::contracts::parsed_contract().expect("contract");
    let declared =
        octocode_runtime_base::contracts::schema_keyword(items, &contract["$defs"], "enum")
            .expect("enum keyword")
            .as_array()
            .expect("an enum")
            .iter()
            .map(|name| name.as_str().expect("name"))
            .collect::<Vec<_>>();
    let engine = super::super::super::detect::detector_names().collect::<Vec<_>>();
    assert_eq!(declared, engine);
}

#[test]
fn find_without_text_lists_every_node_of_the_kind() {
    let (dir, _) = fixture();
    let files = call(
        &dir.0,
        ToolId::GraphQuery,
        json_fixture(
            r#"{
                "operation": "find",
                "path": "app",
                "kind": "file",
                "limit": 1000
            }"#,
        ),
    );
    let mut ids = rows(&files)
        .iter()
        .map(|row| row["id"].as_str().expect("id").to_owned())
        .collect::<Vec<_>>();
    ids.sort();
    assert_eq!(
        ids,
        ["app/a.ts", "app/b.ts", "app/main.ts", "app/orphan.ts"].map(String::from),
        "{files}"
    );
    assert_eq!(files["pagination"]["total"], 4);
}
