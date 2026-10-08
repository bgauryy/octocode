use tree_sitter::Language;

use crate::signatures::languages;
use crate::text::file_extension::extension_of;

/// The grammar of a file with an LSP language id, read from the single
/// language registry (`signatures::languages`).
pub(crate) struct GrammarSpec {
    pub(crate) language_id: &'static str,
    language: &'static Language,
}

impl GrammarSpec {
    pub(crate) fn parse_before(
        &self,
        content: &str,
        deadline: std::time::Instant,
    ) -> Option<tree_sitter::Tree> {
        crate::signatures::extractor::parse_before(content, self.language, deadline)
    }
}

/// The registry grammar for `file_path`'s extension; `None` when no entry
/// owns the extension or the entry carries no LSP language id.
pub(crate) fn grammar_for_file(file_path: &str) -> Option<GrammarSpec> {
    let entry = languages::find_entry(&extension_of(file_path, true, ""))?;
    Some(GrammarSpec {
        language_id: entry.language_id?,
        language: &entry.language,
    })
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
            (cfg!(feature = "tree-sitter-cuda"), &["cu", "cuh"][..]),
            (
                cfg!(feature = "tree-sitter-asm"),
                &["asm", "assembly", "s"][..],
            ),
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
            #[cfg(feature = "tree-sitter-scala")]
            (
                "demo.scala",
                "scala",
                "object Target { def target(): Unit = {} }\n",
            ),
            #[cfg(feature = "tree-sitter-cuda")]
            (
                "demo.cu",
                "cuda",
                "__global__ void target() {}\nvoid launch() { target<<<1, 1>>>(); }\n",
            ),
            #[cfg(feature = "tree-sitter-asm")]
            ("demo.asm", "asm", "target:\n  mov %rax, %rbx\n"),
        ];

        for (file_name, language_id, source) in cases {
            let Some(spec) = grammar_for_file(file_name) else {
                panic!("missing grammar for {file_name}");
            };
            assert_eq!(spec.language_id, language_id);
            let Some(tree) = spec.parse_before(
                source,
                std::time::Instant::now() + crate::signatures::extractor::AST_EXECUTION_TIMEOUT,
            ) else {
                panic!("failed to parse {file_name}");
            };
            assert!(
                !tree.root_node().has_error(),
                "native grammar failed for {file_name}"
            );
        }
    }
}
