//! F4 batch. K4: a batch of graph results on one snapshot states its facts
//! once (moved from `pages/batch` with the stage; unchanged). Shared
//! warnings, shared row fields, hints and merged related calls.
use super::super::batch::keyed;
use super::*;
use octocode_test_support::json_fixture;

/// The group a graph contract declares: a result reads the snapshot its
/// `graph.id` names, and the snapshot's root, creation time, freshness and
/// coverage are facts of that reading.
fn snapshot_group() -> Vec<Keyed> {
    vec![super::keyed_group(
        "/data/graph/id",
        &[
            "/data/graph/root",
            "/data/graph/createdAt",
            "/data/freshness",
            "/data/coverage",
        ],
    )]
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
fn rebuilt(results: &[Value], groups: &[Keyed]) -> Vec<Value> {
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
    keyed(&mut results, &groups);
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
    keyed(&mut results, &groups);
    assert_eq!(results, distinct);
    let mut one = vec![original[0].clone()];
    keyed(&mut one, &groups);
    assert_eq!(one, vec![original[0].clone()]);
}

/// Every tool's declared groups read from the embedded contract, and a
/// contract without any leaves its batches whole.
#[test]
fn every_tool_reads_its_declared_groups() {
    for tool in ToolId::ALL {
        for group in &plan::of(tool).batch.keyed {
            assert!(group.by.starts_with("/data/"), "{tool:?}: {group:?}");
            assert!(
                group.fields.iter().all(|field| field.starts_with("/data/")),
                "{tool:?}: {group:?}"
            );
        }
    }
}

fn row(index: usize, status: Option<&str>, data: Value) -> Value {
    let mut row = json!({"index": index, "data": data});
    if let Some(status) = status {
        row["status"] = json!(status);
    }
    row
}

fn batch_plan(shared: &[&str], hints: bool, related: bool) -> Plan {
    super::plan(
        Vec::new(),
        Vec::new(),
        Batch {
            keyed: Vec::new(),
            shared: shared
                .iter()
                .map(|pointer| Pattern::parse(pointer))
                .collect(),
            hints,
            related,
        },
    )
}

/// A warning every row states is said once for the call; a warning some
/// rows lack stays on the rows that state it; one row repeats nothing.
#[test]
fn a_warning_every_row_states_is_said_once_for_the_call() {
    let original = json!({"results": [
        row(0, None, json!({"warnings": ["rate limited until 12:00", "row 0 only"]})),
        row(1, None, json!({"warnings": ["rate limited until 12:00"]})),
    ]});
    let mut stated = original.clone();
    super::super::batch::warnings(&mut stated);
    assert_eq!(stated["warnings"], json!(["rate limited until 12:00"]));
    assert_eq!(
        stated["results"][0]["data"]["warnings"],
        json!(["row 0 only"])
    );
    assert!(stated["results"][1]["data"].get("warnings").is_none());
    let one = json!({"results": [row(0, None, json!({"warnings": ["w"]}))]});
    let mut alone = one.clone();
    super::super::batch::warnings(&mut alone);
    assert_eq!(alone, one);
}

/// Hoist G-H4 and C-5: the `commitSha` and `lsp` block every row states
/// alike are stated once in the envelope's `shared.data`, and each row reads
/// them back from there. A value one row lacks or states otherwise stays on
/// every row.
#[test]
fn identity_and_disclosure_every_row_states_alike_are_shared_once() {
    let plan = batch_plan(&["/commitSha", "/lsp"], false, false);
    let lsp = json!({"serverAvailable": true, "level": "semantic"});
    let original = json!({"results": [
        row(0, None, json!({"commitSha": "fdf7", "lsp": lsp, "files": [{"path": "a.rs"}]})),
        row(1, None, json!({"commitSha": "fdf7", "lsp": lsp, "files": [{"path": "b.rs"}]})),
        row(2, None, json!({"commitSha": "fdf7", "lsp": lsp, "files": [{"path": "c.rs"}]})),
    ]});
    let stated = super::hoisted(&plan, &original, &super::optional);
    assert_eq!(
        stated["shared"],
        json!({"data": {"commitSha": "fdf7", "lsp": lsp}})
    );
    for result in stated["results"].as_array().expect("rows") {
        assert!(result["data"].get("commitSha").is_none(), "{stated}");
        assert!(result["data"].get("lsp").is_none(), "{stated}");
    }
    super::assert_recovers(&plan, &original, &stated);
    let mut mixed = original.clone();
    mixed["results"][2]["data"]["commitSha"] = json!("0a1b");
    mixed["results"][1]["data"]
        .as_object_mut()
        .expect("data")
        .shift_remove("lsp");
    assert_eq!(super::hoisted(&plan, &mixed, &super::optional), mixed);
}

/// Hoist M03 and L-B: the hints every empty or failed row states alike go
/// to the envelope's `hints`; a row with evidence keeps none of them moved,
/// and a hint one empty row lacks stays where it is.
#[test]
fn hints_every_empty_or_failed_row_states_are_said_once() {
    let plan = batch_plan(&[], true, false);
    let original = json!({"results": [
        row(0, Some("empty"), json!({"files": [], "hints": ["Widen the scope.", "Try matchString as a regex."]})),
        row(1, None, json!({"files": [{"path": "a.rs"}]})),
        row(2, Some("empty"), json!({"files": [], "hints": ["Widen the scope.", "Try matchString as a regex."]})),
        row(3, Some("error"), json!({"error": "e", "errorCode": "pathNotFound", "hints": ["Widen the scope.", "Check the path."]})),
    ]});
    let stated = super::hoisted(&plan, &original, &super::optional);
    assert_eq!(stated["hints"], json!(["Widen the scope."]), "{stated}");
    assert_eq!(
        stated["results"][0]["data"]["hints"],
        json!(["Try matchString as a regex."])
    );
    assert_eq!(
        stated["results"][3]["data"]["hints"],
        json!(["Check the path."])
    );
    let one = json!({"results": [row(0, Some("empty"), json!({"hints": ["h"]}))]});
    assert_eq!(super::hoisted(&plan, &one, &super::optional), one);
}

/// Hoist K-3: single-query `related` calls of one tool from two or more
/// rows become one envelope call holding each row's query in row order, as
/// many per call as the tool takes; every query runs as its row's would.
#[test]
fn single_query_related_calls_of_one_tool_merge_into_one_envelope_call() {
    let plan = batch_plan(&[], false, true);
    let check = |name: &str| json!({"kind": "related", "tool": "securityCheck", "input": {"queries": [{"name": name, "ecosystem": "npm"}]}});
    let other = json!({"kind": "related", "tool": "ghSearchRepo", "input": {"queries": [{"keywords": ["x"]}]}});
    let original = json!({"results": [
        row(0, None, json!({"artifacts": [], "related": [check("lodash"), other.clone()]})),
        row(1, None, json!({"artifacts": [], "related": [check("axios")]})),
        row(2, None, json!({"artifacts": [], "related": [check("lodash")]})),
    ]});
    let stated = super::hoisted(&plan, &original, &super::optional);
    let merged = &stated["related"];
    let per_call = crate::response::result::rows_per_call(ToolId::SecurityCheck);
    let queries = merged
        .as_array()
        .expect("envelope related")
        .iter()
        .inspect(|call| {
            assert_eq!(call["tool"], "securityCheck");
            assert!(call["input"]["queries"].as_array().expect("queries").len() <= per_call);
        })
        .flat_map(|call| {
            call["input"]["queries"]
                .as_array()
                .expect("queries")
                .clone()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        queries,
        [
            json!({"name": "lodash", "ecosystem": "npm"}),
            json!({"name": "axios", "ecosystem": "npm"})
        ]
    );
    assert_eq!(
        stated["results"][0]["data"]["related"],
        json!([other]),
        "{stated}"
    );
    assert!(
        stated["results"][1]["data"].get("related").is_none(),
        "{stated}"
    );
    // One row's calls, or calls of different tools, stay on their rows.
    let lone = json!({"results": [
        row(0, None, json!({"artifacts": [], "related": [check("lodash")]})),
        row(1, None, json!({"artifacts": [], "related": [other]})),
    ]});
    assert_eq!(super::hoisted(&plan, &lone, &super::optional), lone);
}

/// VC2's merge case on the embedded contract (K-3): each artifactSearch row
/// offers one single-query securityCheck call for its resolved package; the
/// batch states one envelope call holding the three queries in row order,
/// and no row keeps its own.
#[test]
fn an_artifact_batch_offers_one_security_check_for_its_packages() {
    let check = |ecosystem: &str, name: &str, version: &str| {
        json!({"kind": "related", "tool": "securityCheck", "input": {"queries": [
            {"operation": "package", "ecosystem": ecosystem, "packageName": name, "version": version}]}})
    };
    let packages = [
        ("npm", "zod", "3.25.76"),
        ("npm", "express", "5.1.0"),
        ("npm", "chalk", "5.4.1"),
    ];
    let mut envelope = json!({"results": packages
        .iter()
        .enumerate()
        .map(|(index, (ecosystem, name, version))| row(index, None, json!({
            "artifacts": [{"name": name, "version": version}],
            "related": [check(ecosystem, name, version)],
        })))
        .collect::<Vec<_>>()});
    apply(ToolId::ArtifactSearch, &mut envelope);
    let merged = envelope["related"].as_array().expect("envelope related");
    assert_eq!(merged.len(), 1, "{envelope}");
    assert_eq!(merged[0]["tool"], "securityCheck");
    assert_eq!(
        merged[0]["input"]["queries"],
        json!(packages
            .iter()
            .map(|(ecosystem, name, version)| json!({
                "operation": "package", "ecosystem": ecosystem, "packageName": name, "version": version}))
            .collect::<Vec<_>>())
    );
    for result in envelope["results"].as_array().expect("rows") {
        assert!(result["data"].get("related").is_none(), "{envelope}");
    }
}
