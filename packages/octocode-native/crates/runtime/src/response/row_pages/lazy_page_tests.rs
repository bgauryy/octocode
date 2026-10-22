//! Lazy row-page planning tests.

use super::*;

fn envelope() -> Map<String, Value> {
    envelope_of(400)
}

/// [`envelope`] with `count` matches in each broad file.
fn envelope_of(count: usize) -> Map<String, Value> {
    let matches = (0..count)
        .map(|line| json!({"line": line, "value": format!("fn item_{line}() {{ \u{1F600} }}")}))
        .collect::<Vec<_>>();
    let huge = json!({"line": 9999, "value": "x".repeat(900)});
    json!({"root": "/repo", "results": [
        {"index": 0, "data": {"files": [{"path": "a.rs", "matches": matches.clone()},
                                        {"path": "b.rs", "matches": [huge]}],
                              "next": {"sidecar": {"tool": "localSearch",
                                                      "query": {"page": 2}}}}},
        {"index": 1, "data": {"content": "small"}},
        {"index": 2, "data": {"files": [{"path": "c.rs", "matches": matches}]}}
    ]})
    .as_object()
    .cloned()
    .expect("object")
}

fn options(length: usize, offset: usize, snapshot: Option<String>) -> ResponsePageOptions {
    ResponsePageOptions {
        response_length: Some(length),
        response_offset: Some(offset),
        response_snapshot: snapshot,
        response_scope: Some("rows".into()),
        render_text: None,
        saved: None,
    }
}

#[test]
fn changing_the_row_page_budget_requires_a_restart() {
    let structured = envelope();
    let full = Value::Object(structured.clone()).to_string();
    let (_, first) = paginate_rows(
        structured.clone(),
        &full,
        &options(1_000, 0, None),
        Vec::new(),
        &[],
        None,
    );
    assert!(first.has_more);
    for changed_length in [700, 2_000] {
        let (page, pagination) = paginate_rows(
            structured.clone(),
            &full,
            &options(changed_length, 1, Some(first.snapshot.clone())),
            Vec::new(),
            &[],
            None,
        );
        assert_eq!(pagination.restart, Some(true), "length {changed_length}");
        assert_eq!(pagination.changed, Some(true));
        assert_eq!(pagination.next_char_offset, Some(0));
        assert_eq!(page["results"], json!([]));
    }
}

#[test]
fn an_earlier_row_plan_version_requires_a_restart() {
    let structured = envelope();
    let full = Value::Object(structured.clone()).to_string();
    let (_, first) = paginate_rows(
        structured.clone(),
        &full,
        &options(1_000, 0, None),
        Vec::new(),
        &[],
        None,
    );
    assert!(first.snapshot.starts_with("response-rows-v6:"));
    let obsolete = first
        .snapshot
        .replacen("response-rows-v6:", "response-rows-v5:", 1);
    let (page, pagination) = paginate_rows(
        structured,
        &full,
        &options(1_000, 1, Some(obsolete)),
        Vec::new(),
        &[],
        None,
    );
    assert_eq!(pagination.restart, Some(true));
    assert_eq!(pagination.next_char_offset, Some(0));
    assert_eq!(page["results"], json!([]));
}

/// Every page of `structured` at `length`, following the continuation.
fn walk(
    structured: &Map<String, Value>,
    length: usize,
) -> Vec<(Map<String, Value>, ResponsePagination)> {
    let full = Value::Object(structured.clone()).to_string();
    let (_, first) = paginate_rows(
        structured.clone(),
        &full,
        &options(length, 0, None),
        Vec::new(),
        &[],
        None,
    );
    (0..first.total_pages)
        .map(|offset| {
            let request = options(length, offset, Some(first.snapshot.clone()));
            let page = paginate_rows(structured.clone(), &full, &request, Vec::new(), &[], None);
            assert!(page.1.restart.is_none(), "length {length} page {offset}");
            page
        })
        .collect()
}

/// (row index, file path, line) of every delivered match.
fn delivered(pages: &[(Map<String, Value>, ResponsePagination)]) -> Vec<(u64, String, u64)> {
    let mut seen = Vec::new();
    for (page, _) in pages {
        for row in page["results"].as_array().expect("rows") {
            for file in row["data"]["files"].as_array().into_iter().flatten() {
                for item in file["matches"].as_array().into_iter().flatten() {
                    seen.push((
                        row["index"].as_u64().expect("index"),
                        file["path"].as_str().expect("path").to_owned(),
                        item["line"].as_u64().expect("line"),
                    ));
                }
            }
        }
    }
    seen.sort();
    seen
}

#[test]
fn every_row_is_delivered_exactly_once_within_the_page_bound() {
    // Every page re-plans every row, so the rows are only as broad as the
    // lengths need: a broad row still spans several pages at the largest
    // length below that splits it (9_000).
    const MATCHES: usize = 200;
    let structured = envelope_of(MATCHES);
    let broad = structured["results"][2].to_string().len();
    assert!(broad > 9_000, "a broad row ({broad} chars) splits at 9_000");
    let mut expected: Vec<(u64, String, u64)> = (0..MATCHES as u64)
        .flat_map(|line| [(0, "a.rs".to_owned(), line), (2, "c.rs".to_owned(), line)])
        .chain([(0, "b.rs".to_owned(), 9999)])
        .collect();
    expected.sort();
    for length in [300, 700, 2_000, 9_000, 200_000] {
        let pages = walk(&structured, length);
        assert_eq!(delivered(&pages), expected, "length {length}");
        let mut parts: std::collections::BTreeMap<u64, Vec<(u64, u64)>> = Default::default();
        let mut small = 0;
        for (page, pagination) in &pages {
            assert!(
                pagination.char_length <= length || pagination.oversized == Some(true),
                "length {length}: {} chars unflagged",
                pagination.char_length
            );
            for row in page["results"].as_array().expect("rows") {
                small += usize::from(row["data"]["content"] == "small");
                let index = row["index"].as_u64().expect("index");
                let part = (
                    row["rowPart"]["part"].as_u64().unwrap_or(1),
                    row["rowPart"]["of"].as_u64().unwrap_or(1),
                );
                parts.entry(index).or_default().push(part);
            }
        }
        assert_eq!(small, 1, "length {length}");
        for (index, seen) in parts {
            let of = seen[0].1;
            assert_eq!(
                seen,
                (1..=of).map(|part| (part, of)).collect::<Vec<_>>(),
                "length {length} row {index}"
            );
        }
    }
}

/// One broad row must not fill the first pages alone; every row of a
/// batch shows its head on page one, within the budget. Each split row's
/// page continuation waits for its last part.
#[test]
fn a_page_with_rows_left_over_leads_with_an_incomplete_warning() {
    let rows = (0..3)
        .map(|index| json!({"index": index, "data": {"content": "x".repeat(9_000)}}))
        .collect::<Vec<_>>();
    let structured = json!({"results": rows})
        .as_object()
        .cloned()
        .expect("object");
    let pages = walk(&structured, 12_000);
    assert!(pages.len() > 1);
    let (first, _) = &pages[0];
    assert_eq!(first.keys().next().map(String::as_str), Some("warnings"));
    let banner = first["warnings"][0].as_str().expect("banner");
    assert!(
        banner.starts_with("incomplete — 2 of 3 rows continue"),
        "{banner}"
    );
    assert!(banner.contains("responsePagination.next"), "{banner}");
    let (last, pagination) = pages.last().expect("last");
    assert!(!pagination.has_more);
    assert!(last.get("warnings").is_none(), "{last:?}");
    for (_, pagination) in &pages {
        assert!(pagination.oversized.is_none());
    }
}

#[test]
fn the_first_page_shows_every_rows_head() {
    let rows = (0..5)
            .map(|index| {
                let matches = (0..300)
                    .map(|line| json!({"line": line, "value": format!("row {index} {}", "x".repeat(60))}))
                    .collect::<Vec<_>>();
                json!({"index": index, "data": {"files": [{"path": format!("f{index}.rs"), "matches": matches}],
                    "next": {"nextPage": {"tool": "localSearch", "query": {"page": 2}}}}})
            })
            .collect::<Vec<_>>();
    let structured = json!({"results": rows})
        .as_object()
        .cloned()
        .expect("object");
    let pages = walk(&structured, 20_000);
    let (first, pagination) = &pages[0];
    assert!(
        pagination.char_length <= 20_000,
        "{}",
        pagination.char_length
    );
    let heads: Vec<_> = first["results"]
        .as_array()
        .expect("rows")
        .iter()
        .map(|row| (row["index"].as_u64(), row["rowPart"]["part"].as_u64()))
        .collect();
    assert_eq!(
        heads,
        (0..5)
            .map(|index| (Some(index), Some(1)))
            .collect::<Vec<_>>()
    );
    let mut carriers = std::collections::BTreeMap::new();
    for (page, pagination) in &pages {
        assert!(
            pagination.char_length <= 20_000,
            "{}",
            pagination.char_length
        );
        assert!(pagination.oversized.is_none(), "{page:?}");
        for row in page["results"].as_array().expect("rows") {
            let part = row["rowPart"]["part"].as_u64().expect("split");
            let of = row["rowPart"]["of"].as_u64().expect("split");
            if row["data"].get("next").is_some() {
                assert_eq!(part, of, "{row}");
                *carriers.entry(row["index"].as_u64()).or_insert(0) += 1;
            }
        }
    }
    assert_eq!(carriers.len(), 5, "{carriers:?}");
    assert!(carriers.values().all(|count| *count == 1), "{carriers:?}");
    assert_eq!(delivered(&pages).len(), 1_500);
}

/// A split row's page continuation rides its last part, so a caller
/// following it never skips the parts in between; a drill-down rides the
/// first part, beside the head of the evidence. Each appears once.
#[test]
fn split_row_continuations_ride_the_part_they_follow() {
    let mut structured = envelope();
    structured["results"][0]["data"]["next"]["read"] =
        json!({"tool": "localFetch", "query": {"path": "a.rs"}});
    structured["results"][0]["data"]["next"]["nextPage"] =
        json!({"tool": "localSearch", "query": {"page": 2}});
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
            ("nextPage".to_owned(), parts),
            ("read".to_owned(), 1),
            ("sidecar".to_owned(), 1),
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
    let query = json!({"paths": [{"path": "a.rs"}, {"path": "b.rs"}]});
    structured["results"][0]["data"]["hints"]["inspect"] =
        json!({"tool": "localFetch", "query": query});
    structured["results"][0]["data"]["next"]["nextPage"] =
        json!({"tool": "localSearch", "query": query});
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
            for channel in ["next", "hints"] {
                for (name, call) in row["data"][channel].as_object().into_iter().flatten() {
                    if name == "inspect" || name == "nextPage" {
                        queries.push(call["query"].clone());
                    }
                }
            }
        }
    }
    assert!(pages.len() > 2);
    assert_eq!(
        records,
        vec![json!({"path": "generated/api.ts", "cause": "ignored"})]
    );
    assert_eq!(queries, vec![query.clone(), query]);
}

#[test]
fn continuation_query_arrays_are_indivisible_when_they_dominate_the_row() {
    let query = json!({"queries":[{"paths": (0..20)
            .map(|index| format!("path_{index}_{}", "segment".repeat(40)))
            .collect::<Vec<_>>()}]});
    let structured = json!({"results":[{"index":0,"data":{
        "files":[{"path":"a.rs","matches":[{"line":1},{"line":2}]}],
        "hints":{"read":{"tool":"localFetch","query":query}},
        "next":{"nextPage":{"tool":"localSearch","query":query}}
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
            for channel in [PAGES_KEY, HINTS_KEY] {
                for (name, call) in row["data"][channel].as_object().into_iter().flatten() {
                    assert!(
                        call["query"] == query,
                        "{channel}.{name} query was fragmented"
                    );
                    calls.push((channel, name.clone()));
                }
            }
        }
    }
    calls.sort();
    assert_eq!(
        calls,
        vec![(HINTS_KEY, "read".into()), (PAGES_KEY, "nextPage".into())]
    );
    assert_eq!(lines, vec![json!(1), json!(2)]);
}

/// A cycle row as a graph tool lists it: `files` module paths, `runtime` runtime
/// cycles of `members` paths each, and its edge records.
fn cycle_row(cycle: usize, files: usize, runtime: usize, members: usize) -> Value {
    let edges = |kind: &str, count: usize| {
        (0..count)
            .map(|n| {
                json!({"from": format!("c{cycle}/{kind}/from_{n}.ts"),
                           "to": format!("c{cycle}/{kind}/to_{n}.ts"),
                           "edgeKinds": ["static-import"]})
            })
            .collect::<Vec<_>>()
    };
    json!({
        "runtimeCycleCount": runtime,
        "runtimeCycles": (0..runtime)
            .map(|r| (0..members)
                .map(|n| format!("packages/c{cycle}/runtime_{r}/member_{n:03}.ts"))
                .collect::<Vec<_>>())
            .collect::<Vec<_>>(),
        "cycleEdges": edges("cycle", 4),
        "runtimeCycleEdges": edges("runtime", 7),
        "edgeKinds": ["static-import", "type-import"],
        "files": (0..files)
            .map(|n| format!("excalidraw-app/c{cycle}/components/Module_{n:04}.tsx"))
            .collect::<Vec<_>>(),
    })
}

/// Every part of row 0 across `pages`, in order: (rowPart, cycles).
fn row_parts(pages: &[(Map<String, Value>, ResponsePagination)]) -> Vec<(Value, Vec<Value>)> {
    pages
        .iter()
        .flat_map(|(page, _)| page["results"].as_array().cloned().unwrap_or_default())
        .filter(|row| row["index"] == 0)
        .map(|row| {
            let cycles = row["data"]["results"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            (row["rowPart"].clone(), cycles)
        })
        .collect()
}

/// Whether `cycle` holds evidence of cycle number `id`.
fn is_cycle(cycle: &Value, id: usize) -> bool {
    let text = cycle.to_string();
    text.contains(&format!("/c{id}/"))
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
    let full = Value::Object(structured.clone()).to_string();
    let (_, first) = paginate_rows(
        structured.clone(),
        &full,
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
        &full,
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
