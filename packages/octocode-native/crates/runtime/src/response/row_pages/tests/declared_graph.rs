use super::*;
use crate::id::ToolId;
use std::collections::BTreeMap;
use tokio_util::sync::CancellationToken;

fn fragments(data: Value, tool: ToolId, budget: usize) -> Vec<Value> {
    let plan = PreparedRows::new(vec![json!({"index":0,"data":data})], &[None], Some(tool))
        .plan(budget, &CancellationToken::new())
        .expect("valid plan");
    assert!(plan.total_pages > 1, "fixture must exercise fragmentation");
    (0..plan.total_pages)
        .flat_map(|page| {
            plan.build_page(page, &[None], &[], &CancellationToken::new())
                .expect("valid build_page")
        })
        .collect()
}

fn coverage() -> Value {
    json!({"scope":"snapshot","population":"syntax-argument-slots","byLanguage":{
        "first":{"extraction":"supported","referenceForms":["identifier","member"]},
        "second":{"extraction":"supported","referenceForms":["identifier","addressOf"]}
    }})
}

fn source_evidence() -> Value {
    json!({
        "snapshots":[{"id":"raw","path":"source","digest":"a".repeat(64),
            "byteLength":40,"encoding":"utf8","bomBytes":0}],
        "representations":[{"id":"parsed","sourceDigest":"a".repeat(64),
            "byteLength":40,"producer":{"id":"decode","version":"1"},"sourceIds":["raw"]}],
        "units":[{"id":"unit","adapter":{"id":"fixture","version":"1"},
            "operation":"graphFacts","representationId":"parsed",
            "regions":[{"startByte":0,"endByte":40}]}]
    })
}

fn source_origin(index: usize) -> Value {
    let origin = json!({"sourceId":"raw","rawRange":{"startByte":index,"endByte":index+1}});
    json!({"analysisUnitId":"unit","segments":[{
        "sourceRange":{"startByte":index,"endByte":index+1},"analysis":"included",
        "branches":[
            {"kind":"copied","origin":origin},
            {"kind":"derived","contributors":[
                {"kind":"source","origin":origin},
                {"kind":"unmapped","reason":"mappingUnavailable"},
                {"kind":"source","origin":origin}
            ]}
        ]
    }]})
}

#[test]
fn every_graph_evidence_fragment_preserves_capabilities_and_validates() {
    let results = (0..40)
        .map(|index| {
            json!({
                "id":format!("site-{index}"), "from":"source#owner", "targetName":"callback",
                "edge":"calls", "file":"source", "line":index+1, "reason":"other",
                "sourceDigest":"a".repeat(64), "sourceRange":{"startByte":index,"endByte":index+1},
                "sourceOrigin":source_origin(index),
                "derivation":{"rule":"resolve:other","version":"test"}, "evidence":"syntax"
            })
        })
        .collect::<Vec<_>>();
    let expected = coverage();
    let descriptors = source_evidence();
    let data = json!({"op":"unresolved","graph":{"id":"snapshot","root":"/scope","createdAt":"now"},
        "results":results,"total":40,"argumentCoverage":expected,"edges":["calls","passes"],
        "scan":{"excludeDir":["vendor","generated"]},"sourceEvidence":descriptors});
    let mut seen = Vec::new();
    for row in fragments(data, ToolId::GraphQuery, 1700) {
        let data = &row["data"];
        crate::tools::test_support::assert_row_contract(ToolId::GraphQuery, data);
        assert_eq!(data["argumentCoverage"], expected);
        assert_eq!(data["sourceEvidence"], descriptors);
        assert_eq!(data["edges"], json!(["calls", "passes"]));
        assert_eq!(data["scan"]["excludeDir"], json!(["vendor", "generated"]));
        seen.extend(data["results"].as_array().expect("results").iter().cloned());
    }
    assert_eq!(seen, results);
}

fn items(label: &str, count: usize) -> Value {
    json!(
        (0..count)
            .map(|index| format!("{label}-{index}-{}", "x".repeat(30)))
            .collect::<Vec<_>>()
    )
}

#[test]
fn declared_sibling_evidence_is_delivered_once_for_large_and_small_lists() {
    for size in [1, 30] {
        let mut data = json!({"op":"impact","results":(0..30).map(|index| json!({"id":format!("result-{index}"),"roles":["test","generated"]})).collect::<Vec<_>>()});
        let paths = [
            "/summary/changed",
            "/summary/configChanges",
            "/summary/affectedEntrypoints",
            "/summary/testsToRun",
            "/summary/testFunctions",
            "/summary/components",
            "/summary/unmapped",
            "/summary/notes",
            "/baseline/resolvedFindings",
            "/languages",
            "/mostImportedFiles",
            "/mostImportedPackages",
            "/mostCalledSymbols",
        ];
        data["summary"] = json!({});
        data["baseline"] = json!({});
        let mut expected = BTreeMap::new();
        for path in paths {
            let (parent, key) = path.rsplit_once('/').expect("path");
            let value = items(path, size);
            data.pointer_mut(parent).expect("parent")[key] = value.clone();
            expected.insert(path, value);
        }
        let results = data["results"].clone();
        expected.insert("/results", results);
        let mut actual = BTreeMap::<&str, Vec<Value>>::new();
        for row in fragments(data, ToolId::GraphQuery, 1600) {
            for path in expected.keys() {
                actual.entry(path).or_default().extend(
                    row["data"]
                        .pointer(path)
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .cloned(),
                );
            }
        }
        for (path, expected) in expected {
            assert_eq!(json!(actual[path]), expected, "{path}, sibling size {size}");
        }
    }
}

#[test]
fn nested_declared_arrays_and_keyed_records_preserve_every_item() {
    let mut expected = BTreeMap::<String, Vec<Value>>::new();
    let rows = (0..3).map(|index| {
        let id = format!("finding-{index}");
        let mut evidence = json!({"members":items("member",40),"witness":items("hop",30),"suggestedCuts":items("cut",1),"textMentions":{}});
        for name in ["0","name/~part"] {
            evidence["textMentions"][name] = items(name,25);
        }
        for (field,value) in evidence.as_object().expect("evidence") {
            if field == "textMentions" {
                for (name, sites) in value.as_object().expect("names") {
                    expected.insert(format!("{id}/{field}/{name}"), sites.as_array().expect("sites").clone());
                }
            } else {
                expected.insert(format!("{id}/{field}"), value.as_array().expect("list").clone());
            }
        }
        json!({"id":id,"evidence":evidence,"controls":["scope confirmed"],"emptyContext":[],"coordinates":[[0,1],[2,3]]})
    }).collect::<Vec<_>>();
    let mut actual = BTreeMap::<String, Vec<Value>>::new();
    for part in fragments(json!({"results":rows}), ToolId::GraphQuery, 1700) {
        for row in part["data"]["results"].as_array().expect("results") {
            let id = row["id"].as_str().expect("stable finding identity");
            assert_eq!(row["controls"], json!(["scope confirmed"]));
            assert_eq!(row["emptyContext"], json!([]));
            assert_eq!(row["coordinates"], json!([[0, 1], [2, 3]]));
            for (field, value) in row["evidence"].as_object().expect("evidence") {
                if field == "textMentions" {
                    for (name, sites) in value.as_object().expect("names") {
                        actual
                            .entry(format!("{id}/{field}/{name}"))
                            .or_default()
                            .extend(sites.as_array().expect("sites").iter().cloned());
                    }
                } else {
                    actual
                        .entry(format!("{id}/{field}"))
                        .or_default()
                        .extend(value.as_array().expect("list").iter().cloned());
                }
            }
        }
    }
    assert_eq!(actual, expected);
}

#[test]
fn graph_build_pruning_pages_do_not_split_capability_metadata() {
    let pruned = items("snapshot", 40);
    let expected = coverage();
    let mut actual = Vec::new();
    for row in fragments(
        json!({"id":"new","pruned":pruned,"argumentCoverage":expected}),
        ToolId::GraphBuild,
        900,
    ) {
        crate::tools::test_support::assert_row_contract(ToolId::GraphBuild, &row["data"]);
        assert_eq!(row["data"]["argumentCoverage"], expected);
        actual.extend(
            row["data"]["pruned"]
                .as_array()
                .expect("pruned")
                .iter()
                .cloned(),
        );
    }
    assert_eq!(json!(actual), pruned);
}

#[test]
fn undeclared_collections_remain_whole_and_oversized_is_terminal() {
    let data = json!({"node":{"id":"atomic","coordinates":items("coordinate",100)},"results":[]});
    let options = ResponsePageOptions {
        response_chars: Some(500),
        paging: true,
        ..Default::default()
    };
    let (page, pagination) = paginate_rows(
        json!({"results":[{"index":0,"data":data}]})
            .as_object()
            .expect("envelope")
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
    assert_eq!(page["results"][0]["data"], data);
    static NO_COLLECTIONS: crate::contracts::output_fields::CollectionRoutes =
        crate::contracts::output_fields::CollectionRoutes { nodes: Vec::new() };
    let mut arena = FragmentArena {
        collection_routes: Some(&NO_COLLECTIONS),
        ..Default::default()
    };
    let mut planned = Vec::new();
    arena
        .plan(
            json!({"data":{"array":items("atomic",100)}}),
            100,
            100,
            &mut planned,
        )
        .expect("valid fragments");
    assert_eq!(
        planned.len(),
        1,
        "explicit empty declarations forbid generic array splitting"
    );
}
