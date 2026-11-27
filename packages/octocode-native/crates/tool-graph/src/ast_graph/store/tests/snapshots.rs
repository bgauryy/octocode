//! Snapshot format, fresh builds, selection and pruning.
use super::*;
use octocode_test_support::json_fixture;

mod builds;

/// Rewrites the manifest of the snapshot at `graph_dir` as the previous
/// analysis version wrote it.
fn as_previous_analysis(graph_dir: &Path) {
    let mut manifest = read_manifest(graph_dir).expect("manifest");
    manifest["analysisVersion"] = json!(crate::ast_graph::ANALYSIS_VERSION - 1);
    std::fs::write(
        graph_dir.join(MANIFEST_FILE),
        serde_json::to_vec(&manifest).expect("json"),
    )
    .expect("write");
}

/// A snapshot's facts were extracted and judged by the analysis version that
/// built it. One built by another version is never read for evidence: every
/// operation refuses it the same way and leads to its rebuild, whether it is
/// the queried snapshot or a baseline.
#[test]
fn another_analysis_version_is_refused_by_every_operation_and_rebuilt() {
    let dir = fixture();
    let root = dir.0.as_path();
    let first = ingest_fixture(root, None);
    let old_id = first.value["graph"].as_str().expect("id").to_owned();
    let graph_dir = PathBuf::from(first.value["dir"].as_str().expect("dir"));
    as_previous_analysis(&graph_dir);

    let refused = ask(root, "stats", None, |o| o.check_freshness = true);
    // It exists: the refusal says it must be rebuilt, never that it is missing.
    assert_eq!(
        refused.class,
        ExitClass::of(ErrorCode::SnapshotIncompatible),
        "{}",
        refused.value
    );
    assert_eq!(
        refused.value["errorCode"], "snapshotIncompatible",
        "{}",
        refused.value
    );
    let message = refused.value["error"].as_str().expect("error");
    for stated in [
        old_id.clone(),
        format!("version {}", crate::ast_graph::ANALYSIS_VERSION - 1),
        format!("version {}", crate::ast_graph::ANALYSIS_VERSION),
        "graphBuild".to_owned(),
    ] {
        assert!(message.contains(&stated), "{stated}: {message}");
    }
    let lead = &refused.value["related"][0];
    assert_eq!(lead["tool"], "graphBuild", "{}", refused.value);
    assert_eq!(lead["input"]["queries"][0]["path"], "app", "{lead}");
    // The refusal is the whole answer: no fact of the snapshot beside it.
    let fields = refused.value.as_object().expect("object");
    assert_eq!(
        fields.keys().collect::<Vec<_>>(),
        ["error", "errorCode", "related"],
        "{}",
        refused.value
    );
    for op in query::OPS {
        let out = ask(root, op, Some("src/lib.ts"), |o| {
            o.graph = Some(old_id.clone());
        });
        assert_eq!(out.value, refused.value, "{op}");
        assert_eq!(out.class, refused.class, "{op}");
    }

    // graphBuild does not reuse it: the same path builds a current snapshot.
    let rebuilt = ingest_fixture(root, None);
    assert_eq!(rebuilt.class, ExitClass::Success);
    assert_fresh_build(&rebuilt);
    assert_ne!(rebuilt.value["graph"], first.value["graph"]);
    assert_fresh_build(&ingest_fixture(root, None));
    let current = ask(root, "stats", None, |o| o.check_freshness = true);
    assert_eq!(current.class, ExitClass::Success, "{}", current.value);
    assert!(
        current.value.get("freshness").is_none(),
        "{}",
        current.value
    );

    // The old snapshot is kept, and as a baseline it is refused alike.
    assert!(graph_dir.is_dir(), "the older snapshot is retained");
    for op in ["drift", "issues"] {
        let out = ask(root, op, None, |o| o.baseline = Some(old_id.clone()));
        assert_eq!(out.value, refused.value, "{op}");
        assert_eq!(out.class, refused.class, "{op}");
    }
}

/// Facts extracted under an older schema build another graph: such a
/// snapshot of an unchanged tree is stale, and an ingest rebuilds it.
#[test]
fn older_fact_schema_is_stale_and_rebuilt() {
    let dir = fixture();
    let first = ingest_fixture(dir.0.as_path(), None);
    let graph_dir = PathBuf::from(first.value["dir"].as_str().expect("dir"));
    let mut manifest = read_manifest(&graph_dir).expect("manifest");
    manifest["factsSchemaVersion"] =
        json!(octocode_engine::signatures::GRAPH_FACTS_SCHEMA_VERSION - 1);
    std::fs::write(
        graph_dir.join(MANIFEST_FILE),
        serde_json::to_vec(&manifest).expect("json"),
    )
    .expect("write");
    let out = ask(dir.0.as_path(), "stats", None, |o| o.check_freshness = true);
    assert_eq!(out.value["freshness"]["fresh"], false, "{}", out.value);
    let rebuilt = ingest_fixture(dir.0.as_path(), None);
    assert_fresh_build(&rebuilt);
    assert_ne!(rebuilt.value["graph"], first.value["graph"]);
}

#[test]
fn oversized_snapshot_is_rejected_before_allocation() {
    let dir = Fixture::new();
    let file = dir.0.as_path().join(GRAPH_FILE);
    std::fs::File::create(&file)
        .expect("file")
        .set_len(crate::ast_graph::limits::LIMITS.snapshot_bytes as u64 + 1)
        .expect("sparse fixture");
    assert!(read_graph_file(&file).is_err());
}

#[test]
fn format_round_trips_deterministically_and_rejects_corruption() {
    let dir = fixture();
    let out = ingest_fixture(dir.0.as_path(), None);
    assert_eq!(out.class, ExitClass::Success, "{}", out.value);
    let graph_dir = PathBuf::from(out.value["dir"].as_str().expect("dir"));
    let bytes = std::fs::read(graph_dir.join(GRAPH_FILE)).expect("graph.bin");
    let (tables, digest) = format::decode(&bytes).expect("decode");
    assert_eq!(digest, out.value["sha256"].as_str().expect("sha"));
    let (again, _) = format::encode(&tables);
    assert_eq!(
        again, bytes,
        "re-encoding a decoded snapshot is byte-identical"
    );

    let mut corrupt = bytes.clone();
    let last = corrupt.len() - 1;
    corrupt[last] ^= 0xff;
    assert!(format::decode(&corrupt).unwrap_err().contains("checksum"));
    assert!(
        format::decode(b"not a graph")
            .unwrap_err()
            .contains("magic")
    );
    let mut previous = bytes.clone();
    previous[8..12].copy_from_slice(&(format::FORMAT_VERSION - 1).to_le_bytes());
    assert!(
        format::decode(&previous)
            .unwrap_err()
            .contains(&format!("v{} is not supported", format::FORMAT_VERSION - 1))
    );
    let mut future = bytes;
    future[8] = 99;
    assert!(
        format::decode(&future)
            .unwrap_err()
            .contains("not supported")
    );
}

#[test]
fn stale_detects_edits_and_prune_keeps_the_newest_snapshots() {
    let dir = fixture();
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let fresh = ask(dir.0.as_path(), "stale", None, |_| {});
    assert_eq!(
        (fresh.class, &fresh.value["fresh"]),
        (ExitClass::Success, &json!(true)),
        "{}",
        fresh.value
    );

    dir.write("app/src/a.ts", "export const a = 2;\n");
    let stale = ask(dir.0.as_path(), "stale", None, |_| {});
    assert_eq!(
        stale.value["rows"],
        json_fixture(r#"[{"path": "app/src/a.ts", "status": "changed"}]"#)
    );

    for _ in 0..2 {
        assert_eq!(
            ingest_fixture(dir.0.as_path(), Some(1)).class,
            ExitClass::Success
        );
    }
    let home = dir.0.as_path().join(".octocode").join("graph");
    assert_eq!(list_snapshots(&home).len(), 1);
}

#[test]
fn same_second_retention_never_reuses_a_pruned_id_or_prunes_the_newest() {
    let dir = Fixture::new();
    let home = dir.0.join("graphs");
    std::fs::create_dir_all(&home).expect("graph home");
    let base = "20261026T070632Z-scope-123456";
    let mut published = Vec::new();
    for _ in 0..14 {
        let id = next_snapshot_id(&home, base).expect("next id");
        assert!(!published.contains(&id), "snapshot id was reused: {id}");
        let manifest = json!({"kind": MANIFEST_KIND, "root": dir.0, "id": id});
        super::super::ingest::publish(&home, &id, "20261026T070632Z", b"snapshot", &manifest)
            .expect("publish");
        published.push(id.clone());
        prune(&home, &dir.0, 3);
        assert!(
            home.join(&id).join(GRAPH_FILE).is_file(),
            "new snapshot pruned"
        );
        let retained = list_snapshots(&home);
        assert_eq!(retained, published[published.len().saturating_sub(3)..]);
    }
}

#[test]
fn a_pruned_snapshot_finds_a_successor_across_sequence_digit_boundaries() {
    let dir = Fixture::new();
    let home = dir.0.join("graphs");
    std::fs::create_dir_all(&home).expect("graph home");
    let root = dir.0.join("app");
    let base = format!(
        "20261026T070632Z-{}-{}",
        scope_slug(&root, &dir.0),
        root_tag(&root)
    );
    for (before, after) in [(9, 10), (99, 100), (999, 1000)] {
        let previous = format!("{base}-{before}");
        let next = format!("{base}-{after}");
        let manifest = json!({"kind": MANIFEST_KIND, "root": root, "id": next});
        super::super::ingest::publish(&home, &next, "20261026T070632Z", b"snapshot", &manifest)
            .expect("publish");
        assert_eq!(
            successor(&home, &dir.0, &root, &previous),
            Some(next.clone())
        );
        assert_eq!(successor(&home, &dir.0, &root, &next), None);
        std::fs::remove_dir_all(home.join(next)).expect("remove successor");
    }
}

#[test]
fn published_manifests_remain_discoverable_with_large_scan_evidence() {
    let dir = Fixture::new();
    let home = dir.0.join("graphs");
    let id = "20261026T070632Z-scope-123456";
    let skipped = (0..16_384)
        .map(|index| {
            format!(
                "nested/directory/{index}/a_file_name_with_scan_evidence_{}.txt",
                "x".repeat(32)
            )
        })
        .collect::<Vec<_>>();
    let manifest = json!({
        "kind": MANIFEST_KIND,
        "root": dir.0,
        "id": id,
        "scope": "scope",
        "formatVersion": format::FORMAT_VERSION,
        "scan": {"filesSkipped": skipped},
    });
    super::super::ingest::publish(&home, id, "20261026T070632Z", b"snapshot", &manifest)
        .expect("publish");
    assert_eq!(read_manifest(&home.join(id)), Some(manifest));
    let metadata = read_snapshot_metadata(&home.join(id)).expect("large manifest metadata");
    assert_eq!(metadata.root.as_deref(), dir.0.to_str());
    assert_eq!(metadata.scope.as_deref(), Some("scope"));
    assert_eq!(
        metadata.format_version,
        Some(u64::from(format::FORMAT_VERSION))
    );
    assert_eq!(list_snapshots(&home), [id]);
}

/// Job retention asks when each root was last published, once per job. One
/// pass over the home answers every root with its newest snapshot.
#[test]
fn published_roots_maps_each_root_to_its_newest_snapshot() {
    let dir = Fixture::new();
    let home = dir.0.join("graphs");
    let snapshot = |id: &str, root: &str| {
        let manifest = json!({
            "kind": MANIFEST_KIND,
            "root": root,
            "id": id,
            "scope": "scope",
            "formatVersion": format::FORMAT_VERSION,
        });
        super::super::ingest::publish(&home, id, "20261026T070632Z", b"snapshot", &manifest)
            .expect("publish");
        std::fs::metadata(home.join(id).join(MANIFEST_FILE))
            .and_then(|meta| meta.modified())
            .expect("mtime")
    };
    let a1 = snapshot("20261026T070632Z-a-111111", "/repo/a");
    let a2 = snapshot("20261026T070633Z-a-222222", "/repo/a");
    let b = snapshot("20261026T070634Z-b-333333", "/repo/b");
    std::fs::create_dir_all(home.join(".facts").join("jobs")).expect("facts");
    let published = published_roots(&home);
    assert_eq!(published.len(), 2, "{published:?}");
    assert_eq!(published.get("/repo/a"), Some(&a1.max(a2)));
    assert_eq!(published.get("/repo/b"), Some(&b));
}

/// A snapshot of an older format (here v4, before diagnostics named their
/// target file) is never reused: the next ingest rebuilds it, and queries
/// answer from the rebuilt snapshot.
#[test]
fn an_older_format_snapshot_is_rebuilt_not_reused() {
    let dir = fixture();
    let first = ingest_fixture(dir.0.as_path(), None);
    assert_eq!(first.class, ExitClass::Success, "{}", first.value);
    let snapshot = PathBuf::from(first.value["dir"].as_str().expect("dir"));
    let older = format::FORMAT_VERSION - 1;
    let manifest_path = snapshot.join(MANIFEST_FILE);
    let mut manifest: Value =
        serde_json::from_slice(&std::fs::read(&manifest_path).expect("manifest")).expect("json");
    manifest["formatVersion"] = json!(older);
    std::fs::write(&manifest_path, manifest.to_string()).expect("older manifest");
    let mut bytes = std::fs::read(snapshot.join(GRAPH_FILE)).expect("graph.bin");
    bytes[8..12].copy_from_slice(&older.to_le_bytes());
    std::fs::write(snapshot.join(GRAPH_FILE), bytes).expect("older graph");
    let stale = ask(dir.0.as_path(), "stats", None, |_| {});
    assert!(
        stale.value["error"]
            .as_str()
            .is_some_and(|e| e.contains("rebuild it with graphBuild")),
        "{}",
        stale.value
    );
    let second = ingest_fixture(dir.0.as_path(), None);
    assert_eq!(second.class, ExitClass::Success, "{}", second.value);
    assert_fresh_build(&second);
    assert_ne!(second.value["graph"], first.value["graph"]);
    let stats = ask(dir.0.as_path(), "stats", None, |_| {});
    assert_eq!(stats.class, ExitClass::Success, "{}", stats.value);
}

/// A settled file records its stamp, an equal stamp proves the bytes
/// without a read, and a same-size rewrite (new times) still rebuilds.
#[test]
fn settled_sources_prove_freshness_and_a_same_size_edit_is_detected() {
    let dir = fixture();
    octocode_engine::stamp::settle_written_so_far();
    let first = ingest_fixture(dir.0.as_path(), None);
    assert_eq!(first.class, ExitClass::Success, "{}", first.value);
    let graph_dir = PathBuf::from(first.value["dir"].as_str().expect("dir"));
    let bytes = std::fs::read(graph_dir.join(GRAPH_FILE)).expect("graph.bin");
    let (tables, _) = format::decode(&bytes).expect("decode");
    assert_eq!(tables.digests.len(), 4);
    assert_eq!(tables.stamps.len(), 4, "every settled source is stamped");
    // The files-only oracle decodes the same persisted file index.
    let (files, files_digest) = format::decode_files(&bytes).expect("decode files");
    assert_eq!(files_digest, first.value["sha256"].as_str().expect("sha"));
    // Record strings only: their ids prefix the full dictionary, which adds
    // the strings only occurrence evidence references.
    assert!(files.strings.len() < tables.strings.len());
    assert_eq!(
        files.strings.iter().collect::<Vec<_>>(),
        tables
            .strings
            .iter()
            .take(files.strings.len())
            .collect::<Vec<_>>()
    );
    assert_eq!(files.digests, tables.digests);
    assert_eq!(files.stamps, tables.stamps);
    assert_eq!(files.nodes.len(), tables.nodes.len());
    assert!(files.edges.is_empty() && !tables.edges.is_empty());
    let (node, stamp) = tables
        .stamps
        .iter()
        .find(|(node, _)| tables.str(tables.nodes[*node as usize].key) == "src/a.ts")
        .expect("a.ts stamp");
    let a = dir.0.as_path().join("app/src/a.ts");
    // An equal stamp is trusted without reading: even a wrong digest passes.
    assert_eq!(
        source_matches(&a, "not-the-digest", Some(stamp)),
        Some(true)
    );
    let digest = tables
        .digests
        .iter()
        .find(|(id, _)| id == node)
        .map(|(_, digest)| tables.str(*digest))
        .expect("a.ts digest");
    assert_eq!(source_matches(&a, digest, None), Some(true));
    assert_eq!(
        source_matches(&dir.0.as_path().join("gone.ts"), digest, Some(stamp)),
        None
    );

    let second = ingest_fixture(dir.0.as_path(), None);
    assert_fresh_build(&second);
    // Same length, new content: the stamp moves, so the hash decides.
    let before = std::fs::read_to_string(&a).expect("a.ts");
    let after = before.replace("b()", "c()");
    assert_eq!(before.len(), after.len());
    std::fs::write(&a, after).expect("edit");
    assert_eq!(source_matches(&a, digest, Some(stamp)), Some(false));
    let third = ingest_fixture(dir.0.as_path(), None);
    assert_eq!(third.class, ExitClass::Success, "{}", third.value);
    assert_fresh_build(&third);
    let stale = ask(dir.0.as_path(), "stale", None, |_| {});
    assert_eq!(stale.value["fresh"], true, "{}", stale.value);
}

/// `--graph` takes an exact id, else the one root whose scope it names,
/// else the one root whose ids contain it; anything that names several
/// roots is ambiguous (exit 2, candidates listed), never a silent pick.
#[test]
fn graph_selection_is_exact_or_ambiguous() {
    let dir = fixture();
    let app = ingest_fixture(dir.0.as_path(), None);
    let src = ingest_root(
        dir.0.as_path(),
        &dir.0.as_path().join("app/src"),
        None,
        false,
    );
    let (app_id, src_id) = (
        app.value["graph"].as_str().expect("id").to_owned(),
        src.value["graph"].as_str().expect("id").to_owned(),
    );
    let stats = |spec: &str| {
        ask(dir.0.as_path(), "stats", None, |o| {
            o.graph = Some(spec.into());
        })
    };
    assert_eq!(
        stats("app").value["graph"]["id"],
        app_id.as_str(),
        "scope app is exact"
    );
    assert_eq!(stats("app-src").value["graph"]["id"], src_id.as_str());
    assert_eq!(stats(&app_id).value["graph"]["id"], app_id.as_str());
    let ambiguous = stats("ap");
    assert_eq!(
        ambiguous.class,
        ExitClass::Failed(octocode_runtime_base::id::error_codes::ErrorClass::CorrectInput),
        "{}",
        ambiguous.value
    );
    assert_eq!(
        ambiguous.value["candidates"],
        json!([app_id.clone(), src_id.clone()])
    );
    assert_eq!(
        stats(&src_id["20".len()..]).value["graph"]["id"],
        src_id.as_str(),
        "a substring of exactly one id"
    );
}

/// Snapshots are pruned per root: two roots whose names share one slug
/// (`a-b` and `a/b`) never delete each other's snapshots, and their ids
/// differ.
#[test]
fn prune_groups_snapshots_by_root_not_by_slug() {
    let dir = fixture();
    dir.write("a-b/x.ts", "export const x = 1;\n");
    dir.write("a/b/y.ts", "export const y = 1;\n");
    let dash = ingest_root(
        dir.0.as_path(),
        &dir.0.as_path().join("a-b"),
        Some(1),
        false,
    );
    let mut nested = Vec::new();
    for _ in 0..3 {
        let out = ingest_root(dir.0.as_path(), &dir.0.as_path().join("a/b"), Some(1), true);
        assert_eq!(out.class, ExitClass::Success, "{}", out.value);
        nested.push(out.value["graph"].as_str().expect("id").to_owned());
    }
    let dash_id = dash.value["graph"].as_str().expect("id");
    assert!(!nested.contains(&dash_id.to_owned()), "{nested:?}");
    let home = dir.0.as_path().join(".octocode").join("graph");
    let left = list_snapshots(&home);
    assert!(left.contains(&dash_id.to_owned()), "{left:?}");
    assert_eq!(left.len(), 2, "one per root: {left:?}");
}

/// Rewrites `snapshot` as one an older build wrote: its manifest and its
/// graph header name the previous format version.
fn age_snapshot(snapshot: &Path) {
    let older = format::FORMAT_VERSION - 1;
    let manifest_path = snapshot.join(MANIFEST_FILE);
    let mut manifest: Value =
        serde_json::from_slice(&std::fs::read(&manifest_path).expect("manifest")).expect("json");
    manifest["formatVersion"] = json!(older);
    std::fs::write(&manifest_path, manifest.to_string()).expect("older manifest");
    let mut bytes = std::fs::read(snapshot.join(GRAPH_FILE)).expect("graph.bin");
    bytes[8..12].copy_from_slice(&older.to_le_bytes());
    std::fs::write(snapshot.join(GRAPH_FILE), bytes).expect("older graph");
}

/// An ingest prunes snapshots of an older format, whatever root they
/// scanned (this build cannot read them), and keeps readable snapshots of
/// other roots.
#[test]
fn ingest_prunes_older_format_snapshots_of_every_root() {
    let dir = fixture();
    dir.write("other/x.ts", "export const x = 1;\n");
    let home = dir.0.as_path().join(".octocode").join("graph");
    let old = ingest_root(dir.0.as_path(), &dir.0.as_path().join("other"), None, false);
    let old_id = old.value["graph"].as_str().expect("id").to_owned();
    let src = ingest_root(
        dir.0.as_path(),
        &dir.0.as_path().join("app/src"),
        None,
        false,
    );
    let src_id = src.value["graph"].as_str().expect("id").to_owned();
    age_snapshot(&home.join(&old_id));
    let app = ingest_fixture(dir.0.as_path(), None);
    assert_eq!(app.class, ExitClass::Success, "{}", app.value);
    assert_eq!(
        app.value["pruned"],
        json!([old_id.clone()]),
        "{}",
        app.value
    );
    let left = list_snapshots(&home);
    assert!(!left.contains(&old_id), "{left:?}");
    assert!(left.contains(&src_id), "{left:?}");
}

/// An older-format snapshot that an older build's reader used within the
/// lease survives the prune, as a same-format one does: a rebuild by this
/// build must not end that reader's walk. Once idle, the next ingest
/// prunes it.
#[test]
fn a_leased_older_format_snapshot_survives_prune() {
    let dir = fixture();
    dir.write("other/x.ts", "export const x = 1;\n");
    let home = dir.0.as_path().join(".octocode").join("graph");
    let old = ingest_root(dir.0.as_path(), &dir.0.as_path().join("other"), None, false);
    let old_id = old.value["graph"].as_str().expect("id").to_owned();
    age_snapshot(&home.join(&old_id));
    renew_lease(&home.join(&old_id)).expect("lease");
    let app = ingest_fixture(dir.0.as_path(), None);
    assert_eq!(app.class, ExitClass::Success, "{}", app.value);
    assert!(app.value.get("pruned").is_none(), "{}", app.value);
    assert!(list_snapshots(&home).contains(&old_id));

    let expired = std::time::SystemTime::now()
        - octocode_runtime_base::cache::CONTINUATION_IDLE
        - std::time::Duration::from_secs(1);
    std::fs::OpenOptions::new()
        .write(true)
        .open(home.join(&old_id).join(READ_LEASE_FILE))
        .expect("lease marker")
        .set_modified(expired)
        .expect("backdate lease");
    dir.write("app/src/late.ts", "export const late = 1;\n");
    let again = ingest_fixture(dir.0.as_path(), None);
    assert_eq!(
        again.value["pruned"],
        json!([old_id.clone()]),
        "{}",
        again.value
    );
}

/// A substring that names only snapshots of one root is no ambiguity: it
/// picks that root's newest snapshot, as a scope does.
#[test]
fn a_substring_naming_one_roots_snapshots_picks_its_newest() {
    let dir = fixture();
    let src = dir.0.as_path().join("app/src");
    let first = ingest_root(dir.0.as_path(), &src, None, true);
    let second = ingest_root(dir.0.as_path(), &src, None, true);
    assert_eq!(second.class, ExitClass::Success, "{}", second.value);
    assert_ne!(first.value["graph"], second.value["graph"]);
    let stats = ask(dir.0.as_path(), "stats", None, |o| {
        o.graph = Some("pp-src".into());
    });
    assert_eq!(
        stats.value["graph"]["id"], second.value["graph"],
        "{}",
        stats.value
    );
}

/// The format that wrote the manifest's excludes as `scan.excludeDir` is no
/// longer read: its excludes would be lost, and report excluded files as added.
#[test]
fn the_format_that_wrote_exclude_dir_is_no_longer_current() {
    assert_ne!(format::FORMAT_VERSION, 16);
}

/// A reader that renews a snapshot's lease after prune judged it idle and
/// before prune acts must keep it: the removal itself decides, so a renewal
/// that landed first is never lost to a stale idle verdict.
#[test]
fn a_lease_renewed_while_prune_acts_keeps_the_snapshot() {
    let dir = fixture();
    let src = dir.0.as_path().join("app/src");
    let home = dir.0.as_path().join(".octocode").join("graph");
    let first = ingest_root(dir.0.as_path(), &src, None, true);
    let second = ingest_root(dir.0.as_path(), &src, None, true);
    let (old_id, new_id) = (
        first.value["graph"].as_str().expect("id").to_owned(),
        second.value["graph"].as_str().expect("id").to_owned(),
    );
    // The older snapshot's lease has run out: prune judges it idle.
    let expired = std::time::SystemTime::now()
        - octocode_runtime_base::cache::CONTINUATION_IDLE
        - std::time::Duration::from_secs(1);
    renew_lease(&home.join(&old_id)).expect("lease marker");
    std::fs::OpenOptions::new()
        .write(true)
        .open(home.join(&old_id).join(READ_LEASE_FILE))
        .expect("lease marker")
        .set_modified(expired)
        .expect("backdate lease");
    let scanned = read_manifest(&home.join(&old_id)).expect("manifest")["root"]
        .as_str()
        .map(PathBuf::from)
        .expect("root");
    let removed = prune_with(&home, &scanned, 1, &|snapshot| {
        renew_lease(snapshot).expect("a reader renews the lease");
    });
    let left = list_snapshots(&home);
    assert!(removed.is_empty(), "{removed:?}");
    assert!(left.contains(&old_id) && left.contains(&new_id), "{left:?}");
}

/// The temp sweep removes only the claims this code makes
/// (`.tmp-removing-<id>`), however old anything else named `.tmp-*` is:
/// an entry Octocode did not create is never its to delete.
#[test]
fn the_temp_sweep_removes_only_its_own_abandoned_claims() {
    let home = tempfile::tempdir().expect("home");
    let aged = std::time::SystemTime::now() - std::time::Duration::from_secs(2 * 3600);
    let age = |path: &Path| {
        std::fs::File::open(path)
            .expect("open")
            .set_times(std::fs::FileTimes::new().set_modified(aged))
            .expect("age");
    };
    let claim = home
        .path()
        .join(".tmp-removing-20260101T000000Z-app-abcdef");
    let user_dir = home.path().join(".tmp-mywork");
    let user_file = home.path().join(".tmp-notes");
    std::fs::create_dir_all(&claim).expect("claim");
    std::fs::create_dir_all(&user_dir).expect("user dir");
    std::fs::write(user_dir.join("file"), "kept").expect("user data");
    std::fs::write(&user_file, "kept").expect("user file");
    for path in [&claim, &user_dir, &user_file] {
        age(path);
    }
    prune(home.path(), Path::new("/no/such/root"), 1);
    assert!(!claim.exists(), "an abandoned claim is swept");
    assert!(user_dir.join("file").exists(), "a user's .tmp-* dir stays");
    assert!(user_file.exists(), "a user's .tmp-* file stays");
}

/// A snapshot of a root that no longer exists answers no query of its
/// scope: the next build of any root reaps it, unless a reader used it
/// within the lease. Snapshots of roots that still exist stay.
#[test]
fn a_build_reaps_snapshots_of_roots_that_no_longer_exist() {
    let dir = fixture();
    let home = dir.0.as_path().join(".octocode").join("graph");
    dir.write("gone/x.ts", "export const x = 1;\n");
    dir.write("leased/y.ts", "export const y = 1;\n");
    let id_of = |root: &str| {
        let built = ingest_root(dir.0.as_path(), &dir.0.as_path().join(root), None, false);
        assert_eq!(built.class, ExitClass::Success, "{}", built.value);
        built.value["graph"].as_str().expect("id").to_owned()
    };
    let (gone, leased) = (id_of("gone"), id_of("leased"));
    renew_lease(&home.join(&leased)).expect("lease");
    std::fs::remove_dir_all(dir.0.as_path().join("gone")).expect("root removed");
    std::fs::remove_dir_all(dir.0.as_path().join("leased")).expect("root removed");

    let app = ingest_fixture(dir.0.as_path(), None);
    assert_eq!(app.class, ExitClass::Success, "{}", app.value);
    let left = list_snapshots(&home);
    assert!(!left.contains(&gone), "reaped with its root: {left:?}");
    assert!(
        left.contains(&leased),
        "a reader's lease holds it: {left:?}"
    );
    assert!(
        left.contains(&app.value["graph"].as_str().expect("id").to_owned()),
        "{left:?}"
    );
    // The lease runs out; the next build reaps it too.
    let expired = std::time::SystemTime::now()
        - octocode_runtime_base::cache::CONTINUATION_IDLE
        - std::time::Duration::from_secs(1);
    std::fs::OpenOptions::new()
        .write(true)
        .open(home.join(&leased).join(READ_LEASE_FILE))
        .expect("lease marker")
        .set_modified(expired)
        .expect("backdate lease");
    dir.write("app/src/late.ts", "export const late = 1;\n");
    ingest_fixture(dir.0.as_path(), None);
    assert!(!list_snapshots(&home).contains(&leased));
}

/// A directory the walk cannot read leaves a trace: the scan counts it as
/// skipped, so the snapshot's answers are marked partial instead of looking
/// complete over a graph that lacks everything below it.
#[cfg(unix)]
#[test]
fn an_unreadable_directory_leaves_the_snapshot_partial() {
    use std::os::unix::fs::PermissionsExt;
    let dir = fixture();
    let locked = dir.0.as_path().join("app/src/locked");
    dir.write("app/src/locked/hidden.ts", "export const hidden = 1;\n");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).expect("lock");
    let unreadable = std::fs::read_dir(&locked).is_err();
    let built = ingest_fixture(dir.0.as_path(), None);
    let stats = ask(dir.0.as_path(), "stats", None, |_| {});
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).expect("unlock");
    if !unreadable {
        return; // a superuser reads any directory
    }
    assert_eq!(built.class, ExitClass::Success, "{}", built.value);
    assert!(
        built.value["scan"]["filesSkipped"].as_u64().unwrap_or(0) >= 1,
        "{}",
        built.value
    );
    let warnings = built.value["warnings"].to_string();
    assert!(
        warnings.contains("src/locked"),
        "the receipt names the directory it could not read: {}",
        built.value
    );
    assert_eq!(stats.value["graph"]["isPartial"], true, "{}", stats.value);
    assert!(
        stats.value["graph"]["partialReasons"]
            .as_array()
            .is_some_and(|reasons| reasons.iter().any(|reason| reason == "filesSkipped")),
        "{}",
        stats.value
    );
}

/// A snapshot whose graph.bin fails its integrity check (a flipped byte, a
/// truncation, an empty file) is refused as one to rebuild: not a retryable
/// decode failure, and with the graphBuild that replaces it as the lead.
#[test]
fn a_corrupt_graph_file_leads_to_a_refreshing_rebuild() {
    for damage in ["flip", "truncate", "empty"] {
        let dir = fixture();
        let built = ingest_fixture(dir.0.as_path(), None);
        let graph = PathBuf::from(built.value["dir"].as_str().expect("dir")).join(GRAPH_FILE);
        let mut bytes = std::fs::read(&graph).expect("graph.bin");
        match damage {
            "flip" => {
                // The last byte of the NODE section, which every query reads.
                let count = u32::from_le_bytes(bytes[12..16].try_into().expect("count")) as usize;
                let entry = (0..count)
                    .map(|index| 48 + index * 52)
                    .find(|start| &bytes[*start..*start + 4] == b"NODE")
                    .expect("NODE section");
                let offset =
                    u64::from_le_bytes(bytes[entry + 4..entry + 12].try_into().expect("offset"));
                let len =
                    u64::from_le_bytes(bytes[entry + 12..entry + 20].try_into().expect("len"));
                bytes[(offset + len - 1) as usize] ^= 0xff;
            }
            "truncate" => bytes.truncate(bytes.len() / 2),
            _ => bytes.clear(),
        }
        std::fs::write(&graph, bytes).expect("damage");
        let refused = ask(dir.0.as_path(), "stats", None, |o| {
            o.check_freshness = false;
        });
        assert_ne!(
            refused.value["errorCode"], "decode",
            "{damage}: {}",
            refused.value
        );
        assert_eq!(
            refused.class,
            ExitClass::of(ErrorCode::SnapshotIncompatible),
            "{damage}: {}",
            refused.value
        );
        let lead = &refused.value["related"][0];
        assert_eq!(lead["tool"], "graphBuild", "{damage}: {}", refused.value);
        assert_eq!(
            lead["input"]["queries"][0]["refresh"], true,
            "{damage}: {lead}"
        );
        assert!(
            refused.value["error"]
                .as_str()
                .is_some_and(|text| text.contains("graphBuild")),
            "{damage}: {}",
            refused.value
        );
    }
}

/// Mark every directory under `root` read-only and every file in it read-only,
/// or writable again; returns whether this process is still stopped by it
/// (a superuser is not).
#[cfg(unix)]
fn set_tree_read_only(root: &std::path::Path, read_only: bool) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fn walk(path: &std::path::Path, read_only: bool) {
        let Ok(meta) = std::fs::symlink_metadata(path) else {
            return;
        };
        if meta.file_type().is_symlink() {
            return;
        }
        let dir = meta.is_dir();
        let mode = match (dir, read_only) {
            (true, true) => 0o555,
            (true, false) => 0o755,
            (false, true) => 0o444,
            (false, false) => 0o644,
        };
        if !dir || !read_only {
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode));
        }
        if dir {
            for entry in std::fs::read_dir(path).into_iter().flatten().flatten() {
                walk(&entry.path(), read_only);
            }
            if read_only {
                let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode));
            }
        }
    }
    walk(root, read_only);
    std::fs::File::create(root.join(".probe")).is_err()
}

/// A graph home the process cannot write answers a scope that has not
/// changed from the snapshot it holds, and a scope that has changed with the
/// recovery of a write (a writable home, free disk space), not a read's.
#[cfg(unix)]
#[test]
fn a_read_only_graph_home_answers_an_unchanged_scope_and_names_a_write_failure_otherwise() {
    let dir = fixture();
    let home = dir.0.as_path().join(".octocode").join("graph");
    let built = ingest_fixture(dir.0.as_path(), None);
    assert_eq!(built.class, ExitClass::Success, "{}", built.value);
    let id = built.value["graph"].as_str().expect("id").to_owned();
    if !set_tree_read_only(&home, true) {
        set_tree_read_only(&home, false);
        return; // a superuser writes anywhere
    }
    let unchanged = ingest_fixture(dir.0.as_path(), None);
    dir.write("app/src/changed.ts", "export const changed = 1;\n");
    let changed = ingest_fixture(dir.0.as_path(), None);
    set_tree_read_only(&home, false);
    assert_eq!(unchanged.class, ExitClass::Success, "{}", unchanged.value);
    assert_eq!(unchanged.value["graph"], id, "{}", unchanged.value);
    assert_eq!(
        changed.value["errorCode"], "fileAccessFailed",
        "{}",
        changed.value
    );
    let hints = changed.value["hints"].to_string();
    assert!(
        hints.contains("OCTOCODE_HOME") && !hints.contains("text encoding"),
        "{}",
        changed.value
    );
}
