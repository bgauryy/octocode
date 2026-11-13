//! The `errors[]` GitHub returns beside GraphQL data, placed by the PR field
//! they nulled.
use octocode_github::GraphQlError;
use serde_json::Value;

/// The PR field an error nulled: the response key under
/// `repository.pullRequest` (a connection's key, or a header field), `None`
/// for an error outside the pull request.
pub(super) fn error_field(error: &GraphQlError) -> Option<&str> {
    match error.path.as_slice() {
        [
            Value::String(repository),
            Value::String(pull),
            Value::String(field),
            ..,
        ] if repository == "repository" && pull == "pullRequest" => Some(field),
        _ => None,
    }
}
