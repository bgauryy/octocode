use super::*;
use octocode_test_support::json_fixture;

fn freshness_checks() -> usize {
    crate::ast_graph::store::FRESHNESS_CHECKS.with(std::cell::Cell::get)
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
        json!({"operation":"hubs","path":"app","graph":built["id"],"kind":"file","limit":1}),
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
        json!({"operation":"hubs","path":"app","graph":built["id"],"kind":"file","limit":1}),
    );
    assert_eq!(again["freshness"]["fresh"], false, "{again}");
    assert_eq!(freshness_checks() - before, 2);
}
