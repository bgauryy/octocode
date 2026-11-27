use super::*;
use octocode_test_support::json_fixture;

#[test]
fn exact_parentage_survives_filtering_and_single_row_pages() {
    let root = Fixture::new();
    let source = root.write(
        "blocks.rs",
        "struct Box; impl Box { fn one() {} } impl Box { fn two() {} }\n",
    );
    let query = json!({"operation":"symbols","path":source});
    let whole = run(&root.0, query.clone()).expect("symbols");
    let mut paged = Vec::new();
    checks::walker::walk("astSearch", json!({"queries":[query]}), |call| {
        let input = octocode_runtime_base::test_support::opened_input(
            octocode_runtime_base::id::ToolId::AstSearch,
            call,
        );
        let page =
            crate::ast_search::test_rows::run_window(&root.0, input["queries"][0].clone(), 1)
                .expect("page");
        let rows = page["symbols"].as_array().expect("rows").clone();
        assert_eq!(rows.len(), 1, "{page}");
        paged.extend(rows.iter().cloned());
        checks::Page {
            output: octocode_runtime_base::test_support::published(
                octocode_runtime_base::id::ToolId::AstSearch,
                &page,
            ),
            rows: rows.iter().map(Value::to_string).collect(),
        }
    });
    assert_eq!(json!(paged), whole["symbols"]);
    assert_eq!(whole["symbols"][1]["members"][0]["symbolName"], "one");
    assert_eq!(whole["symbols"][2]["members"][0]["symbolName"], "two");
    let java = root.write("Box.java", "class Box { Box() {} void run() {} }\n");
    for kinds in [
        json_fixture(r#"["class", "method"]"#),
        json_fixture(r#"["method"]"#),
    ] {
        let out = run(
            &root.0,
            json!({"operation":"symbols","path":java,"symbolKinds":kinds}),
        )
        .expect("filtered symbols");
        let flat = octocode_runtime_base::response::symbol_outline::flatten_members(
            out["symbols"].as_array().expect("symbols"),
        );
        assert!(flat.iter().any(|row| row["symbolName"] == "run"), "{out}");
        assert!(flat.iter().all(|row| row["kind"] != "constructor"), "{out}");
        assert!(!out.to_string().contains("_declarationIndex"), "{out}");
        assert!(!out.to_string().contains("_parentIndex"), "{out}");
        octocode_runtime_base::test_support::checks::assert_output_contract(
            "astSearch",
            &json!({"results":[{"index":0,"data":out}]}),
        );
    }
    let out = run(
        &root.0,
        json!({"operation":"symbols","path":java,"symbolKinds":["constructor","method"]}),
    )
    .expect("class omitted, same-named constructor retained");
    assert_eq!(out["symbols"].as_array().map(Vec::len), Some(2), "{out}");
    assert_eq!(out["symbols"][0]["kind"], "constructor", "{out}");
    assert!(out["symbols"][0].get("members").is_none(), "{out}");
    assert_eq!(out["symbols"][1]["symbolName"], "run", "{out}");
}
