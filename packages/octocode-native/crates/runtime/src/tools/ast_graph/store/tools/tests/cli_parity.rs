//! What only the retired `graph` CLI command answered, through graphQuery:
//! `issues` with its detectors, entries, tiers and baseline, and `find`
//! with no text listing every node.
use super::*;

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
    let built = call(&dir.0, ToolId::GraphBuild, json!({"path":"app"}));
    let id = built["id"].as_str().expect("id").to_owned();
    (dir, id)
}

fn rows(page: &Value) -> &Vec<Value> {
    page["rows"].as_array().expect("results")
}

#[test]
fn issues_runs_the_selected_detectors_with_runnable_pinned_verify_steps() {
    let (dir, id) = fixture();
    let cycles = call(
        &dir.0,
        ToolId::GraphQuery,
        json!({"operation":"issues","path":"app","detectors":["cycle"]}),
    );
    assert!(!rows(&cycles).is_empty(), "{cycles}");
    assert!(rows(&cycles).iter().all(|row| row["detector"] == "cycle"));
    assert_eq!(cycles["summary"]["minTier"], 90, "the contract default");
    let steps = rows(&cycles)
        .iter()
        .flat_map(|row| row["verify"].as_array().cloned().unwrap_or_default())
        .collect::<Vec<_>>();
    assert!(!steps.is_empty(), "{cycles}");
    for step in steps {
        assert_eq!(step["kind"], "related", "{step}");
        assert_eq!(step["tool"], "graphQuery", "{step}");
        let row = step["input"]["queries"][0].clone();
        assert_eq!(row["graph"], json!(id), "{step}");
        assert_eq!(row["path"], "app", "{step}");
        crate::contracts::validate_built_query("graphQuery", row).expect("a valid graphQuery row");
    }
}

#[test]
fn issues_tiers_entries_and_baseline_reach_the_detectors() {
    let (dir, id) = fixture();
    let all = call(
        &dir.0,
        ToolId::GraphQuery,
        json!({"operation":"issues","path":"app","minTier":60,"limit":1000}),
    );
    let default = call(
        &dir.0,
        ToolId::GraphQuery,
        json!({"operation":"issues","path":"app","limit":1000}),
    );
    assert!(rows(&all).len() >= rows(&default).len(), "{all}");
    assert_eq!(all["summary"]["minTier"], 60);
    let orphan = |page: &Value| {
        rows(page)
            .iter()
            .any(|row| row["detector"] == "unreachable-file" && row["subject"] == "orphan.ts")
    };
    assert!(orphan(&all), "{all}");
    let entered = call(
        &dir.0,
        ToolId::GraphQuery,
        json!({"operation":"issues","path":"app","minTier":60,"limit":1000,"entries":["orphan.ts"]}),
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
    let items = crate::contracts::query_schema_value(
        ToolId::GraphQuery,
        Some("issues"),
        "detectors",
        "items",
    )
    .expect("detectors items");
    let declared = items["enum"]
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
        json!({"operation":"find","path":"app","kind":"file","limit":1000}),
    );
    let mut ids = rows(&files)
        .iter()
        .map(|row| row["id"].as_str().expect("id").to_owned())
        .collect::<Vec<_>>();
    ids.sort();
    assert_eq!(
        ids,
        ["a.ts", "b.ts", "main.ts", "orphan.ts"].map(String::from),
        "{files}"
    );
    assert_eq!(files["pagination"]["totalItems"], 4);
}
