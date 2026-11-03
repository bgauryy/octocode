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
