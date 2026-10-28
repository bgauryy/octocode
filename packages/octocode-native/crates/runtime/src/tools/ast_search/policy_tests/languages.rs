//! Explicit language and type selection: `langType`, header grammars and
//! include words.

use super::*;

#[test]
fn lang_type_is_validated_and_intersected_with_include() {
    let root = Fixture::new();
    root.write("a.ts", "oldCall(x);\n");
    root.write("b.py", "oldCall(x)\n");
    assert!(crate::contracts::validate_query("astSearch", json!({"operation":"match","path":root.0,"parser":"klingon","rule":{"pattern":"oldCall($A)"}})).is_err());

    let out = run(
        &root.0,
        json!({"operation":"match","path":root.0,"parser":"typescript","include":["*.ts","*.py"],"rule":{"pattern":"oldCall($A)"}}),
    )
    .expect("intersected include");
    let files = out["files"].as_array().expect("files");
    assert_eq!(files.len(), 1, "{out}");
    assert!(
        files[0]["path"]
            .as_str()
            .is_some_and(|p| p.ends_with("a.ts")),
        "{out}"
    );

    let mismatch = run(
        &root.0,
        json!({"operation":"match","path":root.0.join("b.py"),"parser":"typescript","rule":{"pattern":"oldCall($A)"}}),
    )
    .expect_err("single file outside language");
    assert_eq!(mismatch.code, ErrorCode::LanguageMismatch);

    root.write("notes.txt", "oldCall(x)\n");
    let unsupported = run(
        &root.0,
        json!({"operation":"match","path":root.0.join("notes.txt"),"rule":{"pattern":"oldCall($A)"}}),
    )
    .expect("unsupported single file");
    assert_eq!(unsupported["isPartial"], true, "{unsupported}");
    assert_ne!(unsupported["status"], "empty", "{unsupported}");
}

#[test]
fn include_selects_an_exact_extension_within_a_parser_family() {
    let root = Fixture::new();
    root.write("a.ts", "oldCall(x);\n");
    root.write("b.mts", "oldCall(y);\n");

    let exact = run(
        &root.0,
        json!({"operation":"match","path":root.0,"parser":"typescript","include":["*.ts"],"rule":{"pattern":"oldCall($A)"}}),
    )
    .expect("exact extension");
    let files = exact["files"].as_array().expect("files");
    assert_eq!(files.len(), 1, "{exact}");
    assert!(
        files[0]["path"]
            .as_str()
            .is_some_and(|p| p.ends_with("a.ts"))
    );

    let family = run(
        &root.0,
        json!({"operation":"match","path":root.0,"parser":"typescript","rule":{"pattern":"oldCall($A)"}}),
    )
    .expect("language family");
    assert_eq!(
        family["files"].as_array().expect("files").len(),
        2,
        "{family}"
    );
}

#[test]
fn cpp_header_can_use_explicit_cpp_grammar_without_changing_h_default() {
    let root = Fixture::new();
    let header = root.0.join("widget.h");
    std::fs::write(
        &header,
        "template <typename T> class Widget { public: T value; };\n",
    )
    .expect("header");

    let selected = run(
        &root.0,
        json!({"operation":"match","path":header,"parser":"cpp","rule":{"kind":"class_specifier"}}),
    )
    .expect("explicit C++ match");
    assert_eq!(selected["matchCount"], 1, "{selected}");

    let selected_directory = run(
        &root.0,
        json!({"operation":"match","path":root.0,"parser":"cpp","include":["*.h"],"rule":{"kind":"class_specifier"}}),
    )
    .expect("explicit C++ directory match");
    assert_eq!(selected_directory["matchCount"], 1, "{selected_directory}");

    let selected_directory_default = run(
        &root.0,
        json!({"operation":"match","path":root.0,"parser":"cpp","rule":{"kind":"class_specifier"}}),
    )
    .expect("C++ directory includes ambiguous headers");
    assert_eq!(
        selected_directory_default["matchCount"], 1,
        "{selected_directory_default}"
    );

    let tree = run(
        &root.0,
        json!({"operation":"syntaxTree","path":header,"parser":"cpp"}),
    )
    .expect("explicit C++ tree");
    assert_eq!(tree["isPartial"], false, "{tree}");
    let default_tree =
        run(&root.0, json!({"operation":"syntaxTree","path":header})).expect(".h defaults to C");
    assert_eq!(default_tree["isPartial"], true, "{default_tree}");
    assert!(
        tree["nodes"]
            .as_array()
            .is_some_and(|nodes| nodes.iter().any(|node| node["node"]
                .as_str()
                .and_then(|row| row.split(' ').nth(1))
                == Some("class_specifier"))),
        "{tree}"
    );

    let symbols = run(
        &root.0,
        json!({"operation":"symbols","path":header,"parser":"cpp"}),
    )
    .expect("explicit C++ symbols");
    assert!(
        outline_names(&symbols["symbols"]).contains(&"Widget"),
        "{symbols}"
    );

    let wrong_file = root.0.join("wrong.c");
    std::fs::write(&wrong_file, "int value;\n").expect("C file");
    let mismatch = run(
        &root.0,
        json!({"operation":"match","path":wrong_file,"parser":"cpp","rule":{"kind":"declaration"}}),
    )
    .expect_err("C source is not an ambiguous header");
    assert_eq!(mismatch.code, ErrorCode::LanguageMismatch);
}

#[test]
fn directory_symbols_use_path_scoped_header_parser_and_preserve_it_in_next_page() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join("include")).expect("include directory");
    std::fs::create_dir_all(root.0.join("legacy")).expect("legacy directory");
    root.write("include/widget.h", "namespace Space { class Widget {}; }\n");
    root.write("legacy/plain.h", "struct Plain { int x; };\n");
    let result = crate::tools::ast_search::test_rows::run_window(
        &root.0,
        json!({"operation":"symbols","path":root.0,"parserGlobs":{"cpp":["include/**/*.h"]}}),
        1,
    )
    .expect("directory symbols");
    assert_eq!(result["filesScanned"], 2, "{result}");
    assert_eq!(
        result["next"]["nextPage"]["input"]["queries"][0]["parserGlobs"]["cpp"][0],
        "include/**/*.h"
    );
    let next_query = result["next"]["nextPage"]["input"]["queries"][0].clone();
    let next =
        crate::tools::ast_search::test_rows::run_window(&root.0, next_query, 1).expect("next page");
    let files = [&result["files"][0], &next["files"][0]];
    let cpp = files
        .iter()
        .find(|file| file["path"] == "include/widget.h")
        .expect("C++ header retained across pages");
    let c = files
        .iter()
        .find(|file| file["path"] == "legacy/plain.h")
        .expect("default C header retained across pages");
    assert_eq!(cpp["symbols"].as_array().map(Vec::len), Some(1));
    let namespace = &cpp["symbols"][0];
    assert_eq!(outline_name(namespace), "Space");
    assert_eq!(namespace["kind"], "namespace");
    assert_eq!(namespace["members"].as_array().map(Vec::len), Some(1));
    assert_eq!(outline_name(&namespace["members"][0]), "Widget");
    assert_eq!(namespace["members"][0]["kind"], "class");
    assert_eq!(outline_name(&c["symbols"][0]), "Plain");
    assert_eq!(c["symbols"][0]["kind"], "struct");
    assert_eq!(result["snapshot"], next["snapshot"]);
    assert_eq!(next["pagination"]["hasMore"], false);
}

#[test]
fn authored_include_glob_selects_matching_paths() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join("nested")).expect("dir");
    root.write("nested/handler_test.rs", "fn a() { hit(1); }\n");
    root.write("other.rs", "fn b() { hit(2); }\n");
    let out = run(
        &root.0,
        json!({"operation":"match","path":root.0,"parser":"rust","rule":{"pattern":"hit($A)"},"include":["**/*test*"]}),
    )
    .expect("match");
    assert_eq!(out["matchCount"], 1, "{out}");
    assert!(
        out["files"]
            .as_array()
            .is_some_and(|files| files.iter().any(|file| file["path"]
                .as_str()
                .is_some_and(|path| path.ends_with("handler_test.rs")))),
        "{out}"
    );
}
