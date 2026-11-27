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
        let Some(tree) =
            spec.parse_before(source, crate::signatures::extractor::whole_file_deadline())
        else {
            panic!("failed to parse {file_name}");
        };
        assert!(
            !tree.root_node().has_error(),
            "native grammar failed for {file_name}"
        );
    }
}
