//! The dialect of a `.h` header. C and C++ share the extension, and neither
//! the C nor the C++ standard names one; compilers take the including
//! translation unit's language (scip-clang indexes a header inside the
//! translation units that include it) and clangd's fallback command reads a
//! bare `.h` as C++. Here a header reads as C++ only when it holds syntax C
//! cannot (a `namespace` or `template` declaration, a `class`, `using
//! namespace`, `extern "C++"`), and as CUDA (C++ with execution spaces)
//! when a declaration names `__global__` or `__device__`; every other header
//! stays C, so a C header's identifiers that are C++ keywords (`new`,
//! `this`) never break its parse.

/// Grammar extension for a `.h` header holding `content`, or `None` when
/// it stays C (or its dialect's grammar is not compiled in).
pub(super) fn header_grammar(content: &str) -> Option<&'static str> {
    // A macro may spell an execution space for both dialects
    // (`#define HD __host__ __device__`); only a declaration proves one.
    let cuda = content.lines().any(|line| {
        !line.trim_start().starts_with('#')
            && line
                .split(|c: char| !is_ident(c))
                .any(|word| matches!(word, "__global__" | "__device__"))
    });
    if cuda && cfg!(feature = "tree-sitter-cuda") {
        Some("cuh")
    } else if (cuda || content.lines().any(cpp_only_line)) && cfg!(feature = "tree-sitter-cpp") {
        Some("hpp")
    } else {
        None
    }
}

fn is_ident(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// The text after `keyword` when `line` starts with it as a whole word.
fn after_keyword<'l>(line: &'l str, keyword: &str) -> Option<&'l str> {
    let rest = line.strip_prefix(keyword)?;
    (!rest.starts_with(is_ident)).then(|| rest.trim_start())
}

/// A line opening a declaration only C++ has. Each form starts the line, so
/// comments, strings and expressions never match.
fn cpp_only_line(line: &str) -> bool {
    let line = line.trim_start();
    let starts_name = |rest: &str| rest.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_');
    if let Some(rest) = after_keyword(line, "namespace") {
        return starts_name(rest) || rest.starts_with('{');
    }
    if let Some(rest) = after_keyword(line, "template") {
        return rest.starts_with('<');
    }
    if let Some(rest) = after_keyword(line, "using") {
        return after_keyword(rest, "namespace").is_some_and(starts_name);
    }
    if let Some(rest) = after_keyword(line, "class") {
        return starts_name(rest);
    }
    after_keyword(line, "extern").is_some_and(|rest| rest.starts_with("\"C++\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpp_only_declarations_make_a_header_cpp() {
        for header in [
            "namespace cutlass {\nnamespace library {\nvoid f();\n}\n}\n",
            "namespace {\nint hidden;\n}\n",
            "template <typename T>\nT twice(T x) { return x + x; }\n",
            "template<class T> struct Box;\n",
            "using namespace std;\n",
            "class Widget {\n public:\n  void draw();\n};\n",
            "  class Widget : public Base\n  {\n  };\n",
            "extern \"C++\" {\nint f();\n}\n",
        ] {
            let expected = cfg!(feature = "tree-sitter-cpp").then_some("hpp");
            assert_eq!(header_grammar(header), expected, "{header}");
        }
    }

    #[test]
    fn c_headers_stay_c() {
        for header in [
            "#ifndef API_H\n#define API_H\nint compute(int x);\n#endif\n",
            // The C++ linkage guard of a C header is C.
            "#ifdef __cplusplus\nextern \"C\" {\n#endif\nint f(void);\n#ifdef __cplusplus\n}\n#endif\n",
            // C++ keywords as C identifiers, and keywords inside comments.
            "struct node { struct node *new; int class; };\nint template_count;\n",
            "/* use namespace\n * template <T> */\n// class Widget\nint f(void);\n",
            "int namespace_id;\nint templates;\nvoid classify(void);\n",
        ] {
            assert_eq!(header_grammar(header), None, "{header}");
        }
    }

    #[test]
    fn cuda_execution_spaces_make_a_cpp_header_cuda() {
        let header =
            "namespace k {\n__global__ void scale(float* x);\n__device__ float sq(float v);\n}\n";
        let expected = if cfg!(feature = "tree-sitter-cuda") {
            Some("cuh")
        } else {
            cfg!(feature = "tree-sitter-cpp").then_some("hpp")
        };
        assert_eq!(header_grammar(header), expected);
        assert_eq!(header_grammar("__device__ float sq(float v);\n"), expected);
        // A macro spelling an execution space proves nothing.
        assert_eq!(
            header_grammar("#define HD __host__ __device__\nint f(void);\n"),
            None
        );
    }
}
