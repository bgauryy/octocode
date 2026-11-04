//! Vue, Svelte, Astro, MDX and HTML have no grammar, so they are no graph
//! nodes; the scan says so, and a `dependents` answer names the importers it
//! cannot hold as edges.
use super::*;

fn unlinked(out: &Value) -> Vec<(String, u64)> {
    out["summary"]["unlinkedPathImports"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|row| {
            (
                row["file"].as_str().unwrap_or_default().to_owned(),
                row["line"].as_u64().unwrap_or_default(),
            )
        })
        .collect()
}

/// Every markup kind's script region is read, whatever its attributes.
#[test]
fn dependents_names_markup_importers_it_holds_no_edge_for() {
    let temp = fixture(&[
        ("src/other.ts", "export function other() {}\n"),
        ("src/alone.ts", "export function alone() {}\n"),
        (
            "src/Child.svelte",
            "<script>import { other } from './other'; other();</script>\n<p>hi</p>\n",
        ),
        (
            "src/App.vue",
            "<template><div/></template>\n<script setup lang=\"ts\">\nimport { other } from './other';\nimport type { T } from './alone';\nother();\n</script>\n",
        ),
        (
            "src/Page.astro",
            "---\nimport { other } from './other';\n---\n<p>{other()}</p>\n",
        ),
        (
            "src/doc.mdx",
            "import { other } from './other';\n\n# Title\n",
        ),
        (
            "src/page.html",
            "<!doctype html>\n<SCRIPT type=\"module\">\nimport { other } from './other';\n</SCRIPT>\n",
        ),
        ("src/plain.html", "<p>import nothing from './other';</p>\n"),
    ]);
    let root = ingested(&temp);
    let answer = ask(root, "dependents", Some("src/other.ts"));
    assert_eq!(files(&answer), Vec::<&str>::new(), "{answer}");
    assert_eq!(
        unlinked(&answer),
        [
            ("src/App.vue".to_owned(), 3),
            ("src/Child.svelte".to_owned(), 1),
            ("src/Page.astro".to_owned(), 2),
            ("src/doc.mdx".to_owned(), 1),
            ("src/page.html".to_owned(), 3),
        ],
        "{answer}"
    );
    assert_eq!(
        unlinked(&ask(root, "dependents", Some("src/alone.ts"))),
        [("src/App.vue".to_owned(), 4)]
    );
    let stats = ask(root, "stats", None);
    let not_linked = &stats["scan"]["markupNotLinked"];
    for kind in octocode_engine::signatures::languages::SCRIPT_BEARING_MARKUP
        .iter()
        .filter(|kind| **kind != "htm")
    {
        assert_eq!(not_linked[kind], 1, "{kind}: {stats}");
    }
}

/// A tsconfig alias links from markup as it does from code, and a specifier
/// outside the scan names no scanned file.
#[test]
fn markup_imports_resolve_as_code_imports_do() {
    let temp = fixture(&[
        (
            "tsconfig.json",
            r#"{"compilerOptions":{"baseUrl":".","paths":{"@/*":["src/*"]}}}"#,
        ),
        ("src/lib.ts", "export function lib() {}\n"),
        (
            "App.vue",
            "<script lang=\"ts\">\nimport { lib } from '@/lib';\nimport vue from 'vue';\n</script>\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(
        unlinked(&ask(root, "dependents", Some("src/lib.ts"))),
        [("App.vue".to_owned(), 2)]
    );
}

/// A project without markup states nothing.
#[test]
fn a_scan_without_markup_says_nothing_of_it() {
    let temp = fixture(&[("a.ts", "export const a = 1;\n")]);
    let root = ingested(&temp);
    assert_eq!(
        ask(root, "stats", None)["scan"]["markupNotLinked"],
        Value::Null
    );
}

/// The markup walk reads the same tree as the facts walk: what the path
/// policy withholds is one disclosure, counted once.
#[test]
fn the_markup_walk_counts_nothing_twice() {
    let temp = fixture(&[
        ("a.ts", "export const a = 1;\n"),
        ("b.vue", "<script>import './a';</script>\n"),
        ("src/.aws/key.ts", "export const k = 1;\n"),
    ]);
    let root = ingested(&temp);
    let answer = ask(root, "diagnostics", Some("path-policy-withheld"));
    let rows = answer["rows"].as_array().cloned().unwrap_or_default();
    assert_eq!(rows.len(), 1, "{answer}");
    let message = rows[0]["message"].as_str().unwrap_or_default();
    assert!(
        message.contains(".aws/") && !message.contains('×'),
        "{message}"
    );
}
