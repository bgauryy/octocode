use std::collections::HashMap;
use std::path::Path;
use std::sync::LazyLock;
use tree_sitter::{Language, Parser};

use crate::signatures::languages;

pub struct GrammarSpec {
    pub language_id: &'static str,
    language: Language,
}

impl GrammarSpec {
    pub fn parser(&self) -> Option<Parser> {
        let mut parser = Parser::new();
        parser.set_language(&self.language).ok()?;
        Some(parser)
    }
}

/// Grammars are pre-built once at first use and reused for every subsequent
/// `grammar_for_file` call. `Language` is `Clone + Send + Sync`, so storing it
/// in a `LazyLock<HashMap>` is safe and avoids repeated FFI calls per lookup.
static GRAMMAR_MAP: LazyLock<HashMap<&'static str, GrammarSpec>> = LazyLock::new(init_grammar_map);

pub fn grammar_for_file(file_path: &str) -> Option<&'static GrammarSpec> {
    let ext = Path::new(file_path)
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())?;
    GRAMMAR_MAP.get(ext.as_str())
}

/// Derived from the single language registry (`signatures::languages`): every
/// entry carrying a `language_id` contributes its extensions + grammar. There is
/// no second grammar table, so the LSP grammar map can never drift from the
/// registry the signature/structural layers read.
fn init_grammar_map() -> HashMap<&'static str, GrammarSpec> {
    let mut map = HashMap::with_capacity(32);

    for entry in languages::all_entries() {
        let Some(language_id) = entry.language_id else {
            continue;
        };
        for &ext in entry.extensions {
            map.insert(
                ext,
                GrammarSpec {
                    language_id,
                    language: entry.language.clone(),
                },
            );
        }
    }

    map
}

#[cfg(test)]
mod tests {
    use super::grammar_for_file;

    #[test]
    fn optional_grammars_have_no_cross_language_fallback() {
        for (enabled, extensions) in [
            (
                cfg!(feature = "tree-sitter-cpp"),
                &["cpp", "cc", "cxx", "hpp", "hh", "hxx"][..],
            ),
            (cfg!(feature = "tree-sitter-c-sharp"), &["cs"][..]),
            (cfg!(feature = "tree-sitter-swift"), &["swift"][..]),
        ] {
            for ext in extensions {
                assert_eq!(
                    grammar_for_file(&format!("fixture.{ext}")).is_some(),
                    enabled,
                    ".{ext}"
                );
            }
        }
    }

    #[test]
    fn requested_language_matrix_has_native_grammars() {
        let cases = [
            ("demo.ts", "typescript", "export const target = 1;"),
            (
                "demo.tsx",
                "typescriptreact",
                "export const Target = () => <div />;",
            ),
            ("demo.js", "javascript", "export function target() {}"),
            (
                "demo.jsx",
                "javascript",
                "export const Target = () => <div />;",
            ),
            ("demo.py", "python", "def target():\n    return 1\n"),
            ("demo.go", "go", "package main\nfunc target() {}\n"),
            ("demo.rs", "rust", "fn target() {}\n"),
            ("demo.java", "java", "class Target { void target() {} }\n"),
            ("demo.c", "c", "void target() {}\n"),
            #[cfg(feature = "tree-sitter-cpp")]
            ("demo.cpp", "cpp", "void target() {}\n"),
            #[cfg(feature = "tree-sitter-c-sharp")]
            ("demo.cs", "csharp", "class Target { void target() {} }\n"),
            ("demo.json", "json", "{\"target\": true}\n"),
            ("demo.yaml", "yaml", "target: true\n"),
            ("demo.toml", "toml", "target = true\n"),
            ("demo.html", "html", "<div id=\"target\"></div>\n"),
            ("demo.css", "css", ".target { color: red; }\n"),
            ("demo.scss", "scss", ".target { color: red; }\n"),
        ];

        for (file_name, language_id, source) in cases {
            let Some(spec) = grammar_for_file(file_name) else {
                panic!("missing grammar for {file_name}");
            };
            assert_eq!(spec.language_id, language_id);
            let Some(mut parser) = spec.parser() else {
                panic!("failed to create parser for {file_name}");
            };
            let Some(tree) = parser.parse(source, None) else {
                panic!("failed to parse {file_name}");
            };
            assert!(
                !tree.root_node().has_error(),
                "native grammar failed for {file_name}"
            );
        }
    }
}
