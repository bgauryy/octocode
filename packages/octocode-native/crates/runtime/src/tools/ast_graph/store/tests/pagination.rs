use super::*;
use crate::tools::ast_graph::test_support::{next_offset, next_query};

#[test]
fn explicit_snapshot_paths_page_and_lead_by_their_snapshot_id() {
    let dir = fixture();
    let root = dir.0.as_path();
    let ingested = ingest_fixture(root, None);
    assert_eq!(ingested.class, ExitClass::Success);
    let id = ingested.value["id"].as_str().unwrap();
    let snapshot = root
        .join(".octocode/graph")
        .join(id)
        .canonicalize()
        .unwrap();
    let expected = ask(root, "symbols", Some("src/lib.ts"), |_| {});
    for path in [snapshot.clone(), snapshot.join(GRAPH_FILE)] {
        let mut options = QueryOptions {
            op: "symbols".into(),
            target: Some("src/lib.ts".into()),
            graph: Some(path.to_string_lossy().into_owned()),
            limit: Some(2),
            ..Default::default()
        };
        let mut union = Vec::new();
        for offset in [0, 2, 4] {
            options.offset = Some(offset);
            let page = query(&options, &workspace_policy(root));
            assert_eq!(page.class, ExitClass::Success, "{}", page.value);
            union.extend(ids(&page));
            if offset < 4 {
                assert_eq!(page.value["graph"]["id"], id, "{}", page.value);
                assert_eq!(next_offset(&page), Some(offset + 2), "{}", page.value);
                options.graph = Some(snapshot.to_string_lossy().into_owned());
            }
        }
        assert_eq!(union, ids(&expected));
    }
}

#[test]
fn explicit_snapshot_path_survives_ambiguous_candidate_continuation() {
    let dir = Fixture::new();
    let root = dir.0.as_path();
    for n in 0..25 {
        write_under(
            &dir,
            "app",
            &[(format!("file{n}.ts").as_str(), "export function run() {}")],
        );
    }
    let ingested = ingest_fixture(root, None);
    assert_eq!(ingested.class, ExitClass::Success);
    let snapshot = root
        .join(".octocode/graph")
        .join(ingested.value["id"].as_str().unwrap())
        .canonicalize()
        .unwrap();
    let options = QueryOptions {
        op: "callers".into(),
        target: Some("run".into()),
        graph: Some(snapshot.join(GRAPH_FILE).to_string_lossy().into_owned()),
        ..Default::default()
    };
    let first = query(&options, &workspace_policy(root));
    assert_eq!(first.class, ExitClass::InvalidInput);
    assert_eq!(first.value["pagination"]["totalItems"], 25);
    let next = next_query(&first).unwrap();
    assert_eq!(next.graph.as_deref(), Some(snapshot.to_str().unwrap()));
    assert!(next.workspace.is_none());
    assert_eq!(next.offset, Some(20));
    let rest = query(next, &workspace_policy(root));
    assert_eq!(rest.class, ExitClass::Success, "{}", rest.value);
    let union = first.value["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .chain(rest.value["rows"].as_array().unwrap())
        .map(|row| row["id"].as_str().unwrap())
        .collect::<BTreeSet<_>>();
    assert_eq!(union.len(), 25);
}
