use super::super::test_rows::run_row;
use super::*;
use crate::tools::test_support::{Fixture, simple_policy};
use serde_json::Value;

fn next_query(out: &Value, name: &str) -> Value {
    out["next"][name]["query"]["queries"][0].clone()
}

/// A continuation page served from the first page's stored scan is
/// byte-identical to the page a fresh scan gives; an edit drops the
/// stored scan and the page restarts as before.
#[test]
fn continuation_pages_reuse_the_settled_scan_with_identical_output() {
    let root = Fixture::new();
    for (name, calls) in [("a.ts", 3), ("b.ts", 1), ("c.ts", 2)] {
        let body = (0..calls)
            .map(|i| format!("console.log({i});\n"))
            .collect::<String>();
        std::fs::write(root.0.join(name), body).expect("file");
    }
    std::fs::write(root.0.join("d.ts"), "other();\n").expect("prefilter skip");
    let (paths, security) = simple_policy(&root.0);
    octocode_engine::stamp::settle_written_so_far();
    let first = run_row(
        &json!({"operation":"match","path":root.0,"pattern":"console.log($A)","language":"typescript","pageSize":1,"matchPageSize":2}),
        &paths,
        &security,
    );
    let snapshot = first["snapshot"].as_str().expect("snapshot").to_owned();
    assert!(
        SCANS.holds(&snapshot),
        "a multi-page scan is stored: {first}"
    );
    let mut offering = first.clone();
    let mut file_page_query = Value::Null;
    for name in ["nextMatchPage", "nextPage"] {
        let query = next_query(&offering, name);
        assert!(query.is_object(), "{name}: {offering}");
        if name == "nextPage" {
            file_page_query = query.clone();
        }
        let stored = run_row(&query, &paths, &security);
        SCANS.evict(&snapshot);
        let fresh = run_row(&query, &paths, &security);
        assert_eq!(
            serde_json::to_string(&stored).expect("json"),
            serde_json::to_string(&fresh).expect("json"),
            "{name}"
        );
        // The rescan stores the scan again for the pages after it.
        assert!(SCANS.holds(&snapshot));
        offering = fresh;
    }
    // An edit drops the stored scan: the page restarts from page 1.
    std::fs::write(root.0.join("c.ts"), "console.log(9);\n").expect("edit");
    let page = run_row(&file_page_query, &paths, &security);
    assert_eq!(page["errorCode"], "staleSnapshot", "{page}");
    assert!(!SCANS.holds(&snapshot));
}
