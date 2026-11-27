use super::*;
use octocode_runtime_base::execution::cancel::NeverCancel;
use octocode_runtime_base::test_support::{Fixture, workspace_policy};
use octocode_test_support::json_fixture;

/// The reference row of the `nth` occurrence of `needle` in `source`.
fn row(uri: &str, source: &str, needle: &str, nth: usize) -> Value {
    let (line, text) = source
        .lines()
        .enumerate()
        .flat_map(|(line, text)| {
            text.match_indices(needle)
                .map(move |(at, _)| (line, &text[..at]))
        })
        .nth(nth)
        .expect("occurrence");
    let start = text.encode_utf16().count();
    json!({"uri": uri, "range": {
        "start": {"line": line, "character": start},
        "end": {"line": line, "character": start + needle.len()}
    }})
}

async fn classified_rows(
    root: &Fixture,
    file: &str,
    source: &str,
    sites: &[(&str, usize)],
) -> Vec<Value> {
    let path = root.write(file, source);
    let uri = octocode_engine::lsp::uri::path_to_uri(&path.to_string_lossy()).expect("uri");
    let mut rows = sites
        .iter()
        .map(|(needle, nth)| row(&uri, source, needle, *nth))
        .collect::<Vec<_>>();
    let paths = workspace_policy(&root.0);
    assign_roles(&mut rows, &mut SourceCache::new(&paths), &NeverCancel, None)
        .await
        .expect("roles");
    rows
}

async fn roles(root: &Fixture, file: &str, source: &str, sites: &[(&str, usize)]) -> Vec<Value> {
    let rows = classified_rows(root, file, source, sites).await;
    rows.iter().map(|row| row["role"].clone()).collect()
}

#[tokio::test(flavor = "current_thread")]
async fn explicit_import_aliases_identify_both_tokens() {
    let root = Fixture::new();
    for (file, source) in [
        (
            "a.ts",
            "import {
read as load, plain
} from './port';
load();",
        ),
        ("a.py", "from port import (read as load, plain)\nload()"),
        (
            "a.rs",
            "use port::{
read as load, plain
};
fn f() { load(); }",
        ),
    ] {
        let rows = classified_rows(
            &root,
            file,
            source,
            &[("read", 0), ("load", 0), ("plain", 0), ("load", 1)],
        )
        .await;
        assert_eq!(rows[0]["importRole"], "importedName", "{file}: {rows:?}");
        assert_eq!(rows[1]["importRole"], "localAlias", "{file}: {rows:?}");
        assert!(rows[..3].iter().all(|r| r["role"] == "import"), "{rows:?}");
        assert!(
            rows[2..].iter().all(|r| r.get("importRole").is_none()),
            "{rows:?}"
        );
        assert_eq!(rows[3]["role"], "call");
    }
}

#[tokio::test(flavor = "current_thread")]
async fn js_and_ts_sites_are_imports_calls_and_references() {
    let root = Fixture::new();
    let source = "import { read } from \"./port\";\nimport port from \"./port\";\nexport { read as load } from \"./port\";\nread(1);\nconst r = read;\nport.read(2);\n";
    for file in ["a.ts", "a.js"] {
        assert_eq!(
            roles(
                &root,
                file,
                source,
                &[
                    ("read", 0),
                    ("port", 0),
                    ("read", 1),
                    ("read", 2),
                    ("read", 3),
                    ("read", 4)
                ]
            )
            .await,
            ["import", "import", "import", "call", "reference", "call"],
            "{file}"
        );
    }
}

#[tokio::test(flavor = "current_thread")]
async fn rust_sites_read_the_text_rule() {
    let root = Fixture::new();
    let source = "use crate::port::read;\npub use crate::port::read as load;\nfn main() {\n    read(1);\n    let r = read;\n}\n";
    assert_eq!(
        roles(
            &root,
            "lib.rs",
            source,
            &[("read", 0), ("read", 1), ("read", 2), ("read", 3)]
        )
        .await,
        ["import", "import", "call", "reference"]
    );
}

/// A declaration's own name is its declaration, not a call, though a `(`
/// follows it (`fn read(`), in every language with a declarations parse.
#[tokio::test(flavor = "current_thread")]
async fn a_declaration_site_is_a_declaration() {
    let root = Fixture::new();
    let rust =
        "pub(crate) fn read(\n    at: u32,\n) -> u32 {\n    at\n}\nfn main() {\n    read(1);\n}\n";
    assert_eq!(
        roles(&root, "lib.rs", rust, &[("read", 0), ("read", 1)]).await,
        ["declaration", "call"]
    );
    let ts = "export function read(at: number) {\n  return at;\n}\nread(1);\n";
    assert_eq!(
        roles(&root, "a.ts", ts, &[("read", 0), ("read", 1)]).await,
        ["declaration", "call"]
    );
    let py = "def read(at):\n    return at\n\nread(1)\n";
    assert_eq!(
        roles(&root, "a.py", py, &[("read", 0), ("read", 1)]).await,
        ["declaration", "call"]
    );
}

/// A site whose source this request cannot read keeps no role.
#[tokio::test(flavor = "current_thread")]
async fn an_unreadable_source_leaves_its_rows_unclassified() {
    let root = Fixture::new();
    let mut rows = vec![json_fixture(
        r#"{
            "uri": "file:///outside/the/workspace.ts",
            "range": {
                "start": {"line": 0, "character": 0},
                "end": {"line": 0, "character": 3}
            }
        }"#,
    )];
    let paths = workspace_policy(&root.0);
    assign_roles(&mut rows, &mut SourceCache::new(&paths), &NeverCancel, None)
        .await
        .expect("roles");
    assert!(rows[0].get("role").is_none(), "{}", rows[0]);
}

/// A server may range a method invocation over its arguments (jdtls): the
/// name that starts the range is followed by `(`, the range's end is not.
#[tokio::test(flavor = "current_thread")]
async fn a_range_over_the_arguments_is_still_a_call() {
    let root = Fixture::new();
    let source = "class Use {\n  int useIt() {\n    return Lib.compute(\n      1,\n      2);\n  }\n  int field = Lib.value;\n}\n";
    let path = root.write("Use.java", source);
    let uri = octocode_engine::lsp::uri::path_to_uri(&path.to_string_lossy()).expect("uri");
    let site = |start: (u32, u32), end: (u32, u32)| {
        json!({"uri": uri, "range": {
            "start": {"line": start.0, "character": start.1},
            "end": {"line": end.0, "character": end.1}
        }})
    };
    let mut rows = [
        site((2, 15), (4, 8)),
        site((2, 15), (2, 22)),
        site((6, 18), (6, 23)),
    ];
    let paths = workspace_policy(&root.0);
    assign_roles(&mut rows, &mut SourceCache::new(&paths), &NeverCancel, None)
        .await
        .expect("roles");
    let roles = rows
        .iter()
        .map(|row| row["role"].clone())
        .collect::<Vec<_>>();
    assert_eq!(roles, [json!("call"), json!("call"), json!("reference")]);
}

/// A prototype in a header is a declaration, not a call, in every form a C
/// family header writes it, while a call beside it stays one.
#[tokio::test(flavor = "current_thread")]
async fn a_header_prototype_is_a_declaration() {
    let root = Fixture::new();
    for (file, source) in [
        ("plain.h", "int helper(void);\n"),
        ("extern.h", "extern int helper(void);\n"),
        ("pointer.h", "char *helper(void);\n"),
        ("attr.h", "__attribute__((pure)) int helper(void);\n"),
        ("macro.h", "API_EXPORT int helper(void);\n"),
        (
            "guarded.h",
            "#ifndef X\n#define X\nint helper(int a);\n#endif\n",
        ),
        ("cpp.hpp", "namespace n {\nint helper(int a);\n}\n"),
        (
            "member.hh",
            "class K {\n  virtual int helper(int a) = 0;\n};\n",
        ),
        ("static.h", "static inline int helper(void) { return 0; }\n"),
        (
            "kr.h",
            "#ifdef __cplusplus\nextern \"C\" {\n#endif\nint helper(void);\n#ifdef __cplusplus\n}\n#endif\n",
        ),
    ] {
        assert_eq!(
            roles(&root, file, source, &[("helper", 0)]).await,
            ["declaration"],
            "{file}: {source}"
        );
    }
}

/// Each page of an answer assigns the roles of its rows again; the
/// declarations parse of a file read before is the kept one.
#[tokio::test(flavor = "current_thread")]
async fn a_later_page_reads_each_files_kept_declarations() {
    let root = Fixture::new();
    let source = "package a\n\nfunc NewHead() {}\n\nfunc use() {\n\tNewHead()\n}\n";
    let path = root.write("head.go", source);
    // The path the rows' uri names, as roles read it.
    let file =
        uri_to_path(&octocode_engine::lsp::uri::path_to_uri(&path.to_string_lossy()).expect("uri"));
    let sites = [("NewHead", 0), ("NewHead", 1)];
    for _page in 0..2 {
        assert_eq!(
            roles(&root, "head.go", source, &sites).await,
            ["declaration", "call"]
        );
    }
    assert_eq!(
        octocode_tools_shared::declarations::parses::of("declarations", &file),
        1
    );
}

/// An `impl X` block names `X` and declares nothing (D3), and each name of
/// a multi-line `use {…}` group is an import, not only the line that opens
/// it (D4).
#[tokio::test(flavor = "current_thread")]
async fn impl_blocks_are_uses_and_import_group_lines_are_imports() {
    let root = Fixture::new();
    let source = "use crate::skipped::{\n    Other,\n    SkippedEntry, WalkErrors,\n};\npub struct Mine;\nimpl Mine {\n    fn f(_: SkippedEntry) {}\n}\n";
    assert_eq!(
        roles(
            &root,
            "lib.rs",
            source,
            &[
                ("SkippedEntry", 0),
                ("SkippedEntry", 1),
                ("Mine", 0),
                ("Mine", 1)
            ]
        )
        .await,
        ["import", "reference", "declaration", "reference"]
    );
}

/// A language with no grammar here (Swift): the text rule calls a
/// declaration `func f(` a call; the server's outline names it a
/// declaration, and the call beside it stays one. The outline is
/// sourcekit-lsp's: its selection spans the signature `f(_ x: Int)`.
#[tokio::test(flavor = "current_thread")]
async fn without_a_parse_the_servers_outline_names_declarations() {
    let root = Fixture::new();
    let source = "public func f(_ x: Int) -> Int { x + 1 }\nfunc g() -> Int { f(2) }\n";
    let path = root.write("lib.swift", source);
    let uri = octocode_engine::lsp::uri::path_to_uri(&path.to_string_lossy()).expect("uri");
    let site = |line: u32, character: u32| {
        json!({"uri": uri, "range": {
            "start": {"line": line, "character": character},
            "end": {"line": line, "character": character + 1}
        }})
    };
    let mut rows = [site(0, 12), site(1, 18)];
    let paths = workspace_policy(&root.0);
    let mut sources = SourceCache::new(&paths);
    let source = sources.get(&path.to_string_lossy()).await.expect("source");
    let parsed = parsed_roles(&NeverCancel, &path.to_string_lossy(), &source, &[])
        .await
        .expect("parse");
    assert!(
        parsed.is_none(),
        "a file with no grammar has no declarations parse"
    );
    assign_roles(&mut rows, &mut sources, &NeverCancel, None)
        .await
        .expect("roles");
    assert_eq!(
        rows.each_ref().map(|row| row["role"].clone()),
        ["call", "call"]
    );
    let outline = json_fixture(
        r#"[
            {
                "name": "f",
                "kind": 12,
                "range": {
                    "start": {"line": 0, "character": 0},
                    "end": {"line": 0, "character": 40}
                },
                "selectionRange": {
                    "start": {"line": 0, "character": 12},
                    "end": {"line": 0, "character": 23}
                }
            },
            {
                "name": "g",
                "kind": 12,
                "range": {
                    "start": {"line": 1, "character": 0},
                    "end": {"line": 1, "character": 24}
                },
                "selectionRange": {
                    "start": {"line": 1, "character": 5},
                    "end": {"line": 1, "character": 6}
                }
            }
        ]"#,
    );
    mark_declarations(&mut rows, &[0, 1], &outline);
    assert_eq!(
        rows.each_ref().map(|row| row["role"].clone()),
        ["declaration", "call"]
    );
}
