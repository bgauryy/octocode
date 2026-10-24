//! What the shared response stages ask about localSearch output.

use crate::response::output::ToolOutput;
use octocode_contract_types::ErrorCode;
use serde_json::Value;

pub(crate) struct Output;
impl ToolOutput for Output {
    fn error_hint(&self, code: ErrorCode, _query: &Value) -> Option<&'static str> {
        // No file under the path could be read: access, not the search terms.
        (code == ErrorCode::FileAccessFailed)
            .then_some("Check read access to the path; a broader search reads the same files.")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pagination_producers_follow_the_public_output_contract() {
        use super::super::types::{FilePagination, ItemPagination};
        use serde_json::json;
        let file = ItemPagination {
            page: Some(1),
            total_pages: Some(2),
            total_matches: Some(3),
            has_more: true,
            more_lines: Some(vec![4, 9]),
            more_lines_unlisted: None,
            out_of_range: false,
        };
        let outer = FilePagination {
            unit: None,
            page: 1,
            match_page: None,
            total_pages: 2,
            files_per_page: None,
            total_items: 3,
            has_more: true,
            out_of_range: false,
        };
        let output = json!({"results":[{"index":0,"data":{
            "files":[{"path":"src/lib.rs","pagination":file}],"pagination":outer
        }}]});
        crate::contracts::validate_output("localSearch", &output)
            .expect("serialized producer pages satisfy the canonical schema");
        assert_eq!(output["results"][0]["data"]["pagination"]["page"], 1);
    }

    /// A scope no file of which could be read is not fixed by a broader
    /// search: its tip says so.
    #[test]
    fn an_unreadable_scope_is_not_told_to_broaden() {
        let hint = Output
            .error_hint(ErrorCode::FileAccessFailed, &Value::Null)
            .expect("a tip");
        assert!(!hint.contains("Broaden"), "{hint}");
    }
}
