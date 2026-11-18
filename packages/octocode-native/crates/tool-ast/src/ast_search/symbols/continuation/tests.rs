use super::*;
use crate::ast_search::symbols::{outline::outline_file, select::select};
use octocode_runtime_base::execution::cancel::NeverCancel;
use octocode_runtime_base::test_support::{Fixture, simple_policy};

/// The weight tracks the heap the rows hold, at least what they serialize to and
/// within what a row table costs, as the store's byte budget expects, with and
/// without the provenance fields.
#[test]
fn a_stored_set_weighs_about_its_rows() {
    let root = Fixture::new();
    let source = (0..200).fold(String::new(), |source, index| {
        source + &format!("export function declared{index}(value: number) {{ return value; }}\n")
    });
    let file = root
        .write("a.ts", source)
        .canonicalize()
        .expect("canonical");
    let (paths, _) = simple_policy(&root.0);
    let row_paths = crate::ast_search::RowPaths::new(&paths, &file, true);
    for provenance in [false, true] {
        let q: AstSearchQuerySymbols = serde_json::from_value(
            json!({"operation":"symbols","path":file,"includeProvenance":provenance}),
        )
        .expect("query");
        let outline = outline_file(&q, (&file, "a.ts"), None, &NeverCancel)
            .expect("outline")
            .unwrap_or_else(|row| panic!("terminal row {row}"));
        let set = select(&q, &file, outline, row_paths, &NeverCancel).expect("set");
        let rows = serde_json::to_vec(&set.declarations).expect("rows").len();
        let weight = weight(&set);
        assert!(
            rows <= weight && weight <= rows * 16,
            "{weight} bytes for {rows} of rows (provenance: {provenance})"
        );
    }
}
