//! Detailed single-content results, the content size cap, demands and rebuilds.

use super::*;

#[test]
fn detailed_search_distinguishes_empty_from_unsupported() {
    let empty = evaluate_source("const x = 1;\n", "a.ts", "ts", Some("target($X)"), None);
    assert_eq!(empty.status, "ok");
    assert!(empty.matches.is_empty());
    assert!(empty.diagnostics.is_empty());

    let unsupported = evaluate_source(
        "target(value);\n",
        "note.txt",
        "txt",
        Some("target($X)"),
        None,
    );
    assert_eq!(unsupported.status, "unsupported");
    assert!(unsupported.matches.is_empty());
    assert_eq!(
        unsupported.diagnostics[0].code,
        "structural.language.unsupported"
    );
}

#[test]
fn detailed_search_reports_invalid_query_with_recovery() {
    let result = evaluate_source("x\n", "a.ts", "ts", Some("foo($X)"), Some(&json!({})));
    assert_eq!(result.status, "parserFailed");
    assert_eq!(result.query.kind, "invalid");
    assert_eq!(result.diagnostics[0].code, "structural.query.invalid");
    assert!(result.diagnostics[0].recovery.is_some());
}

#[test]
fn ast_audit_detailed_search_reports_unknown_node_kind() {
    let result = evaluate_source(
        "const x = 1;",
        "a.ts",
        "ts",
        None,
        Some(&json!({"kind":"not_a_real_node_kind_astro_999"})),
    );
    assert_eq!(result.status, "parserFailed");
    assert!(result.matches.is_empty());
    assert_eq!(result.diagnostics[0].code, "structural.query.compileFailed");
    assert!(result.diagnostics[0].message.contains("unknown node kind"));
    assert!(result.diagnostics[0].recovery.is_some());
}

#[test]
fn detailed_search_matches_carry_the_node_kind() {
    let content = "target(value);\n";
    let first = evaluate_source(content, "a.ts", "ts", Some("target($X)"), None);
    assert_eq!(first.matches.len(), 1);
    // node_kind is populated from the matched tree-sitter node (a
    // `target(value)` call), not left None.
    assert_eq!(
        first.matches[0].node_kind.as_deref(),
        Some("call_expression")
    );
}

#[test]
fn detailed_file_search_explains_prefilter_and_unsupported_files() {
    let root = TmpDir::new();
    fs::write(root.join("a.ts"), "target(value);\n").expect("write a");
    fs::write(root.join("b.ts"), "other(value);\n").expect("write b");
    fs::write(root.join("note.txt"), "target(value);\n").expect("write txt");

    let result = search_files_detailed(StructuralSearchFilesOptions {
        path: root.to_string_lossy().to_string(),
        pattern: Some("target($X)".to_owned()),
        rule: None,
        include: None,
        exclude_dir: None,
        default_exclude_dir: None,
        exclude: None,
        ignore: crate::search::walk::IgnorePolicy::default(),
        max_depth: None,
        max_files: Some(10),
        skip_files: None,
        max_file_bytes: None,
    })
    .expect("detailed file search");

    assert_eq!(result.total_matches, 1);
    assert_eq!(result.parsed_files, 1);
    assert_eq!(result.skipped_by_pre_filter, 1);
    assert_eq!(result.skipped_unsupported, 1);
    assert_eq!(result.query.literal_anchor.as_deref(), Some("target"));
    assert!(
        result
            .files
            .iter()
            .any(|file| file.status == "skippedByPreFilter")
    );
    assert!(result.files.iter().any(|file| file.status == "unsupported"));
}

// ── single-content size cap (defense-in-depth on the public entry points) ──
//
// The file walker bounds content at `max_file_bytes`; the single-content
// `search`/`search_detailed` entry points did NOT, so a multi-MB blob passed
// straight to a public entry point could hang in tree-sitter parsing +
// `match_multi_capture` backtracking with no timeoutMs escape. The cap is
// the engine's own backstop — callers defer caps to backends, so the contract
// is satisfied by enforcing one here, mirroring `max_file_bytes`.

fn content_of_at_least(byte_len: usize) -> String {
    // Build valid AST source then pad with line comments to >= byte_len.
    // Padding with `// x` lines keeps the TS grammar happy so the cap — not
    // a parse error — is what trips for oversize fixtures.
    let line = "target(v);\n";
    let mut out = String::from(line);
    while out.len() < byte_len {
        out.push_str("// padding\n");
    }
    out
}

fn content_of_exactly(byte_len: usize) -> String {
    // Exercise the byte boundary with a small AST. Repeating 100,000 calls
    // also measures parser/walker throughput and can legitimately hit the
    // independent execution deadline when the suite runs under CPU load.
    let prefix = "target(v);\n/*";
    let suffix = "*/";
    assert!(byte_len >= prefix.len() + suffix.len());
    let mut out = String::with_capacity(byte_len);
    out.push_str(prefix);
    out.extend(std::iter::repeat_n(
        'x',
        byte_len - prefix.len() - suffix.len(),
    ));
    out.push_str(suffix);
    out
}

#[test]
fn search_rejects_oversize_content() {
    const CAP: usize = crate::signatures::MAX_PARSE_SIZE;
    let content = content_of_at_least(CAP + 1);
    assert!(content.len() > CAP, "fixture must be over the cap");
    let err = match search(&content, "ts", Some("target($X)"), None) {
        Ok(_) => panic!("oversize content must error, not hang"),
        Err(err) => err,
    };
    assert!(
        err.contains("exceeds"),
        "error must explain the cap: got {err:?}"
    );
    assert!(
        err.contains(&CAP.to_string()),
        "error must name the byte limit: got {err:?}"
    );
}

#[test]
fn search_accepts_content_at_cap() {
    const CAP: usize = crate::signatures::MAX_PARSE_SIZE;
    let content = content_of_exactly(CAP);
    assert_eq!(content.len(), CAP, "fixture must be exactly at the cap");
    let matches = search(&content, "ts", Some("target($X)"), None)
        .expect("content at the cap must parse, not error");
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].metavars["X"], ["v"]);
}

#[test]
fn search_detailed_reports_oversize_as_truncated() {
    const CAP: usize = crate::signatures::MAX_PARSE_SIZE;
    let content = content_of_at_least(CAP + 1);
    assert!(content.len() > CAP, "fixture must be over the cap");
    let result = evaluate_source(&content, "a.ts", "ts", Some("target($X)"), None);
    assert_eq!(
        result.status, "truncated",
        "oversize single content is a size truncation, not a parse failure"
    );
    assert!(result.matches.is_empty());
    let diag = result
        .diagnostics
        .iter()
        .find(|d| d.code == "structural.content.tooLarge")
        .expect("a tooLarge diagnostic must explain the cap");
    assert!(diag.message.contains(&CAP.to_string()));
    assert_eq!(diag.recovery, None, "the runtime words it by code");
}

#[test]
fn search_detailed_accepts_content_at_cap() {
    const CAP: usize = crate::signatures::MAX_PARSE_SIZE;
    let content = content_of_exactly(CAP);
    assert_eq!(content.len(), CAP, "fixture must be exactly at the cap");
    let result = evaluate_source(&content, "a.ts", "ts", Some("target($X)"), None);
    assert_eq!(result.status, "ok");
    assert_eq!(result.matches.len(), 1);
    assert!(result.diagnostics.is_empty());
}

/// Every demand finds the same matches in the same order: a count keeps
/// only the count, a price keeps one price per match (each match priced
/// once), and [`file_matches`] rebuilds a priced file's matches exactly
/// until the file changes.
#[test]
fn every_demand_counts_the_same_matches_and_a_priced_file_rebuilds_them() {
    let root = TmpDir::new();
    fs::write(
        root.join("a.rs"),
        "fn a() { let x = f(g(1), h(2)); }\nfn b() { t(u(v(3))); }\n",
    )
    .unwrap();
    fs::write(root.join("b.rs"), "fn c() { w(4); }\n").unwrap();
    let options = || StructuralSearchFilesOptions {
        pattern: Some("$A".to_owned()),
        ..scan_options(&root, 0)
    };
    let scan = |demand| {
        search_files(options(), demand, &|_| true, &|_| "rs".to_owned(), &|| {
            false
        })
        .expect("scan")
    };
    let held = scan(StructuralDemand::Matches);
    let counted = scan(StructuralDemand::Count);
    let priced_calls = std::sync::atomic::AtomicUsize::new(0);
    let price = |m: &StructuralDetailedMatch| {
        priced_calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        u32::try_from(m.text.len()).unwrap()
    };
    let priced = scan(StructuralDemand::Priced(&price));
    assert!(held.total_matches > 10, "{}", held.total_matches);
    assert_eq!(counted.total_matches, held.total_matches);
    assert_eq!(priced.total_matches, held.total_matches);
    assert_eq!(
        priced_calls.load(std::sync::atomic::Ordering::Relaxed),
        held.total_matches as usize
    );
    for ((held, counted), priced) in held.files.iter().zip(&counted.files).zip(&priced.files) {
        assert!(matches!(counted.matches, FileMatches::Counted(_)));
        assert!(counted.matches.held().is_empty());
        assert_eq!(counted.matches.len(), held.matches.len());
        let FileMatches::Priced(prices) = &priced.matches else {
            panic!("priced demand keeps prices");
        };
        let lengths = held
            .matches
            .held()
            .iter()
            .map(|m| u32::try_from(m.text.len()).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(prices, &lengths, "{}", held.path);
        let digest = priced.source_sha256.as_deref().expect("digest");
        let rebuilt = file_matches(
            std::path::Path::new(&priced.path),
            "rs",
            Some("$A"),
            None,
            digest,
            crate::signatures::MAX_PARSE_SIZE as u64,
        )
        .expect("rebuild");
        let super::Rebuilt::Matches(rebuilt) = rebuilt else {
            panic!("unchanged file: {}", held.path);
        };
        let spans = |matches: &[StructuralDetailedMatch]| {
            matches
                .iter()
                .map(|m| {
                    (
                        m.start_line,
                        m.start_col,
                        m.end_line,
                        m.end_col,
                        m.text.clone(),
                    )
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(spans(&rebuilt), spans(held.matches.held()), "{}", held.path);
    }
    let changed = &priced.files[0];
    fs::write(&changed.path, "fn z() {}\n").unwrap();
    assert!(
        matches!(
            file_matches(
                std::path::Path::new(&changed.path),
                "rs",
                Some("$A"),
                None,
                changed.source_sha256.as_deref().expect("digest"),
                crate::signatures::MAX_PARSE_SIZE as u64,
            )
            .expect("rebuild"),
            super::Rebuilt::Changed
        ),
        "a changed file rebuilds nothing"
    );
}

/// Rebuilding a file whose bytes still hash to the scan's gives the scan's
/// matches, or, when matching them did not finish this time (an execution
/// limit), an incomplete rebuild with the limit's diagnostics: never a
/// changed file.
#[test]
fn a_rebuild_cut_by_a_limit_is_incomplete_not_changed() {
    let oversize = "x".repeat(MAX_STRUCTURAL_CONTENT_BYTES + 1);
    let cut = evaluate_source(&oversize, "big.rs", "rs", Some("$A"), None);
    assert_eq!(cut.status, "truncated");
    match files::Rebuilt::of(cut) {
        Rebuilt::Incomplete(diagnostics) => {
            assert_eq!(diagnostics[0].code, "structural.content.tooLarge");
        }
        Rebuilt::Matches(_) | Rebuilt::Changed => panic!("a cut rebuild is incomplete"),
    }
    let whole = evaluate_source("fn a() {}\n", "a.rs", "rs", Some("fn $N() {}"), None);
    assert!(matches!(files::Rebuilt::of(whole), Rebuilt::Matches(matches) if matches.len() == 1));
}
