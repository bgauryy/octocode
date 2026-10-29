use super::evidence_closure::{chars, exact_closure, fixture};
use super::*;
use crate::id::ToolId;

fn pages(data: Value, tool: ToolId, ordinary: usize, terminal: Option<usize>) -> Vec<Value> {
    let plan = PreparedRows::new(vec![json!({"index":0,"data":data})], &[None], Some(tool))
        .plan_with_terminal(ordinary, terminal, &CancellationToken::new())
        .unwrap();
    (0..plan.total_pages)
        .map(|page| {
            json!(
                plan.build_page(page, &[None], &[], &CancellationToken::new())
                    .unwrap()
            )
        })
        .collect()
}

#[test]
fn a_failed_descriptor_union_trial_keeps_the_original_fragments() {
    let mut data = fixture(1, 3);
    data["related"] = json!([{"kind":"related","tool":"localFetch","input":{"queries":[{"path":"deep/".repeat(900)}]}}]);
    let mut tail = json!({"index":0,"data":data.clone()});
    tail["data"].as_object_mut().unwrap().remove("related");
    tail["data"]["results"] = json!(&data["results"].as_array().unwrap()[1..]);
    let full = chars(&tail);
    tail["data"]
        .as_object_mut()
        .unwrap()
        .remove("sourceEvidence");
    let raw = chars(&tail);
    let terminal = (raw + full) / 2;
    let mut one = json!({"index":0,"data":data.clone()});
    one["data"].as_object_mut().unwrap().remove("related");
    one["data"]["results"] = json!([data["results"][1].clone()]);
    let ordinary = chars(&one) + 64;
    assert!(ordinary < terminal && raw + 64 < terminal && terminal < full);
    assert_eq!(
        pages(data.clone(), ToolId::GraphQuery, ordinary, Some(terminal)),
        pages(data, ToolId::GraphQuery, ordinary, None),
        "a speculative descriptor union must not mutate an emitted fragment"
    );
}

#[test]
fn terminal_union_keeps_overlapping_catalogs_and_all_origin_branches() {
    let data = fixture(2, 9);
    let catalog = data["sourceEvidence"].clone();
    let expected = data["results"].as_array().unwrap();
    let mut seen = Vec::new();
    let pages = pages(data.clone(), ToolId::GraphQuery, 6_000, Some(9_000));
    assert!(pages.len() > 1);
    for (index, page) in pages.iter().enumerate() {
        assert!(
            chars(page)
                <= if index + 1 == pages.len() {
                    9_000
                } else {
                    6_000
                }
        );
        for row in page.as_array().unwrap() {
            exact_closure(&row["data"], &catalog);
            seen.extend(row["data"]["results"].as_array().unwrap().iter().cloned());
        }
    }
    assert_eq!(&seen, expected);
}

#[test]
fn preliminary_sibling_arrays_and_records_cannot_spend_terminal_room() {
    let results = (0..18)
        .map(|i| json!({"id":format!("result-{i}"),"roles":["test"],"note":"x".repeat(55)}))
        .collect::<Vec<_>>();
    let names = (0..18)
        .map(|i| format!("path-{i}-{}", "界".repeat(25)))
        .collect::<Vec<_>>();
    let graph = json!({"op":"impact","results":results,"summary":{"testsToRun":names}});
    let branches = (0..18)
        .map(|i| (format!("branch-{i}"), json!("b".repeat(60))))
        .collect::<Map<_, _>>();
    let tree = json!({"defaultBranch":"main","commitSha":"fixed","branches":branches,"tags":names});
    for (tool, data) in [(ToolId::GraphQuery, graph), (ToolId::GhStructure, tree)] {
        let pages = pages(data.clone(), tool, 700, Some(1_100));
        assert!(pages.len() > 2);
        for page in &pages[..pages.len() - 1] {
            assert!(
                chars(page) <= 700,
                "nonterminal sibling crossed the ordinary bound: {page}"
            );
        }
        assert!(chars(pages.last().unwrap()) <= 1_100);
        let rows = pages
            .iter()
            .flat_map(|page| page.as_array().unwrap())
            .collect::<Vec<_>>();
        if tool == ToolId::GraphQuery {
            let collected = rows
                .iter()
                .flat_map(|row| {
                    row["data"]["results"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .cloned()
                })
                .collect::<Vec<_>>();
            assert_eq!(collected, data["results"].as_array().unwrap().to_vec());
            let collected = rows
                .iter()
                .flat_map(|row| {
                    row["data"]["summary"]["testsToRun"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .cloned()
                })
                .collect::<Vec<_>>();
            assert_eq!(collected, json!(names).as_array().unwrap().to_vec());
        } else {
            let collected = rows
                .iter()
                .flat_map(|row| {
                    row["data"]["branches"]
                        .as_object()
                        .into_iter()
                        .flatten()
                        .map(|(k, v)| (k.clone(), v.clone()))
                })
                .collect::<Vec<_>>();
            assert_eq!(
                collected,
                branches
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect::<Vec<_>>()
            );
            let collected = rows
                .iter()
                .flat_map(|row| {
                    row["data"]["tags"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .cloned()
                })
                .collect::<Vec<_>>();
            assert_eq!(collected, json!(names).as_array().unwrap().to_vec());
        }
    }
}
