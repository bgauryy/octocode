use super::*;

/// The row offset, list and cycle where `channel` resumes.
fn resumes(next: &[(Channel, Resume)], channel: &Channel) -> (usize, Option<String>, Option<String>) {
    match next.iter().find(|(name, _)| name == channel) {
        Some((_, Resume::Row { offset, list, cycle })) => (*offset, list.clone(), cycle.clone()),
        other => panic!("no row continuation {channel:?}: {other:?}"),
    }
}

#[test]
fn nested_summary_list_has_a_lossless_paged_route() {
    let mut extra = Map::new();
    let values = (0..125)
        .map(|i| json!(format!("test-{i}")))
        .collect::<Vec<_>>();
    extra.insert(
        "summary".into(),
        json!({"testsToRun":values,"testCount":125}),
    );
    let options = QueryOptions {
        op: "impact".into(),
        ..Default::default()
    };
    let next = preview_extra_lists(&mut extra);
    assert_eq!(
        extra["summary"]["testsToRun"].as_array().unwrap().len(),
        100
    );
    // The list resumes after the rows the preview showed.
    let list = "summary.testsToRun".to_owned();
    assert_eq!(
        resumes(&next, &Channel::List(list.clone())),
        (100, Some(list), None)
    );

    let mut complete = Map::new();
    complete.insert(
        "summary".into(),
        json!({"testsToRun":(0..125).map(|i| format!("test-{i}")).collect::<Vec<_>>() }),
    );
    let list_options = QueryOptions {
        list: Some("summary.testsToRun".into()),
        ..options
    };
    let page = select_list(&list_options, &mut complete)
        .expect("list")
        .expect("rows");
    assert_eq!(page.total, 125);
    assert_eq!(page.rows[124], "test-124");
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
        op: "impact".into(),
        path: Some("packages/app".into()),
        workspace: Some(PathBuf::from("/tmp/graph-fixture")),
        list: Some("summary.testsToRun".into()),
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
        json!({"operation":"impact","path":"packages/app","graph":"snapshot-id","list":"summary.testsToRun","offset":50})
    );
}

#[test]
fn resolved_findings_are_reachable_after_the_preview() {
    let mut extra = Map::new();
    extra.insert(
        "baseline".into(),
        json!({"resolvedFindings":(0..125).map(|i| json!({"id":format!("finding-{i}")})).collect::<Vec<_>>() }),
    );
    let options = QueryOptions {
        op: "issues".into(),
        ..Default::default()
    };
    let next = preview_extra_lists(&mut extra);
    assert_eq!(
        extra["baseline"]["resolvedFindings"]
            .as_array()
            .unwrap()
            .len(),
        100
    );
    let list = "baseline.resolvedFindings".to_owned();
    assert_eq!(
        resumes(&next, &Channel::List(list.clone())),
        (100, Some(list), None)
    );
    let list = QueryOptions {
        list: Some("baseline.resolvedFindings".into()),
        ..options
    };
    let mut complete = Map::new();
    complete.insert("baseline".into(), json!({"resolvedFindings":(0..125).map(|i| json!({"id":format!("finding-{i}")})).collect::<Vec<_>>() }));
    let page = select_list(&list, &mut complete)
        .expect("list")
        .expect("page");
    assert_eq!(page.total, 125);
    assert_eq!(page.rows[124]["id"], "finding-124");
}
