//! A JS/TS file oxc cannot parse keeps its facts through the tree-sitter
//! lane. What that lane cannot settle stays unsettled, and the scan names the
//! route per file: `foo?.bar = 1;` is a fatal oxc error tree-sitter reads clean.
use super::*;

/// Fatal for oxc, clean for tree-sitter.
const OXC_FATAL: &str = "foo?.bar = 1;\n";

fn source(text: &str) -> String {
    format!("{text}{OXC_FATAL}")
}

fn cycle_runtime(root: &Path) -> Vec<Value> {
    ask(root, "cycles", None)["rows"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|row| row["runtime"].clone())
        .collect()
}

fn vias(root: &Path, file: &str) -> Vec<(String, String)> {
    let mut rows = ask(root, "deps", Some(file))["rows"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|row| {
            let text = |key: &str| row[key].as_str().unwrap_or_default().to_owned();
            (text("id"), text("via"))
        })
        .collect::<Vec<_>>();
    rows.sort();
    rows
}

/// `import type` is erased whichever parser reads it: the statement's own
/// `type` token settles it, so no cycle of these can be a runtime one.
#[test]
fn a_fallback_file_reads_import_type_from_the_tree() {
    let (a, b) = (
        source("import type { B } from './b';\nexport class A { b?: B; }\n"),
        source("import type { A } from './a';\nexport class B { a?: A; }\n"),
    );
    let temp = fixture(&[("a.ts", &a), ("b.ts", &b)]);
    let root = ingested(&temp);
    assert_eq!(cycle_runtime(root), [json!(false)]);
    assert_eq!(vias(root, "a.ts"), [to("b.ts", "type-import")]);
}

/// A value import through the fallback lane claims no load order: a cycle
/// only such an edge closes is neither a proven runtime cycle nor proven free
/// of one.
#[test]
fn a_cycle_through_a_fallback_import_has_unknown_runtime() {
    let a = source("import { b } from './b';\nexport const a = () => b;\n");
    let temp = fixture(&[
        ("a.ts", &a),
        (
            "b.ts",
            "import { a } from './a';\nexport const b = () => a;\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(cycle_runtime(root), [json!("unknown")]);
    assert_eq!(vias(root, "a.ts"), [to("b.ts", "recovered-import")]);
    assert_eq!(vias(root, "b.ts"), [to("a.ts", "static-import")]);
    let issues = ask(root, "issues", None);
    let cycle = issues["rows"]
        .as_array()
        .and_then(|rows| rows.iter().find(|row| row["detector"] == "cycle"))
        .cloned()
        .unwrap_or_else(|| panic!("no cycle finding: {issues}"));
    assert_eq!(cycle["evidence"]["runtime"], "unknown", "{cycle}");
}

/// A cycle two oxc-parsed files close stays a runtime cycle, with a fallback
/// edge beside it.
#[test]
fn a_cycle_eager_imports_close_alone_stays_runtime() {
    let c = source("import { a } from './a';\nexport const c = () => a;\n");
    let temp = fixture(&[
        (
            "a.ts",
            "import { b } from './b';\nimport { c } from './c';\nexport const a = () => [b, c];\n",
        ),
        (
            "b.ts",
            "import { a } from './a';\nexport const b = () => a;\n",
        ),
        ("c.ts", &c),
    ]);
    let root = ingested(&temp);
    assert_eq!(cycle_runtime(root), [json!(true)]);
}

/// The route is disclosed per affected file: which file, by what producer, in
/// the scan record both a build and `stats` answer with, and why in a
/// `diagnostics` row.
#[test]
fn the_scan_names_each_file_the_fallback_lane_read() {
    let broken = source("export function run() {}\n");
    let temp = fixture(&[
        ("clean.ts", "export function clean() {}\n"),
        ("broken.ts", &broken),
    ]);
    let root = temp.0.as_path();
    let built = ingest(root, root).value;
    let expected = json!([{"file": "broken.ts", "producer": "tree-sitter-facts:ts"}]);
    assert_eq!(built["scan"]["parserFallback"], expected, "{built}");
    assert_eq!(ask(root, "stats", None)["scan"]["parserFallback"], expected);
    let diagnostics = ask(root, "diagnostics", None);
    let rows = diagnostics["rows"].as_array().cloned().unwrap_or_default();
    let fallback = rows
        .iter()
        .filter(|row| row["code"] == "parser-fallback")
        .collect::<Vec<_>>();
    assert_eq!(fallback.len(), 1, "{diagnostics}");
    assert_eq!(fallback[0]["file"], "broken.ts");
    let message = fallback[0]["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("oxc") && message.contains("tree-sitter"),
        "{message}"
    );
    // A file the owning parser read names no route.
    assert_eq!(rows.len(), 1, "{diagnostics}");
}

/// Facts are reused on a rebuild; the disclosure must not depend on a fresh
/// parse.
#[test]
fn a_rebuild_keeps_the_route_disclosure() {
    let broken = source("export function run() {}\n");
    let temp = fixture(&[("broken.ts", &broken)]);
    let root = temp.0.as_path();
    let first = ingest(root, root).value;
    let second = ingest(root, root).value;
    assert_eq!(
        first["scan"]["parserFallback"], second["scan"]["parserFallback"],
        "{second}"
    );
    assert_eq!(
        second["scan"]["parserFallback"][0]["file"], "broken.ts",
        "{second}"
    );
}

/// Re-exports survive the fallback lane, without a load claim except the
/// statement's own `type` token.
#[test]
fn a_fallback_file_keeps_its_reexport_edges() {
    let index = source(
        "export { a, x as y } from './a';\nexport * from './b';\nexport * as ns from './n';\nexport type { C } from './c';\n",
    );
    let temp = fixture(&[
        ("index.ts", &index),
        ("a.ts", "export const a = 1;\nexport const x = 2;\n"),
        ("b.ts", "export const b = 1;\n"),
        ("n.ts", "export const n = 1;\n"),
        ("c.ts", "export interface C {}\n"),
        (
            "use.ts",
            "import { y, ns } from './index';\nexport const v = () => [y, ns];\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(
        vias(root, "index.ts"),
        [
            to("a.ts", "recovered-named-reexport"),
            to("b.ts", "recovered-star-reexport"),
            to("c.ts", "type-named-reexport"),
            to("n.ts", "recovered-named-reexport"),
        ]
    );
    let dependents = result_ids(&ask(root, "dependents", Some("a.ts")))
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    assert!(
        dependents.iter().any(|id| id == "index.ts"),
        "{dependents:?}"
    );
}

fn symbol<'v>(symbols: &'v Value, name: &str) -> &'v Value {
    symbols["rows"]
        .as_array()
        .and_then(|rows| rows.iter().find(|row| row["name"] == name))
        .unwrap_or_else(|| panic!("no symbol {name}: {symbols}"))
}

/// `exported` follows the written `export`, not the top-level position.
#[test]
fn a_fallback_file_exports_only_what_it_writes_as_exported() {
    let text = source(
        "function hidden() {}\nexport function pub() {}\nexport const arrow = () => 1;\nconst internal = 2;\nexport { internal as renamed };\nexport default class K {}\nclass Unused {}\n",
    );
    let temp = fixture(&[("m.ts", &text)]);
    let root = ingested(&temp);
    let symbols = ask(root, "symbols", Some("m.ts"));
    let exported = |name: &str| symbol(&symbols, name)["exported"] == true;
    assert!(!exported("hidden"), "{symbols}");
    assert!(!exported("Unused"), "{symbols}");
    assert!(exported("pub"), "{symbols}");
    assert!(exported("arrow"), "{symbols}");
    assert!(exported("internal"), "{symbols}");
    assert!(exported("K"), "{symbols}");
}

fn reason(root: &Path, callee: &str) -> Value {
    let out = ask(root, "unresolved", None);
    out["rows"]
        .as_array()
        .and_then(|rows| rows.iter().find(|row| row["targetName"] == callee))
        .map_or_else(|| json!({"linked": callee}), |row| row["reason"].clone())
}

/// A function the file declares, and a call to it: linked, or at least never
/// `not-declared-in-repo`.
#[test]
fn a_fallback_file_links_its_own_arrow_functions() {
    let text = source(
        "const go = () => 1;\nfunction helper() {}\nexport function run() { go(); helper(); }\n",
    );
    let temp = fixture(&[("m.ts", &text)]);
    let root = ingested(&temp);
    assert_eq!(reason(root, "go"), json!({"linked": "go"}));
    assert_eq!(reason(root, "helper"), json!({"linked": "helper"}));
}

/// A declaration parsing recovered is no link target, but the name is
/// declared: `not-declared-in-repo` would send a reader to look elsewhere.
#[test]
fn a_call_to_a_recovered_declaration_says_so() {
    let temp = fixture(&[(
        "a.c",
        "API_EXPORT int macro_decorated(int x) { return x; }\nint uses(void) { return macro_decorated(1) + nowhere(2); }\n",
    )]);
    let root = ingested(&temp);
    assert_eq!(reason(root, "macro_decorated"), "recovered-declaration");
    assert_eq!(reason(root, "nowhere"), "not-declared-in-repo");
}

/// A member call elsewhere shares only the spelling with a recovered
/// declaration: nothing ties it to that one.
#[test]
fn a_member_call_elsewhere_is_not_tied_to_a_recovered_declaration() {
    let temp = fixture(&[
        ("a.c", "API_EXPORT int shared(int x) { return x; }\n"),
        (
            "b.c",
            "struct S { int v; };\nint use(struct S *s) { return s->shared(1); }\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(reason(root, "s->shared"), "not-declared-in-repo");
}
