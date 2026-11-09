//! An answer that lists some uses can still miss a file: a sibling module the
//! server did not load, a crate outside its workspace. A file that spells the name
//! in code and is not in the answer makes the row partial, unless the server places
//! that spelling on another symbol.

use super::*;
use crate::execution::cancel::NeverCancel;
use octocode_engine::lsp::client::LspClientOptions;
use octocode_engine::lsp::pool::{LspClientPool, LspPoolOptions};
use serde_json::Value;
use std::time::Duration;

/// `const elsewhere = ...; const empty = ...;` replaces this line.
const BEHAVIOUR: &str = "/*BEHAVIOUR*/";

/// Answers `references` with the anchor file alone. `definition` in `b.*` names
/// `b.*` itself when `elsewhere` holds (another symbol of the name), and nothing
/// otherwise (a file it did not load).
const SERVER: &str = r#"#!/usr/bin/env node
/*BEHAVIOUR*/
let buf = Buffer.alloc(0);
function send(m) {
  const s = JSON.stringify(m);
  process.stdout.write('Content-Length: ' + Buffer.byteLength(s) + '\r\n\r\n' + s);
}
const site = (uri, line, character) => ({ uri, range: { start: { line, character }, end: { line, character: character + 6 } } });
function handle(msg) {
  if (msg.method === 'initialize') {
    send({ jsonrpc: '2.0', id: msg.id, result: { capabilities: { definitionProvider: true, referencesProvider: true, textDocumentSync: 1 } } });
  } else if (msg.method === 'textDocument/references') {
    const uri = msg.params.textDocument.uri;
    send({ jsonrpc: '2.0', id: msg.id, result: empty ? [] : [site(uri, 0, 9), site(uri, 1, 0)] });
  } else if (msg.method === 'textDocument/definition') {
    const uri = msg.params.textDocument.uri;
    const known = !uri.includes('/b.') || elsewhere;
    const line = uri.includes('/b.') ? msg.params.position.line : 0;
    const character = uri.includes('/b.') ? msg.params.position.character : 9;
    send({ jsonrpc: '2.0', id: msg.id, result: known ? [site(uri, line, character)] : [] });
  } else if (msg.method === 'exit') {
    process.exit(0);
  } else if (msg.id !== undefined && msg.method) {
    send({ jsonrpc: '2.0', id: msg.id, result: null });
  }
}
process.stdin.on('data', (d) => {
  buf = Buffer.concat([buf, d]);
  for (;;) {
    const i = buf.indexOf('\r\n\r\n');
    if (i < 0) return;
    const m = /Content-Length: (\d+)/i.exec(buf.slice(0, i).toString());
    const n = Number(m[1]);
    if (buf.length < i + 4 + n) return;
    const msg = JSON.parse(buf.slice(i + 4, i + 4 + n).toString());
    buf = buf.slice(i + 4 + n);
    handle(msg);
  }
});
"#;

/// The project file of each language's sibling project.
fn project_file(extension: &str) -> (&'static str, &'static str) {
    match extension {
        "go" => ("go.mod", "module example.com/b\n"),
        "java" => ("pom.xml", "<project/>\n"),
        "scala" => ("build.sbt", "name := \"b\"\n"),
        _ => ("Cargo.toml", "[package]\nname = \"b\"\n"),
    }
}

/// The row of `references helper` from `a.{extension}` with a `b.{extension}`
/// that calls `helper`, in a repository: beside the anchor, or in a sibling
/// project `b/` when `sibling`.
fn references(language: &str, extension: &str, elsewhere: bool, sibling: bool) -> Value {
    let (name, content) = project_file(extension);
    let files: &[(&str, &str)] = if sibling { &[(name, content)] } else { &[] };
    references_with(language, extension, elsewhere, sibling, &[], files, false)
}

/// [`references`] with build files: `anchor_files` beside the anchor (its
/// project), `sibling_files` in `b/`, the sibling project, when `sibling`.
fn references_with(
    language: &str,
    extension: &str,
    elsewhere: bool,
    sibling: bool,
    anchor_files: &[(&str, &str)],
    sibling_files: &[(&str, &str)],
    empty: bool,
) -> Value {
    let dir = crate::tools::test_support::Fixture::new();
    let root = dir.0.canonicalize().expect("canonical root");
    std::fs::create_dir_all(root.join(".git")).expect("repository");
    let script = root.join("fake-lsp.js");
    let server = SERVER.replace(
        BEHAVIOUR,
        &format!("const elsewhere = {elsewhere}; const empty = {empty};"),
    );
    crate::tools::lsp_search::tests::write_executable(&script, server.as_bytes());
    let anchor = root.join(format!("a.{extension}"));
    std::fs::write(&anchor, "function helper() {}\nhelper();\n").expect("anchor");
    for (name, content) in anchor_files {
        std::fs::write(root.join(name), content).expect("anchor project file");
    }
    let other = if sibling {
        std::fs::create_dir_all(root.join("b")).expect("sibling");
        for (name, content) in sibling_files {
            std::fs::write(root.join("b").join(name), content).expect("project file");
        }
        root.join(format!("b/b.{extension}"))
    } else {
        root.join(format!("b.{extension}"))
    };
    std::fs::write(other, "helper();\n").expect("other file");
    std::fs::write(
        root.join("lsp-servers.json"),
        json!({"languageServers": {format!(".{extension}"): {
            "command": script.to_string_lossy(), "args": [], "languageId": language
        }}})
        .to_string(),
    )
    .expect("config");
    let execution = crate::tools::lsp_search::LspExecutionConfig {
        octocode_home: Some(root.clone()),
        ..crate::tools::lsp_search::LspExecutionConfig::default()
    };
    let pool = LspClientPool::new(LspPoolOptions {
        client: LspClientOptions {
            settle_cap: Some(Duration::ZERO),
        },
        ..LspPoolOptions::default()
    });
    let paths = crate::tools::test_support::workspace_policy(&root);
    let runtime = tokio::runtime::Runtime::new().expect("rt");
    let row = runtime
        .block_on(crate::tools::lsp_search::execute(
            query(json!({
                "operation": "references",
                "path": anchor.to_string_lossy(),
                "symbolName": "helper",
                "line": 1
            })),
            &NeverCancel,
            &pool,
            &paths,
            &execution,
            None,
            crate::tools::lsp_search::test_answers(),
        ))
        .expect("row");
    runtime.block_on(pool.clear_all());
    row
}

/// The languages with no coverage signal of their own, and Rust.
const CHECKED: [(&str, &str); 4] = [
    ("go", "go"),
    ("java", "java"),
    ("scala", "scala"),
    ("rust", "rs"),
];

#[test]
fn an_answer_listing_uses_is_partial_while_another_project_may_hold_one() {
    if !crate::tools::test_support::require_node() {
        return;
    }
    for (language, extension) in CHECKED {
        let row = references(language, extension, false, true);
        assert_eq!(row["matchCount"], 2, "{language}: {row}");
        assert_eq!(row["isPartial"], true, "{language}: {row}");
        assert_eq!(
            row["partialReasons"],
            json!(["serverUnanswered"]),
            "{language}: {row}"
        );
        assert_ne!(
            row.pointer("/coverage/state"),
            Some(&json!("complete")),
            "{language}: {row}"
        );
    }
}

/// A sibling project under an ignored path is one the server never loaded, as
/// much as a visible one: the row never reads complete while an ignored file
/// spells the name, and it says what the ignore rules hid.
#[test]
fn a_sibling_project_under_an_ignored_path_is_not_complete() {
    if !crate::tools::test_support::require_node() {
        return;
    }
    for (language, extension) in CHECKED {
        let (name, content) = project_file(extension);
        let row = references_with(
            language,
            extension,
            false,
            true,
            &[(".gitignore", "b/\n")],
            &[(name, content)],
            false,
        );
        assert_eq!(row["matchCount"], 2, "{language}: {row}");
        assert_eq!(row["isPartial"], true, "{language}: {row}");
        assert_ne!(
            row.pointer("/coverage/state"),
            Some(&json!("complete")),
            "{language}: {row}"
        );
    }
}

/// The server indexed its own project: a file there that it left out holds no
/// use, however the server would place its spelling, and is never opened to ask.
#[test]
fn an_answer_speaks_for_the_servers_own_project() {
    if !crate::tools::test_support::require_node() {
        return;
    }
    for (language, extension) in CHECKED {
        for elsewhere in [false, true] {
            let row = references(language, extension, elsewhere, false);
            assert_eq!(row["matchCount"], 2, "{language}: {row}");
            assert_ne!(row["isPartial"], true, "{language}: {row}");
        }
    }
}

/// One file of a layout: its name and its content.
type LayoutFile = [(&'static str, String); 1];

/// The anchor's project named by its build file, and a sibling that does or does
/// not depend on it, per language: language, extension, anchor and sibling.
fn dependency_layouts(depends: bool) -> [(&'static str, &'static str, LayoutFile, LayoutFile); 3] {
    let go = if depends {
        "require example.com/a v0.0.0\n"
    } else {
        ""
    };
    let cargo = if depends {
        "[dependencies]\na = { path = \"..\" }\n"
    } else {
        ""
    };
    let maven = if depends {
        "<dependencies><dependency><groupId>g</groupId><artifactId>a</artifactId><version>1</version></dependency></dependencies>"
    } else {
        ""
    };
    [
        ("go", "go", [("go.mod", "module example.com/a\n".into())], [("go.mod", format!("module example.com/b\n{go}"))]),
        ("rust", "rs", [("Cargo.toml", "[package]\nname = \"a\"\n".into())], [("Cargo.toml", format!("[package]\nname = \"b\"\n{cargo}"))]),
        (
            "java",
            "java",
            [("pom.xml", "<project><groupId>g</groupId><artifactId>a</artifactId><version>1</version></project>".into())],
            [("pom.xml", format!("<project><groupId>g</groupId><artifactId>b</artifactId><version>1</version>{maven}</project>"))],
        ),
    ]
}

/// A sibling project that does not depend on the anchor's cannot use it: its
/// spelling is a symbol of its own, and the answer stays complete.
#[test]
fn a_project_that_does_not_depend_on_the_anchor_leaves_the_answer_complete() {
    if !crate::tools::test_support::require_node() {
        return;
    }
    for (language, extension, anchor, sibling) in dependency_layouts(false) {
        let anchor = anchor
            .iter()
            .map(|(n, c)| (*n, c.as_str()))
            .collect::<Vec<_>>();
        let sibling = sibling
            .iter()
            .map(|(n, c)| (*n, c.as_str()))
            .collect::<Vec<_>>();
        let row = references_with(language, extension, false, true, &anchor, &sibling, false);
        assert_eq!(row["matchCount"], 2, "{language}: {row}");
        assert_ne!(row["isPartial"], true, "{language}: {row}");
    }
}

/// One that depends on it may hold a use: partial, the warning naming the
/// project with its file count, and a search of it in `related[]`.
#[test]
fn a_dependent_project_is_named_with_its_count_and_a_search_of_it() {
    if !crate::tools::test_support::require_node() {
        return;
    }
    for (language, extension, anchor, sibling) in dependency_layouts(true) {
        let anchor = anchor
            .iter()
            .map(|(n, c)| (*n, c.as_str()))
            .collect::<Vec<_>>();
        let sibling = sibling
            .iter()
            .map(|(n, c)| (*n, c.as_str()))
            .collect::<Vec<_>>();
        let row = references_with(language, extension, false, true, &anchor, &sibling, false);
        assert_eq!(
            row["partialReasons"],
            json!(["serverUnanswered"]),
            "{language}: {row}"
        );
        assert!(row.to_string().contains("b (1)"), "{language}: {row}");
        let searched = row["related"].as_array().into_iter().flatten().any(|call| {
            call["tool"] == "localSearch"
                && call["input"]["queries"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|query| {
                        query["path"]
                            .as_str()
                            .is_some_and(|path| path == "b" || path.ends_with("/b"))
                    })
        });
        assert!(searched, "{language}: {row}");
    }
}

/// A package that `include!`s build-script output makes a file that spells the
/// name a possible missed use of a type the server never saw. Two `fn helper`
/// of different types are not that: the definition check finds the other file's
/// spelling names its own symbol, so the row is complete with build scripts off.
#[test]
fn another_symbol_of_the_name_is_not_blamed_on_build_scripts() {
    if !crate::tools::test_support::require_node() {
        return;
    }
    let package = [
        ("Cargo.toml", "[package]\nname = \"a\"\n"),
        ("build.rs", "fn main() {}\n"),
        (
            "generated.rs",
            "include!(concat!(env!(\"OUT_DIR\"), \"/out.rs\"));\n",
        ),
    ];
    let own = references_with("rust", "rs", true, false, &package, &[], false);
    assert_eq!(own["matchCount"], 2, "{own}");
    assert_ne!(own["isPartial"], true, "{own}");
    // A spelling the server cannot place stays a possible missed use.
    let unplaced = references_with("rust", "rs", false, false, &package, &[], false);
    assert!(
        unplaced["partialReasons"]
            .as_array()
            .is_some_and(|reasons| reasons.contains(&json!("buildScriptsOff"))),
        "{unplaced}"
    );
}

/// A Rust server that has not taken in an edit, or not loaded the files, answers
/// empty: with files of its own project spelling the name in code and resolving
/// nowhere, the row is partial, never a settled "no uses". One whose every
/// spelling resolves elsewhere stays a settled answer.
#[test]
fn an_empty_rust_answer_with_unconfirmed_spellings_is_partial() {
    if !crate::tools::test_support::require_node() {
        return;
    }
    let crate_file = [("Cargo.toml", "[package]\nname = \"a\"\n")];
    let unconfirmed = references_with("rust", "rs", false, false, &crate_file, &[], true);
    assert_eq!(unconfirmed["status"], "empty", "{unconfirmed}");
    assert!(
        unconfirmed["partialReasons"]
            .as_array()
            .is_some_and(|reasons| reasons.contains(&json!("serverUnanswered"))),
        "{unconfirmed}"
    );
    let elsewhere = references_with("rust", "rs", true, false, &crate_file, &[], true);
    assert_ne!(elsewhere["isPartial"], true, "{elsewhere}");
}

/// `references helper` from `a.rs` with a sibling project `dist/` whose
/// `copy.rs` calls `helper` (a build copy, not ignored), inside a git work
/// tree when `repository`.
fn references_beside_dist(repository: bool) -> Value {
    let dir = crate::tools::test_support::Fixture::new();
    let root = dir.0.canonicalize().expect("canonical root");
    if repository {
        std::fs::create_dir_all(root.join(".git")).expect("repository");
    }
    let script = root.join("fake-lsp.js");
    // The server did not load the copy: `definition` there names nothing.
    let server = SERVER
        .replace(BEHAVIOUR, "const elsewhere = false; const empty = false;")
        .replace("'/b.'", "'/dist/'");
    assert!(
        server.contains("'/dist/'"),
        "the server script names b.* files"
    );
    crate::tools::lsp_search::tests::write_executable(&script, server.as_bytes());
    let anchor = root.join("a.rs");
    std::fs::write(&anchor, "function helper() {}\nhelper();\n").expect("anchor");
    std::fs::create_dir_all(root.join("dist")).expect("dist");
    let (name, content) = project_file("rs");
    std::fs::write(root.join("dist").join(name), content).expect("project file");
    std::fs::write(root.join("dist/copy.rs"), "helper();\n").expect("build copy");
    std::fs::write(
        root.join("lsp-servers.json"),
        json!({"languageServers": {".rs": {
            "command": script.to_string_lossy(), "args": [], "languageId": "rust"
        }}})
        .to_string(),
    )
    .expect("config");
    let execution = crate::tools::lsp_search::LspExecutionConfig {
        octocode_home: Some(root.clone()),
        ..crate::tools::lsp_search::LspExecutionConfig::default()
    };
    let pool = LspClientPool::new(LspPoolOptions {
        client: LspClientOptions {
            settle_cap: Some(Duration::ZERO),
        },
        ..LspPoolOptions::default()
    });
    let paths = crate::tools::test_support::workspace_policy(&root);
    let runtime = tokio::runtime::Runtime::new().expect("rt");
    let row = runtime
        .block_on(crate::tools::lsp_search::execute(
            query(json!({
                "operation": "references",
                "path": anchor.to_string_lossy(),
                "symbolName": "helper",
                "line": 1
            })),
            &NeverCancel,
            &pool,
            &paths,
            &execution,
            None,
            crate::tools::lsp_search::test_answers(),
        ))
        .expect("row");
    runtime.block_on(pool.clear_all());
    row
}

/// RFC Validation 10: the text scan prunes what every local walk prunes. Outside
/// a git work tree a `dist/` copy that spells the name is never walked, so
/// coverage names no text-only file there. Inside one, where a tracked `dist/` is walked by every local tool,
/// the same copy is the text-only file the answer misses. (An include naming
/// `dist/**` lifts the prune: lspSearch takes no include, so that half is
/// `scope::tests::listing::the_text_scan_prunes_the_directories_every_local_walk_prunes`.)
#[test]
fn a_build_copy_in_a_pruned_directory_is_no_text_only_file() {
    if !crate::tools::test_support::require_node() {
        return;
    }
    let pruned = references_beside_dist(false);
    assert_eq!(pruned["matchCount"], 2, "{pruned}");
    // The row stays partial for its own reason (no `.git` bounds the
    // project), but no text-only file is counted and `dist/` is never named.
    assert_eq!(
        pruned.pointer("/coverage/textOnlyFiles"),
        Some(&json!(0)),
        "{pruned}"
    );
    assert!(!pruned.to_string().contains("dist/"), "{pruned}");
    let walked = references_beside_dist(true);
    assert_eq!(walked["matchCount"], 2, "{walked}");
    assert_eq!(walked["isPartial"], true, "{walked}");
    assert_eq!(
        walked.pointer("/coverage/textOnlyFiles"),
        Some(&json!(1)),
        "{walked}"
    );
    assert!(walked.to_string().contains("dist/copy.rs"), "{walked}");
}
