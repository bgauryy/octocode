use super::*;

/// A cycle listing row, one huge cycle between two small
/// ones. Each page holds at most one part of a row, so `results[i]` is
/// the whole of that row on the page; every edge record is delivered
/// once; and the first page is filled with the head of the evidence.
#[test]
fn a_split_row_has_one_part_per_page_and_never_repeats_its_records() {
    let edges = |cycle: usize, kind: &str, count: usize| {
        (0..count)
            .map(|n| {
                json!({"from": format!("c{cycle}/{kind}/from_{n}.ts"),
                           "to": format!("c{cycle}/{kind}/to_{n}.ts"),
                           "edgeKinds": ["static-import"]})
            })
            .collect::<Vec<_>>()
    };
    let cycle = |cycle: usize, files: usize, runtime: usize, members: usize| {
        json!({
            "files": (0..files)
                .map(|n| format!("excalidraw-app/c{cycle}/components/Module_{n:04}.tsx"))
                .collect::<Vec<_>>(),
            "edgeKinds": ["static-import", "type-import"],
            "runtimeCycles": (0..runtime)
                .map(|r| (0..members)
                    .map(|n| format!("packages/c{cycle}/runtime_{r}/member_{n:03}.ts"))
                    .collect::<Vec<_>>())
                .collect::<Vec<_>>(),
            "runtimeCycleCount": runtime,
            "cycleEdges": edges(cycle, "cycle", 4),
            "runtimeCycleEdges": edges(cycle, "runtime", 7),
        })
    };
    let structured = json!({"results": [{"index": 0, "data": {
        "path": "tsx",
        "results": [cycle(1, 4, 1, 2), cycle(2, 349, 5, 30), cycle(3, 4, 1, 2)],
        "summary": {"cycleCount": 3, "runtimeCycleCount": 7},
        "hints": {"read": {"tool": "localFetch", "query": {"path": "tsx/a.ts"}}}
    }}]})
    .as_object()
    .cloned()
    .expect("object");
    let budget = 20_000;
    let pages = walk(&structured, budget);
    assert!(pages.len() > 1, "the row is split");
    let (mut files, mut edges_seen, mut runtime_seen) = (Vec::new(), Vec::new(), Vec::new());
    for (number, (page, _)) in pages.iter().enumerate() {
        let indexes: Vec<_> = page["results"]
            .as_array()
            .expect("rows")
            .iter()
            .map(|row| row["index"].as_u64())
            .collect();
        let mut unique = indexes.clone();
        unique.dedup();
        assert_eq!(indexes, unique, "page {number} repeats a row index");
        for row in page["results"].as_array().expect("rows") {
            for cycle in row["data"]["results"].as_array().into_iter().flatten() {
                files.extend(cycle["files"].as_array().into_iter().flatten().cloned());
                for key in ["cycleEdges", "runtimeCycleEdges"] {
                    edges_seen.extend(cycle[key].as_array().into_iter().flatten().cloned());
                }
                runtime_seen.extend(
                    cycle["runtimeCycles"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .cloned(),
                );
            }
        }
    }
    let count = |values: &[Value]| {
        let mut sorted: Vec<String> = values.iter().map(Value::to_string).collect();
        sorted.sort();
        let all = sorted.len();
        sorted.dedup();
        (all, sorted.len())
    };
    assert_eq!(count(&files), (357, 357), "every file once");
    assert_eq!(count(&edges_seen), (33, 33), "every edge record once");
    assert_eq!(count(&runtime_seen), (7, 7), "every runtime cycle once");
    let first = pages[0].1.char_length;
    assert!(
        first > budget / 2,
        "page one carries {first} of {budget} chars"
    );
}

#[test]
fn an_indivisible_fragment_flags_its_page_oversized() {
    let structured = json!({"results": [
        {"index": 0, "data": {"content": "x".repeat(5_000)}},
        {"index": 1, "data": {"content": "small"}}
    ]})
    .as_object()
    .cloned()
    .expect("object");
    let pages = walk(&structured, 1_000);
    let flagged: Vec<_> = pages.iter().map(|(_, p)| p.oversized).collect();
    assert!(flagged.contains(&Some(true)), "{flagged:?}");
    let whole = pages
        .iter()
        .flat_map(|(page, _)| page["results"].as_array().cloned().unwrap_or_default())
        .filter(|row| {
            row["data"]["content"]
                .as_str()
                .is_some_and(|c| c.len() == 5_000)
        })
        .count();
    assert_eq!(whole, 1, "the string is never cut");
}

#[test]
fn a_page_request_builds_only_that_pages_fragments() {
    let structured = envelope();
    let (_, first) = paginate_rows(
        structured.clone(),
        &options(700, 0, None),
        Vec::new(),
        &[],
        None,
    );
    assert!(first.total_pages > 20, "{}", first.total_pages);
    let last = first.total_pages - 1;
    MATERIALIZED.with(|count| count.set(0));
    let (page, _) = paginate_rows(
        structured,
        &options(700, last, Some(first.snapshot.clone())),
        Vec::new(),
        &[],
        None,
    );
    let returned = page["results"].as_array().map_or(0, Vec::len);
    let built = MATERIALIZED.with(std::cell::Cell::get);
    assert!(
        built <= returned,
        "built {built} fragments to return {returned}"
    );
}

/// A split row's last part carries its pages; a count-less page adds no
/// warning line, and the part's hints go.
#[test]
fn a_last_part_carries_its_pages_without_a_countless_line() {
    let mut row = json!({"index":0,"data":{
        "files":[],
        "next":{"nextPage":{"tool":"astSearch","query":{"queries":[{"page":2}]}}},
        "hints":{"read":{"tool":"localFetch","query":{"path":"a"}}}
    }});
    keep_row_next(&mut row, PartShare::Last, &[]);
    assert!(row["data"].get("warnings").is_none(), "{row}");
    assert!(row["data"]["next"].get("nextPage").is_some(), "{row}");
    assert!(row["data"].get("hints").is_none(), "{row}");
}

/// A split row's first part keeps its warnings while its pages ride the
/// last part: its disclosure, re-rendered from the row's notes, names
/// the response page, never a `next` entry the part does not carry.
#[test]
fn a_first_part_warning_names_only_continuations_it_carries() {
    let notes = [Remaining {
        page: "nextPage".into(),
        left: "4 more pages".into(),
    }];
    let mut row = json!({"index":0,"data":{
        "warnings":["Regex trap."],
        "files":[],
        "next":{"nextPage":{"tool":"astSearch","query":{"page":2}}},
        "hints":{"read":{"tool":"localFetch","query":{"path":"a"}}}
    }});
    keep_row_next(&mut row, PartShare::First, &notes);
    assert!(row["data"].get("next").is_none(), "{row}");
    assert_eq!(
        row["data"]["warnings"],
        json!([
            "Regex trap.",
            "4 more pages (next.nextPage on the row's last part)"
        ]),
        "{row}"
    );
    let mut whole = json!({"index":0,"data":{
        "next":{"nextPage":{"tool":"astSearch","query":{"page":2}}}
    }});
    let before = whole.clone();
    keep_row_next(&mut whole, PartShare::Whole, &notes);
    assert_eq!(whole, before);
}
