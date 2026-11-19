//! VC4: every hoisted response keeps the invariants of hoist §3.6.
//! 1. Every emitted call runs unchanged: the stage never enters a call, and
//!    a merged related call holds every row's query.
//! 2. Every cited value is recoverable: `path` and `line` never move, and
//!    every other value reads back by the one reading rule.
//! 3. A page of a hoisted response reads alone: the envelope's `shared`
//!    and `hints` ride every page.
use super::*;

/// Every form at once over a three-row batch: omitted constants, a nested
/// holder, a shared commit and disclosure block, hints of empty rows and
/// related calls of one tool.
fn every_form() -> Plan {
    super::plan(
        vec![omitted(
            "/files/*/matches/*/side",
            When::Const(json!("RIGHT")),
        )],
        vec![
            super::holder("", &["owner", "repo"], &[]),
            super::holder("/files/*", &["source"], &[]),
        ],
        Batch {
            keyed: Vec::new(),
            shared: vec![Pattern::parse("/commitSha"), Pattern::parse("/coverage")],
            hints: true,
            related: true,
        },
    )
}

fn file(path: &str, lines: &[u64], source: &str) -> Value {
    json!({
        "path": path, "owner": "o", "repo": "r",
        "matches": lines
            .iter()
            .map(|line| json!({"line": line, "value": format!("hit {line}"), "side": "RIGHT", "source": source}))
            .collect::<Vec<_>>(),
    })
}

fn related(term: &str) -> Value {
    json!({"kind": "related", "tool": "localSearch", "input": {"queries": [{"path": "src", "matchString": [term]}]}})
}

fn walk_of(row: usize) -> Value {
    json!({"kind": "continue", "tool": "ghSearchCode", "input": {"cursor": format!("c{row}")}})
}

fn batch_envelope() -> Value {
    let coverage = json!({"state": "partial", "reason": "rateLimited"});
    json!({"results": [
        {"index": 0, "data": {"commitSha": "fdf7", "coverage": coverage, "files": [
            file("src/a.rs", &[3, 9], "search"), file("src/b.rs", &[1], "search"),
        ], "related": [related("alpha")], "next": {"nextPage": walk_of(0)}}},
        {"index": 1, "status": "empty", "data": {"commitSha": "fdf7", "coverage": coverage, "files": [],
            "hints": ["Widen the scope."], "related": [related("beta")]}},
        {"index": 2, "status": "empty", "data": {"commitSha": "fdf7", "coverage": coverage, "files": [],
            "hints": ["Widen the scope."], "related": [related("gamma")]}},
    ]})
}

#[test]
fn a_hoisted_batch_runs_every_call_and_reads_every_value_back() {
    let plan = every_form();
    let original = batch_envelope();
    let stated = super::hoisted(&plan, &original, &super::optional);
    assert!(
        stated.to_string().len() < original.to_string().len(),
        "the batch is smaller"
    );
    // 1. A walk is untouched where it is, and each related query runs in
    //    one call of its tool, as its row's call did.
    assert_eq!(
        stated["results"][0]["data"]["next"],
        original["results"][0]["data"]["next"]
    );
    assert_calls_run(&original, &stated, "every form");
    // 2. `path` and `line` stay where they were; everything else reads back.
    for (row, result) in original["results"]
        .as_array()
        .expect("rows")
        .iter()
        .enumerate()
    {
        for path in super::leaves(&result["data"]) {
            if matches!(path.last().map(String::as_str), Some("path" | "line")) {
                let pointer = format!("/results/{row}/data{}", crate::json_pointer::of(&path));
                assert_eq!(
                    stated.pointer(&pointer),
                    original.pointer(&pointer),
                    "{pointer} moved"
                );
            }
        }
    }
    super::assert_recovers(&plan, &original, &stated);
    assert_eq!(stated["shared"]["data"]["commitSha"], "fdf7");
    assert_eq!(stated["hints"], json!(["Widen the scope."]));
}

/// The stage on the embedded contract keeps a batch's calls and values:
/// what `apply` states once for every tool reads back from the envelope.
#[test]
fn todays_plans_keep_every_value_of_every_tool() {
    for tool in ToolId::ALL {
        let original = batch_envelope();
        let mut stated = original.clone();
        apply(tool, &mut stated);
        super::assert_recovers(plan::of(tool), &original, &stated);
        assert_calls_run(&original, &stated, tool.as_str());
    }
}

/// Paged at a budget that holds its widest row whole, every page of a
/// hoisted batch states the envelope's `shared` and `hints`, so each row
/// reads back whole from its own page; the warnings and the merged related
/// call ride the first page only.
#[test]
fn every_page_of_a_hoisted_batch_reads_alone() {
    use crate::response::page_model::ResponsePageOptions;
    let plan = every_form();
    let original = batch_envelope();
    let mut stated = super::hoisted(&plan, &original, &super::optional);
    stated["warnings"] = json!(["rate limited until 12:00"]);
    let widest = stated["results"]
        .as_array()
        .expect("rows")
        .iter()
        .map(|row| row.to_string().len())
        .max()
        .expect("a row");
    let envelope = stated.as_object().expect("envelope").clone();
    let mut beside = envelope.clone();
    beside.shift_remove("results");
    let overhead = Value::Object(beside).to_string().len() + 64;
    let options = |offset: usize, snapshot: Option<String>| ResponsePageOptions {
        response_chars: Some(overhead + widest),
        response_offset: Some(offset),
        response_snapshot: snapshot,
        paging: true,
        saved: None,
        ..ResponsePageOptions::default()
    };
    let (_, first) = crate::response::row_pages::paginate_rows(
        envelope.clone(),
        &options(0, None),
        Vec::new(),
        ToolId::LocalSearch,
    );
    assert!(first.total_pages > 1, "the batch pages");
    for offset in 0..first.total_pages {
        let (page, _) = crate::response::row_pages::paginate_rows(
            envelope.clone(),
            &options(offset, Some(first.snapshot.clone())),
            Vec::new(),
            ToolId::LocalSearch,
        );
        let page = Value::Object(page);
        assert_eq!(page["shared"], stated["shared"], "page {offset}: {page}");
        assert_eq!(page["hints"], stated["hints"], "page {offset}: {page}");
        assert_eq!(
            page.get("warnings").is_some(),
            offset == 0,
            "page {offset}: {page}"
        );
        for (at, row) in page["results"].as_array().expect("rows").iter().enumerate() {
            assert!(
                row.get("rowPart").is_none(),
                "page {offset}: a row splits: {page}"
            );
            let index = usize::try_from(row["index"].as_u64().expect("index")).expect("index");
            let alone = json!({"results": [original["results"][index].clone()]});
            let mut read = page.clone();
            read["results"] = json!([page["results"][at].clone()]);
            super::assert_recovers(&plan, &alone, &read);
        }
    }
}

/// Every call of `original` runs from `stated` unchanged: a walk or lead as
/// it was, and a related query in one call of its tool holding it (a merged
/// envelope call, K-3).
fn assert_calls_run(original: &Value, stated: &Value, case: &str) {
    let offered = super::calls(stated);
    for call in super::calls(original) {
        if call["kind"] == "related" {
            let query = &call["input"]["queries"][0];
            assert!(
                offered.iter().any(|held| held["tool"] == call["tool"]
                    && held["input"]["queries"]
                        .as_array()
                        .is_some_and(|rows| rows.contains(query))),
                "{case}: {query} runs: {stated}"
            );
        } else {
            assert!(
                offered.contains(&call),
                "{case}: {call} runs unchanged: {stated}"
            );
        }
    }
}
