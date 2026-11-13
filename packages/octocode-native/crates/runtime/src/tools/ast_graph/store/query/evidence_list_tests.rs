use super::*;

/// The row offset, list and cycle where `channel` resumes.
fn resumes(
    next: &[(Channel, Resume)],
    channel: &Channel,
) -> (usize, Option<String>, Option<String>) {
    match next.iter().find(|(name, _)| name == channel) {
        Some((
            _,
            Resume::Row {
                offset,
                list,
                cycle,
            },
        )) => (*offset, list.clone(), cycle.clone()),
        other => panic!("no row continuation {channel:?}: {other:?}"),
    }
}

#[test]
fn large_cycle_preview_links_to_all_members() {
    let keys = (0..51).map(|i| format!("node-{i}")).collect::<Vec<_>>();
    let row = cycle_preview_row(&keys);
    assert_eq!(row["nodes"].as_array().unwrap().len(), 50);
    let mut output = GraphOutput::ok(json!({"rows": [row]}));
    let options = QueryOptions {
        op: "cycles".into(),
        ..Default::default()
    };
    settle(&mut output, &options, 1);
    assert_eq!(
        resumes(&output.next, &Channel::Cycle(0)),
        (50, Some("cycleNodes".into()), Some("node-0".into()))
    );
    assert_eq!(output.value["pagination"]["hasMore"], false);
}

#[test]
fn continuation_keeps_the_scope_path_and_snapshot() {
    let options = QueryOptions {
        op: "cycles".into(),
        path: Some("packages/app".into()),
        workspace: Some(PathBuf::from("/tmp/graph-fixture")),
        list: Some("cycleNodes".into()),
        ..Default::default()
    };
    let resume = Resume::Row {
        offset: 50,
        list: None,
        cycle: None,
    };
    let next = Value::Object(resume_row(&options, "snapshot-id", &resume));
    assert_eq!(
        next,
        json!({"operation":"cycles","path":"packages/app","graph":"snapshot-id","list":"cycleNodes","offset":50})
    );
}

#[test]
fn evidence_continuation_reserves_the_named_target_filter() {
    let symbol_name = "target_".repeat(128);
    for operation in ["witnesses", "unresolved"] {
        let options = QueryOptions {
            op: operation.into(),
            path: Some("packages/app".into()),
            symbol_name: Some(symbol_name.clone()),
            ..Default::default()
        };
        let resume = Resume::Row {
            offset: 1,
            list: None,
            cycle: None,
        };
        let next = Value::Object(resume_row(&options, "snapshot-id", &resume));
        assert_eq!(next["symbolName"], symbol_name);
        assert_eq!(next["operation"], operation);
        assert_eq!(next["offset"], 1);
    }
}

/// A default witness or unresolved row states what differs between rows: its
/// digest `id` is provenance (`includeProvenance:true` returns it), `test`
/// appears only when true, and `evidence` is the page's.
#[test]
fn default_evidence_rows_leave_out_id_false_test_and_the_pages_evidence() {
    use crate::tools::ast_graph::test_support::{ask_with, fixture, ingest};
    let dir = fixture(&[(
        "app/linked.ts",
        "function target() {} export function run() { target(); missing(); }\n",
    )]);
    let built = ingest(&dir.0, &dir.0.join("app"));
    assert!(built.value.get("error").is_none(), "{}", built.value);
    for op in ["witnesses", "unresolved"] {
        let page = |provenance: bool| {
            ask_with(&dir.0, op, Some("linked.ts#run"), |options| {
                options.include_provenance = provenance;
            })
        };
        let lean = page(false);
        let rows = lean["rows"].as_array().expect("rows");
        assert!(!rows.is_empty(), "{op}: {lean}");
        assert_eq!(lean["evidence"], "syntax", "{op}: {lean}");
        for row in rows {
            for key in ["id", "evidence"] {
                assert!(row.get(key).is_none(), "{op} {key}: {row}");
            }
            assert_ne!(row.get("test"), Some(&json!(false)), "{op}: {row}");
        }
        crate::tools::test_support::assert_row_contract(crate::id::ToolId::GraphQuery, &lean);
        let full = page(true);
        for row in full["rows"].as_array().expect("rows") {
            assert_eq!(row["id"].as_str().map(str::len), Some(64), "{op}: {row}");
            assert!(row.get("evidence").is_none(), "{op}: {row}");
        }
        crate::tools::test_support::assert_row_contract(crate::id::ToolId::GraphQuery, &full);
    }
}
