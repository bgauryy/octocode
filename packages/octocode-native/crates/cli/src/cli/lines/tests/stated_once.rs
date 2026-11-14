//! What text states once instead of on every row, checked as invariants:
//! every fact of the JSON stays recoverable from the text (a path joined
//! with its printed root, each counter, each page's numbers, each constant),
//! and a fact said once is not said again per row. Driven by the recorded
//! results where they hold the shape, else by rows shaped as tools return
//! them.
use super::recorded_results::recorded;
use super::*;
use std::collections::BTreeSet;

/// The tools whose rows print as lines of files.
fn line_format(tool: ToolId) -> bool {
    matches!(
        tool,
        ToolId::LocalSearch | ToolId::AstSearch | ToolId::LspSearch
    )
}

/// The `path` of each item of a row's `files` and `matches`.
fn file_paths(data: &Value) -> BTreeSet<String> {
    ["files", "matches"]
        .into_iter()
        .filter_map(|key| data[key].as_array())
        .flatten()
        .filter_map(|item| item["path"].as_str().map(str::to_owned))
        .collect()
}

/// The root a row's stdout states, if any.
fn printed_root(out: &str) -> Option<&str> {
    out.lines().find_map(|line| line.strip_prefix("# root: "))
}

/// Every file path of a line-format row reads back as the stated root
/// joined with the printed path, and no printed line repeats the root.
#[test]
fn every_file_path_is_its_printed_root_joined_with_the_printed_path() {
    let mut rooted = 0;
    for call in recorded() {
        let row = &call.result["results"][0];
        if !line_format(call.tool) || row["status"] == "error" {
            continue;
        }
        let out = render(call.tool, &call.result, &call.query, None)
            .expect("text")
            .out;
        let paths = file_paths(&row["data"]);
        let Some(root) = printed_root(&out) else {
            // Without a root every path prints as the result names it.
            for path in &paths {
                assert!(
                    out.contains(path.as_str()),
                    "{} lost {path}:\n{out}",
                    call.label
                );
            }
            continue;
        };
        rooted += 1;
        for path in &paths {
            let relative = path
                .strip_prefix(&format!("{root}/"))
                .unwrap_or_else(|| panic!("{}: {path} is not under {root}", call.label));
            // A heading, a `path:N` line, or a count table's `N path` row.
            let names = |line: &str| {
                line.split_whitespace()
                    .any(|word| word == relative || word.starts_with(&format!("{relative}:")))
            };
            assert!(out.lines().any(names), "{} lost {path}:\n{out}", call.label);
        }
        assert!(
            !out.lines()
                .any(|line| line.starts_with(&format!("{root}/"))),
            "{} repeats its root:\n{out}",
            call.label
        );
    }
    assert!(rooted > 0, "no recorded row shares a directory");
}

/// A row's counters are one note that names each with its value.
#[test]
fn a_rows_counters_are_one_note() {
    let mut checked = 0;
    for call in recorded() {
        let row = &call.result["results"][0];
        if !line_format(call.tool) || row["status"] == "error" {
            continue;
        }
        let counters: Vec<(&String, u64)> = row["data"]
            .as_object()
            .into_iter()
            .flatten()
            .filter_map(|(key, value)| Some((key, value.as_u64()?)))
            .collect();
        if counters.is_empty() {
            continue;
        }
        checked += 1;
        let notes = render(call.tool, &call.result, &call.query, None)
            .expect("text")
            .notes;
        let holding: Vec<&str> = notes
            .lines()
            .filter(|line| counters.iter().any(|(key, _)| line.contains(key.as_str())))
            .collect();
        assert_eq!(holding.len(), 1, "{}: {notes}", call.label);
        let line = holding[0];
        for (key, value) in &counters {
            // `key value`, or `key=other value` when equal counters run.
            let named = line.split(' ').collect::<Vec<_>>().windows(2).any(|pair| {
                pair[0].split('=').any(|name| name == *key) && pair[1] == value.to_string()
            });
            assert!(named, "{} {key} {value}: {line}", call.label);
        }
    }
    assert!(checked > 0);
}

/// Every page block of a result `(unit, offset, length, total)`.
fn page_blocks(value: &Value, out: &mut Vec<(String, u64, u64, Option<u64>)>) {
    match value {
        Value::Object(fields) => {
            if let (Some(unit), Some(offset), Some(length), Some(_)) = (
                fields.get("unit").and_then(Value::as_str),
                fields.get("offset").and_then(Value::as_u64),
                fields.get("length").and_then(Value::as_u64),
                fields.get("hasMore"),
            ) {
                out.push((
                    unit.to_owned(),
                    offset,
                    length,
                    fields.get("total").and_then(Value::as_u64),
                ));
            }
            fields.values().for_each(|value| page_blocks(value, out));
        }
        Value::Array(items) => items.iter().for_each(|item| page_blocks(item, out)),
        _ => {}
    }
}

/// A page reads as its count: `10 of 37 lines`, `after N` when it starts
/// later in the list; its offset, length and total stay recoverable.
#[test]
fn a_page_reads_as_its_count() {
    let mut checked = 0;
    for call in recorded() {
        let mut pages = Vec::new();
        page_blocks(&call.result, &mut pages);
        let out = render(call.tool, &call.result, &call.query, None)
            .expect("text")
            .out;
        for (unit, offset, length, total) in pages {
            checked += 1;
            let mut count = match total {
                Some(total) => format!("{length} of {total} {unit}"),
                None => format!("{length} {unit}"),
            };
            if offset > 0 {
                count.push_str(&format!(" after {offset}"));
            }
            assert!(out.contains(&count), "{} {count}:\n{out}", call.label);
        }
        assert!(!out.contains("unit "), "{}:\n{out}", call.label);
    }
    assert!(checked > 0);
}

/// Every fraction of `value`.
fn fractions(value: &Value, out: &mut Vec<f64>) {
    match value {
        Value::Number(number) if number.is_f64() => {
            out.extend(number.as_f64().filter(|x| x.fract() != 0.0));
        }
        Value::Object(fields) => fields.values().for_each(|value| fractions(value, out)),
        Value::Array(items) => items.iter().for_each(|item| fractions(item, out)),
        _ => {}
    }
}

/// A fraction prints to 4 significant digits: the printed number is within
/// half a unit of its 4th digit, and the exact spelling of a longer one is
/// gone.
#[test]
fn a_fraction_prints_to_four_significant_digits() {
    let rank = json!({"results":[{"index":0,"data":{"rows":[
        {"id":"a.ts","evidence":{"pageRank":0.04051045341394517}},
        {"id":"b.ts","evidence":{"pageRank":0.11554587546608726}}]}}]});
    let mut results: Vec<(String, ToolId, Value, Value)> = recorded()
        .into_iter()
        .map(|call| (call.label, call.tool, call.query, call.result))
        .collect();
    results.push(("pageRank".into(), ToolId::GraphQuery, json!({}), rank));
    let mut checked = 0;
    for (label, tool, query, result) in results {
        let mut exact = Vec::new();
        fractions(&result, &mut exact);
        let lines = render(tool, &result, &query, None).expect("text");
        let both = format!("{}\n{}", lines.out, lines.notes);
        let printed: Vec<f64> = both
            .split(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == 'e'))
            .filter_map(|word| word.parse().ok())
            .collect();
        for x in exact {
            checked += 1;
            let unit = 10f64.powf(x.abs().log10().floor() - 3.0);
            assert!(
                printed
                    .iter()
                    .any(|p| (p - x).abs() <= unit / 2.0 + f64::EPSILON),
                "{label} {x}: {both}"
            );
            if x.to_string().trim_start_matches(['-', '0', '.']).len() > 4 {
                assert!(!both.contains(&x.to_string()), "{label} {x}: {both}");
            }
        }
    }
    assert!(checked > 2);
}

/// A field every item of a list holds with one value is stated once, after
/// the list's key; the fields that differ stay on each item.
#[test]
fn a_field_every_item_shares_is_stated_once() {
    let issues = json!({"results":[{"index":0,"data":{"rows":[
        {"id":"4ed5","detector":"unused-export","subject":"src/a.ts","title":"1 export(s) with no importer",
         "confidence":0.6,"tier":60,"evidence":{"exports":["reclaimLock"],"count":1,"dynamicallyImported":false}},
        {"id":"1b29","detector":"unused-export","subject":"src/b.ts","title":"2 export(s) with no importer",
         "confidence":0.6,"tier":60,"evidence":{"exports":["run","stop"],"count":2,"dynamicallyImported":false}}]}}]});
    let out = render(ToolId::GraphQuery, &issues, &json!({}), None)
        .expect("text")
        .out;
    for constant in ["detector", "confidence", "tier", "dynamicallyImported"] {
        assert_eq!(out.matches(constant).count(), 1, "{constant}:\n{out}");
    }
    for varying in [
        "4ed5",
        "1b29",
        "src/a.ts",
        "src/b.ts",
        "reclaimLock",
        "stop",
        "1 export(s)",
        "2 export(s)",
    ] {
        assert!(out.contains(varying), "{varying}:\n{out}");
    }
    for shared in ["unused-export", "0.6", "60", "false"] {
        assert!(out.contains(shared), "{shared}:\n{out}");
    }
}

/// The rows of a batch of graph queries, every one on one snapshot.
fn graph_batch() -> (Value, Value) {
    let graph =
        json!({"id":"20261113T192541Z-src-d9b10b","root":"src","createdAt":"2026-11-13T19:25:41Z"});
    let row = |index: u64, rows: Value| json!({"index":index,"data":{"graph":graph,"rows":rows,"freshness":{"fresh":true,"analysisCurrent":true}}});
    let result = json!({"results":[
        row(0, json!([{"id":"src/a.ts","fanIn":3}])),
        row(1, json!([{"id":"src/b.ts","fanIn":1}])),
        row(2, json!([]))]});
    let asked =
        json!({"queries":[{"operation":"hubs"},{"operation":"dependents"},{"operation":"cycles"}]});
    (result, asked)
}

/// A field every row of a batch shares prints once, before the first row;
/// each row keeps the rest.
#[test]
fn a_field_every_batch_row_shares_prints_once() {
    let (result, asked) = graph_batch();
    let out = render(ToolId::GraphQuery, &result, &asked, None)
        .expect("text")
        .out;
    let first_row = out.find("## results[0]").expect("a heading");
    for shared in ["20261113T192541Z-src-d9b10b", "analysisCurrent"] {
        assert_eq!(out.matches(shared).count(), 1, "{shared}:\n{out}");
        assert!(out.find(shared).is_some_and(|at| at < first_row), "{out}");
    }
    for own in ["src/a.ts", "src/b.ts", "## results[2]"] {
        assert!(out.contains(own), "{own}:\n{out}");
    }
    // A failed row shares nothing: every row states its own fields.
    let mut failed = result.clone();
    failed["results"][2] =
        json!({"index":2,"status":"error","data":{"errorCode":"notFound","error":"gone"}});
    let out = render(ToolId::GraphQuery, &failed, &asked, None)
        .expect("text")
        .out;
    assert_eq!(
        out.matches("20261113T192541Z-src-d9b10b").count(),
        2,
        "{out}"
    );
}

/// The batch as native sends it once a snapshot's facts are stated once
/// (`pageContextFields.batchStatedOnce`): a later row of a snapshot an
/// earlier row stated names only its id, and omits the facts that equal
/// those stated for it.
fn graph_batch_stated_once(snapshots: [&str; 3]) -> (Value, Value) {
    let mut stated = BTreeSet::new();
    let rows: Vec<Value> = snapshots
        .iter()
        .enumerate()
        .map(|(index, id)| {
            if stated.insert(*id) {
                json!({"index":index,"data":{
                    "graph":{"id":format!("snap{id}"),"root":format!("ROOT{id}"),"createdAt":format!("TIME{id}")},
                    "rows":[],"freshness":{"fresh":true,"analysisCurrent":true}}})
            } else {
                json!({"index":index,"data":{"graph":{"id":format!("snap{id}")},"rows":[]}})
            }
        })
        .collect();
    let asked =
        json!({"queries":[{"operation":"hubs"},{"operation":"dependents"},{"operation":"cycles"}]});
    (json!({ "results": rows }), asked)
}

/// A later row whose `graph` names only a snapshot an earlier row stated
/// prints nothing new: one snapshot's graph prints once, before the rows.
/// Rows of different snapshots each name their own, a later one by its id
/// alone.
#[test]
fn a_later_row_naming_only_a_stated_snapshot_prints_nothing_new() {
    let (result, asked) = graph_batch_stated_once(["one", "one", "one"]);
    let out = render(ToolId::GraphQuery, &result, &asked, None)
        .expect("text")
        .out;
    let first_row = out.find("## results[0]").expect("a heading");
    for once in ["snapone", "ROOTone", "TIMEone"] {
        assert_eq!(out.matches(once).count(), 1, "{once}:\n{out}");
        assert!(out.find(once).is_some_and(|at| at < first_row), "{out}");
    }
    // A fact outside the graph prints where its row states it.
    assert_eq!(out.matches("analysisCurrent").count(), 1, "{out}");
    let (result, asked) = graph_batch_stated_once(["one", "two", "one"]);
    let out = render(ToolId::GraphQuery, &result, &asked, None)
        .expect("text")
        .out;
    let last_row = out.find("## results[2]").expect("a heading");
    for (fact, count) in [
        ("snapone", 2),
        ("ROOTone", 1),
        ("TIMEone", 1),
        ("snaptwo", 1),
    ] {
        assert_eq!(out.matches(fact).count(), count, "{fact}:\n{out}");
    }
    assert!(out[last_row..].contains("snapone"), "{out}");
}

/// A batch row is headed by its query's place, on the page that holds it
/// alone too, and every `# ` line it prints names it.
#[test]
fn a_batch_row_is_named_by_its_query_on_every_line() {
    let (result, asked) = graph_batch();
    for (index, row) in result["results"]
        .as_array()
        .expect("rows")
        .iter()
        .enumerate()
    {
        let mut row = row.clone();
        row["data"]["pagination"] =
            json!({"unit":"items","offset":0,"length":1,"total":4,"hasMore":true});
        row["data"]["warnings"] = json!(["renamed"]);
        let page = json!({"results":[row]});
        let lines = render(ToolId::GraphQuery, &page, &asked, None).expect("text");
        let label = format!("results[{index}]");
        assert!(
            lines.out.starts_with(&format!("## {label}\n")),
            "{}",
            lines.out
        );
        assert!(!lines.out.contains(" index "), "{}", lines.out);
        for line in lines.out.lines().chain(lines.notes.lines()) {
            if line.starts_with("# ") {
                assert!(line.starts_with(&format!("# {label} ")), "{line}");
            }
        }
    }
}

/// Beside content, `skippedPaths` reads as a count per cause; a row without
/// content lists the entries.
#[test]
fn skipped_paths_beside_content_read_as_counts_per_cause() {
    let skipped = json!({"entries":[
        {"cause":"ignored","count":169,"path":".claude"},
        {"cause":"hidden","count":25,"path":".agents"},
        {"cause":"hidden","path":".github"}],"sampled":true});
    let hit = json!({"results":[{"index":0,"data":{"matchCount":1,"filesScanned":4810,
        "files":[{"path":"a.rs","matches":[{"line":3,"value":"x"}]}],"skippedPaths":skipped}}]});
    let asked = json!({"queries":[{"path":".","matchString":["x"]}]});
    let notes = render(ToolId::LocalSearch, &hit, &asked, None)
        .expect("text")
        .notes;
    let line = notes
        .lines()
        .find(|line| line.starts_with("# skippedPaths: "))
        .unwrap_or_else(|| panic!("{notes}"));
    assert!(
        line.contains("169 ignored") && line.contains("26 hidden"),
        "{line}"
    );
    assert!(line.contains("sampled"), "{line}");
    assert!(!notes.contains(".claude"), "{notes}");
    let empty = json!({"results":[{"index":0,"status":"empty","data":{"filesScanned":4810,
        "skippedPaths":skipped}}]});
    let notes = render(ToolId::LocalSearch, &empty, &asked, None)
        .expect("text")
        .notes;
    for path in [".claude", ".agents", ".github"] {
        assert!(notes.contains(path), "{path}: {notes}");
    }
}

/// A read's numbered lines print as they are, one per line: no indent, no
/// line of the content lost.
#[test]
fn a_reads_numbered_lines_print_unindented() {
    let mut checked = 0;
    for call in recorded() {
        let Some(content) = call.result["results"][0]["data"]["content"].as_str() else {
            continue;
        };
        let numbered = content.lines().all(|line| {
            line.split_once('\t')
                .is_some_and(|(n, _)| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        });
        if !numbered || !content.contains('\n') {
            continue;
        }
        checked += 1;
        let out = render(call.tool, &call.result, &call.query, None)
            .expect("text")
            .out;
        for line in content.lines() {
            assert!(
                out.lines().any(|printed| printed == line),
                "{} {line:?}:\n{out}",
                call.label
            );
        }
    }
    assert!(checked > 0);
}

/// Counts by file read count first under one head; a listing's lone entry
/// is one `dir/entry` line.
#[test]
fn counts_by_file_and_lone_entries_read_as_one_line_each() {
    let counts = json!({"results":[{"index":0,"data":{"matchCount":17,"files":[
        {"path":"src/x/tests.rs","matchCount":9},{"path":"src/x/listing.rs","matchCount":8}]}}]});
    let asked = json!({"queries":[{"path":"src","matchString":["fn "],"view":"countMatches"}]});
    let out = render(ToolId::LocalSearch, &counts, &asked, None)
        .expect("text")
        .out;
    let root = printed_root(&out).unwrap_or("");
    for (count, path) in [(9, "src/x/tests.rs"), (8, "src/x/listing.rs")] {
        let shown = path.strip_prefix(&format!("{root}/")).unwrap_or(path);
        assert!(
            out.lines()
                .any(|line| line.trim() == format!("{count} {shown}")),
            "{out}"
        );
    }
    assert_eq!(out.matches("matchCount").count(), 1, "{out}");
    let sorted = json!({"results":[{"index":0,"data":{"files":[
        {"dir":"src/a","files":["big.rs (900)"]},{"dir":"src/b","files":["mid.rs (500)"]},
        {"dir":"src/a","files":["small.rs (10)","tiny.rs (1)"]}]}}]});
    let out = render(ToolId::StructureSearch, &sorted, &json!({}), None)
        .expect("text")
        .out;
    for group in sorted["results"][0]["data"]["files"]
        .as_array()
        .expect("groups")
    {
        let dir = group["dir"].as_str().expect("a dir");
        let entries = group["files"].as_array().expect("entries");
        match entries.as_slice() {
            // One entry: its line names the dir, no heading of its own.
            [entry] => assert!(
                out.lines()
                    .any(|line| line == format!("{dir}/{}", entry.as_str().unwrap_or_default())),
                "{out}"
            ),
            entries => {
                assert!(out.lines().any(|line| line == format!("{dir}/")), "{out}");
                for entry in entries {
                    let entry = entry.as_str().unwrap_or_default();
                    assert!(
                        out.lines().any(|line| line == format!("  {entry}")),
                        "{out}"
                    );
                }
            }
        }
    }
    assert!(!out.lines().any(|line| line == "src/b/"), "{out}");
}
