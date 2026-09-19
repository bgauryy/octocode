use super::*;
use std::collections::HashSet;
use tree_sitter::{Parser, Query};

#[test]
fn excluded_grammars_report_unsupported_across_native_capabilities() {
    for ext in [
        "sh", "bash", "zsh", "vue", "svelte", "astro", "dart", "less", "ml", "mli", "jl", "r",
        "erl", "hrl", "ex", "exs", "tf", "hcl", "tfvars", "proto", "toml", "lua", "zig", "rb",
        "rake", "gemspec", "ru", "scss", "sql", "swift", "yaml", "yml", "css", "htm", "html",
        "json", "jsonc", "kt", "kts", "php",
    ] {
        assert!(find_entry(ext).is_none(), ".{ext} must not load a grammar");
        assert!(!supported_extensions().contains(&ext));
        assert!(!signature_extensions().contains(&ext));
        let file = format!("fixture.{ext}");
        assert!(crate::lsp::grammar::grammar_for_file(&file).is_none());
        let result = crate::structural::search_detailed(
            "target(value);",
            &file,
            ext,
            Some("target($ARG)"),
            None,
        );
        assert_eq!(result.status, "unsupported", ".{ext}");
        assert_eq!(
            result.diagnostics[0].code,
            "structural.language.unsupported"
        );
    }
}

#[test]
fn default_release_capabilities_are_exactly_the_first_class_extension_set() {
    if !cfg!(all(
        feature = "tree-sitter-cpp",
        feature = "tree-sitter-c-sharp",
        feature = "tree-sitter-scala"
    )) {
        return;
    }
    let expected = [
        "c", "cc", "cjs", "cpp", "cs", "cts", "cxx", "go", "h", "hh", "hpp", "hxx", "java", "js",
        "jsx", "mjs", "mts", "py", "pyi", "rs", "sbt", "sc", "scala", "ts", "tsx",
    ];
    for (name, mut actual) in [
        (
            "structural",
            supported_extensions()
                .into_iter()
                .map(str::to_owned)
                .collect(),
        ),
        (
            "signature",
            signature_extensions()
                .into_iter()
                .map(str::to_owned)
                .collect(),
        ),
        (
            "graph",
            crate::signatures::graph_facts::graph_fact_extensions(),
        ),
    ] {
        actual.sort_unstable();
        assert_eq!(actual, expected, "{name} capability drift");
    }
}

#[test]
fn removed_languages_have_no_analysis_minifier_or_builtin_server_route() {
    for ext in ["toml", "lua", "zig"] {
        let file = format!("fixture.{ext}");
        assert!(crate::signatures::extract_signatures_inner("target(value);", &file).is_none());
        assert!(crate::signatures::extract_graph_facts_inner("target(value);", &file).is_none());
        assert!(!crate::signatures::graph_facts::graph_fact_extensions()
            .iter()
            .any(|item| item == ext));
        assert!(!crate::minify::config::minify_config().contains_key(ext));
        assert!(crate::lsp::config::detect_language_id(file).is_none());
    }
    assert!(crate::minify::comment_remover::rules_for("lua").is_none());
}

#[test]
fn modern_language_constructs_parse_without_errors() {
    let cases = [
        ("ts", "const options = { mode: 'fast' } satisfies Record<string, string>;"),
        ("tsx", "const View = () => <section>{items?.map(item => <span key={item.id}>{item.name}</span>)}</section>;"),
        ("py", "type Vector[T] = list[T]\ndef first[T](values: Vector[T]) -> T:\n    return values[0]\n"),
        ("go", "package main\nfunc First[T any](values []T) T { return values[0] }\n"),
        ("rs", "fn main() { if let Some(x) = Some(1) && x > 0 { println!(\"{x}\"); } }"),
        ("java", "record Point(int x, int y) { int sum() { return switch (x) { case 0 -> y; default -> x + y; }; } }"),
        ("cs", "class Point(int x, int y) { public int[] Values => [x, y]; }"),
        ("cpp", "template<typename T> concept Number = requires(T x) { x + x; };\ntemplate<Number T> T add(T x) { return x + x; }"),
    ];
    for (ext, source) in cases {
        let Some(entry) = find_entry(ext) else {
            continue; // Optional grammar compiled out in a minimal build.
        };
        let mut parser = Parser::new();
        parser.set_language(&entry.language).expect("grammar ABI");
        let tree = parser.parse(source, None).expect("modern syntax tree");
        assert!(
            !tree.root_node().has_error(),
            ".{ext}: {}",
            tree.root_node().to_sexp()
        );
    }
}

// One valid source per grammar. Every alias is exercised against its grammar's
// fixture; adding a registry entry without a fixture fails instead of silently
// leaving a newly advertised language untested.
fn fixture(ext: &str) -> &'static str {
    match ext {
        "ts" => "export function target(value: number): number {\n  const body_marker = value + 1;\n  return body_marker;\n}\n",
        "tsx" => "export function target(value: number) {\n  const body_marker = value + 1;\n  return <div>{body_marker}</div>;\n}\n",
        "js" => "export function target(value) {\n  const body_marker = value + 1;\n  return body_marker;\n}\n",
        "py" => "def target(value):\n    body_marker = value + 1\n    return body_marker\n",
        "go" => "package fixture\nfunc target(value int) int {\n  body_marker := value + 1\n  return body_marker\n}\n",
        "rs" => "fn target(value: i32) -> i32 {\n  let body_marker = value + 1;\n  body_marker\n}\n",
        "java" => "class Fixture {\n  int target(int value) {\n    int body_marker = value + 1;\n    return body_marker;\n  }\n}\n",
        "c" => "int target(int value) {\n  int body_marker = value + 1;\n  return body_marker;\n}\n",
        "cpp" => "class Fixture {\npublic:\n  int target(int value) {\n    int body_marker = value + 1;\n    return body_marker;\n  }\n};\n",
        "cs" => "class Fixture {\n  public int target(int value) {\n    int body_marker = value + 1;\n    return body_marker;\n  }\n}\n",
        "scala" => "object Fixture {\n  def target(value: Int): Int = {\n    val body_marker = value + 1\n    body_marker\n  }\n}\n",
        _ => panic!("missing grammar fixture for .{ext}"),
    }
}

#[test]
fn every_registered_grammar_and_alias_parses_and_searches_real_source() {
    let mut extensions = HashSet::new();
    for entry in all_entries() {
        let source = fixture(entry.extensions[0]);
        let mut parser = Parser::new();
        parser
            .set_language(&entry.language)
            .expect("compatible grammar ABI");
        let tree = parser.parse(source, None).expect("grammar parses fixture");
        assert!(
            !tree.root_node().has_error(),
            ".{} fixture has parse errors: {}",
            entry.extensions[0],
            tree.root_node().to_sexp()
        );
        // Root-kind search must recover the complete input, not merely return
        // success with an empty result after dispatching to the wrong grammar.
        let rule = format!("rule:\n  kind: {}\n", tree.root_node().kind());
        for ext in entry.extensions {
            assert!(extensions.insert(*ext), "duplicate grammar alias .{ext}");
            let matches = crate::structural::search(source, ext, None, Some(&rule))
                .unwrap_or_else(|error| panic!(".{ext}: {error}"));
            assert_eq!(matches.len(), 1, ".{ext}: exact root match");
            assert_eq!(matches[0].text.trim_end(), source.trim_end(), ".{ext}");
            if let Some(language_id) = entry.language_id {
                let file = format!("fixture.{ext}");
                let grammar = crate::lsp::grammar::grammar_for_file(&file)
                    .unwrap_or_else(|| panic!("missing LSP grammar for .{ext}"));
                assert_eq!(grammar.language_id, language_id, ".{ext}");
                assert!(
                    grammar
                        .parse_before(
                            "",
                            std::time::Instant::now()
                                + crate::signatures::extractor::AST_EXECUTION_TIMEOUT
                        )
                        .is_some(),
                    ".{ext}: LSP parser ABI"
                );
            }
        }
    }
    assert_eq!(extensions.len(), supported_extensions().len());
}

#[test]
fn every_advertised_signature_query_compiles_and_removes_a_real_body() {
    let mut failures = Vec::new();
    for entry in all_entries()
        .iter()
        .filter(|entry| !entry.body_query.is_empty())
    {
        let ext = entry.extensions[0];
        if let Err(error) = Query::new(&entry.language, entry.body_query) {
            failures.push(format!(".{ext}: invalid body query: {error}"));
            continue;
        }
        let source = fixture(ext);
        assert!(
            source.contains("body_marker"),
            ".{ext}: fixture body required"
        );
        let config = crate::signatures::extractor::LangExtractConfig {
            language: entry.language.clone(),
            body_query: entry.body_query,
        };
        let kept = crate::signatures::extractor::extract(source, &config)
            .unwrap_or_else(|| panic!(".{ext}: expected signature extraction"));
        let outline = kept
            .iter()
            .map(|(_, line)| line.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        if !outline.contains("target") || outline.contains("body_marker") {
            failures.push(format!(
                ".{ext}: signature must keep target and drop body:\n{outline}"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[test]
fn aliases_advertise_the_same_graph_language_and_fact_families() {
    let capabilities: Vec<serde_json::Value> =
        serde_json::from_str(&crate::signatures::graph_facts::graph_fact_capabilities_json())
            .expect("graph capabilities JSON");
    let mut failures = Vec::new();
    for entry in all_entries()
        .iter()
        .filter(|entry| !entry.body_query.is_empty())
    {
        let canonical = capabilities
            .iter()
            .find(|cap| cap["extension"] == entry.extensions[0])
            .expect("canonical capability");
        for ext in entry.extensions {
            let alias = capabilities
                .iter()
                .find(|cap| cap["extension"] == *ext)
                .expect("alias capability");
            if alias["language"] != canonical["language"]
                || alias["factFamilies"] != canonical["factFamilies"]
            {
                failures.push(format!(
                    ".{ext} disagrees with .{}: {alias}",
                    entry.extensions[0]
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
