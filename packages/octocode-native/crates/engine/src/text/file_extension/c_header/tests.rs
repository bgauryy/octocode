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
