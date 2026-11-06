//! C-family importer recovery against a server that behaves as clangd does
//! with no background index: `references` and call hierarchy come from an index
//! of opened files, a file reaches it only as its diagnostics are published
//! (an outline may answer sooner), `definition` toggles to the declaration and
//! guesses from the word where the AST has nothing, and `hover` reads the AST.

use super::*;

const CLANGD_LIKE: &str = r#"#!/usr/bin/env node
const root = process.env.FAKE_ROOT || __ROOT__;
const uri = (name) => 'file://' + root + '/' + name;
const at = (line, character, length) => ({ start: { line, character }, end: { line, character: character + length } });
const def = { uri: uri('lib.c'), range: at(0, 4, 6) };
const decl = { uri: uri('lib.h'), range: at(0, 4, 6) };
const callSite = { uri: uri('main.c'), range: at(0, 29, 6) };
const item = { name: 'helper', kind: 12, uri: def.uri, range: at(0, 0, 30), selectionRange: def.range };
const indexed = new Set();
// A finished background index holds every project file, opened or not.
const inIndex = () => require('fs').existsSync(root + '/.cache/clangd/index')
  ? require('fs').readdirSync(root).filter((name) => /^c\d+\.c$/.test(name)).map(uri)
  : [];
const known = () => [...new Set([...indexed, ...inIndex()])];
let buf = Buffer.alloc(0);
function send(m) {
  const s = JSON.stringify(m);
  process.stdout.write('Content-Length: ' + Buffer.byteLength(s) + '\r\n\r\n' + s);
}
function result(msg) {
  const p = msg.params || {};
  switch (msg.method) {
    case 'initialize':
      return { capabilities: { textDocumentSync: 1, definitionProvider: true, referencesProvider: true, callHierarchyProvider: true, documentSymbolProvider: true, hoverProvider: true } };
    case 'textDocument/definition':
      return p.textDocument.uri === def.uri ? [decl] : [def];
    case 'textDocument/references': {
      const found = p.context && p.context.includeDeclaration ? [def, decl] : [];
      if (indexed.has(callSite.uri)) found.push(callSite);
      for (const file of known()) if (/\/c\d+\.c$/.test(file)) found.push({ uri: file, range: callSite.range });
      return found;
    }
    case 'textDocument/hover':
      // Another symbol of the same name is in the AST; disabled code is not.
      // A real call is in the AST too, listed or not.
      return p.textDocument.uri === uri('other.c') || /\/c\d+\.c$/.test(p.textDocument.uri) ? { contents: 'int helper(void)' } : null;
    case 'textDocument/prepareCallHierarchy':
      return [item];
    case 'callHierarchy/incomingCalls':
      return known().filter((file) => file === callSite.uri || /\/c\d+\.c$/.test(file)).map((file) => ({
        from: { name: file === callSite.uri ? 'caller_fn' : 'c_' + file.match(/c(\d+)\.c$/)[1], kind: 12, uri: file, range: at(0, 0, 40), selectionRange: at(0, 4, 9) },
        fromRanges: [callSite.range],
      }));
    case 'textDocument/documentSymbol':
      return [];
    default:
      return null;
  }
}
function handle(msg) {
  if (msg.method === 'exit') process.exit(0);
  if (msg.method === 'textDocument/didOpen') {
    const { uri: opened, version } = msg.params.textDocument;
    setTimeout(() => {
      indexed.add(opened);
      send({ jsonrpc: '2.0', method: 'textDocument/publishDiagnostics', params: { uri: opened, version, diagnostics: [] } });
    }, 50);
    return;
  }
  if (msg.id === undefined || !msg.method) return;
  if (msg.method === 'initialize') {
    // Its background index, already finished, is known before any question: a
    // question that overtakes this announcement is answered as unindexed.
    send({ jsonrpc: '2.0', method: '$/progress', params: { token: 'backgroundIndexProgress', value: { kind: 'begin', title: 'indexing' } } });
    send({ jsonrpc: '2.0', method: '$/progress', params: { token: 'backgroundIndexProgress', value: { kind: 'end' } } });
  }
  send({ jsonrpc: '2.0', id: msg.id, result: result(msg) });
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

/// A project with this file in `extra` has no `compile_commands.json`.
const NO_DATABASE: &str = "compile_flags.txt";

/// `callers` or `references` of `helper` in a C project holding the anchor, a
/// real caller, and each of `extra` (`(name, content)`).
fn row_of(operation: &str, extra: &[(&str, &str)]) -> serde_json::Value {
    let mut pages = pages_of(operation, extra);
    pages.remove(0)
}

/// [`row_of`], with every page its `next.nextPage` reaches.
fn pages_of(operation: &str, extra: &[(&str, &str)]) -> Vec<serde_json::Value> {
    pages_with("fake-clangd", &[], operation, extra)
}

/// [`pages_of`] with the server's file `name` and `args`.
fn pages_with(
    name: &str,
    args: &[&str],
    operation: &str,
    extra: &[(&str, &str)],
) -> Vec<serde_json::Value> {
    walk_of(&Walk {
        name,
        args,
        operation,
        extra,
        after_first_page: &[],
        limit: None,
        lead: "nextImporterWindow",
    })
}

/// One walk of `lead`s: `after_first_page` files land between its first two calls.
struct Walk<'a> {
    name: &'a str,
    args: &'a [&'a str],
    operation: &'a str,
    extra: &'a [(&'a str, &'a str)],
    after_first_page: &'a [(&'a str, &'a str)],
    limit: Option<u64>,
    lead: &'a str,
}

fn walk_of(walk: &Walk<'_>) -> Vec<serde_json::Value> {
    use std::os::unix::fs::PermissionsExt;
    let Walk {
        name,
        args,
        operation,
        extra,
        after_first_page,
        limit,
        lead,
    } = *walk;
    let write_all = |root: &Path, files: &[(&str, &str)]| {
        for (name, content) in files {
            let file = root.join(name);
            std::fs::create_dir_all(file.parent().expect("parent")).expect("dirs");
            std::fs::write(file, content).expect("extra");
        }
    };
    let dir = Fixture::new();
    let outer = dir.0.canonicalize().expect("real");
    let root = outer.join("ws");
    std::fs::create_dir_all(root.join(".git")).expect("root");
    let script = outer.join(name);
    std::fs::write(
        &script,
        CLANGD_LIKE.replace(
            "__ROOT__",
            &serde_json::to_string(&root.to_string_lossy()).expect("root"),
        ),
    )
    .expect("script");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("mode");
    if !extra.iter().any(|(name, _)| *name == NO_DATABASE) {
        std::fs::write(root.join("compile_commands.json"), "[]").expect("database");
    }
    std::fs::write(root.join("lib.h"), "int helper(int x);\n").expect("header");
    std::fs::write(root.join("lib.c"), "int helper(int x) { return x; }\n").expect("anchor");
    std::fs::write(
        root.join("main.c"),
        "int caller_fn(void) { return helper(1); }\n",
    )
    .expect("caller");
    write_all(&root, extra);
    std::fs::write(
        root.join("lsp-servers.json"),
        serde_json::json!({"languageServers": {".c": {
            "command": script.to_string_lossy(), "args": args, "languageId": "c"
        }}})
        .to_string(),
    )
    .expect("config");
    let execution = crate::tools::lsp_search::LspExecutionConfig {
        octocode_home: Some(root.clone()),
        ..crate::tools::lsp_search::LspExecutionConfig::default()
    };
    let mut query = serde_json::json!({
        "operation": operation, "path": root.join("lib.c").to_string_lossy(),
        "symbolName": "helper", "line": 1, "workspaceRoot": root.to_string_lossy()
    });
    if let Some(limit) = limit {
        query["limit"] = serde_json::json!(limit);
    }
    let paths = crate::tools::test_support::workspace_policy(&root);
    let pool = fake_server_pool();
    let runtime = tokio::runtime::Runtime::new().expect("rt");
    let mut pages = Vec::new();
    loop {
        let row = runtime
            .block_on(crate::tools::lsp_search::execute(
                serde_json::from_value(query).expect("query"),
                &crate::execution::cancel::NeverCancel,
                &pool,
                &paths,
                &execution,
                None,
                crate::tools::lsp_search::test_answers(),
            ))
            .expect("row");
        assert_row_contract(ToolId::LspSearch, &row);
        let next = next_row(&row, lead);
        pages.push(row);
        if pages.len() == 1 {
            write_all(&root, after_first_page);
        }
        match next {
            Some(next) => query = next,
            None => return pages,
        }
    }
}

fn files_of(row: &serde_json::Value) -> Vec<String> {
    row["files"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|group| group["path"].as_str().unwrap_or_default().to_owned())
        .collect()
}

fn callers_of(row: &serde_json::Value) -> Vec<String> {
    row["files"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|group| group["matches"].as_array().cloned().unwrap_or_default())
        .map(|caller| caller["symbolName"].as_str().unwrap_or_default().to_owned())
        .collect()
}

/// An unopened caller is found, once: the server could not list it until the
/// file was opened and its diagnostics published. With no finished index, text
/// chose the files, so the row is partial however complete it looks.
#[test]
fn an_unopened_caller_is_recovered_once_and_the_row_stays_partial() {
    let row = row_of("callers", &[]);
    assert_eq!(callers_of(&row), ["caller_fn"], "{row}");
    assert_eq!(row["isPartial"], true, "{row}");
    assert_eq!(row["coverage"]["importerScan"], "complete", "{row}");
}

/// `references` asks the same question twice: the second must reach the
/// server, or the answer recorded before the window opened hides the caller.
#[test]
fn references_reach_an_unopened_file() {
    let row = row_of("references", &[]);
    assert!(
        files_of(&row).iter().any(|file| file.ends_with("main.c")),
        "{row}"
    );
    assert_eq!(row["coverage"]["importerScan"], "complete", "{row}");
}

/// A same-named function of another file is another symbol, a comment is no
/// use, and neither counts against the scan.
#[test]
fn other_symbols_and_comments_stay_out_of_the_answer() {
    let row = row_of(
        "references",
        &[
            ("other.c", "int helper(void) { return 1; }\n"),
            ("note.c", "// helper is only described here\nint n;\n"),
        ],
    );
    let files = files_of(&row);
    assert!(files.iter().any(|file| file.ends_with("main.c")), "{row}");
    assert!(!files.iter().any(|file| file.ends_with("other.c")), "{row}");
    assert!(!files.iter().any(|file| file.ends_with("note.c")), "{row}");
    assert_eq!(row["coverage"]["importerScan"], "complete", "{row}");
}

/// Code the preprocessor disabled names no symbol the server can see, and the
/// word-guessing `definition` must not vouch for it: the file stays unverified
/// and the row says so, rather than reading as a complete answer.
#[test]
fn disabled_code_makes_the_answer_partial_not_silently_complete() {
    let row = row_of(
        "callers",
        &[(
            "gated.c",
            "#ifdef NEVER\nint gated(void) { return helper(7); }\n#endif\n",
        )],
    );
    assert_eq!(callers_of(&row), ["caller_fn"], "{row}");
    assert_eq!(row["isPartial"], true, "{row}");
    assert_eq!(row["coverage"]["importerScan"], "failed", "{row}");
}

/// The failed scan names its candidate and why: the server resolved none of
/// its occurrences to the anchor's declaration (clangd with no compilation
/// database reads `gated.c` with the code disabled).
#[test]
fn a_failed_importer_scan_names_the_unverified_candidate_and_its_cause() {
    let row = row_of(
        "references",
        &[(
            "gated.c",
            "#ifdef NEVER\nint gated(void) { return helper(7); }\n#endif\n",
        )],
    );
    assert_eq!(row["coverage"]["importerScan"], "failed", "{row}");
    let warnings = row["warnings"].to_string();
    assert!(
        warnings.contains("resolved no occurrence in gated.c"),
        "{warnings}"
    );
}

/// More candidates than a window: every page reached by `next.nextPage` adds
/// its window's files, each listed exactly once, and the last page is complete.
/// A window whose files the server already answered must not read as the end.
#[test]
fn every_window_is_reached_and_lists_each_file_once() {
    let names = (0..(crate::tools::lsp_search::importers::MAX_CANDIDATE_FILES * 2 + 5))
        .map(|index| format!("c{index:03}.c"))
        .collect::<Vec<_>>();
    let extra = names
        .iter()
        .map(|name| (name.as_str(), "int caller_fn(void) { return helper(1); }\n"))
        .collect::<Vec<_>>();
    for operation in ["callers", "references"] {
        let pages = pages_of(operation, &extra);
        assert!(pages.len() >= 3, "{operation}: {} page(s)", pages.len());
        let listed = pages.iter().flat_map(files_of).collect::<Vec<_>>();
        for name in &names {
            let times = listed.iter().filter(|file| file.ends_with(name)).count();
            assert_eq!(
                times, 1,
                "{operation}: {name} listed {times} time(s): {listed:?}"
            );
        }
        let last = pages.last().expect("a page");
        assert_eq!(
            last["coverage"]["importerScan"], "complete",
            "{operation}: {last}"
        );
        for page in &pages[..pages.len() - 1] {
            assert_eq!(page["isPartial"], true, "{operation}: {page}");
        }
    }
}

mod database;
mod index_finishing;
