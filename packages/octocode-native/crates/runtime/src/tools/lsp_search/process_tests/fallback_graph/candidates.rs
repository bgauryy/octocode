//! A type below the anchor with more implementation candidates than one
//! window resumes as an anchor, where its candidates page by window.

use super::*;

/// `Mid` (a subtype of the anchor `Base`) has one window of candidates and
/// one more: the walk checks the first window, then resumes on `Mid`, whose
/// candidate pages reach the last one. No limit is terminal.
#[test]
fn a_deeper_type_past_one_candidate_window_resumes_and_reaches_every_candidate() {
    let cap = crate::tools::lsp_search::walk::MAX_HIERARCHY_NODES;
    let mut content = "export class Base {}\nexport class Other {}\nexport class Mid extends Base {}\nexport class First extends Mid {}\n".to_owned();
    for index in 1..cap {
        content.push_str(&format!("export class Leaf{index} extends Other {{}}\n"));
    }
    content.push_str("export class Last extends Mid {}\n");
    let last = cap + 3;
    let mut definitions = vec![
        ("deep.ts", 2, "Base", "deep.ts", 0),
        ("deep.ts", 3, "Mid", "deep.ts", 2),
        ("deep.ts", last, "Mid", "deep.ts", 2),
    ];
    definitions.extend((4..last).map(|line| ("deep.ts", line, "Other", "deep.ts", 1)));
    let below_mid = (3..=last).map(|line| ("deep.ts", line)).collect::<Vec<_>>();
    let mut results = fallback_graph_pages(
        &[("deep.ts", &content)],
        &definitions,
        &[
            ("deep.ts", 0, &[("deep.ts", 2)][..]),
            ("deep.ts", 2, &below_mid[..]),
        ],
        &[("subtypes", "deep.ts", 0, 2)],
    );
    let pages = results.remove(0);
    let first = &pages[0];
    assert!(
        first["partialReasons"]
            .as_array()
            .is_some_and(|reasons| reasons.contains(&serde_json::json!("hierarchyCandidateLimit"))),
        "{first}"
    );
    let resume = &first["next"]["continueWalk"]["input"]["queries"][0];
    assert_eq!(resume["symbolName"], "Mid", "{first}");
    assert_eq!(
        crate::tools::test_support::opened(crate::id::ToolId::LspSearch, resume)["depth"],
        1,
        "{first}"
    );
    for row in &pages {
        assert!(row.get("terminalLimit").is_none(), "{row}");
    }
    let listed = pages
        .iter()
        .flat_map(|row| row["matches"].as_array().into_iter().flatten())
        .filter_map(|matched| matched["symbolName"].as_str())
        .collect::<Vec<_>>();
    assert!(listed.contains(&"First"), "{listed:?}");
    assert!(
        listed.contains(&"Last"),
        "the candidate past the first window is reached: {listed:?}"
    );
}

/// A server that pipelines requests (tsgo) is asked about several subtype
/// candidates at once, any other one at a time; both walks answer the same.
#[test]
fn subtype_candidates_pipeline_only_on_a_concurrent_server_with_the_same_answer() {
    let children = 12;
    let mut content = "export class Base {}\n".to_owned();
    for index in 0..children {
        content.push_str(&format!("export class Child{index} extends Base {{}}\n"));
    }
    let definitions = (1..=children)
        .map(|line| ("wide.ts", line, "Base", "wide.ts", 0))
        .collect::<Vec<_>>();
    let candidates = (1..=children)
        .map(|line| ("wide.ts", line))
        .collect::<Vec<_>>();
    let walk = |name| {
        fallback_graph_walk(
            &[("wide.ts", &content)],
            &definitions,
            &[("wide.ts", 0, &candidates[..])],
            &[("subtypes", "wide.ts", 0, 1)],
            Some(Delayed { name, delay_ms: 30 }),
        )
    };
    // Rows name files under each walk's own root: compare what they list.
    let listed = |pages: &[Vec<serde_json::Value>]| {
        pages
            .iter()
            .flatten()
            .flat_map(|row| row["matches"].as_array().cloned().unwrap_or_default())
            .map(|matched| {
                (
                    matched["symbolName"].clone(),
                    matched["displayRange"].clone(),
                )
            })
            .collect::<Vec<_>>()
    };
    let (sequential, one_at_a_time) = walk("delayed-lsp");
    let (pipelined, at_once) = walk("tsgo");
    assert_eq!(one_at_a_time, 1, "one request at a time");
    assert!(at_once > 1, "several candidates at once: {at_once}");
    assert_eq!(listed(&sequential).len(), children, "{sequential:?}");
    assert_eq!(listed(&pipelined), listed(&sequential));
}
