use super::*;

#[test]
fn flow_pragma_must_be_a_word_not_a_package_prefix() {
    assert!(is_flow_source("// @flow strict\nconst a = 1;\n", "js"));
    assert!(is_flow_source("/* @flow */ const a = 1;\n", "cjs"));
    assert!(!is_flow_source(
        "// see @flowjs/flow.js\nconst a = require('x');\n",
        "cjs"
    ));
    assert!(!is_flow_source(
        "// @flow-typed stubs\nconst a = 1;\n",
        "js"
    ));
}

#[test]
fn flow_sources_are_detected_by_pragma_or_flow_only_statements() {
    assert!(is_flow_source("/**\n * @flow\n */\nconst a = 1;\n", "js"));
    assert!(is_flow_source("import type {A} from 'a';\n", "jsx"));
    assert!(is_flow_source(
        "const a = 1;\nexport type B = number;\n",
        "mjs"
    ));
    assert!(!is_flow_source("const a = 1;\n", "js"));
    assert!(!is_flow_source("// @flow\n", "ts"), "TS is never Flow");
    assert_eq!(grammar_extension("// @flow\n", "js"), "tsx");
    assert_eq!(grammar_extension("const a = 1;\n", "js"), "js");
    assert_eq!(grammar_extension("int f(void);\n", "h"), "h");
}

#[cfg(unix)]
#[test]
fn a_backslash_in_a_unix_file_name_is_part_of_the_name() {
    assert_eq!(basename("d/f\\g.rs"), "f\\g.rs");
    assert_eq!(extension_of("f\\g.rs", false, ""), "rs");
    assert_eq!(extension_of("f\\g", false, "none"), "none");
}

#[cfg(windows)]
#[test]
fn a_backslash_separates_windows_components() {
    assert_eq!(basename("d\\g.rs"), "g.rs");
}

#[test]
fn extension_returned_when_path_has_dot() {
    assert_eq!(extension_of("foo.ts", false, ""), "ts");
}

#[test]
fn extension_lowercased_when_lowercase_requested() {
    assert_eq!(extension_of("Foo.TS", true, ""), "ts");
}

#[test]
fn dotfile_name_treated_as_extension() {
    assert_eq!(extension_of(".gitignore", true, ""), "gitignore");
}

#[test]
fn fallback_returned_when_no_extension() {
    assert_eq!(extension_of("Makefile", false, "txt"), "txt");
}

#[test]
fn last_dot_wins_for_multi_dot_names() {
    assert_eq!(extension_of("archive.tar.gz", false, ""), "gz");
}

#[test]
fn last_dot_wins_for_multi_dot_dotfiles() {
    assert_eq!(extension_of(".env.local", false, ""), "local");
}

#[test]
fn windows_path_basename_is_supported() {
    assert_eq!(extension_of(r"C:\tmp\Foo.TS", true, ""), "ts");
}

#[test]
fn dotted_directory_names_are_not_extensions() {
    assert_eq!(extension_of("a.b/Makefile", true, ""), "");
    assert_eq!(extension_of("lib.ts/README", true, "txt"), "txt");
}
