use super::*;
use crate::test_support::TmpDir;
use std::fs;

#[test]
fn public_structural_errors_retain_native_codes() {
    for (pattern, rule, expected) in [
        (Some("fn ???"), None, "structural.query.compileFailed"),
        (
            None,
            Some("kind: not_a_real_kind"),
            "structural.query.compileFailed",
        ),
        (None, Some("kind: ["), "structural.rule.invalid"),
        (None, Some("kindx: call"), "structural.rule.invalid"),
        (None, None, "structural.query.invalid"),
    ] {
        let error = search("fn main() {}", "rs", pattern, rule)
            .err()
            .expect("invalid query");
        assert!(error.starts_with(&format!("[{expected}] ")), "{error}");
    }
    let unsupported = search("echo hi", "sh", Some("$$$"), None)
        .err()
        .expect("unsupported language");
    assert!(unsupported.starts_with("[structural.language.unsupported] "));

    let root = TmpDir::new();
    fs::write(root.join("fixture.rs"), "fn main() {}").unwrap();
    let result = search_files_detailed(review_file_options(&root, "kind: not_a_real_kind", 10))
        .expect("typed failure");
    assert_eq!(result.status, "parserFailed");
    assert!(diagnostic_codes(&result).contains(&"structural.query.compileFailed"));
}

/// A rule that does not parse as a rule (malformed YAML, an unknown key, no
/// matcher, a bad regex) is `structural.rule.invalid`, apart from a rule or
/// pattern this grammar cannot compile (`structural.query.compileFailed`),
/// in a single-content search and in a file scan alike.
#[test]
fn rule_parse_errors_have_their_own_code() {
    let root = TmpDir::new();
    fs::write(root.join("fixture.rs"), "fn main() {}").unwrap();
    for (rule, expected) in [
        ("kind: [", "structural.rule.invalid"),
        ("kindx: function_item", "structural.rule.invalid"),
        ("has:\n  field: name\n", "structural.rule.invalid"),
        ("stopBy: end", "structural.rule.invalid"),
        ("regex: '('", "structural.rule.invalid"),
        ("kind: not_a_real_kind", "structural.query.compileFailed"),
        ("pattern: 'fn ???'", "structural.query.compileFailed"),
    ] {
        let single = evaluate_source("fn main() {}", "fixture.rs", "rs", None, Some(rule));
        assert_eq!(single.status, "parserFailed", "{rule}");
        assert_eq!(single.diagnostics.len(), 1, "{rule}");
        assert_eq!(single.diagnostics[0].code, expected, "{rule}");
        let scan = search_files_detailed(review_file_options(&root, rule, 10)).expect("scan");
        assert_eq!(scan.status, "parserFailed", "{rule}");
        assert!(diagnostic_codes(&scan).contains(&expected), "{rule}");
    }
}

#[test]
fn directory_patterns_and_composed_rules_share_fragment_context() {
    let root = TmpDir::new();
    for (ext, source, pattern) in [
        (
            "java",
            "class Demo { void run() { target(value); } }",
            "target($X)",
        ),
        ("css", ".demo { color: value; }", "color: $VALUE"),
    ] {
        if languages::find_entry(ext).is_none() {
            continue;
        }
        fs::write(root.join(format!("fixture.{ext}")), source).unwrap();
        let rule = format!("all:\n  - pattern: '{pattern}'\n  - regex: '.'");
        for use_rule in [true, false] {
            let mut options = review_file_options(&root, &rule, 10);
            options.include = Some(vec![format!("*.{ext}")]);
            if !use_rule {
                options.rule = None;
                options.pattern = Some(pattern.to_owned());
            }
            let result = search_files_detailed(options).expect("fragment directory search");
            assert_eq!(result.status, "ok");
            assert_eq!(result.total_matches, 1, "{ext}: rule={use_rule}");
        }
    }
}

#[test]
fn ast_audit_unanchored_directory_reports_unsupported_files() {
    let root = TmpDir::new();
    fs::write(root.join("unsupported.sh"), "echo hello\n").expect("shell fixture");
    let mut options = review_file_options(&root, "kind: call_expression", 10);
    options.pattern = Some("$$$".to_owned());
    options.rule = None;
    options.include = Some(vec!["*.sh".to_owned()]);
    let result = search_files_detailed(options).expect("directory search");
    assert_eq!(result.status, "unsupported");
    assert_eq!(result.skipped_unsupported, 1);
    assert_eq!(result.total_matches, 0);
    let query = &result.query;
    assert_eq!(query.kind, "pattern");
    assert_eq!(query.pre_filter, "disabled");
    assert!(diagnostic_codes(&result).contains(&"structural.language.unsupported"));
}

#[test]
fn structural_review_deep_document_keeps_recall() {
    let source = format!("const x = {}probe(){};", "[".repeat(600), "]".repeat(600));
    let result = evaluate_source(&source, "deep.ts", "ts", Some("probe()"), None);
    assert_eq!(result.status, "ok");
    assert_eq!(result.matches.len(), 1);
}

#[test]
fn structural_review_rule_prefilter_preserves_directory_recall() {
    let root = TmpDir::new();
    let source = "function present() { return 1; }";
    fs::write(root.join("fixture.ts"), source).expect("fixture");
    for rule in [
        "kind: function_declaration\nnot:\n  pattern: absent_probe($X)",
        "any:\n  - kind: function_declaration\n  - pattern: absent_probe($X)",
        "any: [{pattern: $X}, {pattern: absent_probe($X)}]",
        "all:\n  - kind: function_declaration\n  - not: {pattern: absent_probe($X)}",
    ] {
        let direct = search(source, "ts", None, Some(rule)).expect("direct search");
        let directory = search_files_detailed(StructuralSearchFilesOptions {
            path: root.to_string_lossy().into_owned(),
            pattern: None,
            rule: Some(rule.to_owned()),
            include: None,
            exclude: None,
            exclude_dir: None,
            default_exclude_dir: None,
            ignore: crate::search::walk::IgnorePolicy::default(),
            max_depth: None,
            max_files: None,
            skip_files: None,
            max_file_bytes: None,
        })
        .expect("directory search");
        assert!(!direct.is_empty(), "fixture must match {rule}");
        assert_eq!(directory.total_matches as usize, direct.len(), "{rule}");
    }
}

/// Diagnostic codes of a directory search, its own and each file's.
fn diagnostic_codes(result: &StructuralSearchFilesDetailedResult) -> Vec<&str> {
    result
        .diagnostics
        .iter()
        .chain(result.files.iter().flat_map(|file| &file.diagnostics))
        .map(|diagnostic| diagnostic.code.as_str())
        .collect()
}

/// Paths of the files a directory search matched in.
fn matched_paths(result: &StructuralSearchFilesDetailedResult) -> Vec<&str> {
    result
        .files
        .iter()
        .filter(|file| !file.matches.is_empty())
        .map(|file| file.path.as_str())
        .collect()
}

fn run_pattern(src: &str, ext: &str, pattern: &str) -> Vec<StructuralMatch> {
    search(src, ext, Some(pattern), None).expect("pattern search should succeed")
}

#[test]
fn finds_call_and_captures_single_metavar() {
    let src = "const a = foo(bar);\nconst b = foo(baz);\n";
    let matches = run_pattern(src, "ts", "foo($X)");
    assert_eq!(matches.len(), 2);
    assert_eq!(matches[0].start_line, 1);
    assert_eq!(
        matches[0].metavars.get("X").map(Vec::as_slice),
        Some(&["bar".to_string()][..])
    );
    assert_eq!(
        matches[1].metavars.get("X").map(Vec::as_slice),
        Some(&["baz".to_string()][..])
    );
}

/// Trailing punctuation
/// the pattern omits (a trailing argument comma, a statement terminator) is
/// not required of the candidate, as in ast-grep.
#[test]
fn pattern_search_preserves_optional_trailing_punctuation() {
    let src = "throw new Error(\"one\");\nthrow new Error(\n  `two ${x}`,\n);\nexport const answer = 42;\n";
    let errors = run_pattern(src, "ts", "new Error($MSG)");
    assert_eq!(
        errors.iter().map(|m| m.start_line).collect::<Vec<_>>(),
        [1, 2]
    );
    assert_eq!(
        errors[1].metavars.get("MSG").map(Vec::as_slice),
        Some(&["`two ${x}`".to_owned()][..])
    );
    let exports = run_pattern(src, "ts", "export const $N = $V");
    assert_eq!(exports.len(), 1);
    assert_eq!(exports[0].start_line, 5);
    assert_eq!(
        exports[0].metavars.get("N").map(Vec::as_slice),
        Some(&["answer".to_owned()][..])
    );
}

#[test]
fn captures_multi_metavar_as_list() {
    let src = "log(1, 2, 3);\n";
    let matches = run_pattern(src, "js", "log($$$ARGS)");
    assert_eq!(matches.len(), 1);
    // Multi-captures preserve punctuation so callers can reconstruct
    // argument lists without guessing where separators were.
    assert_eq!(
        matches[0].metavars.get("ARGS").map(Vec::as_slice),
        Some(
            &[
                "1".to_string(),
                ",".to_string(),
                "2".to_string(),
                ",".to_string(),
                "3".to_string()
            ][..]
        )
    );
}

#[test]
fn document_probe_matches_root_without_ellipsis_panic() {
    for ext in ["ts", "py"] {
        let matches = run_pattern("foo(a)\nbar(b)\n", ext, "$$$");
        assert_eq!(matches.len(), 1, "{ext} should return the document root");
        assert_eq!(matches[0].start_line, 1);
        assert!(!matches[0].text.is_empty());
        assert!(matches[0].metavars.is_empty());
    }
}

#[test]
fn comment_and_string_immunity() {
    // KPI #1: a literal `eval(x)` inside a comment and inside a string must
    // NOT match — only the real call site (line 3) does.
    let src = "// eval(evil)\nconst s = \"eval(evil)\";\neval(real);\n";
    let matches = run_pattern(src, "js", "eval($X)");
    assert_eq!(matches.len(), 1, "only the real call site matches");
    assert_eq!(matches[0].start_line, 3);
    assert_eq!(
        matches[0].metavars.get("X").map(Vec::as_slice),
        Some(&["real".to_string()][..])
    );
}

#[test]
fn python_pattern_with_expando_char() {
    // Python's expando char is µ, not $ — exercises pre_process_pattern.
    let src = "print(hello)\nprint(world)\n";
    let matches = run_pattern(src, "py", "print($X)");
    assert_eq!(matches.len(), 2);
    assert_eq!(
        matches[0].metavars.get("X").map(Vec::as_slice),
        Some(&["hello".to_string()][..])
    );
}

#[test]
fn rust_pattern_with_expando_char() {
    let src = "fn main() {\n    println(a);\n    println(b);\n}\n";
    let matches = run_pattern(src, "rs", "println($X)");
    assert_eq!(matches.len(), 2);
}

#[cfg(feature = "tree-sitter-cuda")]
#[test]
fn cuda_pattern_matches_kernel_launch_and_captures_configuration() {
    let src = "void launch(int *values) { kernel<<<1, 32>>>(values); }\n";
    let matches = run_pattern(
        src,
        "cu",
        "void launch(int *$ARG) { kernel<<<$GRID, $BLOCK>>>($ARG); }",
    );
    assert_eq!(matches.len(), 1);
    for (name, expected) in [("GRID", "1"), ("BLOCK", "32"), ("ARG", "values")] {
        assert_eq!(
            matches[0].metavars.get(name).map(Vec::as_slice),
            Some(&[expected.to_owned()][..]),
            "{name}"
        );
    }
}

#[cfg(feature = "tree-sitter-asm")]
#[test]
fn assembly_pattern_uses_an_ascii_expando() {
    let src = "target:\n  mov %rax, %rbx\n  mov %rcx, %rdx\n";
    let matches = run_pattern(src, "asm", "mov $SOURCE, $DESTINATION");
    assert_eq!(matches.len(), 2);
    assert_eq!(
        matches[0].metavars.get("SOURCE").map(Vec::as_slice),
        Some(&["%rax".to_owned()][..])
    );
}

#[test]
fn relational_rule_inside_function() {
    // KPI: a rule that plain patterns cannot express — `await` calls that
    // are `inside` a for-loop. `stopBy: end` walks all ancestors.
    let src =
        "async function f() {\n  for (const x of xs) {\n    await g(x);\n  }\n  await h();\n}\n";
    let rule =
        "rule:\n  pattern: await $C\n  inside:\n    kind: for_in_statement\n    stopBy: end\n";
    let matches = search(src, "ts", None, Some(rule)).expect("rule search should succeed");
    assert_eq!(
        matches.len(),
        1,
        "only the await inside the for-loop matches"
    );
    assert_eq!(matches[0].start_line, 3);
}

#[test]
fn unsupported_extension_errors() {
    match search("x", "zzz", Some("foo()"), None) {
        Err(e) => assert!(e.contains("does not support")),
        Ok(_) => panic!("expected an unsupported-extension error"),
    }
}

#[test]
fn supported_extensions_are_rust_owned() {
    let exts = languages::supported_extensions();
    assert!(exts.contains(&"ts"));
    assert!(exts.contains(&"rs"));
}

#[test]
fn search_files_finds_matches_and_prefilters_non_matching_files() {
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
    .expect("search files");

    assert_eq!(result.total_matches, 1);
    assert_eq!(matched_paths(&result).len(), 1);
    assert!(matched_paths(&result)[0].ends_with("a.ts"));
    assert_eq!(result.skipped_by_pre_filter, 1);
}

/// A scan reports, per file, whether recovered syntax could hide a match:
/// the file whose method a macro attribute garbled, not the clean one.
#[test]
fn search_files_flags_the_file_whose_recovered_syntax_could_hide_a_match() {
    let root = TmpDir::new();
    fs::write(
        root.join("garbled.cpp"),
        "class lexer {\n    JSON_HEDLEY_NON_NULL(2)\n    token_type scan_literal(const char_type* literal_text, const std::size_t length,\n                            token_type return_type)\n    {\n        JSON_ASSERT(char_traits<char_type>::to_char_type(current) == literal_text[0]);\n        return return_type;\n    }\n};\n",
    )
    .expect("write garbled");
    fs::write(
        root.join("clean.cpp"),
        "int f() { JSON_ASSERT(1); return 2; }\n",
    )
    .expect("write clean");
    let result = search_files_detailed(StructuralSearchFilesOptions {
        path: root.to_string_lossy().to_string(),
        pattern: Some("JSON_ASSERT($$$A)".to_owned()),
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
    .expect("search files");
    let recovered: Vec<_> = result
        .files
        .iter()
        .filter(|file| file.recovered)
        .map(|file| file.path.rsplit('/').next().unwrap_or(&file.path))
        .collect();
    assert_eq!(recovered, ["garbled.cpp"]);
}

#[test]
fn search_files_errors_on_nonexistent_root_for_every_prefilter_branch() {
    let root = TmpDir::new();
    let missing = root.join("nope");

    // Literal-anchored pattern (`target(` anchor → text prefilter branch),
    // no-anchor pattern (`$FN($$$ARGS)` → walker branch), and a rule. A
    // nonexistent root must be a LOUD error on all of them — the text search yields
    // zero candidates from a missing root without complaining.
    for (pattern, rule) in [
        (Some("target($X)".to_owned()), None),
        (Some("$FN($$$ARGS)".to_owned()), None),
        (None, Some("rule:\n  pattern: target($X)".to_owned())),
    ] {
        let result = search_files_detailed(StructuralSearchFilesOptions {
            path: missing.to_string_lossy().to_string(),
            pattern,
            rule,
            include: None,
            exclude_dir: None,
            default_exclude_dir: None,
            exclude: None,
            ignore: crate::search::walk::IgnorePolicy::default(),
            max_depth: None,
            max_files: Some(10),
            skip_files: None,
            max_file_bytes: None,
        });
        match result {
            Ok(ok) => panic!(
                "nonexistent root must error, not return {} matches",
                ok.total_matches
            ),
            Err(err) => assert!(
                err.reason.contains("Cannot access structural search path"),
                "unexpected error text: {err}"
            ),
        }
    }
}

#[test]
fn search_files_respects_excluded_directories_and_large_file_limit() {
    let root = TmpDir::new();
    fs::create_dir_all(root.join("src")).expect("src");
    fs::create_dir_all(root.join("node_modules/pkg")).expect("node_modules");
    fs::write(root.join("src/a.ts"), "target(v);\n").expect("write a");
    fs::write(root.join("src/large.ts"), "target(value);\n").expect("write large");
    fs::write(root.join("node_modules/pkg/b.ts"), "target(value);\n").expect("write b");

    let result = search_files_detailed(StructuralSearchFilesOptions {
        path: root.to_string_lossy().to_string(),
        pattern: Some("target($X)".to_owned()),
        rule: None,
        include: Some(vec!["*.ts".to_owned()]),
        exclude_dir: Some(vec!["node_modules".to_owned()]),
        default_exclude_dir: None,
        exclude: None,
        ignore: crate::search::walk::IgnorePolicy::default(),
        max_depth: None,
        max_files: Some(10),
        skip_files: None,
        max_file_bytes: Some(14),
    })
    .expect("search files");

    assert_eq!(result.total_matches, 1);
    assert_eq!(result.skipped_large, 1);
    assert_eq!(matched_paths(&result).len(), 1);
    assert!(matched_paths(&result)[0].ends_with("src/a.ts"));
}

#[test]
fn search_files_reports_non_utf8_and_oversized_sources_per_file() {
    let root = TmpDir::new();
    fs::write(root.join("binary.ts"), b"\xfftarget(x);").expect("write binary");
    fs::write(root.join("large.ts"), "target(value);\n").expect("write large");

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
        max_file_bytes: Some(14),
    })
    .expect("search files");

    assert_eq!((result.skipped_unreadable, result.skipped_large), (1, 1));
    assert_eq!(result.status, "truncated");
    let row = |name: &str| {
        result
            .files
            .iter()
            .find(|file| file.path.ends_with(name))
            .expect("file row")
    };
    let binary = row("binary.ts");
    assert_eq!(
        (binary.status.as_str(), binary.skipped_reason.as_deref()),
        ("unreadable", Some("read"))
    );
    assert_eq!(
        binary.diagnostics[0].message,
        "Could not read file content as UTF-8: stream did not contain valid UTF-8."
    );
    assert_eq!(
        binary.diagnostics[0].path.as_deref(),
        Some(binary.path.as_str())
    );
    let large = row("large.ts");
    assert_eq!(
        (large.status.as_str(), large.skipped_reason.as_deref()),
        ("truncated", Some("maxFileBytes"))
    );
    assert_eq!(
        large.diagnostics[0].message,
        "File is 15 bytes, above the structural search limit of 14 bytes."
    );
    assert_eq!(
        large.diagnostics[0].path.as_deref(),
        Some(large.path.as_str())
    );
}

#[test]
fn search_files_accepts_single_file_root() {
    let root = TmpDir::new();
    let file = root.join("a.ts");
    fs::write(&file, "target(value);\n").expect("write file");

    let result = search_files_detailed(StructuralSearchFilesOptions {
        path: file.to_string_lossy().to_string(),
        pattern: Some("target($X)".to_owned()),
        rule: None,
        include: None,
        exclude_dir: None,
        default_exclude_dir: None,
        exclude: None,
        ignore: crate::search::walk::IgnorePolicy::default(),
        max_depth: None,
        max_files: None,
        skip_files: None,
        max_file_bytes: None,
    })
    .expect("search file");

    assert_eq!(result.total_matches, 1);
    assert_eq!(matched_paths(&result).len(), 1);
}

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
    let result = evaluate_source("x\n", "a.ts", "ts", Some("foo($X)"), Some("rule: {}"));
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
        Some("kind: not_a_real_node_kind_astro_999"),
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

#[test]
fn both_or_neither_query_errors() {
    assert!(search("x", "ts", Some("a"), Some("b")).is_err());
    assert!(search("x", "ts", None, None).is_err());
}

#[test]
fn invalid_pattern_errors() {
    assert!(search("x", "ts", Some("   "), None).is_err());
}

/// A pattern the grammar cannot parse is an error, not an empty result:
/// tree-sitter recovers `foo(` with a MISSING `)` the pattern never meant.
#[test]
fn unterminated_patterns_are_invalid_not_empty() {
    for (ext, pattern) in [
        ("ts", "foo("),
        ("ts", "if ($A"),
        ("py", "def f("),
        ("rs", "foo("),
    ] {
        let error = search("foo(1);\n", ext, Some(pattern), None)
            .err()
            .unwrap_or_else(|| panic!("{ext} {pattern} must be rejected"));
        assert!(error.contains("syntax errors"), "{ext} {pattern}: {error}");
    }
    // Statement terminators a pattern leaves off stay valid.
    for (ext, pattern) in [
        ("ts", "foo($A)"),
        ("c", "foo($A)"),
        ("go", "foo($A)"),
        ("cs", "Log($A)"),
    ] {
        assert!(
            search("x", ext, Some(pattern), None).is_ok(),
            "{ext} {pattern}"
        );
    }
}

// ── markup / style grammars (HTML/CSS) ────────────────────────────────────

// ── Scala ─────────────────────────────────────────────────────────────────

#[cfg(feature = "tree-sitter-scala")]
#[test]
fn scala_pattern_captures_call_argument() {
    // Expando is µ, so `$X` → `µX`, a valid Scala identifier.
    let src = "object M {\n  def f() = { println(hello); println(world) }\n}\n";
    let matches = run_pattern(src, "scala", "println($X)");
    assert_eq!(matches.len(), 2);
    assert_eq!(
        matches[0].metavars.get("X").map(Vec::as_slice),
        Some(&["hello".to_string()][..])
    );
}

#[cfg(feature = "tree-sitter-scala")]
#[test]
fn scala_comment_and_string_immunity() {
    // KPI #1: a `println(evil)` in a comment and in a string must NOT match.
    let src = "object M {\n  // println(evil)\n  val s = \"println(evil)\"\n  def go() = println(real)\n}\n";
    let matches = run_pattern(src, "scala", "println($X)");
    assert_eq!(matches.len(), 1, "only the real call site matches");
    assert_eq!(
        matches[0].metavars.get("X").map(Vec::as_slice),
        Some(&["real".to_string()][..])
    );
}

#[cfg(feature = "tree-sitter-scala")]
#[test]
fn scala_extensions_are_supported() {
    let exts = languages::supported_extensions();
    for ext in ["scala", "sc", "sbt"] {
        assert!(exts.contains(&ext), "structural search must support .{ext}");
    }
}

// ── First-class extension aliases ─────────────────────────────────────────

#[test]
fn mts_uses_typescript_grammar_and_dollar_expando() {
    // `.mts` must resolve to the TS entry (expando `$`, not the µ fallback).
    let src = "const a = foo(bar);\nconst b = foo(baz);\n";
    let matches = run_pattern(src, "mts", "foo($X)");
    assert_eq!(matches.len(), 2);
    assert_eq!(
        matches[0].metavars.get("X").map(Vec::as_slice),
        Some(&["bar".to_string()][..])
    );
}

#[test]
fn retained_alias_extensions_are_supported() {
    let exts = languages::supported_extensions();
    for ext in ["mts", "cts", "pyi"] {
        assert!(exts.contains(&ext), "structural search must support .{ext}");
    }
}

// ── native walker: recursive globs (#6), ignore semantics (#7), rule
//    prefilter (#9) ─────────────────────────────────────────────────────

#[test]
fn search_files_supports_recursive_glob_includes() {
    let root = TmpDir::new();
    fs::create_dir_all(root.join("src/nested")).expect("nested dir");
    fs::write(root.join("src/a.ts"), "target(v);\n").expect("a");
    fs::write(root.join("src/nested/b.ts"), "target(v);\n").expect("b");
    fs::write(root.join("src/c.js"), "target(v);\n").expect("c");

    let result = search_files_detailed(StructuralSearchFilesOptions {
        path: root.to_string_lossy().to_string(),
        pattern: Some("target($X)".to_owned()),
        rule: None,
        include: Some(vec!["src/**/*.ts".to_owned()]),
        exclude_dir: None,
        default_exclude_dir: None,
        exclude: None,
        ignore: crate::search::walk::IgnorePolicy::default(),
        max_depth: None,
        max_files: Some(50),
        skip_files: None,
        max_file_bytes: None,
    })
    .expect("glob search");

    assert_eq!(
        matched_paths(&result).len(),
        2,
        "both nested .ts match; .js excluded"
    );
    assert!(
        matched_paths(&result)
            .iter()
            .all(|path| path.ends_with(".ts"))
    );
}

#[test]
fn search_files_honors_dot_ignore_files() {
    let root = TmpDir::new();
    fs::create_dir_all(root.join("skip")).expect("skip dir");
    fs::write(root.join(".ignore"), "skip/\n").expect("ignore file");
    fs::write(root.join("keep.ts"), "target(v);\n").expect("keep");
    fs::write(root.join("skip/x.ts"), "target(v);\n").expect("skipped");

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
        max_files: Some(50),
        skip_files: None,
        max_file_bytes: None,
    })
    .expect("ignore search");

    assert_eq!(matched_paths(&result).len(), 1, ".ignore skips skip/");
    assert!(matched_paths(&result)[0].ends_with("keep.ts"));
}

#[test]
fn search_files_prefilters_rule_by_inner_pattern() {
    let root = TmpDir::new();
    fs::write(
        root.join("has.ts"),
        "async function f() {\n  await g();\n}\n",
    )
    .expect("has");
    fs::write(root.join("none.ts"), "function f() {\n  return 1;\n}\n").expect("none");

    let result = search_files_detailed(StructuralSearchFilesOptions {
        path: root.to_string_lossy().to_string(),
        pattern: None,
        rule: Some("rule:\n  pattern: await $C\n".to_owned()),
        include: None,
        exclude_dir: None,
        default_exclude_dir: None,
        exclude: None,
        ignore: crate::search::walk::IgnorePolicy::default(),
        max_depth: None,
        max_files: Some(50),
        skip_files: None,
        max_file_bytes: None,
    })
    .expect("rule search");

    // Anchor "await" lets none.ts skip parsing entirely.
    assert_eq!(result.skipped_by_pre_filter, 1);
    assert_eq!(matched_paths(&result).len(), 1);
    assert!(matched_paths(&result)[0].ends_with("has.ts"));
}

#[test]
fn search_files_operator_only_pattern_parses_every_candidate() {
    let root = TmpDir::new();
    fs::write(root.join("match.js"), "foo && foo();\n").expect("match");
    fs::write(root.join("nomatch.js"), "foo || foo();\n").expect("nomatch");

    let result = search_files_detailed(StructuralSearchFilesOptions {
        path: root.to_string_lossy().to_string(),
        pattern: Some("$A && $A()".to_owned()),
        rule: None,
        include: None,
        exclude_dir: None,
        default_exclude_dir: None,
        exclude: None,
        ignore: crate::search::walk::IgnorePolicy::default(),
        max_depth: None,
        max_files: Some(50),
        skip_files: None,
        max_file_bytes: None,
    })
    .expect("operator anchor search");

    assert_eq!(result.skipped_by_pre_filter, 0);
    assert_eq!(matched_paths(&result).len(), 1);
    assert!(matched_paths(&result)[0].ends_with("match.js"));
}

#[test]
fn operator_spacing_does_not_split_directory_and_file_results() {
    let root = TmpDir::new();
    let source = "if (a && !b) { run(); }\nx = y - 1;\n";
    fs::write(root.join("spaced.js"), source).expect("spaced");
    for pattern in ["$A&&!$B", "$A && !$B", "$A=$B-1"] {
        let file_matches = search(source, "js", Some(pattern), None).expect("file search");
        let result = search_files_detailed(StructuralSearchFilesOptions {
            path: root.to_string_lossy().to_string(),
            pattern: Some(pattern.to_owned()),
            rule: None,
            include: None,
            exclude_dir: None,
            default_exclude_dir: None,
            exclude: None,
            ignore: crate::search::walk::IgnorePolicy::default(),
            max_depth: None,
            max_files: Some(50),
            skip_files: None,
            max_file_bytes: None,
        })
        .expect("directory search");
        assert_eq!(result.skipped_by_pre_filter, 0, "{pattern}");
        let dir_count = result.files.iter().map(|f| f.matches.len()).sum::<usize>();
        assert_eq!(dir_count, file_matches.len(), "{pattern}");
    }
    assert_eq!(
        search(source, "js", Some("$A&&!$B"), None)
            .expect("file")
            .len(),
        1
    );
}

// ── prefilter vs unsupported conflation (evidence: proof vs unevaluated) ─
//
// A `.txt` file that textually contains the anchor is not "anchor-absent"
// (proof of no match) — it's "unsupported extension" (not evaluated).
// `search_files` must report them on separate counters so the warning text
// can't collapse a proof-skip into an unevaluated-skip, the exact
// anti-pattern the evidence-grade contract forbids.

// ── scope parity: exclude / hidden / no_ignore / max_depth ───────────────
// Local search defines `exclude`/`hidden`/`noIgnore`/`maxDepth`; the
// structural lane must honor them like the text/regex lane.

fn write_scope_fixture(root: &std::path::Path) {
    fs::write(root.join("match.ts"), "target(value);\n").expect("match");
    fs::write(root.join("excluded.ts"), "target(value);\n").expect("excluded");
    fs::write(root.join(".hidden.ts"), "target(value);\n").expect("hidden");
    fs::create_dir_all(root.join("nested")).expect("nested");
    fs::write(root.join("nested/deep.ts"), "target(value);\n").expect("deep");
    // A .gitignore that excludes gitignored.ts — proves `no_ignore` unlocks it.
    fs::write(root.join(".gitignore"), "gitignored.ts\n").expect("gitignore");
    fs::write(root.join("gitignored.ts"), "target(value);\n").expect("gitignored");
}

fn scope_result_paths(options: StructuralSearchFilesOptions) -> Vec<String> {
    let result = search_files_detailed(options).expect("scope search");
    let mut paths: Vec<String> = matched_paths(&result)
        .into_iter()
        .map(|path| {
            std::path::Path::new(path)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        })
        .collect();
    paths.sort();
    paths
}

/// Each scope option changes which files a kind rule parses. `rule:kind`
/// has no literal anchor, so every supported .ts is parsed and the option
/// under test is the only thing that adds or drops a file.
#[test]
fn structural_files_honor_scope_options() {
    type Scope = fn(&mut StructuralSearchFilesOptions);
    let cases: [(&str, Scope, &[&str], &[&str]); 4] = [
        (
            "exclude glob drops excluded.ts",
            |o| o.exclude = Some(vec!["excluded.ts".to_owned()]),
            &["match.ts"],
            &["excluded.ts"],
        ),
        // Default (hidden:None) ignores dot-files.
        (
            "hidden:Some(true) includes .hidden.ts",
            |o| o.ignore.include_hidden = true,
            &[".hidden.ts"],
            &[],
        ),
        // .gitignore excludes gitignored.ts.
        (
            "no_ignore:Some(true) includes the .gitignored file",
            |o| o.ignore.include_ignored = true,
            &["gitignored.ts"],
            &[],
        ),
        // max_depth:1 = root only.
        (
            "max_depth:1 does not descend into nested/",
            |o| o.max_depth = Some(1),
            &["match.ts"],
            &["deep.ts"],
        ),
    ];
    for (case, scope, present, absent) in cases {
        let root = TmpDir::new();
        write_scope_fixture(&root);
        let mut options = StructuralSearchFilesOptions {
            path: root.to_string_lossy().to_string(),
            rule: Some("rule:\n  kind: call_expression\n".to_owned()),
            pattern: None,
            include: None,
            exclude: None,
            exclude_dir: None,
            default_exclude_dir: None,
            ignore: crate::search::walk::IgnorePolicy::default(),
            max_depth: None,
            max_files: Some(50),
            skip_files: None,
            max_file_bytes: None,
        };
        scope(&mut options);
        let paths = scope_result_paths(options);
        for name in present {
            assert!(
                paths.iter().any(|p| p == name),
                "{case}: {name} present: got {paths:?}"
            );
        }
        for name in absent {
            assert!(
                !paths.iter().any(|p| p == name),
                "{case}: {name} absent: got {paths:?}"
            );
        }
    }
}

#[test]
fn structural_anchor_prefilter_prunes_beyond_max_depth_before_search() {
    let root = TmpDir::new();
    fs::create_dir_all(root.join("nested")).expect("nested");
    fs::write(root.join("match.ts"), "target(value);\n").expect("match");
    fs::write(root.join("nested/noanchor.ts"), "other(value);\n").expect("deep");

    let result = search_files_detailed(StructuralSearchFilesOptions {
        path: root.to_string_lossy().to_string(),
        pattern: Some("target($X)".to_owned()),
        rule: None,
        include: None,
        exclude_dir: None,
        default_exclude_dir: None,
        exclude: None,
        ignore: crate::search::walk::IgnorePolicy::default(),
        // Structural native depth counts the root as 0 and root files as 1.
        max_depth: Some(1),
        max_files: Some(10),
        skip_files: None,
        max_file_bytes: None,
    })
    .expect("structural search");

    assert_eq!(result.parsed_files, 1);
    assert_eq!(
        result.skipped_by_pre_filter, 0,
        "the anchor prefilter must not scan the nested file before depth filtering"
    );
}

/// The anchor prefilter reads a candidate once: a hit's bytes feed the
/// parse, and a file over the parse cap is streamed, never held. Each skip
/// keeps the reason and text the separate prefilter and parse reads gave.
#[test]
fn search_files_prefilter_read_feeds_the_parse_and_keeps_skip_reasons() {
    let root = TmpDir::new();
    fs::write(root.join("a.ts"), "target(value);\n").expect("hit");
    // Over the cap: the anchor sits past aho-corasick's stream buffer.
    let filler = "// filler line for the stream search\n".repeat(8_000);
    fs::write(root.join("big_hit.ts"), format!("{filler}target(value);\n")).expect("big hit");
    fs::write(root.join("big_miss.ts"), &filler).expect("big miss");
    fs::write(root.join("bad.ts"), b"target(\xff);\n").expect("not utf8");
    let options = |max_file_bytes| StructuralSearchFilesOptions {
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
        max_file_bytes,
    };
    let result = search_files_detailed(options(Some(1_000))).expect("search files");
    assert_eq!(result.total_matches, 1);
    assert_eq!(matched_paths(&result).len(), 1);
    assert_eq!(
        result.skipped_by_pre_filter, 1,
        "big_miss.ts lacks the anchor"
    );
    assert_eq!(result.skipped_large, 1, "big_hit.ts holds it past the cap");
    assert_eq!(result.skipped_unreadable, 1, "bad.ts is not UTF-8");
    let row = |name: &str| {
        result
            .files
            .iter()
            .find(|file| file.path.ends_with(name))
            .expect("file row")
    };
    let big = fs::metadata(root.join("big_hit.ts")).expect("meta").len();
    assert_eq!(
        row("big_hit.ts").skipped_reason.as_deref(),
        Some("maxFileBytes")
    );
    assert!(
        row("big_hit.ts").diagnostics[0]
            .message
            .starts_with(&format!("File is {big} bytes")),
        "{}",
        row("big_hit.ts").diagnostics[0].message
    );
    assert_eq!(
        row("big_miss.ts").skipped_reason.as_deref(),
        Some("preFilter")
    );
    let utf8 = fs::read_to_string(root.join("bad.ts")).expect_err("invalid UTF-8");
    assert_eq!(row("bad.ts").skipped_reason.as_deref(), Some("read"));
    assert_eq!(
        row("bad.ts").diagnostics[0].message,
        format!("Could not read file content as UTF-8: {utf8}.")
    );
    // Under the default cap both big files parse; only big_hit.ts matches.
    let result = search_files_detailed(options(None)).expect("search files");
    assert_eq!(result.total_matches, 2);
    assert_eq!(result.skipped_large, 0);
    assert_eq!(result.skipped_by_pre_filter, 1);
}

#[test]
fn search_files_separates_unsupported_from_prefilter_skips() {
    let root = TmpDir::new();
    // `match.ts` carries the anchor and matches the pattern.
    fs::write(root.join("match.ts"), "target(value);\n").expect("match");
    // `hasanchor.txt` textually contains the anchor but .txt has no
    // grammar — it must read as unsupported, NOT as a prefilter skip.
    fs::write(root.join("hasanchor.txt"), "target(value);\n").expect("txt");
    // `noanchor.ts` lacks the anchor — a genuine prefilter (proof) skip.
    fs::write(root.join("noanchor.ts"), "other(value);\n").expect("noanchor");

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
    .expect("search files");

    assert_eq!(result.total_matches, 1);
    assert_eq!(result.parsed_files, 1);
    assert_eq!(
        result.skipped_by_pre_filter, 1,
        "only noanchor.ts is a proof-skip"
    );
    assert_eq!(
        result.skipped_unsupported, 1,
        "hasanchor.txt is unsupported, not prefilter"
    );
    // The warning text must name unsupported files distinctly — the lumped
    // "Pre-filter skipped parsing N file(s)" line is the imprecision we fix.
    let prefilter_warning = result
        .warnings
        .iter()
        .find(|w| w.starts_with("Pre-filter skipped parsing"));
    let unsupported_warning = result
        .warnings
        .iter()
        .find(|w| w.starts_with("Skipped") && w.contains("unsupported"));
    assert!(
        prefilter_warning.is_some(),
        "prefilter warning still present for the genuine proof-skip"
    );
    assert!(
        unsupported_warning.is_some(),
        "unsupported files need their own warning line, not lumped into prefilter: {:?}",
        result.warnings
    );
}

#[test]
fn structural_review_pattern_depth_is_explicitly_incomplete() {
    let source = format!("const x = {}probe(){};", "[".repeat(600), "]".repeat(600));
    let pattern = format!("{}probe(){}", "[".repeat(600), "]".repeat(600));
    let result = evaluate_source(&source, "deep.ts", "ts", Some(&pattern), None);
    assert_eq!(result.status, "truncated");
    assert!(result.matches.is_empty());
    assert_eq!(result.diagnostics[0].code, "structural.match.depthLimit");
}

#[test]
fn structural_review_backtracking_exhaustion_cannot_satisfy_negation() {
    let source = format!("probe({});", vec!["1"; 150].join(","));
    let rule = "kind: call_expression\nnot:\n  pattern: probe($$$A, $$$B, absent)";
    let result = evaluate_source(&source, "wide.ts", "ts", None, Some(rule));
    assert_eq!(result.status, "truncated");
    assert!(result.matches.is_empty());
    assert_eq!(
        result.diagnostics[0].code,
        "structural.match.backtrackingLimit"
    );
    let error = search(&source, "ts", None, Some(rule))
        .err()
        .expect("legacy must fail explicitly");
    assert!(error.starts_with("[structural.match.backtrackingLimit]"));
}

fn review_file_options(
    root: &std::path::Path,
    rule: &str,
    limit: u32,
) -> StructuralSearchFilesOptions {
    StructuralSearchFilesOptions {
        path: root.to_string_lossy().into_owned(),
        pattern: None,
        rule: Some(rule.to_owned()),
        include: None,
        exclude: None,
        exclude_dir: None,
        default_exclude_dir: None,
        ignore: crate::search::walk::IgnorePolicy::default(),
        max_depth: Some(1),
        max_files: Some(limit),
        skip_files: None,
        max_file_bytes: None,
    }
}

#[test]
fn structural_review_scan_cap_and_depth_have_explicit_completion() {
    let root = TmpDir::new();
    fs::create_dir_all(root.join("aaa_nested")).expect("nested");
    fs::write(root.join("aaa_nested/hidden.ts"), "probe();").expect("deep");
    fs::write(root.join("one.ts"), "probe();").expect("first");
    for rule in [
        "kind: call_expression",
        "pattern: probe()",
        "any: [{pattern: probe()}, {pattern: other()}]",
    ] {
        let exact = search_files_detailed(review_file_options(&root, rule, 1)).expect("exact cap");
        assert_eq!(
            exact.total_matches, 1,
            "native depth1 includes only direct files: {rule}"
        );
        assert!(!exact.scan_truncated, "exact cap is complete: {rule}");
        fs::write(root.join("two.ts"), "probe();").expect("second");
        let limited =
            search_files_detailed(review_file_options(&root, rule, 1)).expect("overflow cap");
        assert!(limited.scan_truncated, "overflow is explicit: {rule}");
        assert_eq!(limited.total_matches, 1);
        let detailed =
            search_files_detailed(review_file_options(&root, rule, 1)).expect("detailed overflow");
        assert!(detailed.scan_truncated);
        fs::remove_file(root.join("two.ts")).expect("reset fixture");
    }
}

#[test]
fn structural_review_file_limits_retain_completed_files() {
    let root = TmpDir::new();
    fs::write(root.join("ok.ts"), "probe(1);").expect("complete file");
    fs::write(
        root.join("wide.ts"),
        format!("probe({});", vec!["1"; 150].join(",")),
    )
    .expect("limited file");
    let rule = "kind: call_expression\nnot:\n  pattern: probe($$$A, $$$B, absent)";
    let result =
        search_files_detailed(review_file_options(&root, rule, 10)).expect("partial file search");
    assert_eq!(result.status, "truncated");
    assert_eq!(result.total_matches, 1);
    assert!(matched_paths(&result)[0].ends_with("ok.ts"));
    assert_eq!(result.diagnostics.len(), 1);
    assert_eq!(
        result.diagnostics[0].code,
        "structural.match.backtrackingLimit"
    );
    assert!(
        result.diagnostics[0]
            .path
            .as_deref()
            .expect("path")
            .ends_with("wide.ts")
    );
    let detailed =
        search_files_detailed(review_file_options(&root, rule, 10)).expect("detailed file search");
    assert_eq!(detailed.status, "truncated");
    assert_eq!(detailed.total_matches, 1);
    assert!(detailed.files.iter().any(|file| file.status == "truncated"));
    assert_eq!(detailed.diagnostics.len(), 1);
}

#[test]
fn structural_review_deep_relational_traversal_keeps_recall() {
    let source = format!("const x = {}probe(){};", "[".repeat(600), "]".repeat(600));
    let has = "kind: lexical_declaration\nhas:\n  pattern: probe()\n  stopBy: end";
    assert_eq!(
        search(&source, "ts", None, Some(has))
            .expect("deep has")
            .len(),
        1
    );
    let inside = "pattern: probe()\ninside:\n  kind: lexical_declaration\n  stopBy: end";
    assert_eq!(
        search(&source, "ts", None, Some(inside))
            .expect("deep inside")
            .len(),
        1
    );
}

#[test]
fn structural_review_compile_interruption_stays_typed_in_detailed_search() {
    octo::INTERRUPT_NEXT_COMPILE_PARSE.with(|interrupt| interrupt.set(true));
    let result = evaluate_source("probe();", "fixture.ts", "ts", Some("probe()"), None);
    assert_eq!(result.status, "truncated");
    assert_eq!(result.diagnostics[0].code, "structural.parse.interrupted");
    assert_eq!(result.diagnostics[0].stage, "parse");
}

#[test]
fn structural_review_compile_interruption_keeps_mixed_language_evidence() {
    let root = TmpDir::new();
    fs::write(root.join("one.js"), "probe();").expect("js fixture");
    fs::write(root.join("two.ts"), "probe();").expect("ts fixture");
    let options = || review_file_options(&root, "pattern: probe()", 10);
    octo::INTERRUPT_NEXT_COMPILE_PARSE.with(|interrupt| interrupt.set(true));
    let result = search_files_detailed(options()).expect("partial mixed-language search");
    assert_eq!(result.status, "truncated");
    assert_eq!(result.total_matches, 1);
    assert_eq!(result.diagnostics[0].code, "structural.parse.interrupted");
    assert!(
        !result
            .warnings
            .iter()
            .any(|warning| warning.contains("not valid syntax"))
    );
    octo::INTERRUPT_NEXT_COMPILE_PARSE.with(|interrupt| interrupt.set(true));
    let detailed = search_files_detailed(options()).expect("detailed mixed-language search");
    assert_eq!(detailed.status, "truncated");
    assert_eq!(detailed.total_matches, 1);
    assert_eq!(detailed.diagnostics[0].code, "structural.parse.interrupted");
}

fn scan_options(root: &std::path::Path, skip: u32) -> StructuralSearchFilesOptions {
    StructuralSearchFilesOptions {
        path: root.to_string_lossy().into_owned(),
        pattern: Some("hit($A)".to_owned()),
        rule: None,
        include: None,
        exclude: None,
        exclude_dir: None,
        default_exclude_dir: None,
        ignore: crate::search::walk::IgnorePolicy::default(),
        max_depth: None,
        max_files: None,
        skip_files: Some(skip),
        max_file_bytes: None,
    }
}

fn write_hit_files(root: &std::path::Path, count: usize) {
    for index in 0..count {
        fs::write(root.join(format!("f{index:05}.ts")), "hit(1);\n").unwrap();
    }
}

/// An unset `max_files` has no file cap: a tree past the old 2,000-file
/// default is scanned whole in one call.
#[test]
fn an_unset_file_cap_scans_the_whole_scope() {
    let root = TmpDir::new();
    write_hit_files(&root, 2_100);
    let result = search_files_detailed(scan_options(&root, 0)).expect("scan");
    assert!(!result.scan_truncated);
    assert_eq!(result.scan_resume, None);
    assert_eq!(result.files.len(), 2_100);
}

/// A scan that runs out of its time budget stops at a chunk boundary and
/// names where to resume; walking the resumes reaches every file exactly once.
#[test]
fn a_scan_past_its_time_budget_resumes_exactly_once() {
    let root = TmpDir::new();
    write_hit_files(&root, 1_300);
    let mut seen = std::collections::BTreeSet::new();
    let mut skip = 0;
    let mut calls = 0;
    loop {
        calls += 1;
        let result = files::search_files_with_budget(
            scan_options(&root, skip),
            StructuralDemand::Matches,
            &|_| true,
            &|_| "ts".to_owned(),
            &|| false,
            Some(std::time::Duration::ZERO),
        )
        .expect("scan");
        for file in &result.files {
            assert!(seen.insert(file.path.clone()), "{} seen twice", file.path);
        }
        match result.scan_resume {
            Some(resume) => {
                assert!(result.scan_truncated);
                assert!(resume > skip, "no progress: {resume} <= {skip}");
                skip = resume;
            }
            None => {
                assert!(!result.scan_truncated);
                break;
            }
        }
    }
    assert_eq!(seen.len(), 1_300);
    assert!(calls > 1, "a zero budget must cut the scan");
}

/// A pattern whose root the grammar names by a `name` field is a
/// declaration: its kind, and its name unless that is a metavariable.
/// Expressions and calls are not declarations.
#[test]
fn a_named_root_pattern_is_a_declaration() {
    let rust = pattern_declaration("fn $NAME($$$ARGS) -> $R { $$$BODY }", "rs").expect("fn");
    assert_eq!(rust.kind, "function_item");
    assert_eq!(rust.name, None);
    let named = pattern_declaration("fn run() {}", "rs").expect("named fn");
    assert_eq!(
        named.name,
        Some(("identifier".to_owned(), "run".to_owned()))
    );
    let class = pattern_declaration("class $C { $$$B }", "ts").expect("class");
    assert_eq!(class.kind, "class_declaration");
    assert_eq!(pattern_declaration("$X.unwrap()", "rs"), None);
    assert_eq!(pattern_declaration("foo($A)", "ts"), None);
    assert_eq!(pattern_declaration("fn ???", "rs"), None);
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

/// A walk cut at its cap names the first candidate it left out: it and every
/// later candidate are unread. The cap is an explicit `max_files` or the
/// walk ceiling of an unbounded scan.
#[test]
fn a_capped_file_walk_names_the_first_candidate_it_left_out() {
    let root = TmpDir::new();
    for index in 0..6 {
        fs::write(root.join(format!("f{index}.rs")), "fn a() {}\n").expect("write");
    }
    let options = |max_files| StructuralSearchFilesOptions {
        path: root.to_string_lossy().to_string(),
        pattern: Some("fn $N() {}".to_owned()),
        rule: None,
        include: None,
        exclude_dir: None,
        default_exclude_dir: None,
        exclude: None,
        ignore: crate::search::walk::IgnorePolicy::default(),
        max_depth: None,
        max_files,
        skip_files: None,
        max_file_bytes: None,
    };
    let name = |path: &Option<std::path::PathBuf>| {
        path.as_ref()
            .and_then(|path| path.file_name())
            .map(|name| name.to_string_lossy().into_owned())
    };
    let capped = search_files_detailed(options(Some(4))).expect("search");
    assert!(capped.scan_truncated);
    assert_eq!(capped.files.len(), 4);
    assert_eq!(name(&capped.scan_frontier).as_deref(), Some("f4.rs"));
    let whole = search_files_detailed(options(Some(6))).expect("search");
    assert!(!whole.scan_truncated && whole.scan_frontier.is_none());
    let _ceiling = crate::scan_budget::lower_walk_ceiling(3);
    let unbounded = search_files_detailed(options(None)).expect("search");
    assert_eq!(unbounded.scan_resume, Some(3));
    assert_eq!(name(&unbounded.scan_frontier).as_deref(), Some("f3.rs"));
}

/// Default-excluded names are pruned like the text search prunes them: in a
/// git work tree only where git ignores them, so a tracked `build/` is
/// source the walk reads and an ignored `out/build/` is not.
#[test]
fn default_excludes_skip_only_what_git_ignores() {
    let root = TmpDir::new();
    fs::create_dir_all(root.join(".git")).expect("git dir");
    fs::write(root.join(".gitignore"), "out/\n").expect("ignore");
    for dir in ["build", "out/build", "src"] {
        fs::create_dir_all(root.join(dir)).expect("dir");
        fs::write(root.join(dir).join("a.ts"), "probe();").expect("file");
    }
    let found = |root: &std::path::Path| {
        let mut options = review_file_options(root, "pattern: probe()", 10);
        options.max_depth = None;
        options.default_exclude_dir = Some(vec!["build".to_owned()]);
        let result = search_files_detailed(options).expect("search");
        let prefix = format!("{}/", root.display());
        matched_paths(&result)
            .iter()
            .map(|path| path.trim_start_matches(&prefix).to_owned())
            .collect::<Vec<_>>()
    };
    assert_eq!(found(&root), ["build/a.ts", "src/a.ts"]);
    fs::remove_dir_all(root.join(".git")).expect("leave the repository");
    // Outside a repository a listed name is pruned by name alone.
    assert_eq!(found(&root), ["src/a.ts"]);
}
