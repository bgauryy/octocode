use super::*;
use crate::index::{
    build_index, index_status, query_index, IndexAccess, IndexBuildOptions, IndexQueryOptions,
    IndexRuntimeLimits, IndexStatusOptions,
};

fn access(temp: &TestDir, root: &std::path::Path) -> IndexAccess {
    IndexAccess {
        home: temp.0.join("home"),
        root: RootIdentity::mutable("runtime", root).expect("root identity"),
        config: IndexConfig {
            expected_schema_version: 1,
            expected_parser_schema_version: 7,
            expected_tool_version: "test-tool".to_owned(),
            max_generations: 3,
            max_bytes: 16 * 1024 * 1024,
        },
    }
}

fn limits() -> IndexRuntimeLimits {
    IndexRuntimeLimits {
        max_files: 100,
        max_entries: 1_000,
        max_depth: 32,
        max_file_bytes: 1024 * 1024,
        max_source_bytes: 8 * 1024 * 1024,
    }
}

#[test]
fn mutation_after_capture_cannot_pair_old_symbols_with_fresh_content() {
    let temp = TestDir::new("runtime-capture-mutation");
    let root = temp.0.join("repo");
    fs::create_dir_all(&root).expect("create root");
    let original = "export function BeforeMutation() { return 1; }\n";
    let replacement = "export function AfterMutation() { return 'different length'; }\n";
    let source = root.join("source.ts");
    fs::write(&source, original).expect("write original");
    let access = access(&temp, &root);
    let mut bounded = limits();
    bounded.max_file_bytes = original.len() as u64;
    bounded.max_source_bytes = original.len() as u64;
    let built = crate::index::runtime::build_index_with_snapshot_observer(
        IndexBuildOptions {
            access: access.clone(),
            exclusions: Vec::new(),
            limits: bounded,
        },
        || fs::write(&source, replacement).expect("mutate after capture"),
    )
    .expect("build captured generation");
    let store =
        IndexStore::open(access.home, &access.root.root_id, access.config).expect("open store");
    let reader = store.open_active(&access.root).expect("open generation");
    let document = &reader.documents()[0];
    assert_eq!(document.content, original);
    assert_eq!(document.identity.size, original.len() as u64);
    assert_eq!(
        document.identity.content_digest,
        content_digest(original.as_bytes())
    );
    assert!(document
        .symbols
        .iter()
        .any(|symbol| symbol.name == "BeforeMutation"));
    assert!(!document
        .symbols
        .iter()
        .any(|symbol| symbol.name == "AfterMutation"));
    assert_eq!(built.indexed_source_bytes, original.len() as u64);
    assert!(!built.usable);
    assert_eq!(built.freshness.dirty, vec!["source.ts"]);
}

#[cfg(unix)]
#[test]
fn source_replaced_by_symlink_after_capture_is_rejected() {
    let temp = TestDir::new("runtime-capture-symlink");
    let root = temp.0.join("repo");
    fs::create_dir_all(&root).expect("create root");
    let source = root.join("source.ts");
    let outside = temp.0.join("outside.ts");
    fs::write(&source, "export function Original() {}\n").expect("write source");
    fs::write(&outside, "export function Outside() {}\n").expect("write outside");
    let result = crate::index::runtime::build_index_with_snapshot_observer(
        IndexBuildOptions {
            access: access(&temp, &root),
            exclusions: Vec::new(),
            limits: limits(),
        },
        || {
            fs::remove_file(&source).expect("remove source");
            std::os::unix::fs::symlink(&outside, &source).expect("replace with symlink");
        },
    );
    assert!(matches!(result, Err(IndexError::SymlinkSource { .. })));
}

#[test]
fn source_capture_is_bounded_and_rejects_non_files() {
    let temp = TestDir::new("runtime-capture-bound");
    let root = temp.0.join("repo");
    fs::create_dir_all(root.join("directory.ts")).expect("create root");
    fs::write(root.join("source.ts"), "0123456789abcdef").expect("write source");
    let (bytes, _) =
        crate::index::store::read_source_bytes(&root, std::path::Path::new("source.ts"), 5)
            .expect("bounded read");
    assert_eq!(bytes, b"012345");
    assert!(matches!(
        crate::index::store::read_source_bytes(&root, std::path::Path::new("directory.ts"), 5,),
        Err(IndexError::NonFileSource { .. })
    ));
}

#[test]
fn runtime_build_query_and_status_are_exact_and_deterministically_paginated() {
    let temp = TestDir::new("runtime-query");
    let root = temp.0.join("repo");
    fs::create_dir_all(root.join("src")).expect("create source tree");
    fs::write(
        root.join("src/b.ts"),
        "export function VisibleAnswer() { return 'needle'; }\n",
    )
    .expect("write b");
    fs::write(root.join("src/a.rs"), "fn first() { /* needle */ }\n").expect("write a");
    let access = access(&temp, &root);

    let built = build_index(IndexBuildOptions {
        access: access.clone(),
        exclusions: Vec::new(),
        limits: limits(),
    })
    .expect("build index");
    assert_eq!(built.document_count, 2);
    assert!(built.complete);
    assert!(built.usable);

    let first = query_index(IndexQueryOptions {
        access: access.clone(),
        text: "needle".to_owned(),
        kind: IndexQueryKind::Content,
        case_sensitive: true,
        offset: 0,
        limit: 1,
        expected_generation: None,
        freshness_max_entries: 1_000,
        freshness_max_depth: 32,
    })
    .expect("first page");
    assert!(first.usable);
    assert_eq!(first.total_matches, 2);
    assert_eq!(first.matches[0].path, "src/a.rs");
    assert_eq!(first.next_offset, Some(1));
    assert!(!first.absence_proven);

    let second = query_index(IndexQueryOptions {
        expected_generation: Some(first.generation),
        offset: first.next_offset.expect("continuation"),
        ..IndexQueryOptions {
            access: access.clone(),
            text: "needle".to_owned(),
            kind: IndexQueryKind::Content,
            case_sensitive: true,
            offset: 0,
            limit: 1,
            expected_generation: None,
            freshness_max_entries: 1_000,
            freshness_max_depth: 32,
        }
    })
    .expect("second page");
    assert_eq!(second.matches[0].path, "src/b.ts");
    assert_eq!(second.next_offset, None);

    let absent = query_index(IndexQueryOptions {
        access: access.clone(),
        text: "definitely absent".to_owned(),
        kind: IndexQueryKind::Content,
        case_sensitive: true,
        offset: 0,
        limit: 10,
        expected_generation: Some(built.generation),
        freshness_max_entries: 1_000,
        freshness_max_depth: 32,
    })
    .expect("absence query");
    assert!(absent.absence_proven);

    let symbols = query_index(IndexQueryOptions {
        access: access.clone(),
        text: "VisibleAnswer".to_owned(),
        kind: IndexQueryKind::Symbol,
        case_sensitive: true,
        offset: 0,
        limit: 10,
        expected_generation: Some(built.generation),
        freshness_max_entries: 1_000,
        freshness_max_depth: 32,
    })
    .expect("symbol query");
    assert_eq!(symbols.total_matches, 1);
    assert_eq!(symbols.matches[0].path, "src/b.ts");

    let status = index_status(IndexStatusOptions {
        access,
        freshness_max_entries: 1_000,
        freshness_max_depth: 32,
    })
    .expect("status");
    assert!(status.indexed);
    assert!(status.usable);
    assert_eq!(status.generation, Some(built.generation));
}

#[test]
fn stale_runtime_never_returns_stale_matches_or_proves_absence() {
    let temp = TestDir::new("runtime-stale");
    let root = temp.0.join("repo");
    fs::create_dir_all(&root).expect("create root");
    fs::write(root.join("file.rs"), "needle\n").expect("write source");
    let access = access(&temp, &root);
    build_index(IndexBuildOptions {
        access: access.clone(),
        exclusions: Vec::new(),
        limits: limits(),
    })
    .expect("build index");
    fs::write(root.join("file.rs"), "changed\n").expect("mutate source");

    let result = query_index(IndexQueryOptions {
        access: access.clone(),
        text: "needle".to_owned(),
        kind: IndexQueryKind::Content,
        case_sensitive: true,
        offset: 0,
        limit: 10,
        expected_generation: None,
        freshness_max_entries: 1_000,
        freshness_max_depth: 32,
    })
    .expect("typed stale result");
    assert!(!result.usable);
    assert!(result.matches.is_empty());
    assert!(!result.absence_proven);
    assert_eq!(result.diagnostic.as_deref(), Some("index.stale"));

    let status = index_status(IndexStatusOptions {
        access,
        freshness_max_entries: 1_000,
        freshness_max_depth: 32,
    })
    .expect("stale status");
    assert!(!status.usable);
    assert_eq!(status.freshness.expect("freshness").dirty, vec!["file.rs"]);
}

#[test]
fn bounded_build_is_incomplete_and_cannot_be_queried_as_authoritative() {
    let temp = TestDir::new("runtime-bounded");
    let root = temp.0.join("repo");
    fs::create_dir_all(&root).expect("create root");
    for name in ["a.rs", "b.rs", "c.rs"] {
        fs::write(root.join(name), format!("{name} needle\n")).expect("write source");
    }
    let access = access(&temp, &root);
    let mut bounded = limits();
    bounded.max_files = 1;
    let built = build_index(IndexBuildOptions {
        access: access.clone(),
        exclusions: Vec::new(),
        limits: bounded,
    })
    .expect("bounded build");
    assert_eq!(built.document_count, 1);
    assert!(!built.complete);
    assert!(built.truncated);
    assert!(!built.usable);

    let result = query_index(IndexQueryOptions {
        access,
        text: "missing".to_owned(),
        kind: IndexQueryKind::Content,
        case_sensitive: true,
        offset: 0,
        limit: 10,
        expected_generation: Some(built.generation),
        freshness_max_entries: 1_000,
        freshness_max_depth: 32,
    })
    .expect("typed incomplete result");
    assert!(!result.usable);
    assert!(!result.absence_proven);
}

#[test]
fn status_reports_absent_without_implying_an_empty_index() {
    let temp = TestDir::new("runtime-absent");
    let root = temp.0.join("repo");
    fs::create_dir_all(&root).expect("create root");
    let status = index_status(IndexStatusOptions {
        access: access(&temp, &root),
        freshness_max_entries: 100,
        freshness_max_depth: 10,
    })
    .expect("absent status");
    assert!(!status.indexed);
    assert!(!status.usable);
    assert_eq!(status.diagnostic.as_deref(), Some("index.absent"));
}

#[test]
fn freshness_budget_and_generation_binding_fail_closed() {
    let temp = TestDir::new("runtime-query-guards");
    let root = temp.0.join("repo");
    fs::create_dir_all(&root).expect("create root");
    fs::write(root.join("a.rs"), "needle\n").expect("write a");
    fs::write(root.join("b.rs"), "needle\n").expect("write b");
    let access = access(&temp, &root);
    let built = build_index(IndexBuildOptions {
        access: access.clone(),
        exclusions: Vec::new(),
        limits: limits(),
    })
    .expect("build index");

    let bounded = query_index(IndexQueryOptions {
        access: access.clone(),
        text: "needle".to_owned(),
        kind: IndexQueryKind::Content,
        case_sensitive: true,
        offset: 0,
        limit: 10,
        expected_generation: Some(built.generation),
        freshness_max_entries: 1,
        freshness_max_depth: 32,
    })
    .expect("bounded freshness result");
    assert!(!bounded.usable);
    assert!(!bounded.freshness.traversal_complete);
    assert!(bounded.matches.is_empty());
    assert!(!bounded.absence_proven);

    let wrong_generation = query_index(IndexQueryOptions {
        access,
        text: "needle".to_owned(),
        kind: IndexQueryKind::Content,
        case_sensitive: true,
        offset: 0,
        limit: 10,
        expected_generation: Some(built.generation + 1),
        freshness_max_entries: 100,
        freshness_max_depth: 32,
    });
    assert!(matches!(
        wrong_generation,
        Err(IndexError::GenerationMismatch { .. })
    ));
}
