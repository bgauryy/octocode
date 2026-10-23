//! Grammar capabilities, selection by name and family, and extension dispatch.

use super::*;

#[test]
fn excluded_grammars_report_unsupported_across_native_capabilities() {
    for ext in [
        "sh", "bash", "zsh", "vue", "svelte", "astro", "dart", "less", "ml", "mli", "jl", "r",
        "erl", "hrl", "ex", "exs", "tf", "hcl", "tfvars", "proto", "toml", "lua", "zig", "rb",
        "rake", "gemspec", "ru", "scss", "sql", "swift", "yaml", "yml", "css", "htm", "html",
        "json", "jsonc", "kt", "kts", "php",
    ] {
        assert!(find_entry(ext).is_none(), ".{ext} must not load a grammar");
        assert!(!supported_extensions().contains(&ext));
        let file = format!("fixture.{ext}");
        assert!(crate::lsp::grammar::grammar_for_file(&file).is_none());
        let result = crate::structural::evaluate_source(
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
fn configured_capabilities_match_the_exact_first_class_extension_set() {
    let mut expected = vec![
        "c", "cjs", "cts", "go", "h", "java", "js", "jsx", "mjs", "mts", "py", "pyi", "rs", "ts",
        "tsx",
    ];
    if cfg!(feature = "tree-sitter-asm") {
        expected.extend(["asm", "assembly", "s"]);
    }
    if cfg!(feature = "tree-sitter-cpp") {
        expected.extend(["cc", "cpp", "cxx", "hh", "hpp", "hxx"]);
    }
    if cfg!(feature = "tree-sitter-c-sharp") {
        expected.push("cs");
    }
    if cfg!(feature = "tree-sitter-cuda") {
        expected.extend(["cu", "cuh"]);
    }
    if cfg!(feature = "tree-sitter-scala") {
        expected.extend(["sbt", "sc", "scala"]);
    }
    expected.sort_unstable();
    for (name, mut actual) in [
        (
            "structural",
            supported_extensions()
                .into_iter()
                .map(str::to_owned)
                .collect(),
        ),
        (
            "signature",
            supported_extensions()
                .into_iter()
                .map(str::to_owned)
                .collect(),
        ),
        (
            "graph",
            crate::signatures::graph_facts::graph_fact_extensions(),
        ),
    ] {
        actual.sort_unstable();
        assert_eq!(actual, expected, "{name} capability drift");
    }
}

#[test]
fn removed_languages_have_no_analysis_minifier_or_builtin_server_route() {
    for ext in ["toml", "lua", "zig"] {
        let file = format!("fixture.{ext}");
        assert!(crate::signatures::extract_signatures_inner("target(value);", &file).is_err());
        assert!(crate::signatures::extract_graph_facts("target(value);", &file).is_none());
        assert!(
            !crate::signatures::graph_facts::graph_fact_extensions()
                .iter()
                .any(|item| item == ext)
        );
        assert!(!crate::minify::config::minify_config().contains_key(ext));
        assert!(crate::lsp::config::detect_language_id(&file).is_none());
    }
    assert!(crate::minify::comment_remover::rules_for("lua").is_none());
}

#[test]
fn selection_names_one_grammar_and_families_span_aliases_and_cpp_headers() {
    use super::{family_extensions, js_source_swaps, label, parser_for_path, select, select_one};
    let names = |selector: &str| select(selector).map(|entry| entry.name).collect::<Vec<_>>();
    assert_eq!(names("typescript"), ["TypeScript", "TSX"]);
    assert_eq!(
        select_one("typescript").map(|entry| entry.name),
        Some("TypeScript")
    );
    assert_eq!(select_one("TSX").map(|entry| entry.name), Some("TSX"));
    assert_eq!(select_one("h").map(|entry| entry.name), Some("C"));
    assert!(select_one("no-such-grammar").is_none());
    let family =
        |selector: &str| family_extensions(selector).map(|set| set.into_iter().collect::<Vec<_>>());
    assert_eq!(
        family("typescript").as_deref(),
        Some(&["cts", "mts", "ts", "tsx"].map(str::to_owned)[..])
    );
    assert_eq!(family(".tsx").as_deref(), Some(&["tsx".to_owned()][..]));
    assert_eq!(family(".nope"), None);
    #[cfg(feature = "tree-sitter-cpp")]
    {
        assert!(family("cpp").is_some_and(|set| set.contains(&"h".to_owned())));
        assert_eq!(parser_for_path("c++", "a/widget.h"), "cpp");
        // Every spelling of the C++ grammar resolves to it before a file is
        // compared: its extensions, with or without the dot, read `.h` as
        // C++, and a `.cc` selector reads a `.hpp` file. In a directory a
        // `.ext` selector still picks only that extension.
        for selector in ["hpp", "cxx", ".hpp", ".cc"] {
            assert!(super::parses_path(selector, "a/widget.h"), "{selector}");
            assert!(super::parses_path(selector, "a/widget.hpp"), "{selector}");
            assert_eq!(parser_for_path(selector, "a/widget.h"), "cpp", "{selector}");
        }
        for selector in ["hpp", "cxx"] {
            assert!(
                family(selector).is_some_and(
                    |set| set.contains(&"h".to_owned()) && set.contains(&"hpp".to_owned())
                ),
                "{selector}"
            );
        }
        assert_eq!(family(".cc").as_deref(), Some(&["cc".to_owned()][..]));
        assert_eq!(parser_for_path(".cc", "a/widget.hpp"), "hpp");
        // C does not also parse C++ files.
        assert!(family("c").is_some_and(|set| !set.contains(&"hpp".to_owned())));
        assert!(!super::parses_path("c", "a/widget.hpp"));
        assert_eq!(parser_for_path("c", "a/widget.h"), "h");
    }
    assert_eq!(parser_for_path("typescript", "a/widget.tsx"), "tsx");
    assert_eq!(parser_for_path("typescript", "a/README"), "typescript");
    assert_eq!(parser_for_path("rust", "a/x.ts"), "rust");
    assert_eq!(select_one("rust").map(label).as_deref(), Some("rust"));
    assert_eq!(js_source_swaps("jsx"), ["tsx", "d.ts"]);
    assert!(js_source_swaps("ts").is_empty());
}

#[test]
fn js_ts_dispatch_extensions_are_the_registry_javascript_and_typescript_entries() {
    let mut registry: Vec<&str> = supported_extensions()
        .into_iter()
        .filter(|extension| {
            find_entry(extension).is_some_and(|entry| {
                matches!(
                    entry.language_id,
                    Some("typescript" | "typescriptreact" | "javascript")
                )
            })
        })
        .collect();
    registry.sort_unstable();
    let mut dispatch = crate::text::JS_TS_EXTENSIONS.to_vec();
    dispatch.sort_unstable();
    assert_eq!(dispatch, registry);
}

#[test]
fn c_family_extensions_are_the_enabled_c_cpp_and_cuda_grammars() {
    for extension in ["c", "h"] {
        assert!(is_c_family_extension(extension), ".{extension}");
    }
    #[cfg(feature = "tree-sitter-cpp")]
    for extension in ["cpp", "hpp", "cc", "cxx", "hh", "hxx"] {
        assert!(is_c_family_extension(extension), ".{extension}");
    }
    #[cfg(feature = "tree-sitter-cuda")]
    for extension in ["cu", "cuh"] {
        assert!(is_c_family_extension(extension), ".{extension}");
    }
    for extension in ["rs", "py", "ts", "cs", "go", "java", "not-an-extension"] {
        assert!(!is_c_family_extension(extension), ".{extension}");
    }
}
