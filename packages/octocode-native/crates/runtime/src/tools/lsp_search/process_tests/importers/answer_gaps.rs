//! Sites an answer does not list inside files it covers: the server says
//! what each names. A spelling that names another declaration is no gap,
//! one that names the anchor is a use left out, and one the server does
//! not place is unverified and proves nothing.

use super::*;

/// [`IMPORTER_SERVER`] answering `definition` per line of `g*.ts`: line 3
/// names a declaration in `other.ts`, line 4 names nothing, and every
/// other position names the anchor. `references` from an importer lists
/// its lines 1 and 2 only, as the base server does.
fn gap_server() -> String {
    let cases = r"    case 'textDocument/definition': {
      const uri = p.textDocument.uri;
      const line = p.position.line;
      const other = 'file://' + root + '/other.ts';
      if (uri === other) return [{ uri, range: at(0, 16, 5) }];
      if (/\/g\d+\.ts$/.test(uri) && line === 2) return [{ uri: other, range: at(0, 16, 5) }];
      if (/\/g\d+\.ts$/.test(uri) && line === 3) return [];
      return [decl];
    }
";
    let start = IMPORTER_SERVER
        .find("    case 'textDocument/definition': {")
        .expect("definition case");
    let end = start
        + IMPORTER_SERVER[start..]
            .find("    case 'textDocument/references':")
            .expect("next case");
    format!(
        "{}{cases}{}",
        &IMPORTER_SERVER[..start],
        &IMPORTER_SERVER[end..]
    )
}

/// `references` for `target` over `a.ts`, the importer `f000.ts` and
/// `g000.ts` holding `content`.
fn references(content: &str) -> (Fixture, serde_json::Value) {
    let (dir, root, execution) = importer_fixture_serving(&[("f", 1)], &gap_server());
    std::fs::write(root.join("g000.ts"), content).expect("candidate");
    octocode_engine::stamp::settle_written_so_far();
    let pool = fake_server_pool();
    let paths = crate::tools::test_support::workspace_policy(&root);
    let query = serde_json::from_value(serde_json::json!({
        "operation": "references", "path": root.join("a.ts").to_string_lossy(),
        "symbolName": "target", "line": 1, "workspaceRoot": root.to_string_lossy()
    }))
    .expect("query");
    let row = tokio::runtime::Runtime::new()
        .expect("rt")
        .block_on(crate::tools::lsp_search::execute(
            query,
            &crate::execution::cancel::NeverCancel,
            &pool,
            &paths,
            &execution,
            None,
            crate::tools::lsp_search::test_answers(),
        ))
        .expect("row");
    assert_row_contract(ToolId::LspSearch, &row);
    (dir, row)
}

/// A file the answer covers also spells the name on a line that names
/// another declaration (`other.target()`): that line is no use of the
/// anchor, so the complete answer stays complete and names no gap.
#[test]
fn a_same_named_site_of_another_symbol_is_no_gap() {
    if !require_node() {
        return;
    }
    let (_dir, row) = references("import { target } from './a';\ntarget();\nother.target();\n");
    assert!(lists_path(&row, "g000.ts"), "{row}");
    assert!(row.get("isPartial").is_none(), "{row}");
    assert!(row.get("partialReasons").is_none(), "{row}");
    assert_eq!(row["coverage"]["state"], "complete", "{row}");
    assert!(!row.to_string().contains("g000.ts:3"), "{row}");
    assert!(!row.to_string().contains("leaves out"), "{row}");
}

/// Of three sites the answer does not list, the one that names the anchor
/// is a use left out (partial, named), the one the server does not place
/// is named as unverified, and the one that names another declaration is
/// not named at all.
#[test]
fn only_a_site_that_names_the_anchor_is_a_use_left_out() {
    if !require_node() {
        return;
    }
    let (_dir, row) = references(
        "import { target } from './a';\ntarget();\nother.target();\nlost.target();\ntarget.call(null);\n",
    );
    assert_eq!(row["isPartial"], true, "{row}");
    assert_eq!(
        row["partialReasons"],
        serde_json::json!(["serverUnanswered"]),
        "{row}"
    );
    let warnings = row["warnings"].as_array().expect("warnings");
    let naming = |text: &str| {
        warnings
            .iter()
            .filter_map(serde_json::Value::as_str)
            .find(|warning| warning.contains(text))
            .unwrap_or_else(|| panic!("no warning with {text}: {row}"))
    };
    let left_out = naming("leaves out 1 code use(s) of `target`");
    assert!(left_out.contains("g000.ts:5"), "{row}");
    assert!(!left_out.contains("g000.ts:4"), "{row}");
    let unverified = naming("unverified");
    assert!(unverified.contains("g000.ts:4"), "{row}");
    assert!(!unverified.contains("g000.ts:5"), "{row}");
    assert!(!row["warnings"].to_string().contains("g000.ts:3"), "{row}");
}

/// A site the server does not place proves no missing use: it is named as
/// unverified with the text lead, and the row is not partial.
#[test]
fn a_site_the_server_does_not_place_is_unverified_and_not_partial() {
    if !require_node() {
        return;
    }
    let (_dir, row) = references("import { target } from './a';\ntarget();\n\nlost.target();\n");
    assert!(row.get("isPartial").is_none(), "{row}");
    assert!(row.get("partialReasons").is_none(), "{row}");
    let warnings = row["warnings"].to_string();
    assert!(
        warnings.contains("g000.ts:4") && warnings.contains("unverified"),
        "{row}"
    );
    let lead = crate::tools::test_support::checks::related_call(&row, "localSearch", |_| true);
    assert!(
        lead["input"]["queries"].to_string().contains("g000.ts"),
        "{row}"
    );
}
