//! A source's size alone never keeps it out of the graph: a generated file
//! past a mebibyte is a node linked like any other, and its snapshot stays
//! fresh until the file changes.
use super::*;

/// A generated table past a mebibyte, the function that reads it, and a
/// caller in another file.
fn large_fixture() -> (Fixture, String) {
    let dir = fixture();
    let rows = (0..80_000)
        .map(|row| format!("  \"row-{row:08}\",\n"))
        .collect::<String>();
    let source = format!(
        "export const ROWS = [\n{rows}];\nexport function rowAt(index: number) {{\n  return ROWS[index];\n}}\n"
    );
    assert!(source.len() > 1 << 20, "the fixture is past a mebibyte");
    write_under(
        &dir,
        "app/src",
        &[
            ("generated.ts", source.as_str()),
            (
                "reader.ts",
                "import { rowAt } from './generated';\nexport const first = () => rowAt(0);\n",
            ),
        ],
    );
    (dir, source)
}

#[test]
fn a_generated_source_past_a_mebibyte_is_a_linked_node() {
    let (dir, _) = large_fixture();
    let built = ingest_fixture(dir.0.as_path(), None);
    assert_eq!(built.class, ExitClass::Success, "{}", built.value);
    let callers = ask(dir.0.as_path(), "callers", Some("rowAt"), |_| {});
    assert_eq!(callers.class, ExitClass::Success, "{}", callers.value);
    let text = callers.value.to_string();
    assert!(text.contains("reader.ts"), "{text}");
    let stale = ask(dir.0.as_path(), "stale", None, |_| {});
    assert_eq!(stale.value["fresh"], true, "{}", stale.value);
}

/// The freshness check proves an unchanged large source by its digest, and
/// a changed one stale, without a size bound of its own.
#[test]
fn a_large_source_is_proven_by_its_digest() {
    let (dir, source) = large_fixture();
    let path = dir.0.as_path().join("app/src/generated.ts");
    let digest = octocode_engine::digest::sha256(source.as_bytes());
    assert_eq!(staleness::source_matches(&path, &digest, None), Some(true));
    std::fs::write(&path, source.replace("row-00000001", "row-0000000X")).expect("edit");
    assert_eq!(staleness::source_matches(&path, &digest, None), Some(false));
}

/// RV-GRAPH2 F4 (B135): dense code is graphed whole through graphBuild's
/// own path. Each declaration costs its source correspondence, which the
/// build's memory budget charges; no fixed per-file pool skips 40,000 Rust
/// functions or 14,000 TypeScript ones.
#[test]
fn dense_sources_are_graphed_whole_by_graph_build() {
    let dir = fixture();
    let rust = (0..40_000)
        .map(|i| format!("pub fn generated_{i:07}() {{}}\n"))
        .collect::<String>();
    let typescript = (0..14_000)
        .map(|id| {
            format!(
                "export function f{id}(a, b) {{ return f{}(b, a); }}\n",
                id + 1
            )
        })
        .collect::<String>();
    write_under(
        &dir,
        "app/src",
        &[
            ("generated.rs", rust.as_str()),
            ("dense.ts", typescript.as_str()),
        ],
    );
    let built = ingest_fixture(dir.0.as_path(), None);
    assert_eq!(built.class, ExitClass::Success, "{}", built.value);
    assert_eq!(
        built.value["scan"]["filesSkipped"], 0,
        "{}",
        built.value["scan"]
    );
    for name in ["generated_0039999", "f13999"] {
        let out = ask(dir.0.as_path(), "find", Some(name), |_| {});
        assert!(
            out.value["pagination"]["totalItems"]
                .as_u64()
                .is_some_and(|found| found >= 1),
            "{name}: {}",
            out.value
        );
    }
}
