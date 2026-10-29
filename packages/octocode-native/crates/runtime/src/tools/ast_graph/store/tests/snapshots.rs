//! Snapshot format, reuse, selection and pruning.
use super::*;

#[test]
fn older_analysis_is_readable_but_stale_and_never_reused() {
    let dir = fixture();
    let first = ingest_fixture(dir.0.as_path(), None);
    let graph_dir = PathBuf::from(first.value["dir"].as_str().expect("dir"));
    let manifest_path = graph_dir.join(MANIFEST_FILE);
    let mut manifest = read_manifest(&graph_dir).expect("manifest");
    manifest
        .as_object_mut()
        .expect("object")
        .remove("analysisVersion");
    std::fs::write(&manifest_path, serde_json::to_vec(&manifest).expect("json")).expect("write");
    let out = ask(dir.0.as_path(), "stats", None, |o| o.check_freshness = true);
    assert_eq!(out.class, ExitClass::Success);
    assert_eq!(out.value["freshness"]["fresh"], false, "{}", out.value);
    assert_eq!(out.value["freshness"]["analysisCurrent"], false);
    let stale = ask(dir.0.as_path(), "stale", None, |_| {});
    assert_eq!(stale.value["fresh"], false);
    let rebuilt = ingest_fixture(dir.0.as_path(), None);
    assert_eq!(rebuilt.class, ExitClass::Success);
    assert_ne!(rebuilt.value["reused"], true, "{}", rebuilt.value);
    assert_ne!(rebuilt.value["id"], first.value["id"]);
    let reused = ingest_fixture(dir.0.as_path(), None);
    assert_eq!(reused.value["reused"], true);
}

#[test]
fn oversized_snapshot_is_rejected_before_allocation() {
    let dir = Fixture::new();
    let file = dir.0.as_path().join(GRAPH_FILE);
    std::fs::File::create(&file)
        .expect("file")
        .set_len(crate::tools::ast_graph::limits::LIMITS.snapshot_bytes as u64 + 1)
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
        stale.value["results"],
        json!([{"file": "src/a.ts", "status": "changed"}])
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
fn plan_9_unchanged_tree_reuses_the_latest_snapshot() {
    let dir = fixture();
    let first = ingest_fixture(dir.0.as_path(), None);
    assert_eq!(first.class, ExitClass::Success);
    let second = ingest_fixture(dir.0.as_path(), None);
    assert_eq!(second.value["reused"], true, "{}", second.value);
    assert_eq!(second.value["id"], first.value["id"]);
    dir.write("app/src/a.ts", "export const a = 5;\n");
    let third = ingest_fixture(dir.0.as_path(), None);
    assert_ne!(third.value["reused"], true, "{}", third.value);
    let forced = ingest(
        &IngestOptions {
            path: dir.0.as_path().join("app"),
            workspace: Some(dir.0.as_path().to_path_buf()),
            force: true,
            ..Default::default()
        },
        &workspace_policy(dir.0.as_path()),
        &ContentSecurity::new(),
        &NeverCancel,
        None,
    );
    assert_ne!(forced.value["reused"], true);
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
            .is_some_and(|e| e.contains("re-run `octocode graph ingest`")),
        "{}",
        stale.value
    );
    let second = ingest_fixture(dir.0.as_path(), None);
    assert_eq!(second.class, ExitClass::Success, "{}", second.value);
    assert_ne!(second.value["reused"], true, "{}", second.value);
    assert_ne!(second.value["id"], first.value["id"]);
    let stats = ask(dir.0.as_path(), "stats", None, |_| {});
    assert_eq!(stats.class, ExitClass::Success, "{}", stats.value);
}

/// A settled file records its stamp, an equal stamp proves the bytes
/// without a read, and a same-size rewrite (new times) still rebuilds.
#[test]
fn settled_sources_reuse_by_stamp_and_a_same_size_edit_still_rebuilds() {
    let dir = fixture();
    octocode_engine::stamp::settle_written_so_far();
    let first = ingest_fixture(dir.0.as_path(), None);
    assert_eq!(first.class, ExitClass::Success, "{}", first.value);
    let graph_dir = PathBuf::from(first.value["dir"].as_str().expect("dir"));
    let bytes = std::fs::read(graph_dir.join(GRAPH_FILE)).expect("graph.bin");
    let (tables, _) = format::decode(&bytes).expect("decode");
    assert_eq!(tables.digests.len(), 4);
    assert_eq!(tables.stamps.len(), 4, "every settled source is stamped");
    // The unchanged-tree check decodes only the file index, the same one.
    let (files, files_digest) = format::decode_files(&bytes).expect("decode files");
    assert_eq!(files_digest, first.value["sha256"].as_str().expect("sha"));
    // Record strings only: their ids prefix the full dictionary, which adds
    // the strings only occurrence evidence references.
    assert!(files.strings.len() < tables.strings.len());
    assert_eq!(files.strings[..], tables.strings[..files.strings.len()]);
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
    assert_eq!(second.value["reused"], true, "{}", second.value);
    // Same length, new content: the stamp moves, so the hash decides.
    let before = std::fs::read_to_string(&a).expect("a.ts");
    let after = before.replace("b()", "c()");
    assert_eq!(before.len(), after.len());
    std::fs::write(&a, after).expect("edit");
    assert_eq!(source_matches(&a, digest, Some(stamp)), Some(false));
    let third = ingest_fixture(dir.0.as_path(), None);
    assert_eq!(third.class, ExitClass::Success, "{}", third.value);
    assert_ne!(third.value["reused"], true, "{}", third.value);
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
        app.value["id"].as_str().expect("id").to_owned(),
        src.value["id"].as_str().expect("id").to_owned(),
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
        ExitClass::InvalidInput,
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
        nested.push(out.value["id"].as_str().expect("id").to_owned());
    }
    let dash_id = dash.value["id"].as_str().expect("id");
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
    let old_id = old.value["id"].as_str().expect("id").to_owned();
    let src = ingest_root(
        dir.0.as_path(),
        &dir.0.as_path().join("app/src"),
        None,
        false,
    );
    let src_id = src.value["id"].as_str().expect("id").to_owned();
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

/// A substring that names only snapshots of one root is no ambiguity: it
/// picks that root's newest snapshot, as a scope does.
#[test]
fn a_substring_naming_one_roots_snapshots_picks_its_newest() {
    let dir = fixture();
    let src = dir.0.as_path().join("app/src");
    let first = ingest_root(dir.0.as_path(), &src, None, true);
    let second = ingest_root(dir.0.as_path(), &src, None, true);
    assert_eq!(second.class, ExitClass::Success, "{}", second.value);
    assert_ne!(first.value["id"], second.value["id"]);
    let stats = ask(dir.0.as_path(), "stats", None, |o| {
        o.graph = Some("pp-src".into());
    });
    assert_eq!(
        stats.value["graph"]["id"], second.value["id"],
        "{}",
        stats.value
    );
}
