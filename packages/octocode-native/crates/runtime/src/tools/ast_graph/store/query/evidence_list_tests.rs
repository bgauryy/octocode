use super::*;

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
    preview_extra_lists(&options, "snapshot-id", &mut extra);
    assert_eq!(
        extra["summary"]["testsToRun"].as_array().unwrap().len(),
        100
    );
    // The list resumes after the rows the preview showed.
    assert!(
        extra["nextLists"]["summary.testsToRun"]
            .as_str()
            .unwrap()
            .contains("--list summary.testsToRun --offset 100")
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
    let row = cycle_preview_row(
        &QueryOptions {
            op: "cycles".into(),
            ..Default::default()
        },
        "snapshot-id",
        &keys,
    );
    assert_eq!(row["nodes"].as_array().unwrap().len(), 50);
    assert!(
        row["nextNodes"]
            .as_str()
            .unwrap()
            .contains("--list cycleNodes")
    );
    assert!(row["nextNodes"].as_str().unwrap().contains("node-0"));
    assert!(
        row["nextNodes"].as_str().unwrap().ends_with("--offset 50"),
        "{row}"
    );
}

#[test]
fn continuation_keeps_the_explicit_workspace() {
    let options = QueryOptions {
        op: "impact".into(),
        workspace: Some(PathBuf::from("/tmp/graph-fixture")),
        list: Some("summary.testsToRun".into()),
        ..Default::default()
    };
    let next = next_command(&options, "snapshot-id", 50);
    assert!(next.contains("--workspace /tmp/graph-fixture"), "{next}");
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
    preview_extra_lists(&options, "snapshot-id", &mut extra);
    assert_eq!(
        extra["baseline"]["resolvedFindings"]
            .as_array()
            .unwrap()
            .len(),
        100
    );
    assert!(
        extra["nextLists"]["baseline.resolvedFindings"]
            .as_str()
            .unwrap()
            .contains("--list baseline.resolvedFindings")
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
