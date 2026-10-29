//! Sources astSearch cannot parse, named alone: text that is not Unicode
//! and a file whose extension has no grammar in this build.
use super::{AstSearchQuery, execute_ast};
use crate::execution::cancel::NeverCancel;
use crate::response::result::ToolError;
use crate::tools::test_support::{Fixture, simple_policy};
use octocode_contract_types::ErrorCode;
use serde_json::{Value, json};

fn run(root: &std::path::Path, query: Value) -> Result<Value, ToolError> {
    let (paths, security) = simple_policy(root);
    let query: AstSearchQuery = serde_json::from_value(query).expect("typed astSearch row");
    execute_ast(&query, &paths, &security, &NeverCancel, None)
}

/// A Latin-1 source named alone fails every operation with one reader-facing
/// message naming the file and the encoding, never an engine type name.
#[test]
fn a_non_unicode_source_names_its_encoding_in_every_operation() {
    let root = Fixture::new();
    let file = root.0.join("latin1.ts");
    std::fs::write(
        &file,
        b"export const s = \"caf\xe9\";\nexport function f() {}\n",
    )
    .expect("source");
    for query in [
        json!({"operation":"match","path":file,"rule":{"kind":"function_declaration"}}),
        json!({"operation":"symbols","path":file}),
        json!({"operation":"syntaxTree","path":file}),
    ] {
        let error = run(&root.0, query.clone()).expect_err("not Unicode");
        assert_eq!(error.code, ErrorCode::FileReadFailed, "{query}");
        assert_eq!(
            error.message,
            "Could not read latin1.ts as source text: not valid UTF-8 or BOM-marked UTF-16 text.",
            "{query}"
        );
    }
}

/// The recovery of a file with no grammar names the input field that sets
/// one (`parser`), not a field the contract does not have.
#[test]
fn an_unsupported_extension_recovery_names_the_parser_field() {
    let root = Fixture::new();
    let file = root.0.join("main.zig");
    std::fs::write(&file, "pub fn main() void {}\n").expect("source");
    let out = run(
        &root.0,
        json!({"operation":"match","path":file,"rule":{"pattern":"main"}}),
    )
    .expect("match");
    let recovery = out["diagnostics"][0]["recovery"]
        .as_str()
        .expect("recovery");
    assert!(recovery.contains("parser"), "{out}");
    assert!(!recovery.contains("language"), "{out}");
}
