//! Failure wording shared by both history tools: the message and the hint
//! for a provider error.
use super::{GhFailure, provider_hint, provider_message, validation_message};
use octocode_github::{ProviderError, ProviderErrorKind, ProviderErrorReason};

/// The note of a pull-request row whose `commentsCount` is GitHub's
/// conversation count: GitHub's whole count (conversation, inline review
/// comments and review bodies), which its search sorts and filters by and
/// both history tools state, could not be read (GraphQL).
pub const CONVERSATION_ONLY: &str = "commentsCount counts conversation comments only: GitHub's whole count, which its sort and filters use, could not be read (GraphQL).";

/// The message and the optional hint for a history failure. `search`
/// distinguishes ghSearchHistory from the direct-fetch ghGetHistoryItem: a
/// search 422 names the query. (The transport types a bogus commit SHA,
/// GitHub's "No commit found", as a missing ref.) Without a hint here the
/// runtime gives the one hint of the error kind, the same for every GitHub
/// tool.
pub fn history_failure(error: &ProviderError, search: bool) -> (String, Option<&'static str>) {
    let message = match error.kind {
        ProviderErrorKind::NotFound
            if matches!(
                error.reason,
                Some(ProviderErrorReason::PullRequestIsIssue | ProviderErrorReason::RefNotFound)
            ) =>
        {
            error.message.to_string()
        }
        // ghGetHistoryItem names the missing item, ref, or repository
        // itself (a bare 404 cannot say which).
        ProviderErrorKind::NotFound if !search || error.reason.is_none() => {
            error.message.to_string()
        }
        ProviderErrorKind::Validation if error.status == Some(422) && search => {
            validation_message(error, "Invalid search query or request parameters")
        }
        ProviderErrorKind::Validation if error.status == Some(422) => {
            validation_message(error, "Invalid request parameters")
        }
        _ => provider_message(error),
    };
    // A search's missing login: GitHub's words name it, and the
    // `withoutUsers` lead is the repair; a hint would only restate them.
    let hint = provider_hint(error)
        .filter(|_| !(search && error.reason == Some(ProviderErrorReason::UserNotFound)));
    (message, hint)
}

/// The failure of a history tool's provider `error`: [`history_failure`]'s
/// message and hint (`search` as there).
pub fn history_tool_failure(error: ProviderError, search: bool) -> GhFailure {
    let (message, hint) = history_failure(&error, search);
    let failure = GhFailure::new(error, message);
    match hint {
        Some(hint) => failure.hint(hint),
        None => failure,
    }
}
