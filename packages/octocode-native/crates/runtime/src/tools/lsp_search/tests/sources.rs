//! Source access: bounded cached reads, line indexes, anchors on the
//! synchronized text and clipped rows.

use super::*;

#[tokio::test(flavor = "current_thread")]
async fn source_cache_reads_each_authorized_bounded_file_once() {
    let (_dir, root, paths) = temp_workspace();
    let inside = root.join("inside.ts");
    std::fs::write(&inside, "export const x = 1;\n").expect("inside");
    let large = root.join("large.ts");
    std::fs::write(&large, vec![b'a'; (MAX_LSP_DIDOPEN_BYTES + 1) as usize]).expect("large");
    let inside = inside.to_string_lossy().into_owned();

    let mut sources = SourceCache::new(&paths);
    assert!(sources.get(&inside).await.is_some());
    assert!(sources.get(&inside).await.is_some());
    assert_eq!(sources.reads(), 1, "a file is read once per request");
    assert!(
        sources.get(&large.to_string_lossy()).await.is_none(),
        "oversized hop targets must not be read or synced"
    );
    let reads = sources.reads();
    assert!(
        sources.get("/etc/hosts").await.is_none(),
        "server-supplied targets outside the read policy must not be read"
    );
    assert_eq!(sources.reads(), reads, "a refused path is never read");
    assert!(!sources.uri_authorized("file:///etc/hosts"));
    assert!(sources.uri_authorized(&inside));
    assert!(matches!(
        read_bounded_source(&large),
        Err(SourceReadError::TooLarge(_))
    ));
    assert!(matches!(
        read_bounded_source(&root),
        Err(SourceReadError::Unreadable(_))
    ));
}

/// An in-policy file whose content cannot be loaded (oversize, not
/// UTF-8) is still authorized, so server locations in it are kept.
#[tokio::test(flavor = "current_thread")]
async fn unreadable_in_policy_files_stay_authorized_and_keep_their_locations() {
    let (_dir, root, paths) = temp_workspace();
    let large = root.join("large.ts");
    std::fs::write(&large, vec![b'a'; (MAX_LSP_DIDOPEN_BYTES + 1) as usize]).expect("large");
    let binary = root.join("binary.ts");
    // Invalid UTF-8 (`ff fe` would be a UTF-16 BOM, which is admitted).
    std::fs::write(&binary, [0xc3_u8, 0x28]).expect("binary");
    let mut sources = SourceCache::new(&paths);
    for file in [&large, &binary] {
        let path = file.to_string_lossy().into_owned();
        assert!(sources.get(&path).await.is_none(), "{path} has no content");
        let uri = octocode_engine::lsp::uri::path_to_uri(&path).expect("uri");
        assert!(
            sources.uri_authorized(&uri),
            "unreadable content must not memoize {path} as unauthorized"
        );
    }
    let q = query(serde_json::json!({
        "operation": "references", "path": large.to_string_lossy(),
        "symbolName": "foo", "line": 1,
        "contextLines":{"before":2,"after":2}
    }));
    let unavailable = "[content unavailable — could not read: file too large]";
    let snippets = [&large, &binary]
        .iter()
        .map(|file| {
            serde_json::json!({
                "uri": octocode_engine::lsp::uri::path_to_uri(&file.to_string_lossy()).expect("uri"),
                "range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 1}},
                "content": unavailable
            })
        })
        .collect::<Vec<_>>();
    let result = locations(
        &q,
        &mut sources,
        "references",
        "referencesProvider",
        snippets,
        TEST_BUDGET,
        None,
    )
    .await;
    assert_eq!(result["pagination"]["totalItems"], 2, "{result}");
    let rows = result["matches"].as_array().expect("rows");
    assert!(
        rows.iter().all(|row| row["content"] == unavailable),
        "the unavailable reason is kept as content: {result}"
    );
}

/// A definition the server places outside the allowed read roots is
/// withheld, not absent: the row says how many were omitted, with no
/// build-script or bad-anchor recovery.
#[tokio::test(flavor = "current_thread")]
async fn locations_outside_the_read_roots_are_reported_withheld() {
    let (_dir, root, paths) = temp_workspace();
    let outside = tempfile::tempdir().expect("outside");
    let target = outside.path().join("walk.rs");
    std::fs::write(&target, "pub struct WalkBuilder;\n").expect("target");
    std::fs::write(root.join("Cargo.toml"), "[package]\nname = \"p\"\n").expect("manifest");
    std::fs::write(root.join("build.rs"), "fn main() {}\n").expect("build.rs");
    std::fs::create_dir_all(root.join("src")).expect("src");
    let anchor = root.join("src/lib.rs");
    std::fs::write(
        &anchor,
        "include!(concat!(env!(\"OUT_DIR\"), \"/gen.rs\"));\nuse ignore::WalkBuilder;\n",
    )
    .expect("lib");
    let q = query(serde_json::json!({
        "operation": "definition", "path": anchor.to_string_lossy(), "symbolName": "WalkBuilder", "line": 2
    }));
    let snippets = vec![serde_json::json!({
        "uri": octocode_engine::lsp::uri::path_to_uri(&target.to_string_lossy()).expect("uri"),
        "range": {"start": {"line": 0, "character": 11}, "end": {"line": 0, "character": 22}},
        "content": octocode_engine::lsp::client::SNIPPET_CONTENT_WITHHELD
    })];
    let row = locations(
        &q,
        &mut SourceCache::new(&paths),
        "definition",
        "definitionProvider",
        snippets,
        TEST_BUDGET,
        None,
    )
    .await;
    assert_eq!(row["status"], "empty", "{row}");
    assert_eq!(row["category"], "locationsWithheld", "{row}");
    assert!(
        row["reason"].as_str().is_some_and(
            |reason| reason.starts_with("1 location") && reason.contains("ALLOWED_PATHS")
        ),
        "{row}"
    );
    assert!(row.get("hints").is_none(), "{row}");
    assert!(
        !row["related"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|call| call["tool"] == "lspSearch"
                && call["input"]["queries"][0]["rustContext"]["buildScripts"] == true),
        "{row}"
    );
}

#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn bounded_source_reads_refuse_fifos_without_blocking() {
    let (_dir, root, paths) = temp_workspace();
    let fifo = root.join("pipe.ts");
    crate::tools::test_support::make_fifo(&fifo);
    assert!(matches!(
        read_bounded_source(&fifo),
        Err(SourceReadError::Unreadable(_))
    ));
    // The async read and the cache go through the O_NONBLOCK open and
    // the handle's fstat, so a FIFO (no writer) never blocks them.
    let bounded = std::time::Duration::from_secs(5);
    let read = tokio::time::timeout(bounded, read_bounded_source_async(fifo.clone()))
        .await
        .expect("async read of a FIFO must not block");
    assert!(matches!(read, Err(SourceReadError::Unreadable(_))));
    let mut sources = SourceCache::new(&paths);
    let cached = tokio::time::timeout(bounded, sources.get(&fifo.to_string_lossy()))
        .await
        .expect("cache read of a FIFO must not block");
    assert!(cached.is_none());
}

#[test]
fn source_line_index_matches_split_inclusive() {
    for text in ["", "a", "a\n", "a\nb", "a\r\nb\r\n", "\n\n"] {
        let source = Source::new(text.to_owned());
        let expected = text.split_inclusive('\n').collect::<Vec<_>>();
        assert_eq!(source.line_count(), expected.len(), "{text:?}");
        for start in 0..=expected.len() {
            for end in start..=expected.len() + 1 {
                assert_eq!(
                    source.lines(start, end),
                    expected[start..end.min(expected.len())].concat(),
                    "{text:?} {start}..{end}"
                );
            }
        }
    }
}

#[test]
fn source_line_index_breaks_on_a_lone_cr() {
    // `\r` alone ends a line, as the server counts it.
    let source = Source::new("a\rb\r\nc\nd\r".to_owned());
    assert_eq!(source.line_count(), 4);
    assert_eq!(source.lines(0, 1), "a\r");
    assert_eq!(source.lines(1, 2), "b\r\n");
    assert_eq!(source.lines(2, 4), "c\nd\r");
}

#[tokio::test(flavor = "current_thread")]
async fn context_windows_come_from_one_cached_read_per_file() {
    let (_dir, root, paths) = temp_workspace();
    let file = root.join("a.ts");
    std::fs::write(&file, "l1\nl2\nl3\nl4\nl5\n").expect("a.ts");
    let uri = octocode_engine::lsp::uri::path_to_uri(&file.to_string_lossy()).expect("uri");
    let location = |line: u64| {
        serde_json::json!({
            "uri": uri,
            "range": {"start": {"line": line, "character": 0}, "end": {"line": line, "character": 2}}
        })
    };
    let mut sources = SourceCache::new(&paths);
    let mut first = location(0);
    let mut last = location(4);
    apply_context_lines(
        &mut first,
        octocode_engine::types::SearchContext {
            before: 1,
            after: 1,
        },
        &mut sources,
    )
    .await;
    apply_context_lines(
        &mut last,
        octocode_engine::types::SearchContext {
            before: 1,
            after: 1,
        },
        &mut sources,
    )
    .await;
    assert_eq!(sources.reads(), 1);
    assert_eq!(first["content"], "l1\nl2\n");
    assert_eq!(
        first["displayRange"],
        serde_json::json!({"startLine": 1, "endLine": 2})
    );
    assert_eq!(last["content"], "l4\nl5\n");
    assert_eq!(
        last["displayRange"],
        serde_json::json!({"startLine": 4, "endLine": 5})
    );
}

#[test]
fn symbol_anchors_resolve_on_the_synchronized_text_not_the_disk() {
    // The path does not exist: resolution must use the supplied content.
    let q = query(serde_json::json!({
        "operation": "definition", "path": "/nonexistent/lib.rs",
        "symbolName": "greet",
        "line": 2
    }));
    let source = "// é😀\nfn é😀() {} fn greet() {}\n";
    let anchor = resolve_anchor(
        &q,
        "/nonexistent/lib.rs",
        "file:///nonexistent/lib.rs",
        Some(source),
    )
    .expect("anchor from content");
    // `fn é😀() {} fn ` is 15 UTF-16 units (é = 1, 😀 = 2).
    assert_eq!((anchor.line, anchor.character), (1, 15));
    let resolved = anchor.resolved_symbol.expect("receipt");
    // Found on its line: the receipt adds the column, not echoes.
    assert!(resolved.get("foundAtLine").is_none(), "{resolved}");
    assert!(resolved.get("name").is_none(), "{resolved}");
    assert_eq!(resolved["foundAtCharacter"], 16);
    // Found off its hint: the line and the deviation are new facts.
    let off = query(serde_json::json!({
        "operation": "definition", "path": "/nonexistent/lib.rs",
        "symbolName": "greet",
        "line": 1
    }));
    let moved = resolve_anchor(
        &off,
        "/nonexistent/lib.rs",
        "file:///nonexistent/lib.rs",
        Some(source),
    )
    .expect("anchor near the hint")
    .resolved_symbol
    .expect("receipt");
    assert_eq!(moved["foundAtLine"], 2, "{moved}");
    assert_eq!(moved["lineDeviation"], 1, "{moved}");
    assert!(resolve_anchor(&q, "/nonexistent/lib.rs", "file:///x", None).is_err());
}

#[test]
fn member_qualified_symbol_names_anchor_on_the_last_member() {
    let anchor_for = |path: &str, source: &str, name: &str, line: u32| {
        let q = query(serde_json::json!({
            "operation": "definition", "path": path, "symbolName": name, "line": line
        }));
        let anchor = resolve_anchor(&q, path, "file:///repo/x", Some(source)).expect(name);
        (anchor.line, anchor.character)
    };
    let js = "var ns = 1;\nthis.ns = function(fn) { var ns = {}; return ns; };\n";
    // `this.ns` anchors on `ns` (column 5), not on `this`.
    assert_eq!(anchor_for("/repo/a.js", js, "this.ns", 2), (1, 5));
    assert_eq!(anchor_for("/repo/a.js", js, "ns", 2), (1, 5));
    // A member repeated earlier on the line resolves inside the qualified
    // expression.
    let chain = "b.x = a.b.x = function () {};\n";
    assert_eq!(anchor_for("/repo/b.js", chain, "a.b.x", 1), (0, 10));
    let rust = "impl Foo {\n    fn bar() {}\n}\nfn main() { Foo::bar(); }\n";
    assert_eq!(anchor_for("/repo/lib.rs", rust, "Foo::bar", 4), (3, 17));
    // Not spelled contiguously: the bare member is resolved near the hint.
    let split = "this\n  .ns = function () {};\n";
    assert_eq!(anchor_for("/repo/c.js", split, "this.ns", 2), (1, 3));
    // Unqualified and non-identifier names resolve unchanged.
    assert_eq!(anchor_for("/repo/a.js", js, "this", 2), (1, 0));
}

/// A definition whose body is clipped is partial with its reason, and the
/// read of the omitted lines rides along.
#[tokio::test(flavor = "current_thread")]
async fn clipped_definition_rows_name_their_partial_reason() {
    let (_dir, root, paths) = temp_workspace();
    let file = root.join("lib.rs");
    std::fs::write(&file, "fn f() {}\n").expect("file");
    let uri = format!("file://{}", file.to_string_lossy());
    let body = (1..=70).map(|n| format!("line {n}\n")).collect::<String>();
    let q = query(serde_json::json!({
        "operation": "definition", "path": file.to_string_lossy(), "symbolName": "f", "line": 1
    }));
    let mut sources = SourceCache::new(&paths);
    let row = super::super::locations::locations(
        &q,
        &mut sources,
        "definition",
        "definitionProvider",
        vec![serde_json::json!({
            "uri": uri,
            "range": {"start": {"line": 0, "character": 3}, "end": {"line": 0, "character": 4}},
            "content": body,
            "displayRange": {"startLine": 1, "endLine": 70}
        })],
        100_000,
        None,
    )
    .await;
    assert_eq!(row["isPartial"], true, "{row}");
    assert_eq!(
        row["partialReasons"],
        serde_json::json!(["declarationClipped"]),
        "{row}"
    );
    assert_eq!(
        row["next"]["readDeclaration"]["tool"], "localFetch",
        "{row}"
    );
}

/// Every reader of source text for the server and the resolvers admits it
/// by the engine's one rule: a UTF-8 BOM is not text, BOM-marked UTF-16
/// decodes, and a NUL-bearing file is binary.
#[test]
fn source_text_is_admitted_by_the_one_engine_rule() {
    assert_eq!(
        admitted_text(b"\xef\xbb\xbffn a() {}\n").as_deref(),
        Ok("fn a() {}\n")
    );
    assert_eq!(
        admitted_text(&[0xff, 0xfe, b'a', 0, b'\n', 0]).as_deref(),
        Ok("a\n")
    );
    assert_eq!(
        admitted_text(b"ab\0cd"),
        Err(octocode_engine::text::Refusal::Binary)
    );
    assert_eq!(
        admitted_text(&[0xc3, 0x28]),
        Err(octocode_engine::text::Refusal::NotUnicode)
    );
}
