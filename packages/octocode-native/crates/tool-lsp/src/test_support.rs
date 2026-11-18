//! Fixtures this crate's tests share with the tests that run lspSearch
//! from outside it.
#![expect(
    clippy::expect_used,
    reason = "fixtures for tests, compiled only for them (`test`, `test-support`): a broken fixture fails its test at once"
)]
use octocode_contract_types::LspSearchQuery;

pub fn query(value: serde_json::Value) -> LspSearchQuery {
    serde_json::from_value(value).expect("lsp query")
}

/// One 200 KB minified line: multi-byte text, a secret before and after the
/// declaration, and `function target` at about byte 150,000. Returns the line
/// and the declaration's zero-based UTF-16 column.
pub fn huge_line() -> (String, usize) {
    const TOKEN: &str = "ghp_abcdefghijklmnopqrstuvwxyz0123456789";
    let mut line = String::from("bundle;");
    let (mut index, mut column) = (0, None);
    while line.len() < 200_000 {
        if (50_000..50_040).contains(&line.len()) || (160_000..160_040).contains(&line.len()) {
            line.push_str(&format!("var token=\"{TOKEN}\";"));
        }
        if column.is_none() && line.len() >= 150_000 {
            column = Some(line.encode_utf16().count());
            line.push_str("function target(a){return a;}");
        }
        line.push_str(&format!("function f{index}(a){{return \"é{index}\"+a;}}"));
        index += 1;
    }
    (line, column.expect("declaration column"))
}
