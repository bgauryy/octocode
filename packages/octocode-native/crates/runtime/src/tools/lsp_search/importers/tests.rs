use super::super::blocking_cancellable;
use super::*;
use octocode_contract_types::ErrorCode;

mod cancellation;
mod coverage;
mod identity;
mod windows;

#[test]
fn occurrences_are_word_bounded_and_utf16() {
    let content = "const { stage } = require('./a');\nstaged(stage);\n// é stage\n";
    assert_eq!(
        occurrences(content, "stage"),
        vec![(0, 8, false), (1, 7, false), (2, 5, false)]
    );
    assert_eq!(occurrences("stage (1)", "stage"), vec![(0, 0, true)]);
}

#[test]
fn occurrences_include_late_call_sites() {
    let content = "// f f f f f\nf();\n";
    let found = occurrences(content, "f");
    assert_eq!(found.len(), 6);
    assert_eq!(found.last(), Some(&(1, 0, true)));
}

#[test]
fn importing_is_by_name_or_namespace_member() {
    let imports = |content: &str, file: &str| {
        octocode_engine::signatures::extract_graph_facts(content, file)
            .map(|facts| imports_name(&facts, content, "x"))
    };
    assert_eq!(
        imports("import { x } from './a';\nx();\n", "a.ts"),
        Some(true)
    );
    assert_eq!(
        imports("import { x as y } from './a';\n", "a.ts"),
        Some(true)
    );
    assert_eq!(
        imports("import * as a from './a';\na.x();\n", "a.ts"),
        Some(true)
    );
    assert_eq!(
        imports("import * as a from './a';\na.xy();\n", "a.ts"),
        Some(false)
    );
    // A bundled copy declaring its own `x` imports nothing of it.
    assert_eq!(
        imports(
            "import fs from 'node:fs';\nexport function x() { return fs; }\n",
            "copy.mjs"
        ),
        Some(false)
    );
}

#[test]
fn call_sites_prefer_calls_one_per_file() {
    let anchor = |path: &str, line, is_call| Anchor {
        path: path.to_owned(),
        line,
        character: 0,
        is_call,
    };
    let importers = Importers {
        anchors: vec![
            anchor("b", 0, false),
            anchor("b", 2, true),
            anchor("c", 1, false),
        ],
        ..Importers::default()
    };
    let sites = importers
        .call_sites()
        .iter()
        .map(|a| (a.path.as_str(), a.line))
        .collect::<Vec<_>>();
    assert_eq!(sites, vec![("b", 2), ("c", 1)]);
    assert_eq!(importers.per_file().count(), 2);
}

#[test]
fn applies_only_to_ts_js_incoming_operations() {
    assert!(applies(Some("javascript"), "references"));
    assert!(applies(Some("typescript"), "callers"));
    assert!(!applies(Some("rust"), "references"));
    assert!(!applies(Some("typescript"), "definition"));
    assert!(!applies(None, "references"));
}

fn candidates(count: usize) -> Vec<String> {
    (0..count)
        .map(|index| format!("/r/f{index:03}.ts"))
        .collect()
}
