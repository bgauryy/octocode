//! Failure wording shared by both history tools: the message and the hint
//! for a provider error.
use crate::providers::github::{ProviderError, ProviderErrorKind, ProviderErrorReason};
use crate::tools::gh_shared::{provider_hint, provider_message, validation_message};

/// The message and the optional hint for a history failure. `search`
/// distinguishes ghSearchHistory from the direct-fetch ghGetHistoryItem: a
/// search 422 names the query. (ghGetHistoryItem maps a bogus commit SHA,
/// GitHub 422 "No commit found for SHA: …", to a missing commit before it
/// gets here.) Without a hint here the runtime gives the one hint of the
/// error kind, the same for every GitHub tool.
pub(crate) fn history_failure(
    error: &ProviderError,
    search: bool,
) -> (String, Option<&'static str>) {
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
    (message, provider_hint(error))
}
