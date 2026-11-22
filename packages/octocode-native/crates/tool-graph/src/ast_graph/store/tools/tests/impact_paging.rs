use super::*;
use octocode_test_support::json_fixture;

/// Every row of the list `first` continues under `next.<channel>`, walked
/// through each page's own `next.nextPage` to its end.
fn walk_list(dir: &Path, first: &Value, channel: &str) -> Vec<Value> {
    let mut request = first
        .pointer(&format!("/next/{channel}/input/queries/0"))
        .cloned()
        .unwrap_or_else(|| panic!("next.{channel}: {first}"));
    let mut rows = Vec::new();
    loop {
        let page = call(dir, ToolId::GraphQuery, request.clone());
        assert_eq!(page["list"], channel, "{page}");
        assert!(page.get("summary").is_none(), "{page}");
        rows.extend(page["rows"].as_array().expect("list rows").iter().cloned());
        match page.pointer("/next/nextPage/input/queries/0") {
            Some(next) => request = next.clone(),
            None => return rows,
        }
    }
}

/// An `impact` summary counts its lists, so its first page holds the
/// affected rows; every test function is read once, under its file, through
/// `next.testsToRun`, however many there are.
#[test]
fn impact_counts_summary_lists_and_pages_each_through_its_own_call() {
    let dir = Fixture::new();
    let count = 125;
    let tests = (0..count)
        .map(|index| format!("#[test] fn case{index:03}() {{ target(); }}\n"))
        .collect::<String>();
    dir.write(
        "app/src/lib.rs",
        format!("pub fn target() {{}}\n#[cfg(test)] mod tests {{ use super::*; {tests} }}\n"),
    );
    dir.write("app/src/main.rs", "fn main() { impact::target(); }\n");
    dir.write(
        "app/Cargo.toml",
        "[package]\nname='impact'\nversion='0.1.0'\n",
    );
    let built = call(
        &dir.0,
        ToolId::GraphBuild,
        json_fixture(r#"{"path": "app"}"#),
    );
    let impact = call(
        &dir.0,
        ToolId::GraphQuery,
        json!({"operation":"impact","path":"app","graph":built["graph"],"target":"src/lib.rs#target","limit":7}),
    );
    let summary = &impact["summary"];
    assert_eq!(summary["testFunctions"], count, "{impact}");
    assert_eq!(summary["testsToRun"], 1, "{impact}");
    let rows = impact["rows"].as_array().expect("impact rows");
    assert_eq!(
        rows.len(),
        impact["pagination"]["total"]
            .as_u64()
            .map_or(0, |total| total.min(7) as usize),
        "the first page holds the affected rows: {impact}"
    );
    let tests = walk_list(&dir.0, &impact, "testsToRun");
    assert_eq!(tests.len(), 1, "{tests:?}");
    assert_eq!(tests[0]["path"], "app/src/lib.rs", "{tests:?}");
    let functions = tests[0]["functions"].as_array().expect("functions");
    let expected = (0..count)
        .map(|index| json!(format!("tests.case{index:03}")))
        .collect::<Vec<_>>();
    assert_eq!(functions, &expected);
    let entrypoints = summary["affectedEntrypoints"].as_u64().expect("count");
    if entrypoints > 0 {
        assert_eq!(
            walk_list(&dir.0, &impact, "affectedEntrypoints").len() as u64,
            entrypoints
        );
    } else {
        assert!(impact.pointer("/next/affectedEntrypoints").is_none());
    }
    assert!(impact.pointer("/next/unmapped").is_none(), "{impact}");
}

/// A `list` the operation does not page is refused as input to correct.
#[test]
fn impact_refuses_a_list_it_does_not_count() {
    let dir = Fixture::new();
    dir.write("app/src/lib.rs", "pub fn target() {}\n");
    dir.write(
        "app/Cargo.toml",
        "[package]\nname='impact'\nversion='0.1.0'\n",
    );
    let built = call(
        &dir.0,
        ToolId::GraphBuild,
        json_fixture(r#"{"path": "app"}"#),
    );
    let row = octocode_runtime_base::contracts::validate_built_query(
        "graphQuery",
        json!({"operation":"impact","path":"app","graph":built["graph"],"target":"src/lib.rs","list":"cycleNodes"}),
    )
    .expect("input contract");
    let refused = execute(ToolId::GraphQuery, &row, &dir.0, &context()).expect("execute");
    assert_eq!(refused.data["errorCode"], json!(ErrorCode::InvalidInput));
}

/// Every summary list a first page counts goes out as its declared
/// `next.<list>` channel (the output contract checks each call), and each
/// list page reads as rows to its end: entrypoints, tests and, for
/// `since`, the changed paths outside the graph.
#[test]
fn impact_since_emits_every_declared_list_channel() {
    let dir = Fixture::new();
    dir.write("app/package.json", r#"{"name":"app","main":"src/main.ts"}"#);
    dir.write("app/src/lib.ts", "export function target() { return 1; }\n");
    dir.write(
        "app/src/main.ts",
        "import { target } from './lib';\nexport function main() { return target(); }\n",
    );
    dir.write(
        "app/src/lib.test.ts",
        "import { target } from './lib';\ntarget();\n",
    );
    dir.write("app/notes.md", "notes\n");
    let app = dir.0.join("app");
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .arg("-C")
            .arg(&app)
            .args(["-c", "user.email=t@t", "-c", "user.name=t"])
            .args(args)
            .output()
            .is_ok_and(|out| out.status.success())
    };
    if !git(&["init", "-q"]) {
        return; // git unavailable in this environment
    }
    assert!(git(&["add", "-A"]) && git(&["commit", "-qm", "base"]));
    dir.write("app/src/lib.ts", "export function target() { return 2; }\n");
    dir.write("app/todo.md", "todo\n");
    let built = call(
        &dir.0,
        ToolId::GraphBuild,
        json_fixture(r#"{"path": "app"}"#),
    );
    let impact = call(
        &dir.0,
        ToolId::GraphQuery,
        json!({"operation":"impact","path":"app","graph":built["graph"],"since":"HEAD"}),
    );
    for channel in super::super::super::impact::LISTS {
        let count = impact["summary"][channel].as_u64().unwrap_or(0);
        assert!(count > 0, "{channel}: {impact}");
        let rows = walk_list(&dir.0, &impact, channel);
        assert_eq!(rows.len() as u64, count, "{channel}: {rows:?}");
    }
}
