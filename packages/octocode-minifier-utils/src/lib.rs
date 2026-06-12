#![allow(clippy::needless_pass_by_value)]

mod apply;
mod comment_remover;
mod config;
mod file_extension;
mod minifier;
mod signatures;
mod strategies;
mod types;
mod yaml_utils;

use napi_derive::napi;
use types::{FileTypeMinifyConfig, GetExtensionOptions, MinifyResult, YamlConversionConfig};

// ── Re-exports so the TS shim can see them ────────────────────────────────────
#[napi]
pub const SIGNATURES_ONLY_HINT: &str = signatures::SIGNATURES_ONLY_HINT;

// ── File extension ────────────────────────────────────────────────────────────
#[napi(js_name = "getExtension")]
pub fn get_extension(file_path: String, options: Option<GetExtensionOptions>) -> String {
    let lowercase = options.as_ref().and_then(|o| o.lowercase).unwrap_or(false);
    let fallback  = options.as_ref().and_then(|o| o.fallback.as_deref()).unwrap_or("");
    file_extension::get_extension_internal(&file_path, lowercase, fallback)
}

// ── Minification ──────────────────────────────────────────────────────────────
#[napi(js_name = "minifyContentSync")]
pub fn minify_content_sync(content: String, file_path: String) -> String {
    minifier::minify_content_sync_inner(&content, &file_path)
}

/// Sync equivalent of TS `minifyContent` — returns MinifyResult.
/// The JS shim wraps this in Promise.resolve() so callers can await it.
#[napi(js_name = "minifyContentResult")]
pub fn minify_content_result(content: String, file_path: String) -> MinifyResult {
    minifier::minify_content_result_inner(&content, &file_path)
}

#[napi(js_name = "applyMinification")]
pub fn apply_minification(content: String, file_path: String) -> String {
    apply::apply_minification_inner(&content, &file_path)
}

#[napi(js_name = "applyContentViewMinification")]
pub fn apply_content_view_minification(content: String, file_path: String) -> String {
    apply::apply_content_view_minification_inner(&content, &file_path)
}

// ── Fine-grained strategy exports ─────────────────────────────────────────────

/// `commentTypes` accepts a single string or array of strings.
#[napi(js_name = "removeComments")]
pub fn remove_comments(content: String, comment_types: serde_json::Value) -> String {
    let groups: Vec<String> = match comment_types {
        serde_json::Value::String(s) => vec![s],
        serde_json::Value::Array(arr) => arr
            .into_iter()
            .filter_map(|v| v.as_str().map(|s| s.to_owned()))
            .collect(),
        _ => return content,
    };
    let refs: Vec<&str> = groups.iter().map(|s| s.as_str()).collect();
    comment_remover::remove_comments(&content, &refs)
}

#[napi(js_name = "minifyConservativeCore")]
pub fn minify_conservative_core(content: String, config: FileTypeMinifyConfig) -> String {
    let groups = parse_comment_groups(&config.comments);
    let refs: Vec<&str> = groups.iter().map(|s| s.as_str()).collect();
    strategies::minify_conservative(&content, if refs.is_empty() { None } else { Some(&refs) })
}

#[napi(js_name = "minifyAggressiveCore")]
pub fn minify_aggressive_core(content: String, config: FileTypeMinifyConfig) -> String {
    let groups = parse_comment_groups(&config.comments);
    let refs: Vec<&str> = groups.iter().map(|s| s.as_str()).collect();
    strategies::minify_aggressive(&content, if refs.is_empty() { None } else { Some(&refs) })
}

#[napi(js_name = "minifyJsonCore")]
pub fn minify_json_core(content: String) -> MinifyResult {
    let (out, failed) = strategies::minify_json_core_inner(&content);
    MinifyResult { content: out, failed, r#type: "json".to_owned(), reason: None }
}

#[napi(js_name = "minifyJsonReadable")]
pub fn minify_json_readable(content: String) -> MinifyResult {
    let (out, failed) = strategies::minify_json_readable_inner(&content);
    MinifyResult { content: out, failed, r#type: "json".to_owned(), reason: None }
}

#[napi(js_name = "minifyCodeCore")]
pub fn minify_code_core(content: String) -> String {
    strategies::minify_code_core(&content)
}

#[napi(js_name = "minifyGeneralCore")]
pub fn minify_general_core(content: String) -> String {
    strategies::minify_general_core(&content)
}

#[napi(js_name = "minifyMarkdownCore")]
pub fn minify_markdown_core(content: String) -> String {
    strategies::minify_markdown_core(&content)
}

#[napi(js_name = "minifyCSSCore")]
pub fn minify_css_core(content: String) -> String {
    strategies::minify_css_core(&content)
}

#[napi(js_name = "minifyHTMLCore")]
pub fn minify_html_core(content: String) -> String {
    strategies::minify_html_core(&content)
}

#[napi(js_name = "minifyJavaScriptCore")]
pub fn minify_javascript_core(content: String) -> String {
    strategies::minify_javascript_core(&content)
}

#[napi(js_name = "minifyCSSQuality")]
pub fn minify_css_quality(content: String) -> String {
    strategies::minify_css_quality(&content)
}

#[napi(js_name = "minifyHTMLQuality")]
pub fn minify_html_quality(content: String) -> String {
    strategies::minify_html_quality(&content)
}

#[napi(js_name = "stripPythonDocstrings")]
pub fn strip_python_docstrings(content: String) -> String {
    comment_remover::strip_python_docstrings(&content)
}

// ── Signature extraction ──────────────────────────────────────────────────────

#[napi(js_name = "extractSignatures")]
pub fn extract_signatures(content: String, file_path: String) -> Option<String> {
    signatures::extract_signatures_inner(&content, &file_path)
}

/// Returns all extensions that have signature extraction support
/// (tree-sitter languages + heuristic-covered languages).
#[napi(js_name = "getSupportedSignatureExtensions")]
pub fn get_supported_signature_extensions() -> Vec<String> {
    // Tree-sitter covered
    let mut exts: Vec<String> = signatures::languages::supported_extensions()
        .into_iter()
        .map(|s| s.to_owned())
        .collect();

    // Heuristic-covered extensions (matching heuristic.rs extract_heuristic routes)
    const HEURISTIC_ONLY: &[&str] = &[
        "kt", "kotlin", "scala",          // JVM family
        "rb",                               // Ruby
        "php",                              // PHP
        "swift",                            // Swift
        "css", "scss", "less",             // CSS family
        "html", "htm",                      // HTML
        "sql", "tsql", "plsql",            // SQL
        "vue", "svelte",                    // SFC components
        "ex", "exs",                        // Elixir
        "hs", "lhs",                        // Haskell
        "lua",                              // Lua
        "erl", "hrl",                       // Erlang
    ];
    for ext in HEURISTIC_ONLY {
        if !exts.iter().any(|e| e == ext) {
            exts.push(ext.to_string());
        }
    }
    exts.sort();
    exts
}

// ── YAML ──────────────────────────────────────────────────────────────────────

#[napi(js_name = "jsonToYamlString")]
pub fn json_to_yaml_string(json_object: serde_json::Value, config: Option<YamlConversionConfig>) -> String {
    let sort_keys     = config.as_ref().and_then(|c| c.sort_keys).unwrap_or(false);
    let priority_keys = config
        .as_ref()
        .and_then(|c| c.keys_priority.as_deref())
        .map(|v| v.to_vec())
        .unwrap_or_default();
    yaml_utils::json_to_yaml_string_inner(json_object, sort_keys, &priority_keys)
}

// ── Config introspection (benchmark / tooling) ──────────────────────────────────

/// Returns the full MINIFY_CONFIG as a JS-compatible object.
/// Shape: `{ fileTypes: Record<string, { strategy: string, comments: string | string[] | null }> }`
#[napi(js_name = "getMINIFY_CONFIG")]
pub fn get_minify_config() -> serde_json::Value {
    let file_types: std::collections::HashMap<String, serde_json::Value> = config::minify_config()
        .iter()
        .map(|(ext, cfg)| {
            let comments: serde_json::Value = match cfg.comments {
                None => serde_json::Value::Null,
                Some(groups) if groups.len() == 1 =>
                    serde_json::Value::String(groups[0].to_string()),
                Some(groups) => serde_json::Value::Array(
                    groups.iter().map(|g| serde_json::Value::String((*g).to_string())).collect(),
                ),
            };
            (ext.to_string(), serde_json::json!({ "strategy": cfg.strategy, "comments": comments }))
        })
        .collect();
    serde_json::json!({ "fileTypes": file_types })
}


// ── Private helpers ───────────────────────────────────────────────────────────

fn parse_comment_groups(val: &Option<serde_json::Value>) -> Vec<String> {
    match val {
        None => vec![],
        Some(serde_json::Value::String(s)) => vec![s.clone()],
        Some(serde_json::Value::Array(arr)) => arr
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_owned()))
            .collect(),
        _ => vec![],
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── file extension ────────────────────────────────────────────────────────
    #[test]
    fn test_get_extension_basic()    { assert_eq!(get_extension("foo.ts".into(), None), "ts"); }
    #[test]
    fn test_get_extension_lowercase(){ assert_eq!(get_extension("Foo.TS".into(), Some(GetExtensionOptions { lowercase: Some(true), fallback: None })), "ts"); }
    #[test]
    fn test_get_extension_dotfile()  { assert_eq!(get_extension(".gitignore".into(), Some(GetExtensionOptions { lowercase: Some(true), fallback: None })), "gitignore"); }
    #[test]
    fn test_get_extension_no_ext()   { assert_eq!(get_extension("Makefile".into(), Some(GetExtensionOptions { lowercase: Some(false), fallback: Some("txt".into()) })), "txt"); }

    // ── comment removal ───────────────────────────────────────────────────────
    #[test]
    fn test_remove_c_style() {
        let src  = "int x = 1; // comment\nint y = 2; /* block */ int z;";
        let out  = remove_comments(src.into(), serde_json::Value::String("c-style".into()));
        assert!(!out.contains("comment"));
        assert!(!out.contains("block"));
        assert!(out.contains("int x"));
        assert!(out.contains("int z"));
    }

    #[test]
    fn test_remove_hash() {
        let src = "x = 1 # inline\n# full line\ny = 2";
        let out = remove_comments(src.into(), serde_json::Value::String("hash".into()));
        assert!(!out.contains("inline"));
        assert!(out.contains("x = 1"));
        assert!(out.contains("y = 2"));
    }

    // ── JSON strategies ───────────────────────────────────────────────────────
    #[test]
    fn test_minify_json_core() {
        let r = minify_json_core("{\"a\": 1,  \"b\": 2 }".into());
        assert_eq!(r.content, r#"{"a":1,"b":2}"#);
        assert!(!r.failed);
    }

    #[test]
    fn test_minify_json_core_jsonc() {
        let src = r#"{ // comment
  "key": "value", // trailing comma
}"#;
        let r = minify_json_core(src.into());
        assert!(!r.failed);
        assert!(r.content.contains("key"));
    }

    // ── conservative / code core ──────────────────────────────────────────────
    #[test]
    fn test_minify_conservative_strips_comments() {
        let cfg = FileTypeMinifyConfig { strategy: "conservative".into(), comments: Some(json!(["c-style"])) };
        let out = minify_conservative_core("int x; // comment\nint y;".into(), cfg);
        assert!(!out.contains("comment"));
        assert!(out.contains("int x"));
    }

    #[test]
    fn test_minify_code_core_collapses_blanks() {
        let src = "a\n\n\n\nb";
        let out = minify_code_core(src.into());
        // max 2 blank lines
        assert!(!out.contains("\n\n\n"));
        assert!(out.contains('a') && out.contains('b'));
    }

    // ── YAML ─────────────────────────────────────────────────────────────────
    #[test]
    fn test_yaml_basic() {
        let val = json!({"z": 1, "a": 2});
        let out = json_to_yaml_string(val, Some(YamlConversionConfig { sort_keys: Some(true), keys_priority: None }));
        // 'a' should come before 'z' when sorted
        let a_pos = out.find('a').unwrap();
        let z_pos = out.find('z').unwrap();
        assert!(a_pos < z_pos);
    }

    #[test]
    fn test_yaml_priority_keys() {
        let val = json!({"c": 3, "a": 1, "b": 2});
        let out = json_to_yaml_string(val, Some(YamlConversionConfig {
            sort_keys:     None,
            keys_priority: Some(vec!["b".into(), "c".into()]),
        }));
        let b_pos = out.find('b').unwrap();
        let c_pos = out.find('c').unwrap();
        let a_pos = out.find("a:").unwrap();
        assert!(b_pos < c_pos);
        assert!(c_pos < a_pos);
    }

    // ── apply helpers ─────────────────────────────────────────────────────────
    #[test]
    fn test_apply_content_view_json() {
        let src = r#"{"a":1,"b":  2}"#;
        let out = apply_content_view_minification(src.into(), "foo.json".into());
        // Should return as-is (already valid JSON, no JSONC noise)
        assert!(out.contains('a'));
    }

    #[test]
    fn test_apply_content_view_markdown() {
        let src = "# Title\n\nText <!-- hidden --> end\n";
        let out = apply_content_view_minification(src.into(), "readme.md".into());
        assert!(out.contains("Title"));
        assert!(!out.contains("hidden"));
    }

    // ── tree-sitter signatures ────────────────────────────────────────────────
    #[test]
    fn test_signatures_typescript() {
        let src = r#"
export function add(a: number, b: number): number {
  return a + b;
}

export class Calc {
  value: number = 0;
  multiply(x: number): number {
    return this.value * x;
  }
}
"#;
        let out = extract_signatures(src.into(), "calc.ts".into());
        assert!(out.is_some(), "should extract signatures from TS");
        let s = out.unwrap();
        assert!(s.contains("add"), "should include function name");
        assert!(s.contains("Calc"), "should include class name");
        assert!(!s.contains("return a + b"), "body should be dropped");
    }

    #[test]
    fn test_signatures_python() {
        let src = r#"
import os

class Foo:
    name: str

    def bar(self, x: int) -> str:
        return str(x)

def top_level():
    pass
"#;
        let out = extract_signatures(src.into(), "foo.py".into());
        assert!(out.is_some());
        let s = out.unwrap();
        assert!(s.contains("def bar"), "should include method signature");
        assert!(s.contains("def top_level"), "should include top-level function");
        assert!(!s.contains("pass"), "body should be dropped");
        assert!(!s.contains("return str"), "body should be dropped");
    }

    #[test]
    fn test_signatures_rust() {
        let src = r#"
pub fn greet(name: &str) -> String {
    format!("Hello, {}", name)
}

pub struct Point { x: f64, y: f64 }

impl Point {
    pub fn distance(&self, other: &Point) -> f64 {
        ((self.x - other.x).powi(2) + (self.y - other.y).powi(2)).sqrt()
    }
}
"#;
        let out = extract_signatures(src.into(), "geo.rs".into());
        assert!(out.is_some());
        let s = out.unwrap();
        assert!(s.contains("greet"), "should include function");
        assert!(!s.contains("format!"), "body should be dropped");
    }

    #[test]
    fn test_signatures_go() {
        let src = r#"
package main

import "fmt"

func Add(a, b int) int {
    return a + b
}

type Server struct {
    Port int
}

func (s *Server) Start() error {
    fmt.Println("starting")
    return nil
}
"#;
        let out = extract_signatures(src.into(), "main.go".into());
        assert!(out.is_some());
        let s = out.unwrap();
        assert!(s.contains("Add") || s.contains("func"), "should include function");
        assert!(!s.contains("Println"), "body should be dropped");
    }

    #[test]
    fn test_signatures_java() {
        let src = r#"
public class Calculator {
    private int value;

    public Calculator(int initial) {
        this.value = initial;
    }

    public int add(int x) {
        return value + x;
    }
}
"#;
        let out = extract_signatures(src.into(), "Calculator.java".into());
        assert!(out.is_some());
        let s = out.unwrap();
        assert!(s.contains("Calculator") || s.contains("add"), "should include class/methods");
        assert!(!s.contains("return value"), "body should be dropped");
    }

    #[test]
    fn test_signatures_c() {
        let src = r#"
#include <stdio.h>

int add(int a, int b) {
    return a + b;
}

void greet(const char *name) {
    printf("Hello, %s\n", name);
}
"#;
        let out = extract_signatures(src.into(), "math.c".into());
        assert!(out.is_some());
        let s = out.unwrap();
        assert!(s.contains("add") || s.contains("int"), "should include function signature");
        assert!(!s.contains("printf"), "body should be dropped");
    }
}

#[cfg(test)]
mod parity_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parity_yaml_multiline_and_sort() {
        let obj = json!({"z": "last", "a": "first\nsecond line", "b": 42, "c": true});
        let out = json_to_yaml_string(obj, Some(YamlConversionConfig { sort_keys: Some(true), keys_priority: None }));
        eprintln!("=== Rust YAML (sorted, multiline) ===\n{}", out);
        // Keys sorted: a < b < c < z
        let a = out.find("a:").unwrap();
        let b = out.find("b:").unwrap();
        let c = out.find("c:").unwrap();
        let z = out.find("z:").unwrap();
        assert!(a < b && b < c && c < z, "keys not sorted: a={a} b={b} c={c} z={z}");
    }

    #[test]
    fn parity_conservative_py() {
        let cfg = FileTypeMinifyConfig { strategy: "conservative".into(), comments: Some(json!("hash")) };
        let out = minify_conservative_core("x = 1 # comment\n# full line\ny = 2".into(), cfg);
        eprintln!("=== Rust conservative (py) === {:?}", out);
        assert!(!out.contains("comment"));
        assert!(!out.contains("full line"));
        assert!(out.contains("x = 1") && out.contains("y = 2"));
    }

    #[test]
    fn parity_code_core_blanks() {
        // TS: "a\n\n\n\nb" → "a\n\nb"  (max 1 blank line)
        let out = minify_code_core("a\n\n\n\nb".into());
        eprintln!("=== Rust codeCore === {:?}", out);
        assert_eq!(out, "a\n\nb", "should match TS output exactly");
    }

    #[test]
    fn parity_sig_py() {
        let src = "\nimport os\n\nclass Foo:\n    name: str\n\n    def bar(self, x: int) -> str:\n        return str(x)\n\ndef top_level():\n    pass\n";
        let out = extract_signatures(src.into(), "foo.py".into());
        eprintln!("=== Rust sig py ===\n{}", out.as_deref().unwrap_or("None"));
        let s = out.unwrap();
        assert!(s.contains("import os"), "must keep import");
        assert!(s.contains("class Foo"), "must keep class");
        assert!(s.contains("def bar"), "must keep method sig");
        assert!(s.contains("def top_level"), "must keep top-level def");
        assert!(!s.contains("return str"), "body dropped");
        assert!(!s.contains("pass"), "body dropped");
    }

    #[test]
    fn parity_sig_ts() {
        let src = "\nexport function add(a: number, b: number): number {\n  return a + b;\n}\n\nexport class Calc {\n  value: number = 0;\n  multiply(x: number): number {\n    return this.value * x;\n  }\n}\n";
        let out = extract_signatures(src.into(), "calc.ts".into());
        eprintln!("=== Rust sig ts ===\n{}", out.as_deref().unwrap_or("None"));
        let s = out.unwrap();
        assert!(s.contains("add"), "function preserved");
        assert!(s.contains("Calc"), "class preserved");
        assert!(s.contains("value"), "field preserved");
        assert!(s.contains("multiply"), "method sig preserved");
        assert!(!s.contains("return a + b"), "body dropped");
        assert!(!s.contains("this.value * x"), "body dropped");
    }
}
