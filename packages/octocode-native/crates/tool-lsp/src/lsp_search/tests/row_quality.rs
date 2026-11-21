//! What lspSearch rows state and what they leave out, served through the
//! published row: a definition body sized by its page, text leads bounded by
//! the declaration's reach, gaps in test code, generated-code and ignored-file
//! coverage, and the coverage a complete scan does not state.

use super::*;
use octocode_runtime_base::execution::cancel::NeverCancel;
use octocode_test_support::json_fixture;
use serde_json::{Value, json};

/// A fake language server: `references` lists the `LISTED` sites, and
/// `definition` places every spelling on the declaration unless the file is
/// `UNRESOLVED`.
const SERVER: &str = r#"#!/usr/bin/env node
/*CONFIG*/
let buf = Buffer.alloc(0);
function send(m) {
  const s = JSON.stringify(m);
  process.stdout.write('Content-Length: ' + Buffer.byteLength(s) + '\r\n\r\n' + s);
}
const uriOf = (file) => 'file://' + ROOT + '/' + file;
const site = (file, line, character) => ({ uri: uriOf(file), range: { start: { line, character }, end: { line, character: character + 6 } } });
function handle(msg) {
  if (msg.method === 'initialize') {
    const types = HOVER === null ? {} : { typeDefinitionProvider: true, hoverProvider: true };
    send({ jsonrpc: '2.0', id: msg.id, result: { capabilities: { definitionProvider: true, referencesProvider: true, textDocumentSync: 1, ...types } } });
  } else if (msg.method === 'textDocument/hover' && HOVER !== null) {
    send({ jsonrpc: '2.0', id: msg.id, result: { contents: { kind: 'markdown', value: HOVER } } });
  } else if (msg.method === 'textDocument/typeDefinition') {
    send({ jsonrpc: '2.0', id: msg.id, result: [] });
  } else if (msg.method === 'textDocument/references') {
    send({ jsonrpc: '2.0', id: msg.id, result: LISTED.map((s) => site(s[0], s[1], s[2])) });
  } else if (msg.method === 'textDocument/definition') {
    const uri = msg.params.textDocument.uri;
    const missing = UNRESOLVED.some((file) => uri.endsWith('/' + file));
    send({ jsonrpc: '2.0', id: msg.id, result: missing ? [] : [site(DECL[0], DECL[1], DECL[2])] });
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

/// One scenario: a checkout of `files` (path, text), the language of its
/// anchor file `anchor`, the sites the fake server lists (file, zero-based
/// line and character), where it places the declaration, and the files it
/// cannot place.
struct Case {
    language: &'static str,
    git: bool,
    files: Vec<(&'static str, String)>,
    anchor: &'static str,
    listed: Vec<(&'static str, u32, u32)>,
    declaration: (&'static str, u32, u32),
    unresolved: Vec<&'static str>,
    /// The hover the server answers with; with one, it also answers
    /// `typeDefinition`, always with no location.
    hover: Option<&'static str>,
    query: Value,
}

impl Case {
    fn rust(files: Vec<(&'static str, String)>, anchor: &'static str) -> Self {
        Self {
            language: "rust",
            git: true,
            files,
            anchor,
            listed: vec![(anchor, 0, 3)],
            declaration: (anchor, 0, 3),
            unresolved: Vec::new(),
            hover: None,
            query: json_fixture(
                r#"{
                    "operation": "references",
                    "symbolName": "helper",
                    "line": 1
                }"#,
            ),
        }
    }

    /// The published row of this case's query.
    fn row(self) -> Value {
        self.page(false)
    }

    /// The row as the runtime serves it: leaned as the dispatcher leans it
    /// ([`crate::lsp_search::wire::lean`]), then published.
    fn served(self) -> Value {
        self.page(true)
    }

    /// The first `count` pages as the runtime serves them, each page asked
    /// with the continuation the one before issued.
    fn served_pages(self, count: usize) -> Vec<Value> {
        self.pages(true, count)
    }

    fn page(self, lean: bool) -> Value {
        self.pages(lean, 1).remove(0)
    }

    fn pages(self, lean: bool, count: usize) -> Vec<Value> {
        self.envelopes(lean, count)
            .into_iter()
            .map(data_of)
            .collect()
    }

    /// The published response of each page: the row with what the response
    /// layer states beside it (its hints).
    fn envelopes(self, lean: bool, count: usize) -> Vec<Value> {
        let dir = octocode_runtime_base::test_support::Fixture::new();
        let root = dir.0.canonicalize().expect("canonical root");
        if self.git {
            std::fs::create_dir_all(root.join(".git")).expect("repository");
        }
        for (name, text) in &self.files {
            let path = root.join(name);
            std::fs::create_dir_all(path.parent().expect("parent")).expect("directory");
            std::fs::write(path, text).expect("file");
        }
        let json = |value: Value| value.to_string();
        let sites = self
            .listed
            .iter()
            .map(|(file, line, character)| json!([file, line, character]))
            .collect::<Vec<_>>();
        let script = root.join("fake-lsp.js");
        let config = format!(
            "const ROOT = {}; const LISTED = {}; const DECL = {}; const UNRESOLVED = {}; const HOVER = {};",
            json(json!(root.to_string_lossy())),
            json(json!(sites)),
            json(json!(self.declaration)),
            json(json!(self.unresolved)),
            json(json!(self.hover)),
        );
        write_executable(&script, SERVER.replace("/*CONFIG*/", &config).as_bytes());
        let extension = std::path::Path::new(self.anchor)
            .extension()
            .and_then(|extension| extension.to_str())
            .expect("anchor extension");
        std::fs::write(
            root.join("lsp-servers.json"),
            json!({"languageServers": {format!(".{extension}"): {
                "command": script.to_string_lossy(), "args": [], "languageId": self.language
            }}})
            .to_string(),
        )
        .expect("config");
        let execution = crate::lsp_search::LspExecutionConfig {
            octocode_home: Some(root.clone()),
            ..crate::lsp_search::LspExecutionConfig::with_path()
        };
        // The fixture is the repository as it stood before the first page:
        // its files are settled, and only a later write is a change. A file
        // written within the settle window of a page's start may have been
        // written while it ran, so that page's continuation would restart.
        octocode_engine::stamp::settle_written_so_far();
        let pool = fake_server_pool();
        let paths = octocode_runtime_base::test_support::workspace_policy(&root);
        let mut query = self.query;
        query["path"] = json!(root.join(self.anchor).to_string_lossy());
        let requested = query["path"].as_str().map(str::to_owned);
        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        let mut pages = Vec::new();
        let mut next = Some(super::query(query));
        while let Some(query) = next.take() {
            let mut row = runtime
                .block_on(crate::lsp_search::execute(
                    query,
                    &NeverCancel,
                    &pool,
                    &paths,
                    &execution,
                    None,
                    crate::lsp_search::test_answers(),
                ))
                .expect("row");
            if pages.len() + 1 < count {
                next = Some(super::query(
                    octocode_runtime_base::test_support::published_next_page(
                        octocode_runtime_base::id::ToolId::LspSearch,
                        &row,
                    ),
                ));
            }
            if lean {
                crate::lsp_search::wire::lean(&mut row, requested.as_deref());
            }
            pages.push(public_row(&row));
        }
        runtime.block_on(pool.clear_all());
        pages
    }
}

/// The row's own fields, out of the published envelope.
fn data_of(published: Value) -> Value {
    published
        .pointer("/results/0/data")
        .cloned()
        .unwrap_or(published)
}

fn is_partial(row: &Value) -> bool {
    row["partialReasons"]
        .as_array()
        .is_some_and(|reasons| !reasons.is_empty())
}

fn related_search_paths(row: &Value) -> Vec<String> {
    row["related"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|call| call["tool"] == "localSearch")
        .flat_map(|call| {
            call["input"]["queries"]
                .as_array()
                .cloned()
                .unwrap_or_default()
        })
        .filter_map(|query| query["path"].as_str().map(str::to_owned))
        .collect()
}

/// An 88-line function (lines 5-88 of its file) as the location of a
/// definition, quoted whole.
/// The published row and the row as the tool built it (its reads in full).
fn definition_row(budget: usize) -> (Value, Value) {
    let (_dir, root, paths) = temp_workspace();
    let file = root.join("lib.rs");
    let body = (5..=88)
        .map(|n| format!("    let v{n} = {n};\n"))
        .collect::<String>();
    std::fs::write(&file, format!("// 1\n// 2\n// 3\n// 4\n{body}")).expect("file");
    let uri = format!("file://{}", file.to_string_lossy());
    let q = query(json!({
        "operation": "definition", "path": file.to_string_lossy(), "symbolName": "f", "line": 5
    }));
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let row = runtime.block_on(super::super::locations::locations(
        &q,
        &mut SourceCache::new(&paths),
        "definition",
        "definitionProvider",
        vec![json!({
            "uri": uri,
            "range": {"start": {"line": 4, "character": 4}, "end": {"line": 4, "character": 12}},
            "value": body,
            "displayRange": {"startLine": 5, "endLine": 88}
        })],
        budget,
        &octocode_runtime_base::security::ContentSecurity::new(),
    ));
    (data_of(public_row(&row)), row)
}

fn content_lines(row: &Value) -> Vec<String> {
    let content = row
        .pointer("/matches/0/value")
        .or_else(|| row.pointer("/files/0/matches/0/value"))
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("a definition body in {row}"));
    content.lines().map(str::to_owned).collect()
}

#[test]
fn a_definition_body_the_page_holds_is_returned_whole_without_a_marker() {
    let (row, _) = definition_row(100_000);
    let lines = content_lines(&row);
    assert_eq!(lines.len(), 84, "{row}");
    assert!(
        lines.iter().all(|line| {
            let (number, text) = line.split_once('\t').unwrap_or_default();
            number.parse::<u32>().is_ok() && !text.is_empty()
        }),
        "every body line is a numbered source line: {row}"
    );
    assert!(row.get("isPartial").is_none(), "{row}");
    assert!(
        !row.to_string().contains("declarationClipped"),
        "no clipped reason: {row}"
    );
}

#[test]
fn a_definition_body_the_page_cannot_hold_is_cut_once_and_read_from_the_cut() {
    let (row, raw) = definition_row(900);
    let lines = content_lines(&row);
    assert!(lines.len() < 84, "{row}");
    let numbers = lines
        .iter()
        .map(|line| {
            line.split_once('\t')
                .and_then(|(number, _)| number.parse::<u64>().ok())
                .unwrap_or_else(|| panic!("a numbered line, not a marker: {line:?} in {row}"))
        })
        .collect::<Vec<_>>();
    assert_eq!(numbers.first(), Some(&5), "{row}");
    assert_eq!(
        row["partialReasons"],
        json_fixture(r#"["declarationClipped"]"#),
        "{row}"
    );
    let read = &raw["next"]["readDeclaration"]["input"]["queries"][0]["ranges"][0];
    let (from, to) = (read["startLine"].as_u64(), read["endLine"].as_u64());
    assert_eq!(from, numbers.last().map(|last| last + 1), "{row}");
    assert_eq!(to, Some(88), "{row}");
}

#[test]
fn a_leading_line_too_long_for_the_page_is_still_windowed_by_bytes() {
    let (row, raw) = definition_row(40);
    assert_eq!(
        row["partialReasons"],
        json_fixture(r#"["declarationClipped"]"#),
        "{row}"
    );
    assert_eq!(
        raw["next"]["readDeclaration"]["tool"], "localFetch",
        "{row}"
    );
}

/// A package with `src/outer.rs` declaring `inner` and six sibling modules
/// that call `helper`, and out-of-reach modules that spell it.
fn reach_files() -> Vec<(&'static str, String)> {
    let mut files = vec![
        ("Cargo.toml", "[package]\nname = \"c\"\n".to_owned()),
        ("src/lib.rs", "mod outer;\nmod other;\n".to_owned()),
        (
            "src/outer.rs",
            "mod inner;\nmod f1;\nmod f2;\nmod f3;\nmod f4;\nmod f5;\nmod f6;\n".to_owned(),
        ),
        (
            "src/outer/inner.rs",
            "pub(super) fn helper() {}\n".to_owned(),
        ),
        (
            "src/other.rs",
            "mod g1;\nmod g2;\nmod g3;\nmod g4;\nmod g5;\nmod g6;\n".to_owned(),
        ),
    ];
    for name in ["f1", "f2", "f3", "f4", "f5", "f6"] {
        files.push((
            Box::leak(format!("src/outer/{name}.rs").into_boxed_str()),
            "fn user() { helper(); }\n".to_owned(),
        ));
    }
    for name in ["g1", "g2", "g3", "g4", "g5", "g6"] {
        files.push((
            Box::leak(format!("src/other/{name}.rs").into_boxed_str()),
            "fn user() { helper(); }\n".to_owned(),
        ));
    }
    files
}

#[test]
fn a_text_lead_over_many_files_searches_the_declarations_module_not_the_root() {
    if !octocode_runtime_base::test_support::require_node() {
        return;
    }
    let mut case = Case::rust(reach_files(), "src/outer/inner.rs");
    case.listed = vec![("src/outer/inner.rs", 0, 14)];
    case.declaration = ("src/outer/inner.rs", 0, 14);
    case.unresolved = (1..=6)
        .map(|n| &*Box::leak(format!("src/outer/f{n}.rs").into_boxed_str()))
        .collect();
    let row = case.row();
    assert!(is_partial(&row), "{row}");
    let paths = related_search_paths(&row);
    assert!(!paths.is_empty(), "a text lead: {row}");
    assert!(
        paths.iter().all(|path| path.ends_with("src/outer")),
        "the lead searches the module's directory: {paths:?} in {row}"
    );
}

const TEST_MODULE: &str = "fn helper() {}\nfn used() { helper(); }\n\n#[cfg(test)]\nmod tests {\n    fn t() { helper(); }\n}\n";

fn gap_case(test_code: &str) -> Value {
    let mut case = Case::rust(
        vec![
            ("Cargo.toml", "[package]\nname = \"c\"\n".to_owned()),
            ("lib.rs", TEST_MODULE.to_owned()),
        ],
        "lib.rs",
    );
    case.listed = vec![("lib.rs", 0, 3), ("lib.rs", 1, 14)];
    case.declaration = ("lib.rs", 0, 3);
    case.query["testCode"] = json!(test_code);
    case.row()
}

#[test]
fn a_site_missed_inside_test_code_is_no_gap_when_test_code_is_excluded() {
    if !octocode_runtime_base::test_support::require_node() {
        return;
    }
    let included = gap_case("include");
    assert!(
        included.to_string().contains("lib.rs:6"),
        "the included gap names its line: {included}"
    );
    let excluded = gap_case("exclude");
    assert!(
        !excluded.to_string().contains("lib.rs:6"),
        "no warning names a line of test code: {excluded}"
    );
    assert!(
        excluded["partialReasons"]
            .as_array()
            .is_none_or(|reasons| !reasons.contains(&json!("serverUnanswered"))),
        "{excluded}"
    );
    assert_eq!(excluded["testCodeExcluded"]["lines"], 1, "{excluded}");
}

/// `references` of the method `Local::helper` from a package whose build
/// output its server never loaded, with `use_site` in `src/b.rs`, which the
/// server places nowhere. Only a method is reached through a receiver, whose
/// type the unloaded output may define.
fn generated_case(use_site: &str) -> Value {
    let mut case = Case::rust(
        vec![
            ("Cargo.toml", "[package]\nname = \"c\"\n".to_owned()),
            ("build.rs", "fn main() {}\n".to_owned()),
            ("src/lib.rs", "mod a;\nmod b;\nmod generated;\n".to_owned()),
            (
                "src/generated.rs",
                "include!(concat!(env!(\"OUT_DIR\"), \"/out.rs\"));\n".to_owned(),
            ),
            (
                "src/a.rs",
                "pub struct Local;\nimpl Local { pub fn helper(&self) {} }\nfn used(l: Local) { l.helper(); }\n"
                    .to_owned(),
            ),
            ("src/b.rs", use_site.to_owned()),
        ],
        "src/a.rs",
    );
    // An answer naming no use stands only where every spelling is placed.
    let method = (
        "src/a.rs",
        1,
        u32::try_from("impl Local { pub fn ".len()).expect("column"),
    );
    case.listed = vec![method];
    case.declaration = method;
    case.query["line"] = json!(2);
    case.unresolved = vec!["src/b.rs"];
    case.row()
}

#[test]
fn a_plain_call_the_server_missed_is_not_blamed_on_build_scripts() {
    if !octocode_runtime_base::test_support::require_node() {
        return;
    }
    let plain = generated_case("fn user() { helper(); }\n");
    assert_eq!(
        plain["partialReasons"],
        json_fixture(r#"["serverUnanswered"]"#),
        "{plain}"
    );
    let through_type = generated_case("fn user(t: Generated) { t.helper(); }\n");
    assert!(
        through_type["partialReasons"]
            .as_array()
            .is_some_and(|reasons| reasons.contains(&json!("buildScriptsOff"))),
        "{through_type}"
    );
}

/// A hidden copy of the code is out of the scan, as a gitignored one is: the
/// row stays whole and counts nothing of it.
#[test]
fn a_hidden_copy_is_left_out() {
    if !octocode_runtime_base::test_support::require_node() {
        return;
    }
    let base = || {
        vec![
            ("Cargo.toml", "[package]\nname = \"c\"\n".to_owned()),
            ("src/lib.rs", "mod a;\n".to_owned()),
            (
                "src/a.rs",
                "pub fn helper() {}\nfn used() { helper(); }\n".to_owned(),
            ),
        ]
    };
    let run = |extra: Vec<(&'static str, String)>| {
        let mut files = base();
        files[1].1 = "mod a;\nmod gen;\n".to_owned();
        files.extend(extra);
        let mut case = Case::rust(files, "src/a.rs");
        case.listed = vec![("src/a.rs", 0, 7), ("src/a.rs", 1, 14)];
        case.declaration = ("src/a.rs", 0, 7);
        case.row()
    };
    let hidden = run(vec![(
        ".cache/copy/a.rs",
        "fn user() { helper(); }\n".to_owned(),
    )]);
    assert!(hidden.get("isPartial").is_none(), "{hidden}");
    assert!(hidden["coverage"].get("ignoredFiles").is_none(), "{hidden}");
}

#[test]
fn a_complete_answer_states_no_complete_scan() {
    if !octocode_runtime_base::test_support::require_node() {
        return;
    }
    let mut case = Case::rust(
        vec![
            ("Cargo.toml", "[package]\nname = \"c\"\n".to_owned()),
            (
                "a.ts",
                "export function helper() {}\nhelper();\n".to_owned(),
            ),
        ],
        "a.ts",
    );
    case.language = "typescript";
    case.listed = vec![("a.ts", 0, 16), ("a.ts", 1, 0)];
    case.declaration = ("a.ts", 0, 16);
    let row = case.row();
    assert_eq!(row["matchCount"], 2, "{row}");
    assert!(row.get("isPartial").is_none(), "{row}");
    assert!(row.get("coverage").is_none(), "{row}");
}

/// A complete Rust answer whose text check agrees with it reads as a complete
/// TypeScript one: no coverage key. A count above zero still states coverage
/// (`a_hidden_copy_is_counted_not_partial`).
#[test]
fn a_complete_rust_answer_states_no_coverage_as_a_typescript_one() {
    if !octocode_runtime_base::test_support::require_node() {
        return;
    }
    let mut case = Case::rust(
        vec![
            ("Cargo.toml", "[package]\nname = \"c\"\n".to_owned()),
            ("src/lib.rs", "mod a;\n".to_owned()),
            (
                "src/a.rs",
                "pub fn helper() {}\nfn used() { helper(); }\n".to_owned(),
            ),
        ],
        "src/a.rs",
    );
    case.listed = vec![("src/a.rs", 0, 7), ("src/a.rs", 1, 12)];
    case.declaration = ("src/a.rs", 0, 7);
    let row = case.row();
    assert_eq!(row["matchCount"], 2, "{row}");
    assert!(row.get("isPartial").is_none(), "{row}");
    assert!(row.get("coverage").is_none(), "{row}");
}

/// Whether a reference row states its column is read over the whole answer:
/// a call whose twin on the same line falls on the next page keeps its
/// column on its own served page.
#[test]
fn a_reference_whose_twin_is_on_the_next_page_keeps_its_column() {
    if !octocode_runtime_base::test_support::require_node() {
        return;
    }
    let mut case = Case::rust(
        vec![(
            "a.ts",
            "export function f() { return 1; }\nexport const y = f() + f();\n".to_owned(),
        )],
        "a.ts",
    );
    case.language = "typescript";
    // The server lists the two calls on line 2; one page holds one row.
    case.listed = vec![("a.ts", 1, 17), ("a.ts", 1, 23)];
    case.declaration = ("a.ts", 0, 16);
    case.query =
        json_fixture(r#"{"operation": "references", "symbolName": "f", "line": 2, "limit": 1}"#);
    let page = case.served();
    assert!(
        page.pointer("/next/nextPage").is_some(),
        "the twin is on the next page: {page}"
    );
    let rows = page["files"][0]["matches"].as_array().expect("rows");
    assert_eq!(rows.len(), 1, "{page}");
    assert_eq!(
        (rows[0]["line"].as_u64(), rows[0]["column"].as_u64()),
        (Some(2), Some(18)),
        "{page}"
    );
}

/// A walk to the next page reads the same answer and serves the pair's
/// other call there, with its column: the first page's continuation pins a
/// settled scope, so the second page is no restart.
#[test]
fn a_reference_twin_on_the_next_page_is_served_there_with_its_column() {
    if !octocode_runtime_base::test_support::require_node() {
        return;
    }
    let mut case = Case::rust(
        vec![(
            "a.ts",
            "export function f() { return 1; }\nexport const y = f() + f();\n".to_owned(),
        )],
        "a.ts",
    );
    case.language = "typescript";
    case.listed = vec![("a.ts", 1, 17), ("a.ts", 1, 23)];
    case.declaration = ("a.ts", 0, 16);
    case.query =
        json_fixture(r#"{"operation": "references", "symbolName": "f", "line": 2, "limit": 1}"#);
    let pages = case.served_pages(2);
    let second = &pages[1];
    assert!(second.get("errorCode").is_none(), "{second}");
    let rows = second["files"][0]["matches"].as_array().expect("rows");
    assert_eq!(rows.len(), 1, "{second}");
    assert_eq!(
        (rows[0]["line"].as_u64(), rows[0]["column"].as_u64()),
        (Some(2), Some(24)),
        "{second}"
    );
}

fn sibling_case(sibling_text: &str) -> Value {
    let mut case = Case::rust(
        vec![
            ("a/Cargo.toml", "[package]\nname = \"a\"\n".to_owned()),
            (
                "a/a.rs",
                "fn helper() {}\nfn used() { helper(); }\n".to_owned(),
            ),
            ("b/Cargo.toml", "[package]\nname = \"b\"\n".to_owned()),
            ("b/b.rs", sibling_text.to_owned()),
        ],
        "a/a.rs",
    );
    case.git = false;
    case.listed = vec![("a/a.rs", 0, 3), ("a/a.rs", 1, 14)];
    case.declaration = ("a/a.rs", 0, 3);
    case.row()
}

#[test]
fn a_sibling_project_that_never_spells_the_name_draws_no_sweep() {
    if !octocode_runtime_base::test_support::require_node() {
        return;
    }
    let quiet = sibling_case("fn other() {}\n");
    assert!(
        !quiet
            .to_string()
            .contains("projects beside it may use this name"),
        "{quiet}"
    );
    assert!(related_search_paths(&quiet).is_empty(), "{quiet}");
    let spelling = sibling_case("fn user() { helper(); }\n");
    assert!(
        spelling
            .to_string()
            .contains("projects beside it may use this name: b"),
        "{spelling}"
    );
    assert!(!related_search_paths(&spelling).is_empty(), "{spelling}");
}

/// A package that `include!`s a file from outside its directory, where that
/// file declares the name of a parameter of the package's own function: the
/// row of `typeDefinition` on that parameter, the server showing `hover` for
/// it and placing the parameter itself unless `unplaced`.
fn empty_type_case(hover: &'static str, unplaced: bool) -> Value {
    let mut case = Case::rust(
        vec![
            ("p/Cargo.toml", "[package]\nname = \"p\"\n".to_owned()),
            (
                "p/src/lib.rs",
                "include!(\"../../shared/types.rs\");\nfn f(value: i64) {}\n".to_owned(),
            ),
            ("shared/types.rs", "pub fn value() {}\n".to_owned()),
        ],
        "p/src/lib.rs",
    );
    case.declaration = ("p/src/lib.rs", 1, 5);
    if unplaced {
        case.unresolved = vec!["p/src/lib.rs"];
    }
    case.hover = Some(hover);
    case.query = json_fixture(
        r#"{
            "operation": "typeDefinition",
            "symbolName": "value",
            "line": 2
        }"#,
    );
    case.row()
}

fn names_the_include(row: &Value) -> bool {
    row["hints"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .any(|hint| hint.contains("include!"))
        || related_search_paths(row)
            .iter()
            .any(|path| path.ends_with("shared/types.rs"))
}

/// A parameter of a primitive type has no type location, and the server
/// knows its type: an outside `include!` that happens to declare the
/// parameter's name explains nothing, so the row is complete.
#[test]
fn an_empty_type_the_server_resolved_is_not_blamed_on_an_outside_include() {
    let row = empty_type_case("```rust\nvalue: i64\n```", false);
    assert_eq!(row["category"], "noLocations", "{row}");
    assert!(!is_partial(&row), "{row}");
    assert!(!names_the_include(&row), "{row}");
    // The workspace spells the parameter, not its type: no text search is
    // offered for a type answer.
    assert!(related_search_paths(&row).is_empty(), "{row}");
}

/// A type the server could not resolve, on a name it places in the
/// package's own source: the package's outside include may hold the type
/// (partial), and it does not declare the name asked about (no include lead).
#[test]
fn an_unresolved_type_of_a_placed_name_is_partial_without_the_name_s_include_lead() {
    let row = empty_type_case("```rust\nvalue: {unknown}\n```", false);
    assert_eq!(
        row["partialReasons"],
        json!(["includeOutsidePackage"]),
        "{row}"
    );
    assert!(!names_the_include(&row), "{row}");
}

/// A name the server places nowhere and an outside include declares: the
/// include is the lead, as on `definition`.
#[test]
fn an_unresolved_type_of_an_unplaced_name_keeps_its_include_lead() {
    let row = empty_type_case("```rust\nvalue: {unknown}\n```", true);
    assert_eq!(
        row["partialReasons"],
        json!(["includeOutsidePackage"]),
        "{row}"
    );
    assert!(names_the_include(&row), "{row}");
}

/// The row of `references` on `helper` at line 1 of a file that spells it
/// once there, asked with `occurrence` and under `name`.
fn anchor_row(name: &str, occurrence: u32) -> Value {
    let mut case = Case::rust(
        vec![("src/a.rs", "fn helper() {}\n".to_owned())],
        "src/a.rs",
    );
    case.query = json!({
        "operation": "references",
        "symbolName": name,
        "line": 1,
        "occurrence": occurrence,
    });
    case.envelopes(false, 1).remove(0)
}

/// An `occurrence` past the name's occurrences on its line: the name and the
/// line are found, and the error states how many there are. Nothing is left
/// to locate, so the row is the error alone; an anchor whose name is missing
/// keeps the calls that locate it.
#[test]
fn an_occurrence_past_the_line_s_own_is_answered_by_its_error_alone() {
    let published = anchor_row("helper", 3);
    assert!(
        published.get("hints").is_none() && published.pointer("/shared/data").is_none(),
        "no hint and no receipt beside the row: {published}"
    );
    let past = data_of(published);
    assert_eq!(past["errorClass"], "correctInput", "{past}");
    let stated = past
        .as_object()
        .expect("row")
        .keys()
        .filter(|key| {
            !["status", "errorCode", "errorClass", "error", "path"].contains(&key.as_str())
        })
        .collect::<Vec<_>>();
    assert!(stated.is_empty(), "nothing beside the error: {past}");
    assert!(
        past["error"]
            .as_str()
            .is_some_and(|error| error.contains("occurrence")),
        "{past}"
    );
    let missing = data_of(anchor_row("absent", 3));
    assert_eq!(missing["errorCode"], "notFound", "{missing}");
    assert!(
        missing["related"]
            .as_array()
            .is_some_and(|calls| !calls.is_empty()),
        "a name the line lacks keeps its leads: {missing}"
    );
}
