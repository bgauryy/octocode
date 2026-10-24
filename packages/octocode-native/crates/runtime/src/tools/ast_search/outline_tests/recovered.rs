//! Recovered parses: one note per file that could hide a declaration.

use super::*;

#[test]
fn flow_js_symbols_list_hooks_and_mark_a_recovered_parse_partial() {
    let root = Fixture::new();
    let source = root.0.join("Hooks.js");
    std::fs::write(
        &source,
        "// @flow\nimport type {D} from 'd';\ntype Dispatch<A> = A => void;\n\
         export function useState<S>(init: (() => S) | S): [S, Dispatch<S>] {\n  if (x) {\n    y();\n  }\n  return z;\n}\n\
         export function useRef<T>(v: T): {current: T} {\n  if (x) {\n    y();\n  }\n  return z;\n}\n",
    )
    .expect("source");
    let out = run(&root.0, json!({"operation":"symbols","path":source})).expect("symbols");
    let names = outline_names(&out["symbols"]);
    assert!(
        names.contains(&"useState") && names.contains(&"useRef"),
        "{out}"
    );
    assert!(!names.contains(&"if"), "{out}");
    let recovered = out["diagnostics"].as_array().is_some_and(|ds| {
        ds.iter().any(|d| {
            d["message"]
                .as_str()
                .is_some_and(|m| m.starts_with("tree-sitter recovered"))
        })
    });
    // A recovered parse is a partial inventory, never a complete one.
    assert!(
        recovered,
        "the Flow type alias forces a recovered parse: {out}"
    );
    assert_eq!(out["isPartial"], json!(true), "{out}");
    // A partial page without `next` still names why it is partial.
    assert_eq!(out["partialReasons"], json!(["parseRecovered"]), "{out}");
}

/// A parse-recovery note repeated by several files that list declarations
/// is one counted entry naming every file, not one entry per file.
#[test]
fn symbols_count_one_recovery_note_across_files_that_list_declarations() {
    let root = Fixture::new();
    for index in 1..=3 {
        root.write(
            &format!("f{index}.rs"),
            format!("fn foo_{index}() {{}}\nfn caller() {{ foo_{index}(; }}\n"),
        );
    }
    let out = run(
        &root.0,
        json!({"operation":"symbols","path":root.0,"symbolFilter":{"terms":["foo_"]}}),
    )
    .expect("symbols");
    let diagnostics = out["diagnostics"].as_array().expect("diagnostics");
    assert_eq!(diagnostics.len(), 1, "{out}");
    let message = diagnostics[0]["message"].as_str().expect("message");
    assert!(message.starts_with("tree-sitter recovered"), "{out}");
    assert!(message.ends_with("(3 files)"), "{out}");
    let mut files = diagnostics[0]["files"]
        .as_array()
        .expect("files")
        .iter()
        .filter_map(|file| file["path"].as_str())
        .map(|path| path.rsplit('/').next().unwrap_or(path).to_owned())
        .collect::<Vec<_>>();
    files.sort();
    assert_eq!(files, ["f1.rs", "f2.rs", "f3.rs"], "{out}");
    assert_eq!(out["partialReasons"], json!(["parseRecovered"]), "{out}");
}

/// Files whose parse-recovery notes share a message share one counted
/// entry with every path, and a text search completes the name lookup the
/// recovered parses may have missed.
#[test]
fn symbols_group_recovery_notes_of_files_without_a_listed_declaration() {
    let root = Fixture::new();
    root.write("a.rs", "fn sdsnewlen() {}\n");
    for file in ["b.rs", "c.rs"] {
        std::fs::write(root.0.join(file), "fn caller() { sdsnewlen(; }\n").expect("broken");
    }
    let out = run(
        &root.0,
        json!({"operation":"symbols","path":root.0,"symbolFilter":{"terms":["sdsnewlen"]}}),
    )
    .expect("symbols");
    let diagnostics = out["diagnostics"].as_array().expect("diagnostics");
    assert_eq!(diagnostics.len(), 1, "{out}");
    assert!(diagnostics[0].get("path").is_none(), "{out}");
    let files = diagnostics[0]["files"].as_array().expect("files");
    assert_eq!(files.len(), 2, "{out}");
    assert!(files.iter().all(|file| file["path"].is_string()), "{out}");
    assert!(
        diagnostics[0]["message"]
            .as_str()
            .is_some_and(|m| m.starts_with("tree-sitter recovered")),
        "{out}"
    );
    assert_eq!(out["isPartial"], true, "{out}");
    let lead = crate::tools::test_support::checks::related_call(&out, "localSearch", |_| true);
    assert_eq!(lead["tool"], "localSearch", "{out}");
    assert_eq!(
        lead["input"]["queries"][0]["matchString"],
        json!(["sdsnewlen"]),
        "{out}"
    );
}

/// A recovered parse can hide a declaration of the queried name only
/// where its syntax errors spell that name. A file that only calls the name
/// in code that parsed keeps no note and leaves the outline complete.
#[test]
fn recovered_notes_only_for_files_whose_errors_spell_the_name() {
    let root = Fixture::new();
    root.write("a.c", "int foo(int x) { return x; }\n");
    // Calls `foo` on a clean line; the syntax error is elsewhere.
    root.write(
        "b.c",
        "int caller(void) { return foo(1); }\n\nint broken( {\n  return 2;\n}\n",
    );
    // The syntax error spells `foo`: a declaration could hide there.
    root.write("c.c", "int user(void) {\n  int foo( = 3;\n}\n");
    let out = run(
        &root.0,
        json!({"operation":"symbols","path":root.0,"symbolFilter":{"terms":["foo"]}}),
    )
    .expect("symbols");
    let diagnostics = out["diagnostics"].as_array().cloned().unwrap_or_default();
    let noted = diagnostics
        .iter()
        .flat_map(|d| {
            d["files"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(|file| file["path"].clone())
                .chain(d.get("path").cloned())
        })
        .filter_map(|path| path.as_str().map(str::to_owned))
        .collect::<Vec<_>>();
    assert!(noted.iter().any(|path| path.ends_with("c.c")), "{out}");
    assert!(!noted.iter().any(|path| path.ends_with("b.c")), "{out}");
    assert_eq!(out["isPartial"], true, "{out}");
    // Only b.c's irrelevant note: the outline is complete.
    std::fs::remove_file(root.0.join("c.c")).expect("remove c");
    let out = run(
        &root.0,
        json!({"operation":"symbols","path":root.0,"symbolFilter":{"terms":["foo"]}}),
    )
    .expect("symbols");
    assert!(
        out["diagnostics"].as_array().is_none_or(Vec::is_empty),
        "{out}"
    );
    assert_eq!(out["isPartial"], false, "{out}");
}

/// A name-filtered outline that lists a declaration from a recovered parse
/// cites a partial inventory: the file keeps its note and the page is
/// partial, wherever the syntax error sits. Recovered files that list
/// nothing and whose errors do not spell the name stay out.
#[test]
fn a_recovered_file_that_lists_the_named_declaration_keeps_its_note() {
    let root = Fixture::new();
    for index in 1..=3 {
        root.write(
            &format!("f{index}.rs"),
            format!("pub fn helper{index}() {{}}\nfn broken( {{\n"),
        );
    }
    let out = run(
        &root.0,
        json!({"operation":"symbols","path":root.0,"symbolFilter":{"terms":["helper2"]}}),
    )
    .expect("symbols");
    assert_eq!(
        outline_names(&out["files"][0]["symbols"]),
        ["helper2"],
        "{out}"
    );
    let diagnostics = out["diagnostics"].as_array().expect("diagnostics");
    assert_eq!(diagnostics.len(), 1, "{out}");
    let noted = diagnostics[0].to_string();
    assert!(noted.contains("f2.rs"), "{out}");
    assert!(
        !noted.contains("f1.rs") && !noted.contains("f3.rs"),
        "{out}"
    );
    assert_eq!(out["isPartial"], true, "{out}");
    assert_eq!(out["partialReasons"], json!(["parseRecovered"]), "{out}");
}
