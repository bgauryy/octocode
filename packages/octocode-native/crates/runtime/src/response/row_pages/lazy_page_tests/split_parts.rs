use super::*;

/// A split row's page continuation rides its last part, so a caller
/// following it never skips the parts in between; a drill-down rides the
/// first part, beside the head of the evidence. Each appears once.
#[test]
fn split_row_continuations_ride_the_part_they_follow() {
    let mut structured = envelope();
    structured["results"][0]["data"]["related"]
        .as_array_mut()
        .unwrap()
        .push(json!({"kind":"related","tool":"localFetch","input":{"queries":[{"path":"a.rs"}]}}));
    structured["results"][0]["data"]["next"]["nextPage"] =
        json!({"kind":"continue","tool":"localSearch","input":{"cursor":"issued"}});
    let pages = walk(&structured, 1_000);
    assert!(pages.len() > 2);
    let mut carriers = Vec::new();
    let mut parts = 0;
    for (page, _) in &pages {
        for row in page["results"].as_array().expect("rows") {
            if row["index"] != 0 {
                continue;
            }
            let part = row["rowPart"]["part"].as_u64().expect("row 0 is split");
            parts = row["rowPart"]["of"].as_u64().expect("row 0 is split");
            for call in row["data"]["related"].as_array().into_iter().flatten() {
                carriers.push((call["tool"].as_str().unwrap().to_owned(), part));
            }
            for name in row["data"]["next"]
                .as_object()
                .into_iter()
                .flatten()
                .map(|(name, _)| name)
            {
                carriers.push((name.clone(), part));
            }
        }
    }
    carriers.sort();
    assert!(parts > 2, "{parts}");
    assert_eq!(
        carriers,
        vec![
            ("localFetch".to_owned(), 1),
            ("localSearch".to_owned(), 1),
            ("nextPage".to_owned(), parts),
        ]
    );
}

/// A split row's secondary lists (diagnostics beside the paged files)
/// are not the paged evidence: the first part delivers them once and the
/// later parts carry them emptied, like an oversized sibling array.
#[test]
fn split_row_secondary_lists_ride_the_first_part_once() {
    let mut structured = envelope();
    structured["results"][0]["data"]["diagnostics"] =
        json!([{"code": "pattern.relaxed", "message": "32 matches as written"}]);
    let pages = walk(&structured, 1_000);
    let mut seen = Vec::new();
    for (page, _) in &pages {
        for row in page["results"].as_array().expect("rows") {
            if row["index"] == 0 {
                let part = row["rowPart"]["part"].as_u64().expect("row 0 is split");
                let listed = row["data"]["diagnostics"].as_array().map_or(0, Vec::len);
                seen.push((part, listed));
            }
        }
    }
    assert!(seen.len() > 2, "{seen:?}");
    for (part, listed) in seen {
        assert_eq!(listed, usize::from(part == 1), "part {part}");
    }
}

#[test]
fn nested_secondary_records_are_once_and_executable_queries_stay_intact() {
    let mut structured = envelope();
    structured["results"][0]["data"]["coverage"] =
        json!({"entries": [{"path": "generated/api.ts", "cause": "ignored"}], "sampled": true});
    let input = json!({"queries": [{"path": "a.rs"}, {"path": "b.rs"}]});
    structured["results"][0]["data"]["related"] =
        json!([{"kind":"related", "tool": "localFetch", "input": input}]);
    structured["results"][0]["data"]["next"]["nextPage"] =
        json!({"kind":"continue", "tool": "localFetch", "input": input});
    let pages = walk(&structured, 1_000);
    let mut records = Vec::new();
    let mut queries = Vec::new();
    for (page, _) in &pages {
        for row in page["results"].as_array().expect("rows") {
            if row["index"] != 0 {
                continue;
            }
            records.extend(
                row["data"]["coverage"]["entries"]
                    .as_array()
                    .expect("entries")
                    .clone(),
            );
            for call in row["data"]["related"]
                .as_array()
                .into_iter()
                .flatten()
                .chain(
                    row["data"]["next"]
                        .as_object()
                        .into_iter()
                        .flat_map(|next| next.values()),
                )
            {
                queries.push(call["input"].clone());
            }
        }
    }
    assert!(pages.len() > 2);
    assert_eq!(
        records,
        vec![json!({"path": "generated/api.ts", "cause": "ignored"})]
    );
    assert_eq!(queries, vec![input.clone(), input]);
}

#[test]
fn continuation_query_arrays_are_indivisible_when_they_dominate_the_row() {
    let query = json!({"queries":[{"paths": (0..20)
            .map(|index| format!("path_{index}_{}", "segment".repeat(40)))
            .collect::<Vec<_>>()}]});
    let structured = json!({"results":[{"index":0,"data":{
        "files":[{"path":"a.rs","matches":[{"line":1},{"line":2}]}],
        "related":[{"kind":"related","tool":"localFetch","input":query}],
        "next":{"nextPage":{"kind":"continue","tool":"localSearch","input":query}}
    }}]})
    .as_object()
    .cloned()
    .expect("envelope");
    let pages = walk(&structured, 1_000);
    let mut lines = Vec::new();
    let mut calls = Vec::new();
    for (page, _) in &pages {
        for row in page["results"].as_array().expect("rows") {
            for file in row["data"]["files"].as_array().into_iter().flatten() {
                for item in file["matches"].as_array().into_iter().flatten() {
                    lines.push(item["line"].clone());
                }
            }
            for (channel, call) in row["data"]["related"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|call| ("related", call))
                .chain(
                    row["data"]["next"]
                        .as_object()
                        .into_iter()
                        .flat_map(|next| next.values())
                        .map(|call| (PAGES_KEY, call)),
                )
            {
                assert_eq!(call["input"], query, "{channel} input was fragmented");
                calls.push(channel);
            }
        }
    }
    calls.sort();
    assert_eq!(calls, vec![PAGES_KEY, "related"]);
    assert_eq!(lines, vec![json!(1), json!(2)]);
}

/// An element too large for a page splits into parts. Its secondary
/// lists (the actionable runtime cycles and edges) ride its first part,
/// before any `files` slice, and every later part names the element it
/// continues on `rowPart.continues`, at position 0 of the paged array.
#[test]
fn split_element_later_parts_carry_continues_marker() {
    for (cycles, huge) in [
        (
            vec![cycle_row(1, 4, 1, 2), cycle_row(2, 349, 5, 30)],
            1usize,
        ),
        (vec![cycle_row(2, 349, 5, 30)], 0),
    ] {
        let structured = json!({"results": [{"index": 0, "data": {
            "path": "tsx",
            "results": cycles,
            "summary": {"cycleCount": 2},
        }}]})
        .as_object()
        .cloned()
        .expect("object");
        let pages = walk(&structured, 20_000);
        let parts = row_parts(&pages);
        let pieces: Vec<_> = parts
            .iter()
            .filter(|(_, cycles)| cycles.iter().any(|cycle| is_cycle(cycle, 2)))
            .collect();
        assert!(pieces.len() > 1, "the huge cycle is split: {parts:?}");
        let (first_part, first_cycles) = pieces[0];
        assert!(first_part.get("continues").is_none(), "{first_part}");
        let head = first_cycles
            .iter()
            .find(|cycle| is_cycle(cycle, 2))
            .expect("cycle 2");
        assert_eq!(
            head["runtimeCycles"].as_array().map(Vec::len),
            Some(5),
            "runtime cycles ride the first part: {head}"
        );
        assert_eq!(head["cycleEdges"].as_array().map(Vec::len), Some(4));
        for (part, cycles) in &pieces[1..] {
            assert_eq!(
                part["continues"],
                json!({"array": "/data/results", "index": huge}),
                "{part}"
            );
            assert!(is_cycle(&cycles[0], 2), "{cycles:?}");
            assert!(
                cycles[0]["runtimeCycles"]
                    .as_array()
                    .is_none_or(Vec::is_empty),
                "{part}"
            );
        }
        let mut files: Vec<String> = parts
            .iter()
            .flat_map(|(_, cycles)| cycles.iter())
            .filter(|cycle| is_cycle(cycle, 2))
            .flat_map(|cycle| cycle["files"].as_array().cloned().unwrap_or_default())
            .map(|file| file.to_string())
            .collect();
        let all = files.len();
        files.sort();
        files.dedup();
        assert_eq!((all, files.len()), (349, 349), "every file once");
    }
}
