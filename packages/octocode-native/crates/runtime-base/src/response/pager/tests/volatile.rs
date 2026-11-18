//! B101: per-call provider facts (`rateLimit`, `clientWaits`, `observedAt`)
//! differ between
//! executions of one request, present on one run and absent on the next. A
//! pinned page re-executed after its saved replay is gone must cut where the
//! first run cut, or the walk gets gaps or duplicates.
use super::*;
use octocode_test_support::json_fixture;

const TOOL: crate::id::ToolId = crate::id::ToolId::GhSearchCode;

/// Two rows of `files`, each wider than one page at the budgets below; the
/// first row carries `field` when `facts` is set.
fn run(field: &str, facts: Option<Value>) -> Value {
    let rows = (0..2)
        .map(|index| {
            let files = (0..24)
                .map(|id| json!({"id": index * 100 + id, "text": "v".repeat(40 + id % 7)}))
                .collect::<Vec<_>>();
            let mut data = json!({"files": files});
            if index == 0
                && let Some(facts) = &facts
            {
                data[field] = facts.clone();
            }
            json!({"index": index, "data": data})
        })
        .collect::<Vec<_>>();
    json!({"results": rows})
}

/// Each volatile field, and a provider resource name longer than any the
/// contract bounds: metadata is never measured against the page target.
fn variants() -> [(&'static str, Value); 4] {
    [
        (
            "rateLimit",
            json_fixture(
                r#"{
                    "resource": "search",
                    "remaining": 0,
                    "resetEpochSeconds": 1794032000
                }"#,
            ),
        ),
        (
            "rateLimit",
            json!({"resource": "r".repeat(240), "retryAfterSeconds": 60}),
        ),
        (
            "clientWaits",
            json_fixture(
                r#"[
                    {"kind": "quotaPacing", "ms": 1307},
                    {"kind": "spacing", "ms": 4}
                ]"#,
            ),
        ),
        ("observedAt", json!("1970-01-02T00:00:00Z")),
    ]
}

fn options(budget: usize) -> ResponsePageOptions {
    ResponsePageOptions {
        response_chars: Some(budget),
        paging: true,
        ..Default::default()
    }
}

fn page(input: &Value, options: &ResponsePageOptions) -> Result<PreparedPage, ResponseError> {
    prepare(
        ResponseInput {
            tool: TOOL,
            structured: input.clone(),
            options: options.clone(),
        },
        &Cancellation::new(),
    )
}

/// The file ids of one page, by row and part: where the page's edges lie.
type Edges = Vec<(u64, Vec<u64>)>;

fn edges(page: &Value) -> Edges {
    page["results"]
        .as_array()
        .expect("rows")
        .iter()
        .map(|row| {
            let ids = row["data"]["files"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|file| file["id"].as_u64().expect("id"))
                .collect();
            (row["index"].as_u64().expect("index"), ids)
        })
        .collect()
}

/// Every page of `input`: its edges and the cursor that reads on.
fn walk(input: &Value, budget: usize) -> Vec<(Edges, Option<String>)> {
    let mut options = options(budget);
    let mut pages = Vec::new();
    loop {
        let prepared = page(input, &options).expect("page");
        assert!(
            prepared.value.get("oversized").is_none(),
            "per-call metadata never flags a page: {}",
            prepared.value
        );
        let next = prepared
            .next
            .as_ref()
            .map(|cursor| cursor.content_snapshot("build"));
        pages.push((edges(&prepared.value), next));
        let Some(cursor) = prepared.next else {
            return pages;
        };
        options.open(&cursor);
        assert!(pages.len() < 1000, "finite page walk");
    }
}

/// Every value of row 0's `field` across the walk of `input` at `budget`,
/// an array's items one by one.
fn row_values(input: &Value, budget: usize, field: &str) -> Vec<Value> {
    let mut options = options(budget);
    let mut values = Vec::new();
    loop {
        let prepared = page(input, &options).expect("page");
        for row in prepared.value["results"].as_array().into_iter().flatten() {
            match row["data"].get(field) {
                Some(Value::Array(items)) if row["index"] == 0 => values.extend(items.clone()),
                Some(value) if row["index"] == 0 => values.push(value.clone()),
                _ => {}
            }
        }
        let Some(cursor) = prepared.next else {
            return values;
        };
        options.open(&cursor);
    }
}

/// B105: the moment a saved answer was reused differs between executions of
/// one request (a re-execution may reuse, at another moment, what the first
/// run fetched live), so it moves neither the snapshot a pinned page checks
/// nor any page edge, reaches its row once as `observedAt`, and leaves the
/// row's own hints as they were.
#[test]
fn a_reuse_moment_moves_neither_page_edges_nor_the_snapshot() {
    for kept in [None, Some(json_fixture(r#"["A hint every run states."]"#))] {
        let base = run("hints", kept.clone());
        let kept_hints = kept
            .clone()
            .map_or_else(Vec::new, |hints| hints.as_array().expect("hints").clone());
        for (observed_ms, moment) in [
            (86_400_000, "1970-01-02T00:00:00Z"),
            (172_800_000, "1970-01-03T00:00:00Z"),
        ] {
            let mut reused = base.clone();
            crate::providers::saved_answers::disclose_reuse(
                &mut reused["results"][0]["data"],
                observed_ms,
            );
            for budget in [600, 900, 1500, 2600] {
                assert_eq!(
                    content_snapshot(&reused, budget, "build"),
                    content_snapshot(&base, budget, "build"),
                    "{kept:?} at {budget}: snapshot digest"
                );
                assert_eq!(
                    walk(&reused, budget),
                    walk(&base, budget),
                    "{kept:?} at {budget}: page edges"
                );
                assert_eq!(
                    row_values(&reused, budget, "observedAt"),
                    [json!(moment)],
                    "{kept:?} at {budget}: delivered once"
                );
                assert_eq!(
                    row_values(&reused, budget, "hints"),
                    kept_hints,
                    "{kept:?} at {budget}: hints untouched"
                );
            }
        }
    }
}

#[test]
fn volatile_presence_moves_neither_page_edges_nor_the_snapshot() {
    for (field, facts) in variants() {
        for budget in [600, 900, 1500, 2600] {
            let present = run(field, Some(facts.clone()));
            let absent = run(field, None);
            assert_eq!(
                content_snapshot(&present, budget, "build"),
                content_snapshot(&absent, budget, "build"),
                "{field} at {budget}: snapshot digest"
            );
            let (with, without) = (walk(&present, budget), walk(&absent, budget));
            assert!(with.len() > 1, "{field} at {budget}: a split walk");
            assert_eq!(with, without, "{field} at {budget}: page edges");
        }
    }
}

/// B105 R2-1: taking the facts out keeps the order of the keys left, so a
/// run whose row states them mid-`data` has the snapshot of a run without
/// them (the digest reads keys in order).
#[test]
fn facts_mid_row_keep_the_key_order_and_the_snapshot() {
    let row = |facts: bool| {
        let mut data = serde_json::Map::new();
        data.insert("files".into(), json!([{"id": 1, "text": "v".repeat(60)}]));
        if facts {
            data.insert("observedAt".into(), json!("1970-01-02T00:00:00Z"));
            data.insert("rateLimit".into(), json_fixture(r#"{"remaining": 0}"#));
        }
        data.insert("totalLines".into(), json!(3));
        data.insert("pagination".into(), json_fixture(r#"{"hasMore": false}"#));
        data.insert("modified".into(), json!("2026-11-07T08:13:26.323Z"));
        json!({"results": [{"index": 0, "data": data}]})
    };
    let (with, without) = (row(true), row(false));
    let mut taken = with.clone();
    take_volatile_fields(taken.as_object_mut().expect("envelope"));
    assert_eq!(taken.to_string(), without.to_string(), "key order");
    for budget in [200, 600, 2600] {
        assert_eq!(
            content_snapshot(&with, budget, "build"),
            content_snapshot(&without, budget, "build"),
            "snapshot at {budget}"
        );
    }
}

/// The chars `volatile_chars` names are exactly what removing the facts
/// takes off the rendering, for rows with none, an empty `data`, facts
/// beside evidence, and facts alone. Each row's `data` without its facts is
/// written out here, apart from any function under test.
#[test]
fn volatile_chars_is_the_exact_rendered_size_of_the_facts() {
    let facts = json_fixture(r#"{"remaining": 0, "resource": "core"}"#);
    let waits = json_fixture(r#"[{"kind": "spacing", "ms": 4}]"#);
    let moment = json!("1970-01-02T00:00:00Z");
    for (data, bare) in [
        (json!({}), json!({})),
        (
            json_fixture(r#"{"files": []}"#),
            json_fixture(r#"{"files": []}"#),
        ),
        (
            json!({"files": [], "rateLimit": facts, "clientWaits": waits}),
            json_fixture(r#"{"files": []}"#),
        ),
        (json!({"rateLimit": facts}), json!({})),
        (json!({"rateLimit": facts, "clientWaits": waits}), json!({})),
        (json!({"observedAt": moment}), json!({})),
        (
            json!({"files": [], "hints": ["kept 界"], "observedAt": moment}),
            json_fixture(r#"{"files": [], "hints": ["kept 界"]}"#),
        ),
        (
            json!({"observedAt": moment, "files": ["界"], "rateLimit": facts}),
            json_fixture(r#"{"files": ["界"]}"#),
        ),
        (
            json!({"clientWaits": waits, "observedAt": moment, "rateLimit": facts}),
            json!({}),
        ),
    ] {
        let with = json!({"results": [{"index": 0, "data": data}, {"index": 1}]});
        let without = json!({"results": [{"index": 0, "data": bare}, {"index": 1}]});
        let width = |value: &Value| value.to_string().encode_utf16().count();
        assert_eq!(
            volatile_chars(with.as_object().expect("envelope")),
            width(&with) - width(&without),
            "{with}"
        );
    }
}

/// Page one comes from a run with the facts, every later page from a
/// re-execution without them (and the reverse): each file arrives once.
#[test]
fn a_walk_over_re_executed_pages_returns_every_row_exactly_once() {
    for (field, facts) in variants() {
        for budget in [600, 900, 1500, 2600] {
            let runs = [run(field, Some(facts.clone())), run(field, None)];
            let expected = edges(&runs[0])
                .into_iter()
                .flat_map(|(_, ids)| ids)
                .collect::<Vec<_>>();
            for first in 0..2 {
                let mut options = options(budget);
                let mut seen = Vec::new();
                let mut facts_seen = 0;
                let mut source = &runs[first];
                loop {
                    let prepared = page(source, &options)
                        .unwrap_or_else(|error| panic!("{field} at {budget}: {error:?}"));
                    for row in prepared.value["results"].as_array().expect("rows") {
                        facts_seen += usize::from(row["data"].get(field).is_some());
                    }
                    seen.extend(edges(&prepared.value).into_iter().flat_map(|(_, ids)| ids));
                    let Some(cursor) = prepared.next else {
                        break;
                    };
                    options.open(&cursor);
                    source = &runs[1 - first];
                }
                assert_eq!(seen, expected, "{field} at {budget}, first run {first}");
                assert!(facts_seen <= 1, "{field}: provider facts ride one part");
            }
        }
    }
}
