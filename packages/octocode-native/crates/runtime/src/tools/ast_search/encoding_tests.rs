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

/// A Latin-1 source named alone is read as a directory scan reads it: every
/// operation answers, and says it decoded the file as Latin-1.
#[test]
fn a_legacy_source_is_read_and_names_its_decoding_in_every_operation() {
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
        let out = run(&root.0, query.clone()).expect("legacy text is read");
        assert!(
            out.to_string()
                .contains(octocode_engine::text::LATIN1_WARNING),
            "{query}: {out}"
        );
    }
    // Binary content stays refused, under the one binary code.
    let binary = root.0.join("bin.ts");
    std::fs::write(&binary, b"export const s = 1;\0").expect("binary");
    let error = run(
        &root.0,
        json!({"operation":"match","path":binary,"rule":{"kind":"function_declaration"}}),
    )
    .expect_err("binary");
    assert_eq!(error.code, ErrorCode::BinaryFileUnsupported);
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
