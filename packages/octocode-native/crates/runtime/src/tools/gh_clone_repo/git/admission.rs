//! Admission of network git transfers through the shared GitHub limiter.
use crate::tools::gh_clone_repo::{CloneContext, CloneError};
use octocode_contract_types::ErrorCode;
use std::time::Duration;

/// Longest cooldown a clone waits out before failing fast.
const GIT_COOLDOWN_CAP: Duration = Duration::from_secs(10);

pub(super) fn git_permit(
    context: &CloneContext<'_>,
    repository_url: &str,
    token: Option<&str>,
) -> Result<tokio::sync::OwnedSemaphorePermit, CloneError> {
    use octocode_github::{GitHubBudget, LimiterKey};
    let Ok(url) = url::Url::parse(repository_url) else {
        return Err(CloneError::new(
            ErrorCode::InvalidInput,
            "Repository URL could not be parsed",
        ));
    };
    let budget = GitHubBudget::global();
    let state_dir = context
        .config
        .persistent
        .then(|| context.config.home.join("tmp").join("ratelimit"));
    budget
        .acquire_git_blocking(
            &LimiterKey::for_url(&url, token),
            state_dir.as_deref(),
            GIT_COOLDOWN_CAP,
            context.deadline,
            &|| context.cancellation.check().is_err(),
        )
        .map_err(admission_error)
}

/// Why the shared GitHub limiter refused a git transfer: the caller's own
/// cancellation or deadline, else a secondary-limit cooldown or an open
/// circuit, which the row states as the limiter's own provider failure
/// (rate-limit facts, retryable, `next.retry`).
pub(super) fn admission_error(error: octocode_github::ProviderError) -> CloneError {
    use octocode_github::ProviderErrorKind;
    match error.kind {
        ProviderErrorKind::Cancelled => {
            CloneError::new(ErrorCode::Cancelled, error.message.to_string())
        }
        ProviderErrorKind::Timeout => {
            CloneError::new(ErrorCode::Timeout, error.message.to_string())
        }
        _ => CloneError::provider(error),
    }
}
