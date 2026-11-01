//! Renaming imports read from the declarations parse.

use super::*;

/// The declarations parse alias recovery reads (shared, digest-cached)
/// carries every renaming import, and only an import whose imported name
/// the answer reported is a candidate: the full graph-facts walk finds no
/// other.
#[test]
fn renaming_imports_come_from_the_declarations_parse() {
    for (file, source, sites, expected) in [
        (
            "/repo/b.ts",
            "import { foo as bar, foo, baz as qux } from './a';\nbar();\n",
            vec![(0, 9)],
            vec![("bar".to_owned(), 0, 16)],
        ),
        (
            "/repo/src/b.rs",
            "use crate::a::foo as bar;\nuse crate::a::foo;\nfn f() { bar(); }\n",
            vec![(0, 14)],
            vec![("bar".to_owned(), 0, 21)],
        ),
        (
            "/repo/b.py",
            "from a import foo as bar\nbar()\n",
            vec![(0, 14)],
            vec![("bar".to_owned(), 0, 21)],
        ),
        // An answer that does not reach the import names no candidate.
        (
            "/repo/c.ts",
            "import { foo as bar } from './a';\n",
            vec![(5, 0)],
            vec![],
        ),
    ] {
        let document = crate::tools::declarations_cache::file_document(source, file)
            .expect("declarations parse");
        let full =
            octocode_engine::signatures::extract_graph_facts(source, file).expect("graph facts");
        assert_eq!(
            aliasing_imports(&document, "foo", &sites),
            expected,
            "{file}"
        );
        assert_eq!(aliasing_imports(&full, "foo", &sites), expected, "{file}");
    }
}
