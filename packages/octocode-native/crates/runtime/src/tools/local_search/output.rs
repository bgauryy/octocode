//! What the shared response stages ask about localSearch output.

use crate::response::output::ToolOutput;
use octocode_contract_types::ErrorCode;
use serde_json::Value;

pub(crate) struct Output;
impl ToolOutput for Output {
    fn error_hint(&self, code: ErrorCode, _query: &Value) -> Option<&'static str> {
        Some(match code {
            // No file under the path could be read: access, not the search terms.
            ErrorCode::FileAccessFailed => {
                "Check read access to the path; a broader search reads the same files."
            }
            ErrorCode::InvalidPattern => {
                "Correct the regex named in the error. If the intended match is literal text, set regexEngine to literal and keep matchString unchanged."
            }
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pagination_producers_follow_the_public_output_contract() {
        use super::super::types::Paging;
        use serde_json::json;
        let file = Paging::of("matches", (0, 2, 3), true, false);
        let outer = Paging::of("files", (0, 1, 3), true, false);
        let output = json!({"results":[{"index":0,"data":{
            "files":[{"path":"src/lib.rs","pagination":file}],"pagination":outer
        }}]});
        crate::contracts::validate_output("localSearch", &output)
            .expect("serialized producer pages satisfy the canonical schema");
        assert_eq!(
            output["results"][0]["data"]["pagination"],
            json!({"unit":"files","offset":0,"length":1,"total":3,"hasMore":true})
        );
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
