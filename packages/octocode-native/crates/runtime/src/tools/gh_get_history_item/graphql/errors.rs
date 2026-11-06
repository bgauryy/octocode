//! The `errors[]` GitHub returns beside GraphQL data, described for a
//! reader (class, text, path) and placed by the PR field they nulled.
use super::document::Connection;
use octocode_github::GraphQlError;
use serde_json::Value;

/// One GraphQL error as a reader sees it: its class, text and the response
/// path GitHub nulled (`repository.pullRequest.body`).
fn describe_error(error: &GraphQlError) -> String {
    let class = error
        .error_type
        .as_deref()
        .or_else(|| error.extensions.get("code").and_then(Value::as_str));
    let mut text = match class {
        Some(class) => format!("{class}: {}", error.message),
        None => error.message.clone(),
    };
    let path = error
        .path
        .iter()
        .map(|segment| match segment {
            Value::String(name) => name.clone(),
            other => other.to_string(),
        })
        .collect::<Vec<_>>()
        .join(".");
    if !path.is_empty() {
        text.push_str(&format!(" at {path}"));
    }
    text
}

/// Every error of `errors`, described and joined.
pub(super) fn describe_errors<'a>(errors: impl IntoIterator<Item = &'a GraphQlError>) -> String {
    errors
        .into_iter()
        .map(describe_error)
        .collect::<Vec<_>>()
        .join("; ")
}

/// Why a GraphQL answer cannot serve the read: every error's class, text and
/// path.
pub(super) fn graphql_failure(errors: &[GraphQlError]) -> String {
    if errors.is_empty() {
        return "GraphQL returned no pull request".to_owned();
    }
    describe_errors(errors)
}

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

/// The warning for a connection GraphQL answered with `errors[]`.
pub(super) fn connection_warning(connection: &Connection, errors: &str) -> String {
    format!(
        "GraphQL answered {} with errors, so REST read them instead: {errors}",
        connection.section
    )
}
