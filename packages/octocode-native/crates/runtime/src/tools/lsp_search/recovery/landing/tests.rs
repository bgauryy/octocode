//! Landings of the locations the three servers answer first.

use super::*;

/// The first definition answer of typescript-language-server, pyright and
/// rust-analyzer starts the declaration's name: the chain ends there. An
/// import, a rename and a barrel re-export are aliases and still hop.
#[test]
fn declarations_end_the_chain_and_bindings_hop() {
    for (file, source, at, expected) in [
        (
            "/repo/a.ts",
            "export function x() {}\n",
            (0, 16),
            Landing::Declaration,
        ),
        (
            "/repo/a.py",
            "def x():\n    pass\n",
            (0, 4),
            Landing::Declaration,
        ),
        (
            "/repo/src/a.rs",
            "pub fn x() {}\n",
            (0, 7),
            Landing::Declaration,
        ),
        (
            "/repo/b.ts",
            "import { x as y } from './a';\ny();\n",
            (0, 14),
            Landing::Alias,
        ),
        (
            "/repo/d.ts",
            "export { x as z } from './a';\n",
            (0, 14),
            Landing::Alias,
        ),
        (
            "/repo/c.ts",
            "export { x } from './a';\n",
            (0, 9),
            Landing::Alias,
        ),
        (
            "/repo/src/b.rs",
            "use crate::a::x as y;\n",
            (0, 19),
            Landing::Alias,
        ),
        // A use is neither: the chain asks the server.
        (
            "/repo/e.ts",
            "import { x } from './a';\nx();\n",
            (1, 0),
            Landing::Unknown,
        ),
    ] {
        let document =
            crate::tools::declarations_cache::file_document(source, file).expect("parse");
        assert_eq!(lands(&document, at.0, at.1), expected, "{file} {at:?}");
    }
}
