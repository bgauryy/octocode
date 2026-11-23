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
        // A nondefault confidence must survive continuation compaction.
        let request = json!({"operation":"dependents","path":"app","graph":built["graph"],"target":"core.ts","view":view, "depth":1, "confidence":"medium"});
        let out = call(&dir.0, ToolId::GraphQuery, request.clone());
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
        let mut expected = request;
        expected["edges"] = json!(["imports"]);
        assert_eq!(row, &expected);
        octocode_runtime_base::contracts::validate_built_query("graphQuery", row.clone())
            .expect("the import-only lead is accepted");
        let imports = call(&dir.0, ToolId::GraphQuery, row.clone());
        assert_eq!(edge_count(&imports), 1, "{imports}");
        assert!(
            imports["related"]
                .as_array()
                .into_iter()
                .flatten()
                .all(|lead| lead["input"]["queries"][0]["edges"] != json!(["imports"])),
            "an explicit edge selection gets no default-edge lead: {imports}"
        );
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

/// The cases every graph-algorithm implementation answers the same way
/// (`test-utils/graph-algorithms/SPEC.md`); the TypeScript graph reads the
/// same file.
fn algorithm_cases() -> Vec<Value> {
    let text = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../test-utils/graph-algorithms/cases.json"),
    )
    .expect("the shared graph-algorithm cases");
    json_fixture(&text)
        .as_array()
        .expect("a list of cases")
        .clone()
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .expect("a list")
        .iter()
        .map(|item| item.as_str().expect("a string").to_owned())
        .collect()
}

/// A node id as the case names it, without the tree the files were written under.
fn unscoped_id(id: &str) -> String {
    id.strip_prefix("app/").unwrap_or(id).to_owned()
}

fn unscoped(ids: Vec<String>) -> Vec<String> {
    ids.iter().map(|id| unscoped_id(id)).collect()
}

/// `target` as an import specifier written in `source`.
fn specifier(source: &str, target: &str) -> String {
    let from = source.split('/').collect::<Vec<_>>();
    let to = target
        .trim_end_matches(".ts")
        .split('/')
        .collect::<Vec<_>>();
    let shared = from[..from.len() - 1]
        .iter()
        .zip(&to)
        .take_while(|(a, b)| a == b)
        .count();
    let up = from.len() - 1 - shared;
    let mut parts = if up == 0 {
        vec![".".to_owned()]
    } else {
        vec!["..".to_owned(); up]
    };
    parts.extend(to[shared..].iter().map(|part| (*part).to_owned()));
    parts.join("/")
}

/// The shared cases through the real tools: files on disk, `graphBuild`,
/// then `graphQuery` over import edges. The fixture's nodes are files and its
/// edges imports, written in the order the case lists them.
#[test]
fn graph_queries_answer_the_shared_algorithm_cases() {
    for case in algorithm_cases() {
        // A file importing itself is not a graph edge; an empty tree builds nothing.
        let nodes = strings(&case["nodes"]);
        let edges = case["edges"].as_array().expect("edges");
        let name = case["name"].as_str().expect("name");
        if nodes.is_empty() || edges.iter().any(|edge| edge[0] == edge[1]) {
            continue;
        }
        let dir = Fixture::new();
        for node in &nodes {
            let mut text = String::new();
            for edge in edges.iter().filter(|edge| edge[0] == node.as_str()) {
                let target = edge[1].as_str().expect("target");
                text += &format!("import \"{}\";\n", specifier(node, target));
            }
            dir.write(
                &format!("app/{node}"),
                &(text + "export const value = 1;\n"),
            );
        }
        let built = call(&dir.0, ToolId::GraphBuild, json!({"path": "app"}));
        let query = |request: Value| {
            let mut row = json!({"path": "app", "graph": built["graph"]});
            for (key, value) in request.as_object().expect("a request") {
                row[key] = value.clone();
            }
            call(&dir.0, ToolId::GraphQuery, row)
        };
        let expected = &case["expected"];

        let cycles = query(json!({"operation": "cycles", "edges": ["imports"]}));
        let found = cycles["rows"]
            .as_array()
            .expect("cycle rows")
            .iter()
            .map(|row| unscoped(strings(&row["nodes"])))
            .collect::<Vec<_>>();
        let wanted = expected["cycles"]
            .as_array()
            .expect("cycles")
            .iter()
            .map(strings)
            .collect::<Vec<_>>();
        assert_eq!(found, wanted, "{name}: cycle groups {cycles}");

        for step in expected["path"].as_array().expect("paths") {
            let out = query(json!({
                "operation": "path", "target": format!("app/{}", step["from"].as_str().expect("from")), "to": format!("app/{}", step["to"].as_str().expect("to")), "direction": "out", "edges": ["imports"],
            }));
            let found = (out["found"] == true).then(|| {
                out["path"]
                    .as_array()
                    .expect("hops")
                    .iter()
                    .map(|hop| unscoped_id(hop["id"].as_str().expect("a hop id")))
                    .collect::<Vec<_>>()
            });
            let wanted = (!step["path"].is_null()).then(|| strings(&step["path"]));
            assert_eq!(found, wanted, "{name}: path {step} {out}");
        }

        for reach in expected["reach"].as_array().expect("reach") {
            let seeds = strings(&reach["seeds"])
                .into_iter()
                .map(|seed| format!("app/{seed}"))
                .collect::<Vec<_>>();
            let out = query(json!({"operation": "impact", "changed": seeds}));
            let mut found = serde_json::Map::new();
            // `impact` also lists each changed file at depth 0; the case lists reached files.
            for row in out["rows"]
                .as_array()
                .expect("impact rows")
                .iter()
                .filter(|row| row["depth"] != 0)
            {
                found.insert(
                    unscoped_id(row["id"].as_str().expect("id")),
                    row["depth"].clone(),
                );
            }
            assert_eq!(Value::Object(found), reach["depths"], "{name}: reach {out}");
        }
    }
}
