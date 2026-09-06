use super::*;
use std::collections::HashSet;
use tree_sitter::{Parser, Query};

#[test]
fn excluded_grammars_report_unsupported_across_native_capabilities() {
    for ext in [
        "sh", "bash", "zsh", "vue", "svelte", "astro", "dart", "less", "ml", "mli", "jl", "r",
        "erl", "hrl", "ex", "exs", "tf", "hcl", "tfvars", "proto",
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
        "rb" => "def target(value)\n  body_marker = value + 1\n  body_marker\nend\n",
        "php" => "<?php\nfunction target($value) {\n  $body_marker = $value + 1;\n  return $body_marker;\n}\n",
        "kt" => "fun target(value: Int): Int {\n  val body_marker = value + 1\n  return body_marker\n}\n",
        "lua" => "function target(value)\n  local body_marker = value + 1\n  return body_marker\nend\n",
        "sql" => "SELECT target FROM users WHERE active = true;\n",
        "zig" => "fn target(value: i32) i32 {\n  const body_marker = value + 1;\n  return body_marker;\n}\n",
        "html" => "<div id=\"target\"><span>value</span></div>\n",
        "css" => ".target { color: red; }\n",
        "scss" => "$color: red;\n.target { color: $color; }\n",
        "scala" => "object Fixture {\n  def target(value: Int): Int = {\n    val body_marker = value + 1\n    body_marker\n  }\n}\n",
        "json" => "{\"target\": true}\n",
        "yaml" => "target: true\n",
        "toml" => "target = true\n",
        "swift" => "func target(value: Int) -> Int {\n  let body_marker = value + 1\n  return body_marker\n}\n",
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
                assert!(grammar.parser().is_some(), ".{ext}: LSP parser ABI");
            }
        }
    }
    assert_eq!(extensions.len(), supported_extensions().len());
}

#[test]
fn php_patterns_accept_existing_opening_tags_and_bare_fragments() {
    let source = fixture("php");
    for pattern in [
        source.to_owned(),
        format!("  {source}"),
        source.replacen("<?php", "", 1),
    ] {
        let result =
            crate::structural::search_detailed(source, "fixture.php", "php", Some(&pattern), None);
        assert_eq!(result.status, "ok", "{pattern}");
        assert_eq!(result.matches.len(), 1, "{pattern}");
        assert!(result.matches[0].text.contains("function target"));
    }
}

#[test]
fn php_tagged_patterns_preserve_uppercase_short_echo_and_closing_tags() {
    for source in [
        "<?PHP target(1);",
        "<?= target(1) ?>",
        "<?php target(1); ?>",
    ] {
        let entry = find_entry("php").expect("PHP grammar");
        let mut parser = Parser::new();
        parser.set_language(&entry.language).expect("PHP ABI");
        let tree = parser.parse(source, None).expect("PHP parse");
        assert!(!tree.root_node().has_error(), "{source}");
        let result =
            crate::structural::search_detailed(source, "fixture.php", "php", Some(source), None);
        assert_eq!(result.status, "ok", "{source}");
        assert_eq!(result.matches.len(), 1, "{source}");
        assert!(result.matches[0].text.contains("target(1)"));
    }
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
