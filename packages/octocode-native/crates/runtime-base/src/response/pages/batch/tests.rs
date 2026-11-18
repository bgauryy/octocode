//! K4: a batch of graph results on one snapshot states its facts once.
use super::*;
use octocode_test_support::json_fixture;
use serde_json::json;

/// The group a graph contract declares: a result reads the snapshot its
/// `graph.id` names, and the snapshot's root, creation time, freshness and
/// coverage are facts of that reading.
fn snapshot_group() -> Vec<BatchGroup> {
    vec![BatchGroup {
        by: "/data/graph/id".into(),
        fields: [
            "/data/graph/root",
            "/data/graph/createdAt",
            "/data/freshness",
            "/data/coverage",
        ]
        .map(str::to_owned)
        .to_vec(),
    }]
}

/// A batch over two snapshots, interleaved: results of one snapshot share
/// its facts, and a result whose call coverage differs (another language)
/// states its own. A failed row reads nothing.
fn batch() -> Vec<Value> {
    let result = |snapshot: &str, language: &str, fresh: bool| {
        json!({"status":"success","data":{
            "graph":{"id":snapshot,"root":format!("{snapshot}/root"),"createdAt":format!("{snapshot}-time")},
            "rows":[{"id":format!("{snapshot}-{language}")}],
            "freshness":{"fresh":fresh,"analysisCurrent":true},
            "coverage":{"language":language,"linkage":{"ratio":0.5}},
        }})
    };
    vec![
        result("a", "typescript", true),
        result("a", "typescript", true),
        result("b", "typescript", true),
        result("a", "rust", true),
        json_fixture(
            r#"{
                "status": "error",
                "data": {"error": "gone", "errorCode": "notFound"}
            }"#,
        ),
        result("a", "rust", true),
        result("b", "typescript", false),
        result("a", "typescript", true),
    ]
}

/// The results as an agent rebuilds them in batch order: a fact a result
/// omits is the one last stated for the snapshot its `by` names.
fn rebuilt(results: &[Value], groups: &[BatchGroup]) -> Vec<Value> {
    let mut whole = results.to_vec();
    for group in groups {
        let mut stated: Vec<(Value, String, Value)> = Vec::new();
        for result in &mut whole {
            let Some(by) = result.pointer(&group.by).cloned() else {
                continue;
            };
            for field in &group.fields {
                match result.pointer(field).cloned() {
                    Some(value) => {
                        stated.retain(|(read, name, _)| !(*read == by && name == field));
                        stated.push((by.clone(), field.clone(), value));
                    }
                    None => {
                        let (parent, key) = field.rsplit_once('/').expect("pointer");
                        if let Some((_, _, value)) = stated
                            .iter()
                            .find(|(read, name, _)| *read == by && name == field)
                        {
                            result.pointer_mut(parent).expect("parent")[key] = value.clone();
                        }
                    }
                }
            }
        }
    }
    whole
}

#[test]
fn a_batch_states_each_snapshot_fact_once_and_rebuilds_whole() {
    let groups = snapshot_group();
    let original = batch();
    let mut results = original.clone();
    state_once(&mut results, &groups);
    assert_eq!(rebuilt(&results, &groups), original, "lossless");
    let group = &groups[0];
    let mut last: Vec<(Value, &str, Value)> = Vec::new();
    let mut omitted = 0;
    for (result, whole) in results.iter().zip(&original) {
        if whole["status"] == "error" {
            assert_eq!(result, whole, "a failed row is left whole");
            continue;
        }
        // Every result still names what it read.
        let by = result.pointer(&group.by).cloned().expect("by stays");
        for field in &group.fields {
            let earlier = last
                .iter()
                .position(|(read, name, _)| *read == by && *name == field.as_str());
            match (result.pointer(field), earlier) {
                (Some(value), Some(at)) => {
                    assert_ne!(*value, last[at].2, "{field} restated: {result}");
                    last[at].2 = value.clone();
                }
                (Some(value), None) => last.push((by.clone(), field, value.clone())),
                (None, Some(_)) => omitted += 1,
                (None, None) => panic!("the first result of a snapshot omits {field}: {result}"),
            }
        }
    }
    assert!(omitted > 0, "a later result omits a repeated fact");
}

/// A batch of one result, or of results that read different snapshots,
/// repeats nothing and is left as it is.
#[test]
fn results_that_share_no_snapshot_stay_whole() {
    let groups = snapshot_group();
    let original = batch();
    let distinct = [&original[0], &original[2]].map(Clone::clone).to_vec();
    let mut results = distinct.clone();
    state_once(&mut results, &groups);
    assert_eq!(results, distinct);
    let mut one = vec![original[0].clone()];
    state_once(&mut one, &groups);
    assert_eq!(one, vec![original[0].clone()]);
}

/// Every tool's declared groups read from the embedded contract, and a
/// contract without any leaves its batches whole.
#[test]
fn every_tool_reads_its_declared_groups() {
    for tool in ToolId::ALL {
        for group in batch_stated_once(tool) {
            assert!(group.by.starts_with("/data/"), "{tool:?}: {group:?}");
            assert!(
                group.fields.iter().all(|field| field.starts_with("/data/")),
                "{tool:?}: {group:?}"
            );
        }
    }
}
