use super::*;
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEMP: AtomicU64 = AtomicU64::new(1);

struct TestDir(std::path::PathBuf);

impl TestDir {
    fn new(label: &str) -> Self {
        let sequence = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "octocode-index-{label}-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("create test directory");
        Self(path)
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn mutable_root(root: &std::path::Path) -> RootIdentity {
    RootIdentity::mutable("fixture", root).expect("mutable root identity")
}

fn generation(root: &std::path::Path, schema_version: u32) -> GenerationSpec {
    GenerationSpec {
        root: mutable_root(root),
        index_schema_version: schema_version,
        parser_schema_version: 7,
        tool_version: "test-tool".to_owned(),
        exclusions: vec!["target".to_owned()],
        complete: true,
    }
}

fn store(temp: &TestDir, max_generations: usize) -> IndexStore {
    IndexStore::open(
        temp.0.join("home"),
        "fixture",
        IndexConfig {
            expected_schema_version: 1,
            expected_parser_schema_version: 7,
            expected_tool_version: "test-tool".to_owned(),
            max_generations,
            max_bytes: 16 * 1024 * 1024,
        },
    )
    .expect("open store")
}

fn commit_file(store: &IndexStore, root: &std::path::Path, content: &str) -> GenerationManifest {
    fs::create_dir_all(root.join("src")).expect("create fixture source");
    fs::write(root.join("src/lib.rs"), content).expect("write fixture source");

    let mut writer = store
        .begin_generation(generation(root, 1))
        .expect("begin generation");
    writer
        .add_file(
            root,
            "src/lib.rs",
            "rust",
            vec![SymbolRecord {
                name: "answer".to_owned(),
                kind: "function".to_owned(),
                start_byte: 0,
                end_byte: content.len() as u64,
            }],
        )
        .expect("add source file");
    writer.commit().expect("commit generation")
}

#[test]
fn commits_versioned_generation_and_reopens_content_and_symbols() {
    let temp = TestDir::new("commit");
    let root = temp.0.join("repo");
    let store = store(&temp, 3);

    let manifest = commit_file(&store, &root, "fn answer() -> u8 { 42 }\n");
    assert_eq!(manifest.generation, 1);
    assert_eq!(manifest.index_schema_version, 1);
    assert!(manifest.complete);

    let reader = store
        .open_active(&mutable_root(&root))
        .expect("open active generation");
    assert_eq!(reader.generation(), 1);
    assert_eq!(reader.documents().len(), 1);
    assert_eq!(reader.documents()[0].content, "fn answer() -> u8 { 42 }\n");
    assert_eq!(reader.documents()[0].symbols[0].name, "answer");
    assert_eq!(reader.verify_strict(&root), FreshnessReport::fresh(1));
}

#[test]
fn dropped_uncommitted_writer_and_torn_pointer_never_replace_active_generation() {
    let temp = TestDir::new("crash");
    let root = temp.0.join("repo");
    let store = store(&temp, 3);
    commit_file(&store, &root, "old\n");

    fs::write(root.join("src/lib.rs"), "new\n").expect("mutate source");
    {
        let mut crashed = store
            .begin_generation(generation(&root, 1))
            .expect("begin staged generation");
        crashed
            .add_file(&root, "src/lib.rs", "rust", Vec::new())
            .expect("stage source");
    }

    let reader = store
        .open_active(&mutable_root(&root))
        .expect("old active survives abandoned staging");
    assert_eq!(reader.generation(), 1);
    assert_eq!(reader.documents()[0].content, "old\n");

    fs::write(store.active_pointer_path(), b"not-a-generation\n").expect("simulate torn pointer");
    assert!(matches!(
        store.open_active(&mutable_root(&root)),
        Err(IndexError::CorruptActivePointer { .. })
    ));
}

#[test]
fn document_payload_digest_rejects_valid_json_torn_or_tampered_bytes() {
    let temp = TestDir::new("payload-digest");
    let root = temp.0.join("repo");
    let store = store(&temp, 3);
    commit_file(&store, &root, "safe\n");
    let documents_path = store
        .directory()
        .join("generations/generation-00000000000000000001/documents.json");
    let encoded = fs::read_to_string(&documents_path).expect("read documents payload");
    assert!(encoded.contains("safe\\n"));
    fs::write(&documents_path, encoded.replace("safe\\n", "evil\\n"))
        .expect("tamper with valid JSON payload");

    assert!(matches!(
        store.open_active(&mutable_root(&root)),
        Err(IndexError::CorruptGeneration { generation: 1, .. })
    ));
}

#[test]
fn reader_keeps_its_generation_during_refresh_and_writers_are_locked() {
    let temp = TestDir::new("concurrent");
    let root = temp.0.join("repo");
    let store = store(&temp, 3);
    commit_file(&store, &root, "one\n");
    let old_reader = store
        .open_active(&mutable_root(&root))
        .expect("open first reader");

    fs::write(root.join("src/lib.rs"), "two\n").expect("mutate source");
    let mut writer = store
        .begin_generation(generation(&root, 1))
        .expect("begin refresh");
    assert!(matches!(
        store.begin_generation(generation(&root, 1)),
        Err(IndexError::WriterLocked { .. })
    ));
    writer
        .add_file(&root, "src/lib.rs", "rust", Vec::new())
        .expect("add refreshed source");
    writer.commit().expect("commit refresh");

    assert_eq!(old_reader.generation(), 1);
    assert_eq!(old_reader.documents()[0].content, "one\n");
    let new_reader = store
        .open_active(&mutable_root(&root))
        .expect("open refreshed reader");
    assert_eq!(new_reader.generation(), 2);
    assert_eq!(new_reader.documents()[0].content, "two\n");
}

#[test]
fn stale_crashed_writer_lock_is_reclaimed_but_live_owner_is_not() {
    let temp = TestDir::new("stale-lock");
    let root = temp.0.join("repo");
    fs::create_dir_all(&root).expect("create repo");
    let store = store(&temp, 3);
    let lock_path = store.directory().join("WRITE.lock");
    fs::write(
        &lock_path,
        br#"{"pid":4294967295,"processIdentity":"missing","nonce":"crashed"}"#,
    )
    .expect("write crashed writer lock");

    let writer = store
        .begin_generation(generation(&root, 1))
        .expect("reclaim stale crashed-writer lock");
    let ownership: serde_json::Value =
        serde_json::from_slice(&fs::read(&lock_path).expect("read ownership record"))
            .expect("parse ownership record");
    assert_eq!(ownership["pid"], serde_json::json!(std::process::id()));
    assert!(ownership["processIdentity"].is_string());
    assert!(ownership["nonce"]
        .as_str()
        .is_some_and(|nonce| !nonce.is_empty()));
    assert!(matches!(
        store.begin_generation(generation(&root, 1)),
        Err(IndexError::WriterLocked { .. })
    ));
    drop(writer);
    assert!(
        lock_path.exists(),
        "persistent lock inode prevents replacement races"
    );
    drop(
        store
            .begin_generation(generation(&root, 1))
            .expect("persistent lock is reusable after release"),
    );
}

#[test]
fn rejects_schema_and_immutable_source_identity_drift() {
    let temp = TestDir::new("identity");
    let repo = temp.0.join("repo");
    fs::create_dir_all(&repo).expect("create repo");
    fs::write(repo.join("file.py"), "x = 1\n").expect("write source");
    let store = store(&temp, 3);
    let source = RootIdentity::immutable_git("fixture", &repo, "commit-a", "tree-a")
        .expect("immutable root identity");
    let mut writer = store
        .begin_generation(GenerationSpec {
            root: source,
            index_schema_version: 1,
            parser_schema_version: 7,
            tool_version: "test-tool".to_owned(),
            exclusions: Vec::new(),
            complete: true,
        })
        .expect("begin immutable generation");
    writer
        .add_file(&repo, "file.py", "python", Vec::new())
        .expect("add source");
    writer.commit().expect("commit immutable generation");

    let wrong_schema = IndexStore::open(
        temp.0.join("home"),
        "fixture",
        IndexConfig {
            expected_schema_version: 2,
            expected_parser_schema_version: 7,
            expected_tool_version: "test-tool".to_owned(),
            max_generations: 3,
            max_bytes: 16 * 1024 * 1024,
        },
    )
    .expect("open schema-drift store");
    assert!(matches!(
        wrong_schema.open_active(
            &RootIdentity::immutable_git("fixture", &repo, "commit-a", "tree-a")
                .expect("expected root")
        ),
        Err(IndexError::SchemaMismatch {
            expected: 2,
            actual: 1
        })
    ));

    assert!(matches!(
        store.open_active(
            &RootIdentity::immutable_git("fixture", &repo, "commit-b", "tree-b")
                .expect("changed source")
        ),
        Err(IndexError::SourceIdentityMismatch { .. })
    ));

    let wrong_root_spec = GenerationSpec {
        root: RootIdentity::immutable_git("other", &repo, "commit-a", "tree-a")
            .expect("other root identity"),
        index_schema_version: 1,
        parser_schema_version: 7,
        tool_version: "test-tool".to_owned(),
        exclusions: Vec::new(),
        complete: true,
    };
    assert!(matches!(
        store.begin_generation(wrong_root_spec),
        Err(IndexError::RootIdMismatch { .. })
    ));
}

#[test]
fn rejects_parser_and_tool_version_drift_and_incomplete_absence_proof() {
    let temp = TestDir::new("version-drift");
    let root = temp.0.join("repo");
    let store = store(&temp, 3);
    commit_file(&store, &root, "value\n");

    let wrong_parser = IndexStore::open(
        temp.0.join("home"),
        "fixture",
        IndexConfig {
            expected_schema_version: 1,
            expected_parser_schema_version: 8,
            expected_tool_version: "test-tool".to_owned(),
            max_generations: 3,
            max_bytes: 16 * 1024 * 1024,
        },
    )
    .expect("open parser-drift store");
    assert!(matches!(
        wrong_parser.open_active(&mutable_root(&root)),
        Err(IndexError::ParserSchemaMismatch {
            expected: 8,
            actual: 7
        })
    ));

    let wrong_tool = IndexStore::open(
        temp.0.join("home"),
        "fixture",
        IndexConfig {
            expected_schema_version: 1,
            expected_parser_schema_version: 7,
            expected_tool_version: "next-tool".to_owned(),
            max_generations: 3,
            max_bytes: 16 * 1024 * 1024,
        },
    )
    .expect("open tool-drift store");
    assert!(matches!(
        wrong_tool.open_active(&mutable_root(&root)),
        Err(IndexError::ToolVersionMismatch { .. })
    ));

    fs::write(root.join("src/lib.rs"), "incomplete\n").expect("update source");
    let mut incomplete_spec = generation(&root, 1);
    incomplete_spec.complete = false;
    let mut writer = store
        .begin_generation(incomplete_spec)
        .expect("begin incomplete generation");
    writer
        .add_file(&root, "src/lib.rs", "rust", Vec::new())
        .expect("add incomplete source");
    writer.commit().expect("commit incomplete generation");
    let report = store
        .open_active(&mutable_root(&root))
        .expect("open incomplete generation")
        .verify_strict(&root);
    assert_eq!(report.fresh, 1);
    assert!(!report.generation_complete);
    assert!(!report.can_prove_absence());
}

#[test]
fn strict_freshness_reports_dirty_deleted_and_unverifiable_files() {
    let temp = TestDir::new("freshness");
    let root = temp.0.join("repo");
    let store = store(&temp, 3);
    commit_file(&store, &root, "same-size-a\n");
    let reader = store
        .open_active(&mutable_root(&root))
        .expect("open reader");

    fs::write(root.join("src/lib.rs"), "same-size-b\n").expect("dirty source");
    let dirty = reader.verify_strict(&root);
    assert_eq!(dirty.dirty, vec!["src/lib.rs"]);
    assert!(!dirty.can_prove_absence());

    fs::remove_file(root.join("src/lib.rs")).expect("delete source");
    let deleted = reader.verify_strict(&root);
    assert_eq!(deleted.deleted, vec!["src/lib.rs"]);
    assert!(!deleted.can_prove_absence());

    fs::create_dir_all(root.join("src/lib.rs")).expect("replace file with directory");
    let unverifiable = reader.verify_strict(&root);
    assert_eq!(unverifiable.unverifiable, vec!["src/lib.rs"]);
    assert!(!unverifiable.can_prove_absence());
}

#[test]
fn strict_freshness_detects_new_files_that_could_invalidate_absence() {
    let temp = TestDir::new("stale-absence");
    let root = temp.0.join("repo");
    let store = store(&temp, 3);
    commit_file(&store, &root, "indexed\n");
    let reader = store
        .open_active(&mutable_root(&root))
        .expect("open reader");

    fs::write(root.join("new.rs"), "new_match\n").expect("add unindexed source");
    let report = reader.verify_strict(&root);
    assert_eq!(report.added, vec!["new.rs"]);
    assert!(!report.can_prove_absence());

    fs::create_dir_all(root.join("target")).expect("create excluded directory");
    fs::write(root.join("target/generated.rs"), "ignored\n").expect("write excluded source");
    let excluded_report = reader.verify_strict(&root);
    assert_eq!(excluded_report.added, vec!["new.rs"]);
}

#[cfg(unix)]
#[test]
fn symlinks_are_never_indexed_as_source_files() {
    use std::os::unix::fs::symlink;

    let temp = TestDir::new("symlink");
    let root = temp.0.join("repo");
    fs::create_dir_all(&root).expect("create repo");
    let outside = temp.0.join("outside.rs");
    fs::write(&outside, "secret\n").expect("write outside source");
    symlink(&outside, root.join("link.rs")).expect("create symlink");
    let store = store(&temp, 3);
    let mut writer = store
        .begin_generation(generation(&root, 1))
        .expect("begin generation");

    assert!(matches!(
        writer.add_file(&root, "link.rs", "rust", Vec::new()),
        Err(IndexError::SymlinkSource { .. })
    ));
}

#[test]
fn content_addressed_sidecars_deduplicate_and_old_generations_are_pruned() {
    let temp = TestDir::new("sidecars");
    let root = temp.0.join("repo");
    let store = store(&temp, 2);

    for value in ["one\n", "two\n", "three\n"] {
        fs::create_dir_all(&root).expect("create repo");
        fs::write(root.join("file.rs"), value).expect("write source");
        let mut writer = store
            .begin_generation(generation(&root, 1))
            .expect("begin generation");
        writer
            .add_file(&root, "file.rs", "rust", Vec::new())
            .expect("add source");
        let first = writer
            .add_graph_fact_sidecar(7, br#"{"facts":[]}"#)
            .expect("add graph facts");
        let second = writer
            .add_graph_fact_sidecar(7, br#"{"facts":[]}"#)
            .expect("deduplicate graph facts");
        assert_eq!(first, second);
        writer.commit().expect("commit generation");
    }

    assert_eq!(
        store.list_generations().expect("list generations"),
        vec![2, 3]
    );
    let reader = store
        .open_active(&mutable_root(&root))
        .expect("open latest generation");
    assert_eq!(reader.manifest().graph_fact_sidecars.len(), 1);
    assert_eq!(
        reader
            .read_graph_fact_sidecar(&reader.manifest().graph_fact_sidecars[0])
            .expect("read sidecar"),
        br#"{"facts":[]}"#
    );
}

#[test]
fn manifest_integrity_rejects_tampered_absence_and_compatibility_fields() {
    let temp = TestDir::new("manifest-integrity");
    let root = temp.0.join("repo");
    let store = store(&temp, 3);
    commit_file(&store, &root, "safe\n");
    let manifest_path = store
        .directory()
        .join("generations/generation-00000000000000000001/manifest.json");
    let original_bytes = fs::read(&manifest_path).expect("read manifest");
    let original: serde_json::Value =
        serde_json::from_slice(&original_bytes).expect("parse manifest");
    let mut variants = Vec::new();
    for field in ["complete", "toolVersion", "exclusions", "root", "sidecars"] {
        let mut tampered = original.clone();
        match field {
            "complete" => tampered["complete"] = serde_json::json!(false),
            "toolVersion" => tampered["toolVersion"] = serde_json::json!("other-tool"),
            "exclusions" => tampered["exclusions"] = serde_json::json!(["different"]),
            "root" => tampered["root"]["canonicalRoot"] = serde_json::json!("/different"),
            "sidecars" => {
                tampered["graphFactSidecars"] = serde_json::json!([{
                    "digest": "0".repeat(64),
                    "schemaVersion": 7,
                    "payloadBytes": 0
                }]);
            }
            _ => unreachable!(),
        }
        variants.push((field, tampered));
    }

    for (field, tampered) in variants {
        fs::write(
            &manifest_path,
            serde_json::to_vec(&tampered).expect("encode tampered manifest"),
        )
        .expect("tamper manifest");
        let result = store.open_active(&mutable_root(&root));
        assert!(
            matches!(
                &result,
                Err(IndexError::CorruptGeneration { generation: 1, .. })
            ),
            "manifest digest must bind {field}, got {result:?}"
        );
    }
    fs::write(&manifest_path, original_bytes).expect("restore manifest");
}

#[test]
fn sidecar_payload_length_is_verified_against_manifest_reference() {
    let temp = TestDir::new("sidecar-length");
    let root = temp.0.join("repo");
    fs::create_dir_all(&root).expect("create repo");
    fs::write(root.join("file.rs"), "source\n").expect("write source");
    let store = store(&temp, 3);
    let mut writer = store
        .begin_generation(generation(&root, 1))
        .expect("begin generation");
    writer
        .add_file(&root, "file.rs", "rust", Vec::new())
        .expect("add file");
    writer
        .add_graph_fact_sidecar(7, b"sidecar payload")
        .expect("add sidecar");
    writer.commit().expect("commit generation");
    let manifest_path = store
        .directory()
        .join("generations/generation-00000000000000000001/manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest_path).expect("read manifest"))
            .expect("parse manifest");
    manifest["graphFactSidecars"][0]["payloadBytes"] = serde_json::json!(99);
    fs::write(
        &manifest_path,
        serde_json::to_vec(&manifest).expect("encode tampered manifest"),
    )
    .expect("write tampered payload length");
    store
        .resign_manifest_for_test(1)
        .expect("resign fixture to isolate payload-length validation");

    let result = store.open_active(&mutable_root(&root));
    assert!(
        matches!(
            &result,
            Err(IndexError::CorruptGeneration { generation: 1, .. })
        ),
        "payload-length mismatch must reject the generation, got {result:?}"
    );
}

#[test]
fn unique_sidecars_are_collected_without_breaking_open_readers_or_quota() {
    let temp = TestDir::new("sidecar-gc");
    let root = temp.0.join("repo");
    fs::create_dir_all(&root).expect("create repo");
    let store = IndexStore::open(
        temp.0.join("home"),
        "fixture",
        IndexConfig {
            expected_schema_version: 1,
            expected_parser_schema_version: 7,
            expected_tool_version: "test-tool".to_owned(),
            max_generations: 1,
            max_bytes: 32 * 1024,
        },
    )
    .expect("open store");
    let mut first_reader = None;
    let mut first_reference = None;

    for generation_number in 0..12 {
        fs::write(
            root.join("file.rs"),
            format!("source {generation_number}\n"),
        )
        .expect("write source");
        let mut writer = store
            .begin_generation(generation(&root, 1))
            .expect("begin generation");
        writer
            .add_file(&root, "file.rs", "rust", Vec::new())
            .expect("add source");
        let payload = vec![generation_number as u8; 4 * 1024];
        let reference = writer
            .add_graph_fact_sidecar(7, &payload)
            .expect("add unique sidecar");
        writer.commit().expect("commit within retained-data quota");
        if generation_number == 0 {
            let reader = store
                .open_active(&mutable_root(&root))
                .expect("open pinned reader");
            first_reference = Some(reference);
            first_reader = Some(reader);
        }
    }

    assert_eq!(
        store.list_generations().expect("list generations"),
        vec![12]
    );
    let sidecar_count = fs::read_dir(store.directory().join("sidecars"))
        .expect("list sidecars")
        .count();
    assert_eq!(sidecar_count, 1);
    assert_eq!(
        first_reader
            .expect("first reader")
            .read_graph_fact_sidecar(&first_reference.expect("first reference"))
            .expect("pinned reader keeps sidecar"),
        vec![0; 4 * 1024]
    );
}

#[test]
fn activation_is_success_even_when_post_commit_pruning_fails() {
    let temp = TestDir::new("prune-failure");
    let root = temp.0.join("repo");
    let store = store(&temp, 1);
    commit_file(&store, &root, "one\n");
    store.fail_next_prune_for_test();

    fs::write(root.join("src/lib.rs"), "two\n").expect("update source");
    let mut writer = store
        .begin_generation(generation(&root, 1))
        .expect("begin generation");
    writer
        .add_file(&root, "src/lib.rs", "rust", Vec::new())
        .expect("add source");
    let committed = writer.commit().expect("activation defines commit success");
    assert_eq!(committed.generation, 2);
    let active = store
        .open_active(&mutable_root(&root))
        .expect("new generation is active");
    assert_eq!(active.generation(), 2);
}

#[test]
fn quota_failure_does_not_activate_staged_generation() {
    let temp = TestDir::new("quota");
    let root = temp.0.join("repo");
    let store = IndexStore::open(
        temp.0.join("home"),
        "fixture",
        IndexConfig {
            expected_schema_version: 1,
            expected_parser_schema_version: 7,
            expected_tool_version: "test-tool".to_owned(),
            max_generations: 2,
            max_bytes: 128,
        },
    )
    .expect("open quota store");
    fs::create_dir_all(&root).expect("create repo");
    fs::write(root.join("large.txt"), "x".repeat(1024)).expect("write large source");
    let mut writer = store
        .begin_generation(generation(&root, 1))
        .expect("begin generation");
    writer
        .add_file(&root, "large.txt", "text", Vec::new())
        .expect("add source");

    assert!(matches!(
        writer.commit(),
        Err(IndexError::QuotaExceeded { .. })
    ));
    assert!(matches!(
        store.open_active(&mutable_root(&root)),
        Err(IndexError::NoActiveGeneration)
    ));
}

#[test]
fn quota_accounts_for_existing_generations_not_only_new_payload() {
    let temp = TestDir::new("total-quota");
    let root = temp.0.join("repo");
    let roomy = store(&temp, 3);
    commit_file(&roomy, &root, "first-generation\n");
    let existing_bytes = directory_bytes(roomy.directory());

    let limited = IndexStore::open(
        temp.0.join("home"),
        "fixture",
        IndexConfig {
            expected_schema_version: 1,
            expected_parser_schema_version: 7,
            expected_tool_version: "test-tool".to_owned(),
            max_generations: 3,
            max_bytes: existing_bytes + 64,
        },
    )
    .expect("reopen with tight aggregate quota");
    fs::write(root.join("src/lib.rs"), "second-generation\n").expect("update source");
    let mut writer = limited
        .begin_generation(generation(&root, 1))
        .expect("begin generation");
    writer
        .add_file(&root, "src/lib.rs", "rust", Vec::new())
        .expect("add source");
    assert!(matches!(
        writer.commit(),
        Err(IndexError::QuotaExceeded { .. })
    ));

    let active = roomy
        .open_active(&mutable_root(&root))
        .expect("previous generation remains active");
    assert_eq!(active.generation(), 1);
    assert_eq!(active.documents()[0].content, "first-generation\n");
}

fn directory_bytes(path: &std::path::Path) -> u64 {
    fs::read_dir(path)
        .expect("read directory")
        .map(|entry| {
            let entry = entry.expect("read entry");
            let metadata = entry.metadata().expect("read metadata");
            if metadata.is_dir() {
                directory_bytes(&entry.path())
            } else {
                metadata.len()
            }
        })
        .sum()
}

#[test]
fn sha256_matches_standard_vector() {
    assert_eq!(
        content_digest(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}
