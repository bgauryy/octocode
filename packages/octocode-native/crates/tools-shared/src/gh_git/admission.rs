//! Admission of network git transfers through the shared GitHub limiter.
use super::{GitContext, GitError};
use octocode_contract_types::ErrorCode;

pub(crate) fn git_permit(
    context: &GitContext<'_>,
    repository_url: &str,
    token: Option<&str>,
) -> Result<tokio::sync::OwnedSemaphorePermit, GitError> {
    use octocode_github::LimiterKey;
    let Ok(url) = url::Url::parse(repository_url) else {
        return Err(GitError::new(
            ErrorCode::InvalidInput,
            "Repository URL could not be parsed",
        ));
    };
    context
        .git_limiter
        .acquire_blocking(&LimiterKey::for_url(&url, token), context.deadline, &|| {
            context.cancellation.check().is_err()
        })
        .map_err(admission_error)
}

/// Why the shared GitHub limiter refused a git transfer: the caller's own
/// cancellation or deadline, else a secondary-limit cooldown or an open
/// circuit, which the row states as the limiter's own provider failure
/// (rate-limit facts, retryable, `next.retry`).
pub fn admission_error(error: octocode_github::ProviderError) -> GitError {
    use octocode_github::ProviderErrorKind;
    match error.kind {
        ProviderErrorKind::Cancelled => {
            GitError::new(ErrorCode::Cancelled, error.message.to_string())
        }
        ProviderErrorKind::Timeout => GitError::new(ErrorCode::Timeout, error.message.to_string()),
        _ => GitError::provider(error),
    }
}
