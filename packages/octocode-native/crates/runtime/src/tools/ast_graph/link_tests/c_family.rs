//! C-family linking: header dialects, prototypes and definitions, linkage,
//! preprocessor recovery and assembly (C++ and CUDA idioms: `cpp.rs`).
use super::*;

pub(super) fn callers_at(root: &Path, target: &str, confidence: &str) -> Vec<String> {
    let out = ask_with(root, "callers", Some(target), |o| {
        o.confidence = Some(confidence.into());
    });
    assert_eq!(out.get("error"), None, "{out}");
    let mut ids = result_ids(&out)
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    ids.sort();
    ids
}

pub(super) fn callers(root: &Path, target: &str) -> Vec<String> {
    callers_at(root, target, "low")
}

fn language(root: &Path, file: &str) -> Value {
    ask(root, "node", Some(file))["node"]["language"].clone()
}

// ── Header dialect ──────────────────────────────────────────────────────────

/// CUTLASS shape: a `.h` header holding C++ (namespaces, a template) is read
/// as C++, so its template function is a declaration that `.cu` callers in
/// the same namespace reach, explicit template arguments and all.
#[test]
fn cpp_dot_h_headers_parse_as_cpp_and_link_template_calls() {
    let temp = fixture(&[
        (
            "src/reference/gemm_reference_operation.h",
            "#pragma once\nnamespace cutlass {\nnamespace library {\n\
             template <typename A, typename B = A>\n\
             void make_layouts(Manifest &manifest) {\n  make_gemm<A, B>(manifest);\n}\n\
             template <typename A, typename B>\nvoid make_gemm(Manifest &manifest) {}\n\
             }\n}\n",
        ),
        (
            "src/reference/gemm_f8.cu",
            "#include \"gemm_reference_operation.h\"\nnamespace cutlass {\nnamespace library {\n\
             void initialize_f8(Manifest &manifest) {\n  make_layouts<\n    float_e4m3_t,\n    float\n  >(manifest);\n}\n\
             }\n}\n",
        ),
        (
            "src/reference/gemm_s8.cu",
            "#include \"gemm_reference_operation.h\"\nnamespace cutlass {\nnamespace library {\n\
             void initialize_s8(Manifest &manifest) {\n  make_layouts<int8_t>(manifest);\n}\n\
             }\n}\n",
        ),
    ]);
    let root = ingested(&temp);
    let header = "src/reference/gemm_reference_operation.h";
    assert_eq!(language(root, header), "cpp");
    assert_eq!(
        callers(root, &format!("{header}#cutlass.library.make_layouts")),
        [
            "src/reference/gemm_f8.cu#cutlass.library.initialize_f8",
            "src/reference/gemm_s8.cu#cutlass.library.initialize_s8",
        ]
    );
    assert_eq!(
        callers(root, &format!("{header}#cutlass.library.make_gemm")),
        [format!("{header}#cutlass.library.make_layouts")]
    );
}

/// A plain C header stays C, even beside C++ sources.
#[test]
fn c_headers_stay_c() {
    let temp = fixture(&[
        (
            "api.h",
            "#ifdef __cplusplus\nextern \"C\" {\n#endif\nint compute(int x);\n#ifdef __cplusplus\n}\n#endif\n",
        ),
        (
            "lib.cpp",
            "#include \"api.h\"\nint run() { return compute(1); }\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(language(root, "api.h"), "c");
}

// ── C ───────────────────────────────────────────────────────────────────────

/// A `static` function has internal linkage: only its own translation unit
/// calls it. One in a header is visible to every file that includes it.
#[test]
fn c_static_functions_never_link_across_files() {
    let temp = fixture(&[
        (
            "a.c",
            "static int helper_one(void) { return 1; }\nint fa(void) { return helper_one(); }\n",
        ),
        ("sub/b.c", "int fb(void) { return helper_one(); }\n"),
        (
            "util.h",
            "static inline int clamp_it(int x) { return x; }\n",
        ),
        (
            "sub/c.c",
            "#include \"../util.h\"\nint fc(void) { return clamp_it(1); }\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(callers(root, "a.c#helper_one"), ["a.c#fa"]);
    assert_eq!(callers(root, "util.h#clamp_it"), ["sub/c.c#fc"]);
}

/// A header prototype and the definition in a `.c` file are one function:
/// callers in any directory, C or C++ (through `extern "C"`), reach the
/// definition, and the prototype is no second target. Two programs' `main`
/// make the same first call: each site links.
#[test]
fn c_calls_reach_the_definition_behind_a_header_prototype() {
    let temp = fixture(&[
        (
            "include/api.h",
            "#ifdef __cplusplus\nextern \"C\" {\n#endif\nint compute(int x);\n#ifdef __cplusplus\n}\n#endif\n",
        ),
        (
            "src/api.c",
            "#include \"../include/api.h\"\nint compute(int x) { return x; }\n",
        ),
        (
            "src/main.c",
            "#include \"../include/api.h\"\nint main(void) { return compute(1); }\n",
        ),
        (
            "app/tool.cpp",
            "#include \"../include/api.h\"\nint run_tool() { return compute(2); }\n",
        ),
        (
            "test/main.c",
            "#include \"../include/api.h\"\nint main(void) { return compute(3); }\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(
        callers(root, "src/api.c#compute"),
        [
            "app/tool.cpp#run_tool",
            "src/main.c#main",
            "test/main.c#main"
        ]
    );
    assert_eq!(callers(root, "include/api.h#compute"), Vec::<String>::new());
}

/// A forward `static` prototype in the same file is the same function as
/// its later definition: one target, high confidence.
#[test]
fn c_forward_declarations_link_their_own_definition() {
    let temp = fixture(&[(
        "x.c",
        "static int later(void);\nint first(void) { return later(); }\nstatic int later(void) { return 1; }\n",
    )]);
    let root = ingested(&temp);
    let callees = ask_with(root, "callees", Some("x.c#first"), |o| {
        o.confidence = Some("high".into());
    });
    assert_eq!(result_ids(&callees), ["x.c#later@3"], "{callees}");
}

/// Two programs each defining `usage`: each calls its own, and a third
/// file's call is ambiguous, so it links neither.
#[test]
fn c_same_extern_name_in_two_files_never_crosses() {
    let temp = fixture(&[
        (
            "tools/a.c",
            "void usage(void) {}\nint main_a(void) { usage(); return 0; }\n",
        ),
        (
            "tools2/b.c",
            "void usage(void) {}\nint main_b(void) { usage(); return 0; }\n",
        ),
        ("other/c.c", "int main_c(void) { usage(); return 0; }\n"),
    ]);
    let root = ingested(&temp);
    assert_eq!(callers(root, "tools/a.c#usage"), ["tools/a.c#main_a"]);
    assert_eq!(callers(root, "tools2/b.c#usage"), ["tools2/b.c#main_b"]);
}

/// Known limits, pinned: a call written inside a macro body is not expanded
/// at its use, and a call through a function pointer names no declaration.
#[test]
fn c_macro_bodies_and_function_pointers_link_nothing() {
    let temp = fixture(&[
        ("api.h", "int do_work(void);\n#define CALL_IT() do_work()\n"),
        ("api.c", "int do_work(void) { return 2; }\n"),
        (
            "main.c",
            "#include \"api.h\"\nstatic int local_only(void) { return 3; }\n\
             int main(void) { int (*cb)(void) = local_only; CALL_IT(); return cb(); }\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(callers(root, "api.c#do_work"), Vec::<String>::new());
    assert_eq!(callers(root, "main.c#local_only"), Vec::<String>::new());
}

/// A preprocessor branch tree-sitter cannot parse (`else` followed by an
/// `#ifdef`) is recovered locally: the other statements of the function
/// still link (libjpeg-turbo `djpeg.c` `main`).
#[test]
fn c_preprocessor_recovery_keeps_sibling_statements_linked() {
    let temp = fixture(&[
        ("err.c", "int std_error_init(int *e) { return *e; }\n"),
        (
            "main.c",
            "int main(int argc) {\n  int jerr = 0;\n  int r = std_error_init(&jerr);\n\
             if (argc > 1)\n    r = 1;\n  else\n#ifdef LOSSLESS\n    r = 2;\n#else\n    r = 3;\n#endif\n\
             return r;\n}\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(callers(root, "err.c#std_error_init"), ["main.c#main"]);
}

// ── Assembly ────────────────────────────────────────────────────────────────

/// Known limit, pinned: assembly is its own link family, so a C call to a
/// symbol an assembly file defines stays unresolved rather than guessed.
#[test]
fn c_calls_to_assembly_symbols_stay_unlinked() {
    let temp = fixture(&[
        (
            "simd/jsimd_neon.S",
            ".text\n.globl jsimd_idct_neon\njsimd_idct_neon:\n    ret\n",
        ),
        (
            "simd/jsimd.c",
            "void jsimd_idct_neon(void);\nvoid jsimd_idct(void) { jsimd_idct_neon(); }\n",
        ),
    ]);
    let root = ingested(&temp);
    let callees = ask_with(root, "callees", Some("simd/jsimd.c#jsimd_idct"), |o| {
        o.confidence = Some("low".into());
    });
    let ids = result_ids(&callees);
    assert!(
        ids.iter().all(|id| !id.starts_with("simd/jsimd_neon.S")),
        "{callees}"
    );
}
