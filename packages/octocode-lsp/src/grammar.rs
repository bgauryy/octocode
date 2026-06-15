use std::collections::HashMap;
use std::path::Path;
use std::sync::LazyLock;
use tree_sitter::{Language, Parser};

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

/// Language objects are pre-built once at first use and reused for every subsequent
/// `grammar_for_file` call.  `Language` is `Copy + Send + Sync`, so storing it in
/// a `LazyLock<HashMap>` is safe and avoids repeated FFI calls per symbol lookup.
static GRAMMAR_MAP: LazyLock<HashMap<&'static str, GrammarSpec>> =
    LazyLock::new(init_grammar_map);

pub fn grammar_for_file(file_path: &str) -> Option<&'static GrammarSpec> {
    let ext = Path::new(file_path)
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())?;
    GRAMMAR_MAP.get(ext.as_str())
}

fn init_grammar_map() -> HashMap<&'static str, GrammarSpec> {
    let mut map = HashMap::with_capacity(32);

    let rows: &[(&[&str], &str, Language)] = &[
        (
            &["ts", "mts", "cts"],
            "typescript",
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        ),
        (
            &["tsx"],
            "typescriptreact",
            tree_sitter_typescript::LANGUAGE_TSX.into(),
        ),
        (
            &["js", "mjs", "cjs", "jsx"],
            "javascript",
            tree_sitter_javascript::LANGUAGE.into(),
        ),
        (&["py", "pyi"], "python", tree_sitter_python::LANGUAGE.into()),
        (&["go"], "go", tree_sitter_go::LANGUAGE.into()),
        (&["rs"], "rust", tree_sitter_rust::LANGUAGE.into()),
        (&["java"], "java", tree_sitter_java::LANGUAGE.into()),
        (&["c", "h"], "c", tree_sitter_c::LANGUAGE.into()),
        (&["cpp", "cc", "cxx", "hpp"], "cpp", cpp_language()),
        (&["cs"], "csharp", csharp_language()),
        (
            &["sh", "bash", "zsh"],
            "shellscript",
            tree_sitter_bash::LANGUAGE.into(),
        ),
        (
            &["json", "jsonc"],
            "json",
            tree_sitter_json::LANGUAGE.into(),
        ),
        (
            &["yaml", "yml"],
            "yaml",
            tree_sitter_yaml::LANGUAGE.into(),
        ),
        (&["toml"], "toml", tree_sitter_toml_ng::LANGUAGE.into()),
        (
            &["html", "htm"],
            "html",
            tree_sitter_html::LANGUAGE.into(),
        ),
        (&["css"], "css", tree_sitter_css::LANGUAGE.into()),
        (&["scss"], "scss", tree_sitter_scss::language()),
        (&["less"], "less", tree_sitter_less::language()),
    ];

    for (exts, language_id, language) in rows {
        for &ext in *exts {
            map.insert(
                ext,
                GrammarSpec {
                    language_id,
                    language: language.clone(),
                },
            );
        }
    }

    map
}

#[cfg(feature = "tree-sitter-cpp")]
fn cpp_language() -> Language {
    tree_sitter_cpp::LANGUAGE.into()
}

#[cfg(not(feature = "tree-sitter-cpp"))]
fn cpp_language() -> Language {
    tree_sitter_c::LANGUAGE.into()
}

#[cfg(feature = "tree-sitter-c-sharp")]
fn csharp_language() -> Language {
    tree_sitter_c_sharp::LANGUAGE.into()
}

#[cfg(not(feature = "tree-sitter-c-sharp"))]
fn csharp_language() -> Language {
    tree_sitter_c::LANGUAGE.into()
}

#[cfg(test)]
mod tests {
    use super::grammar_for_file;

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
            ("demo.cpp", "cpp", "void target() {}\n"),
            ("demo.cs", "csharp", "class Target { void target() {} }\n"),
            ("demo.sh", "shellscript", "target() { echo ok; }\n"),
            ("demo.json", "json", "{\"target\": true}\n"),
            ("demo.yaml", "yaml", "target: true\n"),
            ("demo.toml", "toml", "target = true\n"),
            ("demo.html", "html", "<div id=\"target\"></div>\n"),
            ("demo.css", "css", ".target { color: red; }\n"),
            ("demo.scss", "scss", ".target { color: red; }\n"),
            ("demo.less", "less", ".target { color: red; }\n"),
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
