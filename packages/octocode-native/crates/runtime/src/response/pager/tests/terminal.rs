use super::*;

#[test]
fn a_complete_tail_spends_no_room_on_a_response_continuation() {
    let files = (0..7)
        .map(|index| format!("entry-{index}-{}.rs", "界😀".repeat(7)))
        .collect::<Vec<_>>();
    let input = json!({"warnings":["source coverage remains limited"],"results":[{"index":0,"data":{
        "path":"unit","files":[{"dir":"unit","files":files}],"snapshot":"s".repeat(64),
        "pagination":{"page":1,"totalPages":1,"totalItems":7,"hasMore":false},
        "summary":{"files":7,"dirs":0,"bytes":1000},
        "related":[{"kind":"related","tool":"localFetch","input":{"queries":[{"path":"deep/".repeat(40)}]}}]
    }}]});
    let pages = walk_tool("structureSearch", &input, 750);
    assert_eq!(
        pages.len(),
        2,
        "all remaining records fit the finished envelope"
    );
    assert!(pages[1].to_string().encode_utf16().count() <= 750);
    let collected = pages
        .iter()
        .flat_map(|page| page["results"].as_array().unwrap())
        .flat_map(|row| row["data"]["files"].as_array().unwrap())
        .flat_map(|group| group["files"].as_array().unwrap())
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(collected, json!(files).as_array().unwrap().to_vec());
    assert_eq!(
        pages[1]["warnings"],
        json!(["source coverage remains limited"])
    );
    assert!(pages[1]["results"][0]["data"].get("related").is_none());
}

#[test]
fn terminal_room_preserves_mixed_query_identity_and_nested_order() {
    let rows = (0..2).map(|index| json!({"index":index,"data":{
        "path":format!("root-{index}"),"files":[{"dir":format!("root-{index}"),
        "files":(0..16).map(|item|format!("file-{index}-{item}-{}.rs","界😀".repeat(3))).collect::<Vec<_>>() }],
        "snapshot":format!("snapshot-{index}"),"summary":{"files":16,"dirs":0,"bytes":500}
    }})).collect::<Vec<_>>();
    let input = json!({"results":rows});
    for budget in [600, 900, 1_300] {
        let pages = walk_tool("structureSearch", &input, budget);
        let mut seen = [Vec::new(), Vec::new()];
        for page in &pages {
            assert!(page.to_string().encode_utf16().count() <= budget);
            for row in page["results"].as_array().unwrap() {
                let index = row["index"].as_u64().unwrap() as usize;
                assert_eq!(row["data"]["path"], rows[index]["data"]["path"]);
                assert_eq!(row["data"]["snapshot"], rows[index]["data"]["snapshot"]);
                for group in row["data"]["files"].as_array().unwrap() {
                    assert_eq!(group["dir"], rows[index]["data"]["files"][0]["dir"]);
                    seen[index].extend(group["files"].as_array().unwrap().iter().cloned());
                }
            }
        }
        for index in 0..2 {
            assert_eq!(
                &seen[index],
                rows[index]["data"]["files"][0]["files"].as_array().unwrap()
            );
        }
    }
}
