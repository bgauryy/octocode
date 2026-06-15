use std::path::Path;
use tree_sitter::{Language, Parser};

#[derive(Clone, Copy)]
pub struct GrammarSpec {
    pub language_id: &'static str,
    language: fn() -> Language,
}

impl GrammarSpec {
    pub fn parser(self) -> Option<Parser> {
        let mut parser = Parser::new();
        parser.set_language(&(self.language)()).ok()?;
        Some(parser)
    }
}

pub fn grammar_for_file(file_path: &str) -> Option<GrammarSpec> {
    grammar_for_extension(&extension_key(file_path)?)
}

fn extension_key(file_path: &str) -> Option<String> {
    Path::new(file_path)
        .extension()
        .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
}

fn grammar_for_extension(extension: &str) -> Option<GrammarSpec> {
    let spec = match extension {
        "ts" | "mts" | "cts" => GrammarSpec {
            language_id: "typescript",
            language: || tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        },
        "tsx" => GrammarSpec {
            language_id: "typescriptreact",
            language: || tree_sitter_typescript::LANGUAGE_TSX.into(),
        },
        "js" | "mjs" | "cjs" | "jsx" => GrammarSpec {
            language_id: "javascript",
            language: || tree_sitter_javascript::LANGUAGE.into(),
        },
        "py" | "pyi" => GrammarSpec {
            language_id: "python",
            language: || tree_sitter_python::LANGUAGE.into(),
        },
        "go" => GrammarSpec {
            language_id: "go",
            language: || tree_sitter_go::LANGUAGE.into(),
        },
        "rs" => GrammarSpec {
            language_id: "rust",
            language: || tree_sitter_rust::LANGUAGE.into(),
        },
        "java" => GrammarSpec {
            language_id: "java",
            language: || tree_sitter_java::LANGUAGE.into(),
        },
        "c" | "h" => GrammarSpec {
            language_id: "c",
            language: || tree_sitter_c::LANGUAGE.into(),
        },
        "cpp" | "cc" | "cxx" | "hpp" => GrammarSpec {
            language_id: "cpp",
            language: cpp_language,
        },
        "cs" => GrammarSpec {
            language_id: "csharp",
            language: csharp_language,
        },
        "sh" | "bash" | "zsh" => GrammarSpec {
            language_id: "shellscript",
            language: || tree_sitter_bash::LANGUAGE.into(),
        },
        "json" | "jsonc" => GrammarSpec {
            language_id: "json",
            language: || tree_sitter_json::LANGUAGE.into(),
        },
        "yaml" | "yml" => GrammarSpec {
            language_id: "yaml",
            language: || tree_sitter_yaml::LANGUAGE.into(),
        },
        "toml" => GrammarSpec {
            language_id: "toml",
            language: || tree_sitter_toml_ng::LANGUAGE.into(),
        },
        "html" | "htm" => GrammarSpec {
            language_id: "html",
            language: || tree_sitter_html::LANGUAGE.into(),
        },
        "css" => GrammarSpec {
            language_id: "css",
            language: || tree_sitter_css::LANGUAGE.into(),
        },
        "scss" => GrammarSpec {
            language_id: "scss",
            language: tree_sitter_scss::language,
        },
        "less" => GrammarSpec {
            language_id: "less",
            language: tree_sitter_less::language,
        },
        _ => return None,
    };
    Some(spec)
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
