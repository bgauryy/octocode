//! Resume coverage and durable skipped-file outcomes.
use super::*;
use octocode_test_support::json_fixture;

/// Saved offsets cannot skip an unrecorded, deleted, or same-size edited
/// prefix, including a continuation opening a newly versioned snapshot.
#[tokio::test]
async fn materialize_resume_restarts_at_the_first_unverified_prefix_file() {
    let home = tempfile::tempdir().unwrap();
    let snapshot = snapshot(home.path());
    let context = context();
    let entries = [
        entry("a.rs", b"aa"),
        entry("b.rs", b"bb"),
        entry("c.rs", b"cc"),
    ];
    let mut store = Store::open(&snapshot, &context).await.unwrap();
    assert_eq!(
        store
            .resume_offset(home.path(), &entries, 2, &context)
            .await
            .unwrap(),
        0
    );
    for (path, bytes) in [("a.rs", b"aa"), ("b.rs", b"bb")] {
        write_file(home.path(), &store.root, path, bytes)
            .await
            .unwrap();
        store
            .record(
                vec![(path.into(), FileIdentity::of(bytes))],
                &MaterializeSkips::default(),
            )
            .await
            .unwrap();
    }
    assert_eq!(
        store
            .resume_offset(home.path(), &entries, 2, &context)
            .await
            .unwrap(),
        2
    );
    std::fs::write(store.root.join("a.rs"), b"xx").unwrap();
    assert_eq!(
        store
            .resume_offset(home.path(), &entries, 2, &context)
            .await
            .unwrap(),
        0
    );
    write_file(home.path(), &store.root, "a.rs", b"aa")
        .await
        .unwrap();
    std::fs::remove_file(store.root.join("b.rs")).unwrap();
    assert_eq!(
        store
            .resume_offset(home.path(), &entries, 2, &context)
            .await
            .unwrap(),
        1
    );
    let mut skipped = MaterializeSkips::default();
    skipped
        .unreadable
        .push(("b.rs".into(), "unavailable".into()));
    store.record(Vec::new(), &skipped).await.unwrap();
    assert_eq!(
        store
            .resume_offset(home.path(), &entries, 2, &context)
            .await
            .unwrap(),
        2
    );
}

/// Snapshot skips survive reopening, remain scoped to the resumed selection,
/// and disappear once the formerly skipped path is successfully rewritten.
#[tokio::test]
async fn recorded_materialize_skips_survive_resume_and_resolve_after_a_write() {
    let home = tempfile::tempdir().unwrap();
    let snapshot = snapshot(home.path());
    let mut skipped = MaterializeSkips::new(context().budget.max_body_bytes);
    skipped
        .oversized
        .push(("large.rs".into(), skipped.read_limit));
    skipped.binary.push("image.bin".into());
    skipped
        .unreadable
        .push(("other.rs".into(), "unavailable".into()));
    let mut store = Store::open(&snapshot, &context()).await.unwrap();
    store.record(Vec::new(), &skipped).await.unwrap();
    drop(store);

    let mut store = Store::open(&snapshot, &context()).await.unwrap();
    let selected = [entry("large.rs", b"large"), entry("image.bin", b"image")];
    let restored = store.prior_skips(&selected, context().budget.max_body_bytes);
    assert_eq!(restored.paths(), ["large.rs", "image.bin"]);
    assert!(restored.leaves_a_gap());
    assert!(
        restored.unreadable.is_empty(),
        "another selection's skip stays out"
    );
    write_file(home.path(), &store.root, "large.rs", b"large")
        .await
        .unwrap();
    store
        .record(
            vec![("large.rs".into(), FileIdentity::of(b"large"))],
            &MaterializeSkips::default(),
        )
        .await
        .unwrap();
    drop(store);
    let store = Store::open(&snapshot, &context()).await.unwrap();
    let restored = store.prior_skips(&selected, context().budget.max_body_bytes);
    assert_eq!(restored.paths(), ["image.bin"]);
    assert!(!restored.leaves_a_gap(), "only a binary remains skipped");
}

/// A later materialize page still reports a text file skipped before its
/// cursor, even after all remaining files are written successfully.
#[tokio::test]
async fn resumed_materialization_keeps_prior_skips_and_incomplete_coverage() {
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };
    let server = MockServer::start().await;
    let source = b"fn source() {}\n";
    for name in ["kept.rs", "last.rs"] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/octo/demo/contents/{name}")))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "application/vnd.github.raw")
                    .set_body_bytes(source),
            )
            .mount(&server)
            .await;
    }
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/octo/demo/contents/last.rs"))
        .respond_with(ResponseTemplate::new(429))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    let snapshot = Snapshot {
        commit: "1111111111111111111111111111111111111111",
        ..snapshot(home.path())
    };
    let entries = [
        TreeEntry {
            path: "oversized.rs".into(),
            kind: EntryKind::File,
            size: Some(context().budget.max_body_bytes as u64 + 1),
            oid: None,
        },
        entry("kept.rs", source),
        entry("last.rs", source),
    ];
    let provider = octocode_tools_shared::gh_shared::test_support::one_shot_provider(&server);
    let first = super::materialize_tree(&provider, &snapshot, &entries, 0, "", &context())
        .await
        .unwrap()
        .unwrap();
    assert!(first.gap, "the oversized text file is missing");
    let offset = first.next_offset.expect("the rate limit left one file");
    let provider = octocode_tools_shared::gh_shared::test_support::one_shot_provider(&server);
    let last = super::materialize_tree(&provider, &snapshot, &entries, offset, "", &context())
        .await
        .unwrap()
        .unwrap();
    assert!(last.next_offset.is_none(), "the remaining file was written");
    assert!(last.gap, "a resumed page must retain the previous text gap");
    assert_eq!(
        last.location["skipped"],
        json_fixture(r#"["oversized.rs"]"#)
    );
}
