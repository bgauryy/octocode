//! Every bound native reads from the contract (`query_schema_max`) is one
//! core still declares: a bound core drops must fail this test, never panic
//! a tool call.
use crate::id::ToolId;

#[test]
fn every_contract_maximum_native_reads_is_declared() {
    let call = regex::Regex::new(
        r#"query_schema_max\(\s*(?:crate::id::)?ToolId::(\w+),\s*(None|Some\("(\w+)"\)),\s*"(\w+)",?\s*\)"#,
    )
    .expect("pattern");
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    super::module_boundaries::rust_files(&src, &mut files);
    let mut reads = 0;
    for path in files {
        let source = crate::tools::test_support::production_text(
            &std::fs::read_to_string(&path).expect("source"),
        );
        for found in call.captures_iter(&source) {
            let variant = &found[1];
            let tool = ToolId::ALL
                .into_iter()
                .find(|tool| format!("{tool:?}") == variant)
                .unwrap_or_else(|| panic!("{}: no tool {variant}", path.display()));
            let operation = found.get(3).map(|operation| operation.as_str());
            let field = &found[4];
            assert!(
                crate::contracts::query_schema_number(tool, operation, field, "maximum").is_some(),
                "{}: {tool} {operation:?} {field} has no maximum in the contract",
                path.display()
            );
            reads += 1;
        }
    }
    assert!(reads > 0, "the scan found no query_schema_max read");
}
