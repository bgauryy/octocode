use super::*;
use crate::id::ToolId;
use crate::tools::test_support::assert_row_contract;
use octocode_contract_types::{OutputSourceEvidence, OutputSourceOrigin};
use std::collections::{BTreeMap, BTreeSet};
use tokio_util::sync::CancellationToken;

fn graph() -> Value {
    json!({"id":"snapshot","root":"/fixture","createdAt":"now"})
}

fn range() -> Value {
    json!({"startByte":0,"endByte":1})
}
pub(super) fn chars(value: &Value) -> usize {
    octocode_engine::json_size::utf16_units(value).unwrap()
}

pub(super) fn catalog(count: usize) -> Value {
    let mut snapshots = (0..count)
        .map(|i| {
            json!({
                "id":format!("raw:{i:03}"),"path":format!("/fixture/source-{i:03}.rs"),
                "digest":"a".repeat(64),"byteLength":40,"encoding":"utf8","bomBytes":0
            })
        })
        .collect::<Vec<_>>();
    snapshots.push(json!({"id":"raw:shared","path":"/fixture/shared.rs",
        "digest":"a".repeat(64),"byteLength":40,"encoding":"utf8","bomBytes":0}));
    let value = json!({"snapshots":snapshots,
        "representations":(0..count).map(|i|json!({"id":format!("repr:{i:03}"),
            "sourceDigest":"a".repeat(64),"byteLength":40,
            "producer":{"id":"fixture-map","version":"1"},
            "sourceIds":[format!("raw:{i:03}"),"raw:shared"]})).collect::<Vec<_>>(),
        "units":(0..count).map(|i|json!({"id":format!("unit:{i:03}"),
            "adapter":{"id":"fixture-adapter","version":"1"},"operation":"graphFacts",
            "representationId":format!("repr:{i:03}"),
            "regions":[{"startByte":0,"endByte":40}]})).collect::<Vec<_>>()});
    serde_json::from_value::<OutputSourceEvidence>(value.clone()).unwrap();
    value
}

pub(super) fn origin(i: usize) -> Value {
    let source = json!({"sourceId":format!("raw:{i:03}"),"rawRange":range()});
    let value = json!({"analysisUnitId":format!("unit:{i:03}"),"segments":[{
        "sourceRange":range(),"analysis":"included","branches":[
            {"kind":"copied","origin":source},
            {"kind":"derived","contributors":[{"kind":"source","origin":source},
                {"kind":"source","origin":{"sourceId":"raw:shared","rawRange":range()}},
                {"kind":"unmapped","reason":"mappingUnavailable"},
                {"kind":"source","origin":source}]}]}]});
    serde_json::from_value::<OutputSourceOrigin>(value.clone()).unwrap();
    value
}

pub(super) fn fixture(units: usize, rows: usize) -> Value {
    json!({"op":"unresolved","graph":graph(),
        "total":rows,"sourceEvidence":catalog(units),"results":(0..rows).map(|i|json!({
            "id":format!("site:{i:03}"),"from":format!("source-{i:03}.rs#owner"),
            "targetName":"callback","edge":"passes","file":format!("source-{i:03}.rs"),"line":1,
            "reason":"binding-unrepresented","sourceDigest":"a".repeat(64),
            "sourceRange":range(),"sourceOrigin":origin(i%units),
            "selectionByteRange":range(),"selectionOrigin":origin(i%units),
            "recipient":{"occurrenceId":format!("call:{i:03}"),"sourceRange":range(),
                "sourceOrigin":origin((i+1)%units)},
            "bindingSourceRange":range(),"bindingOrigin":origin((i+2)%units),
            "argument":{"groupIndex":0,"index":0,"form":"identifier"},
            "parseAssessment":{"status":"parsed","scope":"occurrence"},
            "derivation":{"rule":"fixture:blocked-reference","version":"1"},"evidence":"syntax"
        })).collect::<Vec<_>>()})
}

fn unit_ids(value: &Value, ids: &mut BTreeSet<String>) {
    match value {
        Value::Object(object) => {
            if let (Some(id), Some(_)) = (
                object.get("analysisUnitId").and_then(Value::as_str),
                object.get("segments").and_then(Value::as_array),
            ) {
                ids.insert(id.to_owned());
            } else {
                for (key, child) in object {
                    if key != "sourceEvidence" {
                        unit_ids(child, ids);
                    }
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                unit_ids(item, ids);
            }
        }
        _ => {}
    }
}

pub(super) fn exact_closure(data: &Value, full: &Value) {
    let mut ids = BTreeSet::new();
    unit_ids(data, &mut ids);
    let select = |key: &str, ids: &BTreeSet<String>| {
        full[key]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| ids.contains(row["id"].as_str().unwrap()))
            .cloned()
            .collect::<Vec<_>>()
    };
    let units = select("units", &ids);
    assert_eq!(units.len(), ids.len(), "every origin resolves");
    let representations = select(
        "representations",
        &units
            .iter()
            .map(|unit| unit["representationId"].as_str().unwrap().to_owned())
            .collect(),
    );
    let snapshots = select(
        "snapshots",
        &representations
            .iter()
            .flat_map(|repr| {
                repr["sourceIds"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|id| id.as_str().unwrap().to_owned())
            })
            .collect(),
    );
    assert_eq!(
        data["sourceEvidence"],
        json!({"snapshots":snapshots,
        "representations":representations,"units":units}),
        "exact ordered descriptor closure"
    );
}

fn plan(data: Value, tool: ToolId, budget: usize) -> RowPlan {
    assert_row_contract(tool, &data);
    PreparedRows::new(vec![json!({"index":0,"data":data})], &[None], Some(tool))
        .plan(budget, &CancellationToken::new())
        .expect("valid dependency closure")
}

#[test]
fn hundred_unit_graph_fragments_fit_and_preserve_all_evidence_lazily() {
    let data = fixture(100, 100);
    let catalog = data["sourceEvidence"].clone();
    let expected = data["results"].as_array().unwrap().clone();
    MATERIALIZED.with(|count| count.set(0));
    let plan = plan(data, ToolId::GraphQuery, 12_000);
    assert!(plan.total_pages > 1);
    assert_eq!(MATERIALIZED.with(std::cell::Cell::get), 0);
    let late = plan
        .build_page(
            plan.total_pages - 1,
            &[None],
            &[],
            &CancellationToken::new(),
        )
        .unwrap();
    assert!(!late.is_empty());
    assert_eq!(MATERIALIZED.with(std::cell::Cell::get), late.len());
    let mut seen = Vec::new();
    for page in 0..plan.total_pages {
        let rows = plan
            .build_page(page, &[None], &[], &CancellationToken::new())
            .unwrap();
        assert!(
            chars(&json!(rows)) <= 12_000,
            "page {page} exceeds its row budget"
        );
        for row in rows {
            assert_row_contract(ToolId::GraphQuery, &row["data"]);
            exact_closure(&row["data"], &catalog);
            assert!(
                row["data"]["sourceEvidence"]["units"]
                    .as_array()
                    .unwrap()
                    .len()
                    < 100
            );
            seen.extend(row["data"]["results"].as_array().unwrap().iter().cloned());
        }
    }
    assert_eq!(
        seen, expected,
        "all four origins and ordered/repeated contributors survive"
    );
}

#[test]
fn shared_descriptor_union_is_charged_once_when_packing_fragments() {
    let data = fixture(1, 30);
    let catalog = data["sourceEvidence"].clone();
    let mut three = data.clone();
    three["results"].as_array_mut().unwrap().truncate(3);
    let budget = chars(&json!({"index":0,"data":three})) + 256;
    let expected = data["results"].clone();
    let plan = plan(data, ToolId::GraphQuery, budget);
    assert!(plan.total_pages > 1);
    let mut seen = Vec::new();
    for page in 0..plan.total_pages {
        let rows = plan
            .build_page(page, &[None], &[], &CancellationToken::new())
            .unwrap();
        assert!(chars(&json!(rows)) <= budget);
        let mut count = 0;
        for row in rows {
            exact_closure(&row["data"], &catalog);
            let results = row["data"]["results"].as_array().unwrap();
            count += results.len();
            seen.extend(results.iter().cloned());
        }
        if page + 1 < plan.total_pages {
            assert!(count >= 3, "shared descriptors were charged per item");
        }
    }
    assert_eq!(json!(seen), expected);
}

fn symbol(name: &str, unit: usize) -> Value {
    json!({"symbolName":name,"kind":"class","line":1,"sourceDigest":"a".repeat(64),
        "sourceRange":range(),"sourceOrigin":origin(unit),
        "selectionByteRange":range(),"selectionOrigin":origin(unit)})
}

fn leaves(symbols: &Value, expected: &BTreeMap<String, Value>, seen: &mut Vec<Value>) {
    for symbol in symbols.as_array().unwrap() {
        let name = symbol["symbolName"].as_str().unwrap();
        for key in [
            "sourceOrigin",
            "selectionOrigin",
            "sourceRange",
            "selectionByteRange",
        ] {
            assert_eq!(symbol[key], expected[name][key]);
        }
        if let Some(members) = symbol.get("members") {
            leaves(members, expected, seen);
        } else {
            seen.push(symbol.clone());
        }
    }
}

#[test]
fn recursive_joined_neighbors_keep_their_own_closures_before_and_after() {
    let before = symbol("Before", 0);
    let after = symbol("After", 1);
    let children = (0..19)
        .map(|i| symbol(&format!("Leaf{i}"), i + 3))
        .collect::<Vec<_>>();
    let mut parent = symbol("Container", 2);
    parent["members"] = json!(children);
    let expected = std::iter::once(before.clone())
        .chain(children.clone())
        .chain(std::iter::once(after.clone()))
        .collect::<Vec<_>>();
    let by_name = expected
        .iter()
        .chain(std::iter::once(&parent))
        .map(|row| (row["symbolName"].as_str().unwrap().to_owned(), row.clone()))
        .collect::<BTreeMap<_, _>>();
    let catalog = catalog(22);
    let data = json!({"symbols":[before,parent,after],"sourceEvidence":catalog});
    let (mut joined_before, mut joined_after) = (false, false);
    for budget in [10_000, 14_000, 19_000] {
        let plan = plan(data.clone(), ToolId::AstSearch, budget);
        assert!(plan.total_pages > 1);
        let mut seen = Vec::new();
        for page in 0..plan.total_pages {
            let rows = plan
                .build_page(page, &[None], &[], &CancellationToken::new())
                .unwrap();
            assert!(chars(&json!(rows)) <= budget);
            for row in rows {
                assert_row_contract(ToolId::AstSearch, &row["data"]);
                exact_closure(&row["data"], &catalog);
                let symbols = &row["data"]["symbols"];
                let names = symbols
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v["symbolName"].as_str().unwrap())
                    .collect::<Vec<_>>();
                joined_before |= names.contains(&"Before") && names.contains(&"Container");
                joined_after |= names.contains(&"After") && names.contains(&"Container");
                leaves(symbols, &by_name, &mut seen);
            }
        }
        assert_eq!(seen, expected);
    }
    assert!(
        joined_before && joined_after,
        "fixture must cross both Joined seams"
    );
}

#[test]
fn escaped_keyed_records_preserve_each_origin_and_only_its_descriptor_closure() {
    let mut mentions = Map::new();
    for (key, start) in [("0", 0), ("name/~part", 20), ("last", 40)] {
        mentions.insert(
            key.into(),
            json!(
                (start..start + 20)
                    .map(|i| json!({"id":i,"sourceOrigin":origin(i)}))
                    .collect::<Vec<_>>()
            ),
        );
    }
    let catalog = catalog(60);
    let data = json!({"op":"drift","graph":graph(),"results":[{"id":"finding",
        "evidence":{"textMentions":mentions}}],"sourceEvidence":catalog});
    let plan = plan(data, ToolId::GraphQuery, 12_000);
    assert!(plan.total_pages > 1);
    let mut seen = BTreeMap::<String, Vec<Value>>::new();
    for page in 0..plan.total_pages {
        let rows = plan
            .build_page(page, &[None], &[], &CancellationToken::new())
            .unwrap();
        assert!(chars(&json!(rows)) <= 12_000);
        for row in rows {
            assert_row_contract(ToolId::GraphQuery, &row["data"]);
            exact_closure(&row["data"], &catalog);
            for finding in row["data"]["results"].as_array().unwrap() {
                for (key, values) in finding["evidence"]["textMentions"].as_object().unwrap() {
                    seen.entry(key.clone())
                        .or_default()
                        .extend(values.as_array().unwrap().iter().cloned());
                }
            }
        }
    }
    assert_eq!(json!(seen), json!(mentions));
}

#[test]
fn indivisible_origin_is_complete_and_explicitly_oversized_at_tiny_budget() {
    let catalog = catalog(100);
    let data = json!({"op":"node","graph":graph(),"node":{"id":"atomic","sourceRange":range(),
        "sourceOrigin":origin(0)},"sourceEvidence":catalog});
    assert_row_contract(ToolId::GraphQuery, &data);
    let options = ResponsePageOptions {
        response_chars: Some(128),
        paging: true,
        ..Default::default()
    };
    let (page, pagination) = paginate_rows(
        json!({"results":[{"index":0,"data":data}]})
            .as_object()
            .unwrap()
            .clone(),
        &options,
        Vec::new(),
        &[],
        Some(ToolId::GraphQuery),
    );
    assert_eq!(pagination.total_pages, 1);
    assert_eq!(pagination.oversized, Some(true));
    assert!(!pagination.has_more);
    assert!(pagination.next_offset.is_none());
    let actual = &page["results"][0]["data"];
    assert_eq!(actual["node"], data["node"]);
    exact_closure(actual, &catalog);
}

#[test]
fn artifact_engine_names_are_metadata_without_a_source_catalog() {
    let engines = (0..25)
        .map(|i| (format!("engine{i}"), json!("version".repeat(20))))
        .chain([
            ("analysisUnitId".into(), json!(">=22")),
            ("segments".into(), json!("^1.0")),
        ])
        .collect::<Map<String, Value>>();
    let data = json!({"artifacts":[{"packageName":"fixture","engines":engines}]});
    let plan = plan(data, ToolId::ArtifactSearch, 700);
    assert!(plan.total_pages > 1);
    let mut seen = BTreeMap::new();
    for page in 0..plan.total_pages {
        for row in plan
            .build_page(page, &[None], &[], &CancellationToken::new())
            .unwrap()
        {
            assert_row_contract(ToolId::ArtifactSearch, &row["data"]);
            assert!(row["data"].get("sourceEvidence").is_none());
            for artifact in row["data"]["artifacts"].as_array().unwrap() {
                for (key, value) in artifact["engines"].as_object().unwrap() {
                    assert!(seen.insert(key.clone(), value.clone()).is_none());
                }
            }
        }
    }
    assert_eq!(json!(seen), json!(engines));
}
