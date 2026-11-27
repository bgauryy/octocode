use super::*;

/// The classified site of the first `needle` occurrence on 0-based `line`
/// (UTF-16 column of its first unit).
fn site(content: &str, path: &str, line: u32, needle: &str) -> ReferenceSite {
    let text = content.lines().nth(line as usize).expect("line");
    let byte = text.find(needle).expect("needle");
    let character = u32::try_from(text[..byte].encode_utf16().count()).expect("column");
    let sites =
        reference_sites(content, path, &[GraphPosition { line, character }]).expect("parsed");
    sites.into_iter().next().expect("one site")
}

fn is_call(content: &str, path: &str, line: u32, needle: &str) -> Option<bool> {
    site(content, path, line, needle).is_call
}

#[test]
fn an_implementing_declaration_is_not_a_call_but_its_member_call_is() {
    // The #480 shape: `ledger.ts` implements and calls `Port.read`.
    let ledger = "import { Port } from \"./port\";\nexport class Ledger implements Port {\n  read(at: number): string { return String(at); }\n}\nexport function useLedger(p: Port): string {\n  return p.read(2);\n}\n";
    let declaration = site(ledger, "ledger.ts", 2, "read");
    assert_eq!(declaration.is_call, Some(false));
    let call = site(ledger, "ledger.ts", 5, "read");
    assert_eq!(call.is_call, Some(true));
    let caller = call.caller.expect("enclosing function");
    assert_eq!(
        (caller.name.as_str(), caller.kind),
        ("useLedger", symbol_kind::FUNCTION)
    );
    assert_eq!(
        caller.selection_range.start,
        GraphPosition {
            line: 4,
            character: 16
        }
    );
}

#[test]
fn every_typescript_call_site_form_is_a_call() {
    let source = "f<T>(x);\nf?.(x);\nf`tpl`;\nnew F();\n(f as any)(x);\nregister(p.read);\nx[\"read\"]();\nclass C { #m() {} run() { this.#m(); } }\n@dec class D {}\n@ns.dec() class E {}\n";
    for (line, needle) in [
        (0, "f"),
        (1, "f"),
        (2, "f"),
        (3, "F"),
        (4, "f"),
        (5, "read"),
        (6, "read"),
        (7, "#m();"),
        (8, "dec"),
        (9, "dec"),
    ] {
        assert_eq!(
            is_call(source, "m.ts", line, needle),
            Some(true),
            "line {line} {needle}"
        );
    }
    // A reference to the bare private name points past its `#`.
    assert_eq!(is_call(source, "m.ts", 7, "m();"), Some(true));
}

#[test]
fn jsx_opening_tags_are_calls_and_closing_tags_are_not() {
    let source = "export const a = <Comp />;\nexport const b = <ui.Item></ui.Item>;\nexport const c = <Comp>x</Comp>;\n";
    assert_eq!(is_call(source, "v.tsx", 0, "Comp"), Some(true));
    assert_eq!(is_call(source, "v.tsx", 1, "Item"), Some(true));
    assert_eq!(is_call(source, "v.tsx", 2, "Comp"), Some(true));
    let closing = source
        .lines()
        .nth(2)
        .expect("line")
        .rfind("Comp")
        .expect("closing");
    let sites = reference_sites(
        source,
        "v.tsx",
        &[GraphPosition {
            line: 2,
            character: u32::try_from(closing).expect("column"),
        }],
    )
    .expect("parsed");
    assert_eq!(sites[0].is_call, Some(false));
}

#[test]
fn bindings_values_and_receivers_are_not_calls() {
    let source =
        "import { f } from './f';\nconst g = f;\nf.call(null);\nexport { f };\nfunction f2() {}\n";
    assert_eq!(is_call(source, "m.ts", 0, "f"), Some(false));
    assert_eq!(is_call(source, "m.ts", 1, "f"), Some(false));
    assert_eq!(is_call(source, "m.ts", 2, "f"), Some(false));
    assert_eq!(is_call(source, "m.ts", 3, "f"), Some(false));
    assert_eq!(is_call(source, "m.ts", 4, "f2"), Some(false));
}

#[test]
fn comments_are_undecided_and_strings_are_not_calls() {
    let source = "// f(1) in a comment\nconst s = 'f(2)';\n";
    assert_eq!(is_call(source, "m.ts", 0, "f"), None);
    assert_eq!(is_call(source, "m.ts", 1, "f"), Some(false));
}

#[test]
fn callers_are_the_innermost_callable_or_none_at_top_level() {
    let source = "const s = \"😀\"; run();\nclass K {\n  constructor() { run(); }\n  m() { const inner = () => { run(); }; }\n}\n";
    let top = site(source, "m.ts", 0, "run");
    assert_eq!(
        top.is_call,
        Some(true),
        "UTF-16 column after a non-BMP character"
    );
    assert!(top.caller.is_none());
    let constructor = site(source, "m.ts", 2, "run").caller.expect("constructor");
    assert_eq!(constructor.kind, symbol_kind::CONSTRUCTOR);
    let nested = site(source, "m.ts", 3, "run")
        .caller
        .expect("arrow function");
    assert_eq!(
        (nested.name.as_str(), nested.kind),
        ("inner", symbol_kind::FUNCTION)
    );
}

#[test]
fn unparsable_or_foreign_sources_are_not_classified() {
    let start = [GraphPosition {
        line: 0,
        character: 0,
    }];
    assert!(reference_sites(")))", "m.ts", &start).is_none());
    assert!(reference_sites("def f(): pass\n", "m.py", &start).is_none());
}

/// A name inside an import or a re-export from another module is an import
/// site: the binding that brings the symbol in, never a call or a use.
#[test]
fn import_and_re_export_names_are_import_sites() {
    let source = "import { read } from \"./port\";\nimport port from \"./port\";\nexport { read as load } from \"./port\";\nread(1);\nconst r = read;\nexport { r };\n";
    let import = site(source, "a.ts", 0, "read");
    assert_eq!(
        (import.is_import, import.is_call),
        (Some(true), Some(false))
    );
    assert_eq!(site(source, "a.ts", 2, "read").is_import, Some(true));
    // A default binding starts no name node of the parse: unclassified,
    // never a non-import (the caller's text rule reads its `import` line).
    assert_ne!(site(source, "a.ts", 1, "port").is_import, Some(false));
    let call = site(source, "a.ts", 3, "read");
    assert_eq!((call.is_import, call.is_call), (Some(false), Some(true)));
    assert_eq!(site(source, "a.ts", 4, "read").is_import, Some(false));
    // A local export names no other module: not an import.
    assert_ne!(site(source, "a.ts", 5, "r").is_import, Some(true));
}

/// Sites of one binding share its id: an import and its use name one
/// thing; a parameter of the same name is another; a property name, a
/// shorthand property (a binding and a property at once) and a global
/// name no binding of the file.
#[test]
fn sites_of_one_binding_share_its_id() {
    let source = "import { helper } from 'pkg';\nexport const p = helper('x');\nfunction f(helper) { return helper; }\nconst o = { helper };\no.helper;\nundeclared();\n";
    let import = site(source, "p.ts", 0, "helper").binding;
    assert!(import.is_some());
    assert_eq!(site(source, "p.ts", 1, "helper").binding, import);
    let parameter = site(source, "p.ts", 2, "helper").binding;
    assert!(parameter.is_some() && parameter != import);
    assert_eq!(site(source, "p.ts", 3, "helper").binding, None);
    assert_eq!(site(source, "p.ts", 4, "helper").binding, None);
    assert_eq!(site(source, "p.ts", 5, "undeclared").binding, None);
}
