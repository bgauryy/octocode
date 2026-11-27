//! The syntax rung through the public entry: a file whose server does not
//! start answers every operation from tree-sitter facts, labelled and paged
//! exactly once; a file with no grammar stays a text row with its leads.
use octocode_runtime_base::id::ToolId;
use octocode_runtime_base::test_support::checks::{Page, Walker};
use serde_json::{Value, json};
use std::path::Path;

mod raw_identifiers;

const JAVA: &str = "package demo;\n\ninterface Shape { double area(); }\n\nclass Square implements Shape {\n    public double area() { return side(); }\n    double side() { return 2; }\n}\n\nclass Main {\n    void run() { new Square().area(); }\n}\n";

/// A workspace with `name` holding `content`, and a home whose user config
/// routes `extension` to a server command that does not exist.
fn fixture(
    name: &str,
    content: &str,
    extension: &str,
) -> (
    (
        octocode_runtime_base::test_support::Fixture,
        octocode_runtime_base::test_support::Fixture,
    ),
    std::path::PathBuf,
    super::super::LspExecutionConfig,
) {
    let dir = octocode_runtime_base::test_support::Fixture::new();
    let root = dir.0.canonicalize().expect("canonical root");
    let file = root.join(name);
    std::fs::write(&file, content).expect("source");
    let home = octocode_runtime_base::test_support::Fixture::new();
    std::fs::write(
        home.0.join("lsp-servers.json"),
        json!({"languageServers": {extension: {
            "command": home.0.join("missing-server").to_string_lossy(), "args": [],
            "languageId": "plaintext"
        }}})
        .to_string(),
    )
    .expect("config");
    // The files above settle at once: a continuation seals a revision only
    // for a scope whose files are older than the settle window.
    octocode_engine::stamp::settle_written_so_far();
    let execution = super::super::LspExecutionConfig {
        octocode_home: Some(home.0.clone()),
        ..super::super::LspExecutionConfig::with_path()
    };
    ((dir, home), file, execution)
}

/// Every page of `start`, walked through its `next` leads (each row once).
async fn walk(
    root: &Path,
    execution: &super::super::LspExecutionConfig,
    start: Value,
    budget: Option<usize>,
) -> Vec<Value> {
    let paths = octocode_runtime_base::test_support::workspace_policy(root);
    let pool = super::super::tests::fake_server_pool();
    let mut walker = Walker::new(ToolId::LspSearch.as_str(), json!({ "queries": [start] }));
    let mut pages = Vec::new();
    while let Some(call) = walker.next_query() {
        let row =
            octocode_runtime_base::test_support::opened_input(ToolId::LspSearch, &call)["queries"]
                [0]
            .clone();
        let result = super::super::execute(
            serde_json::from_value(row).expect("query"),
            &octocode_runtime_base::execution::cancel::NeverCancel,
            &pool,
            &paths,
            execution,
            budget,
            super::super::test_answers(),
        )
        .await
        .expect("row");
        walker.accept(Page {
            output: octocode_runtime_base::test_support::published(ToolId::LspSearch, &result),
            rows: Vec::new(),
        });
        pages.push(result);
    }
    walker.finish();
    pages
}

fn assert_syntax(row: &Value) {
    assert_eq!(row["lsp"]["level"], "syntax", "{row}");
    assert_eq!(row["lsp"]["source"], "native-graph-facts", "{row}");
    assert_eq!(row["lsp"]["serverAvailable"], false, "{row}");
    assert_eq!(row["lsp"]["fellBackFrom"]["level"], "server", "{row}");
    assert_eq!(row["isPartial"], true, "{row}");
    assert!(
        row["partialReasons"]
            .to_string()
            .contains("serverUnanswered"),
        "{row}"
    );
}

/// Every operation on a Java file whose server is missing answers at the
/// syntax level with what tree-sitter sees.
#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn a_file_without_a_server_answers_every_operation_from_syntax() {
    let (_dirs, file, execution) = fixture("Shapes.java", JAVA, ".java");
    let root = file.parent().expect("root").to_path_buf();
    let path = file.to_string_lossy().into_owned();
    let cases = [
        (
            json!({"operation": "documentSymbols", "path": path}),
            "Square",
        ),
        (json!({"operation": "diagnostic", "path": path}), ""),
        (
            json!({"operation": "workspaceSymbol", "path": path, "symbolFuzzy": "Squ"}),
            "Square",
        ),
        (
            json!({"operation": "definition", "path": path, "symbolName": "area", "line": 11}),
            "area",
        ),
        (
            json!({"operation": "typeDefinition", "path": path, "symbolName": "Square", "line": 11}),
            "Square",
        ),
        (
            json!({"operation": "references", "path": path, "symbolName": "side", "line": 7}),
            "side",
        ),
        (
            json!({"operation": "implementation", "path": path, "symbolName": "Shape", "line": 3}),
            "Square",
        ),
        (
            json!({"operation": "subtypes", "path": path, "symbolName": "Shape", "line": 3}),
            "Square",
        ),
        (
            json!({"operation": "supertypes", "path": path, "symbolName": "Square", "line": 5}),
            "Shape",
        ),
        (
            json!({"operation": "callers", "path": path, "symbolName": "side", "line": 7}),
            "area",
        ),
        (
            json!({"operation": "callees", "path": path, "symbolName": "area", "line": 6}),
            "side",
        ),
        (
            json!({"operation": "hover", "path": path, "symbolName": "side", "line": 7}),
            "double side()",
        ),
    ];
    for (query, expected) in cases {
        let pages = walk(&root, &execution, query.clone(), None).await;
        let row = &pages[0];
        assert_syntax(row);
        assert!(row.to_string().contains(expected), "{query}: {row}");
        // A references row marks the declaration occurrence, as a server's does.
        if query["operation"] == "references" {
            assert!(row.to_string().contains(r#""role":"declaration""#), "{row}");
        }
    }
}

/// A syntax outline cut into small pages reaches every symbol exactly once.
#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn a_syntax_outline_pages_every_symbol_once() {
    let source = (0..40)
        .map(|index| format!("fn function_{index}() {{}}\n"))
        .collect::<String>();
    let (_dirs, file, execution) = fixture("lib.rs", &source, ".rs");
    let root = file.parent().expect("root").to_path_buf();
    let query = json!({"operation": "documentSymbols", "path": file.to_string_lossy()});
    let pages = walk(&root, &execution, query, Some(600)).await;
    assert!(pages.len() > 1, "{pages:?}");
    let mut names = Vec::new();
    for page in &pages {
        assert_syntax(page);
        for symbol in page["symbols"].as_array().into_iter().flatten() {
            names.push(symbol["symbolName"].as_str().unwrap_or_default().to_owned());
        }
    }
    let expected = (0..40)
        .map(|index| format!("function_{index}"))
        .collect::<Vec<_>>();
    assert_eq!(names, expected);
}

/// A file with no grammar and no server stays a text row with its leads.
#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn a_file_without_a_grammar_stays_a_text_row_with_leads() {
    let (_dirs, file, execution) = fixture("notes.rb", "def greet\n  1\nend\n", ".zzz");
    let root = file.parent().expect("root").to_path_buf();
    let query = json!({
        "operation": "definition", "path": file.to_string_lossy(),
        "symbolName": "greet", "line": 1
    });
    let pages = walk(&root, &execution, query, None).await;
    let row = &pages[0];
    assert_eq!(row["errorCode"], "serverUnavailable", "{row}");
    assert_eq!(row["lsp"]["level"], "text", "{row}");
    assert!(row["related"].to_string().contains("localSearch"), "{row}");
}

/// `includeDeclaration: false` leaves the declaration row out of syntax
/// references, as it does of a server's, and keeps the uses.
#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn references_without_the_declaration_leave_its_row_out() {
    let (_dirs, file, execution) = fixture("Shapes.java", JAVA, ".java");
    let root = file.parent().expect("root").to_path_buf();
    let path = file.to_string_lossy().into_owned();
    let query = |include: bool| {
        json!({
            "operation": "references", "path": path, "symbolName": "side", "line": 7,
            "includeDeclaration": include
        })
    };
    let with = walk(&root, &execution, query(true), None).await;
    assert!(
        with[0].to_string().contains(r#""role":"declaration""#),
        "{}",
        with[0]
    );
    let without = walk(&root, &execution, query(false), None).await;
    assert_syntax(&without[0]);
    let text = without[0].to_string();
    assert!(!text.contains(r#""role":"declaration""#), "{text}");
    assert!(text.contains(r#""role":"call""#), "{text}");
}

/// A field's own line is its declaration at the syntax level, as a method's
/// is, so `includeDeclaration: false` leaves it out and keeps the uses.
#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn a_field_declaration_row_is_its_declaration() {
    for (name, extension, source, line) in [
        (
            "Square.cs",
            ".cs",
            "public class Square\n{\n    public int Side;\n    public int Area() { return Side * Side; }\n}\n",
            3,
        ),
        (
            "Square.java",
            ".java",
            "public class Square {\n    public int side;\n    public int area() { return side * side; }\n}\n",
            2,
        ),
    ] {
        let (_dirs, file, execution) = fixture(name, source, extension);
        let root = file.parent().expect("root").to_path_buf();
        let path = file.to_string_lossy().into_owned();
        let symbol = if extension == ".cs" { "Side" } else { "side" };
        let query = |include: bool| {
            json!({
                "operation": "references", "path": path, "symbolName": symbol, "line": line,
                "includeDeclaration": include
            })
        };
        let roles = |pages: &[Value]| {
            let mut roles = Vec::new();
            for page in pages {
                assert_syntax(page);
                collect_roles(page, &mut roles);
            }
            roles.sort();
            roles
        };
        let with = roles(&walk(&root, &execution, query(true), None).await);
        let declaration = (line, "declaration".to_owned());
        assert!(with.contains(&declaration), "{name}: {with:?}");
        let without = roles(&walk(&root, &execution, query(false), None).await);
        assert!(
            without.iter().all(|(_, role)| role != "declaration"),
            "{name}: {without:?}"
        );
        assert_eq!(
            without.len() + 1,
            with.len(),
            "{name}: {with:?} {without:?}"
        );
    }
}

/// `(startLine, role)` of every site row in `value`.
fn collect_roles(value: &Value, roles: &mut Vec<(u64, String)>) {
    match value {
        Value::Object(fields) => {
            if let (Some(role), Some(line)) = (
                fields.get("role").and_then(Value::as_str),
                fields.get("line").and_then(Value::as_u64),
            ) {
                roles.push((line, role.to_owned()));
            }
            fields
                .values()
                .for_each(|inner| collect_roles(inner, roles));
        }
        Value::Array(items) => items.iter().for_each(|inner| collect_roles(inner, roles)),
        _ => {}
    }
}

/// Syntax references cut into small pages reach every site exactly once,
/// each page re-deriving the same ordered answer its snapshot names.
#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn syntax_references_page_every_site_once() {
    let source = std::iter::once("fn target() {}\n".to_owned())
        .chain((0..30).map(|index| format!("fn caller_{index}() {{ target(); }}\n")))
        .collect::<String>();
    let (_dirs, file, execution) = fixture("lib.rs", &source, ".rs");
    let root = file.parent().expect("root").to_path_buf();
    let query = json!({
        "operation": "references", "path": file.to_string_lossy(),
        "symbolName": "target", "line": 1
    });
    let pages = walk(&root, &execution, query, Some(900)).await;
    assert!(pages.len() > 1, "{} page(s)", pages.len());
    let mut lines = Vec::new();
    for page in &pages {
        assert_syntax(page);
        collect_sites(page, &mut lines);
    }
    // The declaration (line 1) and one call on each of lines 2..=31.
    let expected = (1..=31).collect::<Vec<u64>>();
    assert_eq!(lines, expected);
}

/// The first line of every row that carries a `role`, in page order.
fn collect_sites(value: &Value, lines: &mut Vec<u64>) {
    match value {
        Value::Object(fields) => {
            if fields.contains_key("role")
                && let Some(line) = fields.get("line").and_then(Value::as_u64)
            {
                lines.push(line);
            }
            fields
                .values()
                .for_each(|inner| collect_sites(inner, lines));
        }
        Value::Array(items) => items.iter().for_each(|inner| collect_sites(inner, lines)),
        _ => {}
    }
}

/// Syntax `callers` keeps the callers the query's `testCode` keeps, as a
/// server's walk does: `exclude` leaves the test file's caller out and counts
/// it in `testCodeExcluded`, `only` keeps only that caller.
#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn syntax_callers_honor_test_code() {
    let (_dirs, file, execution) = fixture(
        "a.ts",
        "export function target() {}\nexport function caller() { target(); }\n",
        ".ts",
    );
    let root = file.parent().expect("root").to_path_buf();
    std::fs::write(
        root.join("a.test.ts"),
        "import {target} from './a';\nexport function in_test() { target(); }\n",
    )
    .expect("test file");
    octocode_engine::stamp::settle_written_so_far();
    let path = file.to_string_lossy().into_owned();
    let query = |test_code: &str| {
        json!({
            "operation": "callers", "path": path, "symbolName": "target", "line": 1,
            "testCode": test_code
        })
    };
    let every = walk(&root, &execution, query("include"), None).await;
    let text = every[0].to_string();
    assert!(
        text.contains("in_test") && text.contains("\"caller\""),
        "{text}"
    );
    let excluded = walk(&root, &execution, query("exclude"), None).await;
    assert_syntax(&excluded[0]);
    let text = excluded[0].to_string();
    assert!(
        !text.contains("in_test") && text.contains("\"caller\""),
        "{text}"
    );
    assert!(excluded[0].get("testCodeExcluded").is_some(), "{text}");
    let only = walk(&root, &execution, query("only"), None).await;
    assert_syntax(&only[0]);
    let text = only[0].to_string();
    assert!(
        text.contains("in_test") && !text.contains("\"caller\""),
        "{text}"
    );
}

/// `callers` and `callees` anchored on a declaration no call names answer
/// `notCallable` from the anchor's parse, before any server, and lead to
/// `references` and `implementation`, in the LSP kind vocabulary (a trait is an
/// `interface`), though an `impl` block names the type too; a callable anchor
/// still goes on.
#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn a_call_hierarchy_of_a_non_callable_declaration_leads_to_its_uses() {
    let source = "pub enum Code { A }\n\npub trait Scan {\n    fn scan(&self);\n}\n\npub fn run() {}\n\nimpl Code {\n    pub fn a(&self) {}\n}\n";
    let (_dirs, file, execution) = fixture("lib.rs", source, ".rs");
    let root = file.parent().expect("root").to_path_buf();
    for (operation, symbol, line, kind) in [
        ("callers", "Code", 1, "enum"),
        ("callees", "Scan", 3, "interface"),
    ] {
        let query = json!({
            "operation": operation, "path": file.to_string_lossy(),
            "symbolName": symbol, "line": line
        });
        let pages = walk(&root, &execution, query, None).await;
        assert_eq!(pages.len(), 1, "{pages:?}");
        let row = &pages[0];
        assert_eq!(row["status"], "empty", "{row}");
        assert_eq!(row["category"], "notCallable", "{row}");
        assert_eq!(row["kind"], kind, "{row}");
        let related = row["related"].to_string();
        assert!(related.contains("\"references\""), "{row}");
        assert!(related.contains("\"implementation\""), "{row}");
    }
    // A function anchor is callable: the server rung runs (here it is
    // missing, so the syntax rung answers).
    let query = json!({
        "operation": "callers", "path": file.to_string_lossy(),
        "symbolName": "run", "line": 7
    });
    let pages = walk(&root, &execution, query, None).await;
    assert_ne!(pages[0]["category"], "notCallable", "{}", pages[0]);
    assert_syntax(&pages[0]);
}
