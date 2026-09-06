use super::*;
use std::fs;
use std::path::PathBuf;

#[test]
fn public_structural_errors_retain_native_codes() {
    for (pattern, rule, expected) in [
        (Some("fn ???"), None, "structural.query.compileFailed"),
        (
            None,
            Some("kind: not_a_real_kind"),
            "structural.query.compileFailed",
        ),
        (None, Some("kind: ["), "structural.query.compileFailed"),
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

    let root = temp_root("typed_compile_error");
    fs::write(root.join("fixture.rs"), "fn main() {}").unwrap();
    let error = search_files(review_file_options(&root, "kind: not_a_real_kind", 10))
        .err()
        .expect("invalid rule");
    assert!(
        error.starts_with("[structural.query.compileFailed] "),
        "{error}"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn directory_patterns_and_composed_rules_share_fragment_context() {
    let root = temp_root("shared_fragment_context");
    for (ext, source, pattern) in [
        (
            "java",
            "class Demo { void run() { target(value); } }",
            "target($X)",
        ),
        ("css", ".demo { color: value; }", "color: $VALUE"),
        ("scss", ".demo { color: value; }", "color: $VALUE"),
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
            let result = search_files(options).expect("fragment directory search");
            assert_eq!(result.status, "ok");
            assert_eq!(result.total_matches, 1, "{ext}: rule={use_rule}");
        }
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn ast_audit_unanchored_directory_reports_unsupported_files() {
    let root = temp_root("audit_unsupported_unanchored");
    fs::write(root.join("unsupported.sh"), "echo hello\n").expect("shell fixture");
    let mut options = review_file_options(&root, "kind: call_expression", 10);
    options.pattern = Some("$$$".to_owned());
    options.rule = None;
    options.include = Some(vec!["*.sh".to_owned()]);
    let result = search_files(options).expect("directory search");
    assert_eq!(result.status, "partial");
    assert_eq!(result.skipped_unsupported, 1);
    assert_eq!(result.total_matches, 0);
    let query = result
        .query
        .as_ref()
        .expect("async directory result includes its query plan");
    assert_eq!(query.kind, "pattern");
    assert_eq!(query.pre_filter, "disabled");
    assert!(result
        .diagnostics
        .iter()
        .any(|diagnostic| { diagnostic.code == "structural.language.unsupported" }));
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn structural_review_deep_document_keeps_recall() {
    let source = format!("const x = {}probe(){};", "[".repeat(600), "]".repeat(600));
    let result = search_detailed(&source, "deep.ts", "ts", Some("probe()"), None);
    assert_eq!(result.status, "ok");
    assert_eq!(result.matches.len(), 1);
}

#[test]
fn structural_review_rule_prefilter_preserves_directory_recall() {
    let root = temp_root("review_prefilter");
    let source = "function present() { return 1; }";
    fs::write(root.join("fixture.ts"), source).expect("fixture");
    for rule in [
        "kind: function_declaration\nnot:\n  pattern: absent_probe($X)",
        "any:\n  - kind: function_declaration\n  - pattern: absent_probe($X)",
        "any: [{pattern: $X}, {pattern: absent_probe($X)}]",
        "all:\n  - kind: function_declaration\n  - not: {pattern: absent_probe($X)}",
    ] {
        let direct = search(source, "ts", None, Some(rule)).expect("direct search");
        let directory = search_files(StructuralSearchFilesOptions {
            path: root.to_string_lossy().into_owned(),
            pattern: None,
            rule: Some(rule.to_owned()),
            include: None,
            exclude: None,
            exclude_dir: None,
            hidden: None,
            no_ignore: None,
            max_depth: None,
            max_files: None,
            max_file_bytes: None,
        })
        .expect("directory search");
        assert!(!direct.is_empty(), "fixture must match {rule}");
        assert_eq!(directory.total_matches as usize, direct.len(), "{rule}");
    }
    fs::remove_dir_all(root).expect("cleanup");
}

fn temp_root(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "octocode_structural_{}_{}",
        name,
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("create temp root");
    root
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
    for ext in ["ts", "py", "html", "json"] {
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
fn php_pattern_can_start_with_a_variable_capture() {
    let source = "<?php\n$first = 5;\n$second = 6;\n";
    let matches = run_pattern(source, "php", "$NAME = $VALUE;");
    assert_eq!(matches.len(), 2);
    assert_eq!(matches[0].metavars["NAME"], vec!["$first"]);
    assert_eq!(matches[1].metavars["NAME"], vec!["$second"]);
}

#[test]
fn supported_extensions_are_rust_owned() {
    let exts = supported_extensions();
    assert!(exts.iter().any(|ext| ext == "ts"));
    assert!(exts.iter().any(|ext| ext == "rs"));
}

#[test]
fn search_files_finds_matches_and_prefilters_non_matching_files() {
    let root = temp_root("files");
    fs::write(root.join("a.ts"), "target(value);\n").expect("write a");
    fs::write(root.join("b.ts"), "other(value);\n").expect("write b");
    fs::write(root.join("note.txt"), "target(value);\n").expect("write txt");

    let result = search_files(StructuralSearchFilesOptions {
        path: root.to_string_lossy().to_string(),
        pattern: Some("target($X)".to_owned()),
        rule: None,
        include: None,
        exclude_dir: None,
        exclude: None,
        hidden: None,
        no_ignore: None,
        max_depth: None,
        max_files: Some(10),
        max_file_bytes: None,
    })
    .expect("search files");

    assert_eq!(result.total_matches, 1);
    assert_eq!(result.files.len(), 1);
    assert!(result.files[0].path.ends_with("a.ts"));
    assert_eq!(result.skipped_by_pre_filter, 1);
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn search_files_errors_on_nonexistent_root_for_every_prefilter_branch() {
    let root = temp_root("gone");
    fs::remove_dir_all(&root).ok();
    let missing = root.join("nope");

    // Literal-anchored pattern (`target(` anchor → ripgrep prefilter branch),
    // no-anchor pattern (`$FN($$$ARGS)` → walker branch), and a rule. A
    // nonexistent root must be a LOUD error on all of them — the anchored
    // branch used to return Ok(0 matches) because ripgrep yields zero
    // candidates from a missing root without complaining.
    for (pattern, rule) in [
        (Some("target($X)".to_owned()), None),
        (Some("$FN($$$ARGS)".to_owned()), None),
        (None, Some("rule:\n  pattern: target($X)".to_owned())),
    ] {
        let result = search_files(StructuralSearchFilesOptions {
            path: missing.to_string_lossy().to_string(),
            pattern,
            rule,
            include: None,
            exclude_dir: None,
            exclude: None,
            hidden: None,
            no_ignore: None,
            max_depth: None,
            max_files: Some(10),
            max_file_bytes: None,
        });
        match result {
            Ok(ok) => panic!(
                "nonexistent root must error, not return {} matches",
                ok.total_matches
            ),
            Err(err) => assert!(
                err.contains("Cannot access structural search path"),
                "unexpected error text: {err}"
            ),
        }
    }
}

#[test]
fn search_files_respects_excluded_directories_and_large_file_limit() {
    let root = temp_root("filters");
    fs::create_dir_all(root.join("src")).expect("src");
    fs::create_dir_all(root.join("node_modules/pkg")).expect("node_modules");
    fs::write(root.join("src/a.ts"), "target(v);\n").expect("write a");
    fs::write(root.join("src/large.ts"), "target(value);\n").expect("write large");
    fs::write(root.join("node_modules/pkg/b.ts"), "target(value);\n").expect("write b");

    let result = search_files(StructuralSearchFilesOptions {
        path: root.to_string_lossy().to_string(),
        pattern: Some("target($X)".to_owned()),
        rule: None,
        include: Some(vec!["*.ts".to_owned()]),
        exclude_dir: Some(vec!["node_modules".to_owned()]),
        exclude: None,
        hidden: None,
        no_ignore: None,
        max_depth: None,
        max_files: Some(10),
        max_file_bytes: Some(14),
    })
    .expect("search files");

    assert_eq!(result.total_matches, 1);
    assert_eq!(result.skipped_large, 1);
    assert_eq!(result.files.len(), 1);
    assert!(result.files[0].path.ends_with("src/a.ts"));
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn search_files_accepts_single_file_root() {
    let root = temp_root("single");
    let file = root.join("a.ts");
    fs::write(&file, "target(value);\n").expect("write file");

    let result = search_files(StructuralSearchFilesOptions {
        path: file.to_string_lossy().to_string(),
        pattern: Some("target($X)".to_owned()),
        rule: None,
        include: None,
        exclude_dir: None,
        exclude: None,
        hidden: None,
        no_ignore: None,
        max_depth: None,
        max_files: None,
        max_file_bytes: None,
    })
    .expect("search file");

    assert_eq!(result.total_matches, 1);
    assert_eq!(result.files.len(), 1);
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn detailed_search_distinguishes_empty_from_unsupported() {
    let empty = search_detailed("const x = 1;\n", "a.ts", "ts", Some("target($X)"), None);
    assert_eq!(empty.status, "ok");
    assert!(empty.matches.is_empty());
    assert!(empty.diagnostics.is_empty());

    let unsupported = search_detailed(
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
    let result = search_detailed("x\n", "a.ts", "ts", Some("foo($X)"), Some("rule: {}"));
    assert_eq!(result.status, "parserFailed");
    assert_eq!(result.query.kind, "invalid");
    assert_eq!(result.diagnostics[0].code, "structural.query.invalid");
    assert!(result.diagnostics[0].recovery.is_some());
}

#[test]
fn ast_audit_detailed_search_reports_unknown_node_kind() {
    let result = search_detailed(
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
fn detailed_search_match_ids_are_stable() {
    let content = "target(value);\n";
    let first = search_detailed(content, "a.ts", "ts", Some("target($X)"), None);
    let second = search_detailed(content, "a.ts", "ts", Some("target($X)"), None);
    assert_eq!(first.matches.len(), 1);
    assert_eq!(first.matches[0].id, second.matches[0].id);
    assert_eq!(first.matches[0].confidence, "exact-ast");
    // node_kind is populated from the matched tree-sitter node (a
    // `target(value)` call), not left None.
    assert_eq!(
        first.matches[0].node_kind.as_deref(),
        Some("call_expression")
    );
}

#[test]
fn detailed_file_search_explains_prefilter_and_unsupported_files() {
    let root = temp_root("detailed_files");
    fs::write(root.join("a.ts"), "target(value);\n").expect("write a");
    fs::write(root.join("b.ts"), "other(value);\n").expect("write b");
    fs::write(root.join("note.txt"), "target(value);\n").expect("write txt");

    let result = search_files_detailed(StructuralSearchFilesOptions {
        path: root.to_string_lossy().to_string(),
        pattern: Some("target($X)".to_owned()),
        rule: None,
        include: None,
        exclude_dir: None,
        exclude: None,
        hidden: None,
        no_ignore: None,
        max_depth: None,
        max_files: Some(10),
        max_file_bytes: None,
    })
    .expect("detailed file search");

    assert_eq!(result.total_matches, 1);
    assert_eq!(result.parsed_files, 1);
    assert_eq!(result.skipped_by_pre_filter, 1);
    assert_eq!(result.skipped_unsupported, 1);
    assert_eq!(result.query.literal_anchor.as_deref(), Some("target"));
    assert!(result
        .files
        .iter()
        .any(|file| file.status == "skippedByPreFilter"));
    assert!(result.files.iter().any(|file| file.status == "unsupported"));
    fs::remove_dir_all(root).expect("cleanup");
}

// ── single-content size cap (defense-in-depth on the public napi path) ──
//
// The file walker bounds content at `max_file_bytes`; the single-content
// `search`/`search_detailed` entry points did NOT, so a multi-MB blob passed
// straight to the public napi export could hang in tree-sitter parsing +
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
    const CAP: usize = 1_000_000;
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
    const CAP: usize = 1_000_000;
    let content = content_of_exactly(CAP);
    assert_eq!(content.len(), CAP, "fixture must be exactly at the cap");
    let matches = search(&content, "ts", Some("target($X)"), None)
        .expect("content at the cap must parse, not error");
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].metavars["X"], ["v"]);
}

#[test]
fn search_detailed_reports_oversize_as_truncated() {
    const CAP: usize = 1_000_000;
    let content = content_of_at_least(CAP + 1);
    assert!(content.len() > CAP, "fixture must be over the cap");
    let result = search_detailed(&content, "a.ts", "ts", Some("target($X)"), None);
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
    assert!(diag.recovery.is_some());
}

#[test]
fn search_detailed_accepts_content_at_cap() {
    const CAP: usize = 1_000_000;
    let content = content_of_exactly(CAP);
    assert_eq!(content.len(), CAP, "fixture must be exactly at the cap");
    let result = search_detailed(&content, "a.ts", "ts", Some("target($X)"), None);
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

// ── markup / style grammars (HTML/CSS/SCSS/LESS) ──────────────────────────

#[test]
fn css_pattern_captures_declaration_value() {
    // Expando is `_`, so `$C` → `_C`, a valid CSS identifier.
    let src = ".btn {\n  color: red;\n}\n";
    let matches = run_pattern(src, "css", ".btn { color: $C; }");
    assert_eq!(matches.len(), 1);
    assert_eq!(
        matches[0].metavars.get("C").map(Vec::as_slice),
        Some(&["red".to_string()][..])
    );
}

#[test]
fn css_rule_matches_by_kind() {
    // A `rule` surface needs no expando — match every rule_set.
    let src = ".a { color: red; }\n.b { color: blue; }\n";
    let rule = "rule:\n  kind: rule_set\n";
    let matches = search(src, "css", None, Some(rule)).expect("css rule search");
    assert_eq!(matches.len(), 2);
}

#[test]
fn scss_pattern_matches_and_keeps_literal_lowercase_var() {
    // Lowercase `$base` is a literal SCSS variable (NOT replaced — only
    // `$UPPER`/`$$$` become metavars), so it must match verbatim while `$C`
    // captures the property value.
    let src = ".card {\n  color: $base;\n}\n";
    let matches = run_pattern(src, "scss", ".card { color: $base; }");
    assert_eq!(matches.len(), 1, "literal $base preserved as a real var");

    let captured = run_pattern(src, "scss", ".card { color: $C; }");
    assert_eq!(captured.len(), 1);
    assert_eq!(
        captured[0].metavars.get("C").map(Vec::as_slice),
        Some(&["$base".to_string()][..])
    );
}

#[test]
fn html_tag_name_metavar_resolves_with_z_expando() {
    // The reason HTML's expando is `z`, not `µ`: tree-sitter-html's tagName
    // scanner rejects non-ASCII, so a tag-name metavar only works with `z`.
    let src = "<input>\n";
    let matches = run_pattern(src, "html", "<$TAG>");
    assert_eq!(matches.len(), 1);
    assert_eq!(
        matches[0].metavars.get("TAG").map(Vec::as_slice),
        Some(&["input".to_string()][..])
    );
}

#[test]
fn html_element_pattern_matches_nested_tag() {
    let src = "<section>\n  <button id=\"go\">Click</button>\n</section>\n";
    let matches = run_pattern(src, "html", "<button id=\"go\">$$$</button>");
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].start_line, 2);
}

#[test]
fn markup_and_style_extensions_are_supported() {
    let exts = supported_extensions();
    for ext in ["html", "htm", "css", "scss"] {
        assert!(
            exts.iter().any(|e| e == ext),
            "structural search must support .{ext}"
        );
    }
}

// ── Scala ─────────────────────────────────────────────────────────────────

#[cfg(feature = "tree-sitter-extended")]
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

#[cfg(feature = "tree-sitter-extended")]
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

#[cfg(feature = "tree-sitter-extended")]
#[test]
fn scala_extensions_are_supported() {
    let exts = supported_extensions();
    for ext in ["scala", "sc", "sbt"] {
        assert!(
            exts.iter().any(|e| e == ext),
            "structural search must support .{ext}"
        );
    }
}

// ── config grammars (JSON / YAML) + extension aliases ──────────────

#[test]
fn json_rule_matches_pairs() {
    let src = "{\n  \"a\": 1,\n  \"b\": 2\n}\n";
    let rule = "rule:\n  kind: pair\n";
    let matches = search(src, "json", None, Some(rule)).expect("json rule search");
    assert_eq!(matches.len(), 2);
}

#[test]
fn yaml_rule_matches_block_mapping_pairs() {
    let src = "a: 1\nb: 2\n";
    let rule = "rule:\n  kind: block_mapping_pair\n";
    let matches = search(src, "yaml", None, Some(rule)).expect("yaml rule search");
    assert_eq!(matches.len(), 2);
}

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
fn config_and_alias_extensions_are_supported() {
    let exts = supported_extensions();
    for ext in ["json", "jsonc", "yaml", "yml", "mts", "cts", "pyi"] {
        assert!(
            exts.iter().any(|e| e == ext),
            "structural search must support .{ext}"
        );
    }
}

// ── native walker: recursive globs (#6), ignore semantics (#7), rule
//    prefilter (#9) ─────────────────────────────────────────────────────

#[test]
fn search_files_supports_recursive_glob_includes() {
    let root = temp_root("globs");
    fs::create_dir_all(root.join("src/nested")).expect("nested dir");
    fs::write(root.join("src/a.ts"), "target(v);\n").expect("a");
    fs::write(root.join("src/nested/b.ts"), "target(v);\n").expect("b");
    fs::write(root.join("src/c.js"), "target(v);\n").expect("c");

    let result = search_files(StructuralSearchFilesOptions {
        path: root.to_string_lossy().to_string(),
        pattern: Some("target($X)".to_owned()),
        rule: None,
        include: Some(vec!["src/**/*.ts".to_owned()]),
        exclude_dir: None,
        exclude: None,
        hidden: None,
        no_ignore: None,
        max_depth: None,
        max_files: Some(50),
        max_file_bytes: None,
    })
    .expect("glob search");

    assert_eq!(result.files.len(), 2, "both nested .ts match; .js excluded");
    assert!(result.files.iter().all(|f| f.path.ends_with(".ts")));
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn search_files_honors_dot_ignore_files() {
    let root = temp_root("ignore");
    fs::create_dir_all(root.join("skip")).expect("skip dir");
    fs::write(root.join(".ignore"), "skip/\n").expect("ignore file");
    fs::write(root.join("keep.ts"), "target(v);\n").expect("keep");
    fs::write(root.join("skip/x.ts"), "target(v);\n").expect("skipped");

    let result = search_files(StructuralSearchFilesOptions {
        path: root.to_string_lossy().to_string(),
        pattern: Some("target($X)".to_owned()),
        rule: None,
        include: None,
        exclude_dir: None,
        exclude: None,
        hidden: None,
        no_ignore: None,
        max_depth: None,
        max_files: Some(50),
        max_file_bytes: None,
    })
    .expect("ignore search");

    assert_eq!(result.files.len(), 1, ".ignore skips skip/");
    assert!(result.files[0].path.ends_with("keep.ts"));
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn search_files_prefilters_rule_by_inner_pattern() {
    let root = temp_root("ruleanchor");
    fs::write(
        root.join("has.ts"),
        "async function f() {\n  await g();\n}\n",
    )
    .expect("has");
    fs::write(root.join("none.ts"), "function f() {\n  return 1;\n}\n").expect("none");

    let result = search_files(StructuralSearchFilesOptions {
        path: root.to_string_lossy().to_string(),
        pattern: None,
        rule: Some("rule:\n  pattern: await $C\n".to_owned()),
        include: None,
        exclude_dir: None,
        exclude: None,
        hidden: None,
        no_ignore: None,
        max_depth: None,
        max_files: Some(50),
        max_file_bytes: None,
    })
    .expect("rule search");

    // Anchor "await" lets none.ts skip parsing entirely.
    assert_eq!(result.skipped_by_pre_filter, 1);
    assert_eq!(result.files.len(), 1);
    assert!(result.files[0].path.ends_with("has.ts"));
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn search_files_prefilters_operator_anchor_before_structural_match() {
    let root = temp_root("operatoranchor");
    fs::write(root.join("match.js"), "foo && foo();\n").expect("match");
    fs::write(root.join("nomatch.js"), "foo || foo();\n").expect("nomatch");

    let result = search_files(StructuralSearchFilesOptions {
        path: root.to_string_lossy().to_string(),
        pattern: Some("$A && $A()".to_owned()),
        rule: None,
        include: None,
        exclude_dir: None,
        exclude: None,
        hidden: None,
        no_ignore: None,
        max_depth: None,
        max_files: Some(50),
        max_file_bytes: None,
    })
    .expect("operator anchor search");

    assert_eq!(result.skipped_by_pre_filter, 1);
    assert_eq!(result.files.len(), 1);
    assert!(result.files[0].path.ends_with("match.js"));
    fs::remove_dir_all(root).expect("cleanup");
}

// ── prefilter vs unsupported conflation (evidence: proof vs unevaluated) ─
//
// A `.txt` file that textually contains the anchor is not "anchor-absent"
// (proof of no match) — it's "unsupported extension" (not evaluated).
// `search_files` must report them on separate counters so the warning text
// can't collapse a proof-skip into an unevaluated-skip, the exact
// anti-pattern the evidence-grade contract forbids.

// ── scope parity: exclude / hidden / no_ignore / max_depth ───────────────
// Local search defines `exclude`/`hidden`/`noIgnore`/`maxDepth`
// and the text/regex lane forwards them. The structural lane previously
// dropped them silently — a typed-contract violation. These tests pin the
// parity.

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
    let result = search_files(options).expect("scope search");
    let mut paths: Vec<String> = result
        .files
        .iter()
        .map(|f| {
            std::path::Path::new(&f.path)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        })
        .collect();
    paths.sort();
    paths
}

#[test]
fn structural_files_honors_exclude_globs() {
    let root = temp_root("scope_exclude");
    write_scope_fixture(&root);
    // rule:kind has no literal anchor, so every supported .ts is parsed —
    // the exclude glob is the only thing that can drop `excluded.ts`.
    let paths = scope_result_paths(StructuralSearchFilesOptions {
        path: root.to_string_lossy().to_string(),
        rule: Some("rule:\n  kind: call_expression\n".to_owned()),
        pattern: None,
        include: None,
        exclude: Some(vec!["excluded.ts".to_owned()]),
        exclude_dir: None,
        hidden: None,
        no_ignore: None,
        max_depth: None,
        max_files: Some(50),
        max_file_bytes: None,
    });
    assert!(paths.iter().any(|p| p == "match.ts"), "match.ts present");
    assert!(
        !paths.iter().any(|p| p == "excluded.ts"),
        "exclude glob must drop excluded.ts: got {paths:?}"
    );
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn structural_files_honors_hidden_flag() {
    let root = temp_root("scope_hidden");
    write_scope_fixture(&root);
    // Default (hidden:None) ignores dot-files; Some(true) must include .hidden.ts.
    let with_hidden = scope_result_paths(StructuralSearchFilesOptions {
        path: root.to_string_lossy().to_string(),
        rule: Some("rule:\n  kind: call_expression\n".to_owned()),
        pattern: None,
        include: None,
        exclude: None,
        exclude_dir: None,
        hidden: Some(true),
        no_ignore: None,
        max_depth: None,
        max_files: Some(50),
        max_file_bytes: None,
    });
    assert!(
        with_hidden.iter().any(|p| p == ".hidden.ts"),
        "hidden:Some(true) must include .hidden.ts: got {with_hidden:?}"
    );
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn structural_files_honors_no_ignore_flag() {
    let root = temp_root("scope_noignore");
    write_scope_fixture(&root);
    // .gitignore excludes gitignored.ts; no_ignore:Some(true) must surface it.
    let with_no_ignore = scope_result_paths(StructuralSearchFilesOptions {
        path: root.to_string_lossy().to_string(),
        rule: Some("rule:\n  kind: call_expression\n".to_owned()),
        pattern: None,
        include: None,
        exclude: None,
        exclude_dir: None,
        hidden: None,
        no_ignore: Some(true),
        max_depth: None,
        max_files: Some(50),
        max_file_bytes: None,
    });
    assert!(
        with_no_ignore.iter().any(|p| p == "gitignored.ts"),
        "no_ignore:Some(true) must include the .gitignored file: got {with_no_ignore:?}"
    );
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn structural_files_honors_max_depth() {
    let root = temp_root("scope_maxdepth");
    write_scope_fixture(&root);
    // max_depth:1 = root only; nested/deep.ts must NOT be reached.
    let paths = scope_result_paths(StructuralSearchFilesOptions {
        path: root.to_string_lossy().to_string(),
        rule: Some("rule:\n  kind: call_expression\n".to_owned()),
        pattern: None,
        include: None,
        exclude: None,
        exclude_dir: None,
        hidden: None,
        no_ignore: None,
        max_depth: Some(1),
        max_files: Some(50),
        max_file_bytes: None,
    });
    assert!(paths.iter().any(|p| p == "match.ts"), "root file present");
    assert!(
        !paths.iter().any(|p| p == "deep.ts"),
        "max_depth:1 must not descend into nested/: got {paths:?}"
    );
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn search_files_separates_unsupported_from_prefilter_skips() {
    let root = temp_root("conflation");
    // `match.ts` carries the anchor and matches the pattern.
    fs::write(root.join("match.ts"), "target(value);\n").expect("match");
    // `hasanchor.txt` textually contains the anchor but .txt has no
    // grammar — it must read as unsupported, NOT as a prefilter skip.
    fs::write(root.join("hasanchor.txt"), "target(value);\n").expect("txt");
    // `noanchor.ts` lacks the anchor — a genuine prefilter (proof) skip.
    fs::write(root.join("noanchor.ts"), "other(value);\n").expect("noanchor");

    let result = search_files(StructuralSearchFilesOptions {
        path: root.to_string_lossy().to_string(),
        pattern: Some("target($X)".to_owned()),
        rule: None,
        include: None,
        exclude_dir: None,
        exclude: None,
        hidden: None,
        no_ignore: None,
        max_depth: None,
        max_files: Some(10),
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
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn structural_review_pattern_depth_is_explicitly_incomplete() {
    let source = format!("const x = {}probe(){};", "[".repeat(600), "]".repeat(600));
    let pattern = format!("{}probe(){}", "[".repeat(600), "]".repeat(600));
    let result = search_detailed(&source, "deep.ts", "ts", Some(&pattern), None);
    assert_eq!(result.status, "truncated");
    assert!(result.matches.is_empty());
    assert_eq!(result.diagnostics[0].code, "structural.match.depthLimit");
}

#[test]
fn structural_review_backtracking_exhaustion_cannot_satisfy_negation() {
    let source = format!("probe({});", vec!["1"; 150].join(","));
    let rule = "kind: call_expression\nnot:\n  pattern: probe($$$A, $$$B, absent)";
    let result = search_detailed(&source, "wide.ts", "ts", None, Some(rule));
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
        hidden: None,
        no_ignore: None,
        max_depth: Some(1),
        max_files: Some(limit),
        max_file_bytes: None,
    }
}

#[test]
fn structural_review_scan_cap_and_depth_have_explicit_completion() {
    let root = temp_root("review_cap_depth");
    fs::create_dir_all(root.join("aaa_nested")).expect("nested");
    fs::write(root.join("aaa_nested/hidden.ts"), "probe();").expect("deep");
    fs::write(root.join("one.ts"), "probe();").expect("first");
    for rule in [
        "kind: call_expression",
        "pattern: probe()",
        "any: [{pattern: probe()}, {pattern: other()}]",
    ] {
        let exact = search_files(review_file_options(&root, rule, 1)).expect("exact cap");
        assert_eq!(
            exact.total_matches, 1,
            "native depth1 includes only direct files: {rule}"
        );
        assert!(!exact.scan_truncated, "exact cap is complete: {rule}");
        fs::write(root.join("two.ts"), "probe();").expect("second");
        let limited = search_files(review_file_options(&root, rule, 1)).expect("overflow cap");
        assert!(limited.scan_truncated, "overflow is explicit: {rule}");
        assert_eq!(limited.total_matches, 1);
        let detailed =
            search_files_detailed(review_file_options(&root, rule, 1)).expect("detailed overflow");
        assert!(detailed.scan_truncated);
        fs::remove_file(root.join("two.ts")).expect("reset fixture");
    }
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn structural_review_file_limits_retain_completed_files() {
    let root = temp_root("review_file_limits");
    fs::write(root.join("ok.ts"), "probe(1);").expect("complete file");
    fs::write(
        root.join("wide.ts"),
        format!("probe({});", vec!["1"; 150].join(",")),
    )
    .expect("limited file");
    let rule = "kind: call_expression\nnot:\n  pattern: probe($$$A, $$$B, absent)";
    let result = search_files(review_file_options(&root, rule, 10)).expect("partial file search");
    assert_eq!(result.status, "truncated");
    assert_eq!(result.total_matches, 1);
    assert!(result.files[0].path.ends_with("ok.ts"));
    assert_eq!(result.diagnostics.len(), 1);
    assert_eq!(
        result.diagnostics[0].code,
        "structural.match.backtrackingLimit"
    );
    assert!(result.diagnostics[0]
        .path
        .as_deref()
        .expect("path")
        .ends_with("wide.ts"));
    let detailed =
        search_files_detailed(review_file_options(&root, rule, 10)).expect("detailed file search");
    assert_eq!(detailed.status, "truncated");
    assert_eq!(detailed.total_matches, 1);
    assert!(detailed.files.iter().any(|file| file.status == "truncated"));
    assert_eq!(detailed.diagnostics.len(), 1);
    fs::remove_dir_all(root).expect("cleanup");
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
    let result = search_detailed("probe();", "fixture.ts", "ts", Some("probe()"), None);
    assert_eq!(result.status, "truncated");
    assert_eq!(result.diagnostics[0].code, "structural.parse.interrupted");
    assert_eq!(result.diagnostics[0].stage, "parse");
}

#[test]
fn structural_review_compile_interruption_keeps_mixed_language_evidence() {
    let root = temp_root("review_compile_interruption");
    fs::write(root.join("one.js"), "probe();").expect("js fixture");
    fs::write(root.join("two.ts"), "probe();").expect("ts fixture");
    let options = || review_file_options(&root, "pattern: probe()", 10);
    octo::INTERRUPT_NEXT_COMPILE_PARSE.with(|interrupt| interrupt.set(true));
    let result = search_files(options()).expect("partial mixed-language search");
    assert_eq!(result.status, "truncated");
    assert_eq!(result.total_matches, 1);
    assert_eq!(result.diagnostics[0].code, "structural.parse.interrupted");
    assert!(!result
        .warnings
        .iter()
        .any(|warning| warning.contains("not valid syntax")));
    octo::INTERRUPT_NEXT_COMPILE_PARSE.with(|interrupt| interrupt.set(true));
    let detailed = search_files_detailed(options()).expect("detailed mixed-language search");
    assert_eq!(detailed.status, "truncated");
    assert_eq!(detailed.total_matches, 1);
    assert_eq!(detailed.diagnostics[0].code, "structural.parse.interrupted");
    fs::remove_dir_all(root).expect("cleanup");
}
