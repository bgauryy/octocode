use super::*;
use octocode_test_support::json_fixture;

fn freshness_checks() -> usize {
    crate::ast_graph::store::FRESHNESS_CHECKS.with(std::cell::Cell::get)
}

#[test]
fn grouped_dependents_lead_to_the_same_view_with_only_import_edges() {
    let dir = Fixture::new();
    dir.write("app/core.ts", "export const seed = 1;\n");
    dir.write("app/barrel.ts", "export { seed } from './core';\n");
    dir.write(
        "app/service.ts",
        "import { seed } from './barrel';\nexport const value = seed;\n",
    );
    let built = call(&dir.0, ToolId::GraphBuild, json!({"path": "app"}));
    for view in ["files", "modules"] {
        let out = call(
            &dir.0,
            ToolId::GraphQuery,
            json!({"operation":"dependents","path":"app","graph":built["graph"],"target":"core.ts","view":view}),
        );
        let edge_count = |answer: &Value| {
            answer["rows"]
                .as_array()
                .expect("grouped rows")
                .iter()
                .map(|row| row["edges"].as_u64().expect("edge count"))
                .sum::<u64>()
        };
        assert_eq!(edge_count(&out), 2, "{out}");
        let lead = out["related"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|lead| lead["input"]["queries"][0]["edges"] == json!(["imports"]))
            .unwrap_or_else(|| panic!("an import-only lead: {out}"));
        assert_eq!(lead["tool"], "graphQuery");
        let row = &lead["input"]["queries"][0];
        assert_eq!(row["path"], "app");
        assert_eq!(row["graph"], built["graph"]);
        assert_eq!(row["view"], view);
        let imports = call(&dir.0, ToolId::GraphQuery, row.clone());
        assert_eq!(edge_count(&imports), 1, "{imports}");
    }
}

/// A walk reads one immutable snapshot: its first page checks the tree and
/// states a stale verdict once; a later page neither checks nor repeats it.
#[test]
fn a_walk_checks_freshness_on_its_first_page_only() {
    let dir = Fixture::new();
    for index in 0..4 {
        dir.write(
            &format!("app/m{index}.ts"),
            format!(
                "import {{ v{next} }} from './m{next}';\nexport const v{index} = v{next};\n",
                next = (index + 1) % 4
            ),
        );
    }
    let built = call(
        &dir.0,
        ToolId::GraphBuild,
        json_fixture(r#"{"path": "app"}"#),
    );
    dir.write("app/m0.ts", "export const v0 = 0;\n");
    let before = freshness_checks();
    let first = call(
        &dir.0,
        ToolId::GraphQuery,
        json!({"operation":"hubs","path":"app","graph":built["graph"],"kind":"file","limit":1}),
    );
    assert_eq!(first["freshness"]["fresh"], false, "{first}");
    assert_eq!(freshness_checks() - before, 1, "{first}");
    let mut request = first
        .pointer("/next/nextPage/input/queries/0")
        .cloned()
        .unwrap_or_else(|| panic!("a second page: {first}"));
    let mut pages = 1;
    loop {
        let page = call(&dir.0, ToolId::GraphQuery, request.clone());
        pages += 1;
        assert!(page.get("freshness").is_none(), "page {pages}: {page}");
        assert_eq!(page["rows"].as_array().map(Vec::len), Some(1), "{page}");
        match page.pointer("/next/nextPage/input/queries/0") {
            Some(next) => request = next.clone(),
            None => break,
        }
    }
    assert!(pages > 2, "the walk pages several times");
    assert_eq!(
        freshness_checks() - before,
        1,
        "a later page checks nothing"
    );
    // A fresh request is a new question: it checks again.
    let again = call(
        &dir.0,
        ToolId::GraphQuery,
        json!({"operation":"hubs","path":"app","graph":built["graph"],"kind":"file","limit":1}),
    );
    assert_eq!(again["freshness"]["fresh"], false, "{again}");
    assert_eq!(freshness_checks() - before, 2);
}

/// An answer from a stale snapshot, rows or none, leads to the build that
/// replaces it: a note names graphBuild, and `related` rebuilds the scope
/// with the walk it was built with.
#[test]
fn a_stale_answer_leads_to_the_rebuild_of_its_scope() {
    let dir = Fixture::new();
    dir.write(
        "app/a.ts",
        "import { b } from './b';\nexport const a = b;\n",
    );
    dir.write("app/b.ts", "export const b = 1;\n");
    let built = call(
        &dir.0,
        ToolId::GraphBuild,
        json_fixture(r#"{"path": "app", "exclude": ["gen"]}"#),
    );
    dir.write("app/b.ts", "export const b = 2;\n");
    for request in [
        json!({"operation":"hubs","path":"app","graph":built["graph"],"kind":"file"}),
        json!({"operation":"dependents","path":"app","graph":built["graph"],"target":"a.ts"}),
    ] {
        let out = call(&dir.0, ToolId::GraphQuery, request);
        assert_eq!(out["freshness"]["fresh"], false, "{out}");
        let notes = format!("{}{}", out["warnings"], out["hints"]);
        assert!(notes.contains("graphBuild"), "{out}");
        let rebuild = out["related"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|call| call["tool"] == "graphBuild")
            .unwrap_or_else(|| panic!("a rebuild lead: {out}"));
        let row = &rebuild["input"]["queries"][0];
        assert_eq!(row["path"], "app", "{rebuild}");
        assert_eq!(row["exclude"], json!(["gen"]), "{rebuild}");
    }
}

/// A symbol answer's rows are syntax candidates: `related` holds the
/// lspSearch call that checks them at the symbol's file, name and line,
/// with the operation that asks the same question; a file answer has none.
#[test]
fn symbol_answers_lead_to_the_lsp_search_that_checks_them() {
    let dir = Fixture::new();
    dir.write(
        "app/src/lib.ts",
        "export function target() { return 1; }\nexport function run() { return target(); }\n",
    );
    let built = call(
        &dir.0,
        ToolId::GraphBuild,
        json_fixture(r#"{"path": "app"}"#),
    );
    let checks = |answer: &Value| {
        answer["related"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|call| call["tool"] == "lspSearch")
            .map(|call| call["input"]["queries"][0].clone())
            .collect::<Vec<_>>()
    };
    for (operation, check) in [
        ("callers", "callers"),
        ("callees", "callees"),
        ("node", "references"),
        ("impact", "references"),
        ("walk", "references"),
    ] {
        let answer = call(
            &dir.0,
            ToolId::GraphQuery,
            json!({"operation":operation,"path":"app","graph":built["graph"],"target":"src/lib.ts#target"}),
        );
        let checks = checks(&answer);
        assert_eq!(checks.len(), 1, "{operation}: {answer}");
        let row = &checks[0];
        assert_eq!(row["operation"], check, "{operation}: {row}");
        assert_eq!(row["path"], "app/src/lib.ts", "{operation}: {row}");
        assert_eq!(row["symbolName"], "target", "{operation}: {row}");
        assert_eq!(row["line"], 1, "{operation}: {row}");
        octocode_runtime_base::contracts::validate_built_query("lspSearch", row.clone())
            .unwrap_or_else(|error| panic!("{operation}: {error:?}: {row}"));
    }
    let file = call(
        &dir.0,
        ToolId::GraphQuery,
        json!({"operation":"dependents","path":"app","graph":built["graph"],"target":"src/lib.ts"}),
    );
    assert!(checks(&file).is_empty(), "{file}");
    // A walk both ways states each row's direction, a declared row field.
    let both = call(
        &dir.0,
        ToolId::GraphQuery,
        json!({"operation":"walk","path":"app","graph":built["graph"],"target":"src/lib.ts#target","direction":"both","depth":1,"edges":["calls"]}),
    );
    let ways = both["rows"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|row| row["direction"].clone())
        .collect::<Vec<_>>();
    assert_eq!(ways, [json!("in")], "{both}");
}
