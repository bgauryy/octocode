/// Reference count of the top-level declaration `name` in `source`.
fn count(source: &str, name: &str) -> u32 {
    let extraction = super::super::js_oxc::extract_graph_facts_with_metadata(source, "mod.ts")
        .expect("graph facts");
    let id = &extraction
        .facts
        .declarations
        .iter()
        .find(|declaration| declaration.name == name && declaration.parent.is_none())
        .unwrap_or_else(|| panic!("declaration {name}"))
        .id;
    extraction
        .reference_counts
        .iter()
        .find(|count| &count.declaration_id == id)
        .expect("counted")
        .count
}

#[test]
fn comments_and_strings_are_not_references() {
    let source = "export function helper() {}\n// helper is documented here\n/* helper */\nconst label = 'helper';\nconst tpl = `helper ${label}`;\n";
    assert_eq!(count(source, "helper"), 0);
}

#[test]
fn value_uses_count_but_calls_and_export_clauses_do_not() {
    let source = "function cb() {}\nfunction called() {}\nclass Built {}\nlet slot = 0;\nregister(cb);\nconst alias = cb;\ncalled(); (called)(); new Built();\nslot = 1;\nexport { cb, called as renamed };\nexport default slot;\n";
    assert_eq!(count(source, "cb"), 2, "argument + assignment");
    assert_eq!(count(source, "called"), 0, "call targets are edges");
    assert_eq!(count(source, "Built"), 0, "construction is an edge");
    assert_eq!(count(source, "slot"), 0, "a write and an export clause");
    assert_eq!(count(source, "alias"), 0);
}

#[test]
fn a_shadowing_local_does_not_reference_the_export() {
    let source = "export function run() { return 1 }\nexport function other() { const run = () => 2; register(run); }\n";
    assert_eq!(count(source, "run"), 0);
    let escaped =
        "export function run() { return 1 }\nexport function other() { register(run); }\n";
    assert_eq!(count(escaped, "run"), 1);
}

#[test]
fn type_positions_reference_type_declarations() {
    let source = "export interface Shape {}\nexport type Unused = number;\nexport function area(s: Shape) { return s }\n";
    assert_eq!(count(source, "Shape"), 1);
    assert_eq!(count(source, "Unused"), 0);
}

/// `typeUseOnly` of each import binding of `file`, by local name.
fn type_use_only(source: &str, file: &str) -> Vec<(String, bool)> {
    super::super::js_oxc::extract_graph_facts_with_metadata(source, file)
        .expect("graph facts")
        .facts
        .imports
        .iter()
        .map(|import| {
            (
                import.local_name.clone().unwrap_or_default(),
                import.type_use_only,
            )
        })
        .collect()
}

/// TypeScript elides an import binding no value position reads; a JSX
/// tag, an expression, an export clause and a decorator read it.
#[test]
fn an_import_read_only_in_type_positions_is_type_use_only() {
    let source = "import { Shape } from './shape';\nimport { Kind } from './kind';\nimport * as ns from './ns';\nimport Unused from './unused';\nimport { Plain } from './plain';\nimport { Again } from './again';\nimport { Deco } from './deco';\nimport type { Erased } from './erased';\nexport function area(s: Shape, k: typeof Kind): ns.T { return s as unknown as ns.T; }\nclass C implements Shape {}\nconst p = Plain;\nexport { Again };\n@Deco class D {}\nlet e: Erased;\n";
    assert_eq!(
        type_use_only(source, "mod.ts"),
        [
            ("Shape".to_owned(), true),
            ("Kind".to_owned(), true),
            ("ns".to_owned(), true),
            ("Unused".to_owned(), true),
            ("Plain".to_owned(), false),
            ("Again".to_owned(), false),
            ("Deco".to_owned(), false),
            ("Erased".to_owned(), false),
        ]
    );
}

/// A component rendered as a JSX tag is a value: the emitted call reads it.
#[test]
fn a_jsx_tag_reads_its_import_as_a_value() {
    let source = "import { Button } from './button';\nimport { Props } from './props';\nimport { Icon } from './icon';\nexport function App(p: Props) { return <div><Button /><Icon.Small /></div>; }\n";
    assert_eq!(
        type_use_only(source, "app.tsx"),
        [
            ("Button".to_owned(), false),
            ("Props".to_owned(), true),
            ("Icon".to_owned(), false),
        ]
    );
}

/// Each specifier of one declaration is judged by its own binding.
#[test]
fn specifiers_of_one_declaration_are_judged_apart() {
    let source = "import { B, make } from './b';\nexport function build(): B { return make(); }\n";
    assert_eq!(
        type_use_only(source, "a.ts"),
        [("B".to_owned(), true), ("make".to_owned(), false)]
    );
}

/// JavaScript emit keeps every import, read or not.
#[test]
fn a_javascript_import_is_never_type_use_only() {
    assert_eq!(
        type_use_only("import { unused } from './a';\n", "mod.js"),
        [("unused".to_owned(), false)]
    );
}

#[test]
fn members_count_non_call_member_accesses() {
    let extraction = super::super::js_oxc::extract_graph_facts_with_metadata(
        "export class Svc { go() {} stop() {} tick() {}\n  start() { this.go(); setTimeout(this.stop); } }\n// this.tick\n",
        "svc.ts",
    )
    .expect("graph facts");
    let member = |name: &str| {
        let id = &extraction
            .facts
            .declarations
            .iter()
            .find(|declaration| declaration.name == name)
            .expect("member")
            .id;
        extraction
            .reference_counts
            .iter()
            .find(|count| &count.declaration_id == id)
            .expect("counted")
            .count
    };
    assert_eq!(member("go"), 0, "a method call is an edge");
    assert_eq!(member("stop"), 1, "a method passed as a value escapes");
    assert_eq!(member("tick"), 0, "a comment is not a reference");
}
