//! Structural (AST) search over the tree-sitter grammars we already link.
//!
//! This is octocode's L2 search layer: it answers shape questions ripgrep
//! can't (a call shaped `foo($X)`, an `eval()` call site that is NOT inside a
//! comment/string) and that LSP is too heavy for. The default matcher is
//! Octocode-owned; the grammars are the exact `tree_sitter::Language` values in
//! [`crate::signatures::languages`] — no second grammar set, no link collision.

mod files;
mod language;
mod octo;
mod query;
mod types;

pub use files::{search_files, search_files_detailed};
pub use types::{
    StructuralDetailedMatch, StructuralDiagnostic, StructuralMatch, StructuralSearchDetailedResult,
    StructuralSearchFilesDetailedResult, StructuralSearchFilesOptions, StructuralSearchFilesResult,
};

use crate::signatures::languages;
use language::AgLanguage;
use octo::compile_matcher;
use query::{invalid_query_explanation, StructuralQuery};
use types::{structural_query_fingerprint, STRUCTURAL_ANALYZER, STRUCTURAL_ANALYZER_VERSION};

/// Defense-in-depth cap on content handed to the single-content structural
/// entry points (`search`, `search_detailed`). The file walker already bounds
/// per-file bytes via `max_file_bytes`; this mirrors that backstop on the path
/// OQL continuations and the public napi export hand off to, so a multi-MB blob
/// can't hang tree-sitter parsing or `match_multi_capture` backtracking with no
/// timeoutMs escape. At-or-below passes; over returns an error / `truncated`.
const MAX_STRUCTURAL_CONTENT_BYTES: usize = 1_000_000;

/// Run a structural search over `content`, parsed with the grammar resolved
/// from `ext`. Exactly one of `pattern` / `rule` must be `Some`.
///
/// Returns `Err` for: an unsupported extension, an invalid pattern, invalid
/// rule YAML, or both/neither query supplied — the napi layer maps these to a
/// JS error so the caller can surface guidance instead of a silent empty set.
pub fn supported_extensions() -> Vec<String> {
    languages::supported_extensions()
        .into_iter()
        .map(str::to_owned)
        .collect()
}

pub fn search(
    content: &str,
    ext: &str,
    pattern: Option<&str>,
    rule: Option<&str>,
) -> Result<Vec<StructuralMatch>, String> {
    if content.len() > MAX_STRUCTURAL_CONTENT_BYTES {
        return Err(format!(
            "structural search content exceeds {MAX_STRUCTURAL_CONTENT_BYTES} byte limit"
        ));
    }
    let query = StructuralQuery::new(pattern, rule)?;
    let entry = languages::find_entry(ext)
        .ok_or_else(|| format!("structural search does not support .{ext} files"))?;
    let lang = AgLanguage::new(ext, entry);
    let run = compile_matcher(&lang, query)?;
    Ok(run(content))
}

pub fn search_detailed(
    content: &str,
    file_path: &str,
    ext: &str,
    pattern: Option<&str>,
    rule: Option<&str>,
) -> StructuralSearchDetailedResult {
    let query_fingerprint = structural_query_fingerprint(pattern, rule);
    if content.len() > MAX_STRUCTURAL_CONTENT_BYTES {
        let diagnostic = StructuralDiagnostic::new(
            "structural.content.tooLarge",
            "warning",
            "parse",
            format!(
                "Structural search content is {} bytes, above the single-content limit of {MAX_STRUCTURAL_CONTENT_BYTES} bytes.",
                content.len()
            ),
        )
        .with_path(file_path)
        .with_recovery("Scope the content with fetch.content charLength/charOffset or run a file search with maxFileBytes instead.");
        return StructuralSearchDetailedResult {
            path: file_path.to_owned(),
            analyzer: STRUCTURAL_ANALYZER.to_owned(),
            analyzer_version: STRUCTURAL_ANALYZER_VERSION.to_owned(),
            status: "truncated".to_owned(),
            language_id: None,
            query: invalid_query_explanation(
                pattern,
                rule,
                "content exceeds single-content byte limit",
            ),
            matches: Vec::new(),
            diagnostics: vec![diagnostic],
        };
    }
    let query = match StructuralQuery::new(pattern, rule) {
        Ok(query) => query,
        Err(message) => {
            let diagnostic = StructuralDiagnostic::new(
                "structural.query.invalid",
                "error",
                "match",
                message.clone(),
            )
            .with_path(file_path)
            .with_recovery("Provide exactly one non-empty structural pattern or YAML rule.");
            return StructuralSearchDetailedResult {
                path: file_path.to_owned(),
                analyzer: STRUCTURAL_ANALYZER.to_owned(),
                analyzer_version: STRUCTURAL_ANALYZER_VERSION.to_owned(),
                status: "parserFailed".to_owned(),
                language_id: None,
                query: invalid_query_explanation(pattern, rule, &message),
                matches: Vec::new(),
                diagnostics: vec![diagnostic],
            };
        }
    };

    let query_explanation = query.explanation();
    let Some(entry) = languages::find_entry(ext) else {
        let diagnostic = StructuralDiagnostic::new(
            "structural.language.unsupported",
            "warning",
            "parse",
            format!("Structural search does not support .{ext} files."),
        )
        .with_path(file_path)
        .with_recovery("Use text search for this extension or add a tree-sitter grammar mapping.");
        return StructuralSearchDetailedResult {
            path: file_path.to_owned(),
            analyzer: STRUCTURAL_ANALYZER.to_owned(),
            analyzer_version: STRUCTURAL_ANALYZER_VERSION.to_owned(),
            status: "unsupported".to_owned(),
            language_id: None,
            query: query_explanation,
            matches: Vec::new(),
            diagnostics: vec![diagnostic],
        };
    };

    let lang = AgLanguage::new(ext, entry);
    let run = match compile_matcher(&lang, query) {
        Ok(run) => run,
        Err(message) => {
            let diagnostic = StructuralDiagnostic::new(
                "structural.query.compileFailed",
                "error",
                "match",
                message.clone(),
            )
            .with_path(file_path)
            .with_recovery(
                "Check the structural pattern or YAML rule against this file's language grammar.",
            );
            return StructuralSearchDetailedResult {
                path: file_path.to_owned(),
                analyzer: STRUCTURAL_ANALYZER.to_owned(),
                analyzer_version: STRUCTURAL_ANALYZER_VERSION.to_owned(),
                status: "parserFailed".to_owned(),
                language_id: entry.language_id.map(str::to_owned),
                query: query_explanation,
                matches: Vec::new(),
                diagnostics: vec![diagnostic],
            };
        }
    };

    let matches = run(content)
        .into_iter()
        .map(|matched| StructuralDetailedMatch::from_match(file_path, &query_fingerprint, matched))
        .collect();

    StructuralSearchDetailedResult {
        path: file_path.to_owned(),
        analyzer: STRUCTURAL_ANALYZER.to_owned(),
        analyzer_version: STRUCTURAL_ANALYZER_VERSION.to_owned(),
        status: "ok".to_owned(),
        language_id: entry.language_id.map(str::to_owned),
        query: query_explanation,
        matches,
        diagnostics: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

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
        for ext in ["ts", "py", "sh", "html", "json", "toml"] {
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
        let src = "async function f() {\n  for (const x of xs) {\n    await g(x);\n  }\n  await h();\n}\n";
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
    fn detailed_search_match_ids_are_stable() {
        let content = "target(value);\n";
        let first = search_detailed(content, "a.ts", "ts", Some("target($X)"), None);
        let second = search_detailed(content, "a.ts", "ts", Some("target($X)"), None);
        assert_eq!(first.matches.len(), 1);
        assert_eq!(first.matches[0].id, second.matches[0].id);
        assert_eq!(first.matches[0].confidence, "exact-ast");
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
    // the engine's own backstop — OQL defers caps to backends, so the contract
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
        // Valid AST source truncated to an exact byte length (newline-terminated
        // repetitions, then a final partial line). Used for the at-cap case so
        // the cap boundary is exact, not soft.
        let line = "target(v);\n";
        let mut out = String::with_capacity(byte_len);
        while out.len() + line.len() <= byte_len {
            out.push_str(line);
        }
        while out.len() < byte_len {
            out.push(' ');
        }
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
        let matches = search(&content, "ts", Some("nomatch($X)"), None)
            .expect("content at the cap must parse, not error");
        assert!(matches.is_empty());
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
        let result = search_detailed(&content, "a.ts", "ts", Some("nomatch($X)"), None);
        assert_eq!(result.status, "ok");
        assert!(result.matches.is_empty());
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
    fn less_pattern_matches_rule() {
        let src = ".box {\n  width: 10px;\n}\n";
        let matches = run_pattern(src, "less", ".box { width: $W; }");
        assert_eq!(matches.len(), 1);
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
        for ext in ["html", "htm", "css", "scss", "less"] {
            assert!(
                exts.iter().any(|e| e == ext),
                "structural search must support .{ext}"
            );
        }
    }

    // ── Scala ─────────────────────────────────────────────────────────────────

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

    // ── config grammars (JSON / YAML / TOML) + extension aliases ──────────────

    #[test]
    fn json_rule_matches_pairs() {
        let src = "{\n  \"a\": 1,\n  \"b\": 2\n}\n";
        let rule = "rule:\n  kind: pair\n";
        let matches = search(src, "json", None, Some(rule)).expect("json rule search");
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn toml_rule_matches_pairs() {
        let src = "a = 1\nb = 2\n";
        let rule = "rule:\n  kind: pair\n";
        let matches = search(src, "toml", None, Some(rule)).expect("toml rule search");
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
        for ext in ["json", "jsonc", "yaml", "yml", "toml", "mts", "cts", "pyi"] {
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

    // ── prefilter vs unsupported conflation (OQL evidence: proof vs unevaluated) ─
    //
    // A `.txt` file that textually contains the anchor is not "anchor-absent"
    // (proof of no match) — it's "unsupported extension" (not evaluated).
    // `search_files` must report them on separate counters so the warning text
    // can't collapse a proof-skip into an unevaluated-skip, the exact
    // anti-pattern OQL's evidence kinds forbid.

    // ── scope parity: exclude / hidden / no_ignore / max_depth ───────────────
    // OQL `QueryScope` defines `exclude`/`hidden`/`noIgnore`/`maxDepth` and the
    // text/regex lane forwards them. The structural lane previously dropped
    // them silently — a typed-contract violation. These tests pin the parity.

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
}
