//! A runtime the server keeps, and when a clasify call rebuilds it.
use octocode_native::id::ToolId;

/// How long a kept runtime whose clasify provider ping failed keeps that
/// answer: a later clasify call rebuilds it, which pings again.
pub(super) const PROVIDER_RECHECK: std::time::Duration = std::time::Duration::from_secs(30);

/// A runtime a server keeps for one working directory and environment,
/// with the identity it was built under and when.
pub(super) struct Served {
    pub(super) runtime: octocode_native::runtime::ToolRuntime,
    pub(super) identity: String,
    pub(super) built: std::time::Instant,
}

impl Served {
    pub(super) fn provider_failed(&self) -> bool {
        matches!(
            self.runtime.availability(ToolId::Clasify.as_str()),
            Some(
                octocode_native::execution::availability::Availability::ProviderUnavailable { .. }
            )
        )
    }
}

/// Whether a `tool` call rebuilds a kept runtime: only a clasify call, only
/// after its failed provider ping is [`PROVIDER_RECHECK`] old, so other tools
/// never wait on a ping.
pub(super) fn recheck_provider(
    tool: &str,
    provider_failed: bool,
    age: std::time::Duration,
) -> bool {
    provider_failed && age >= PROVIDER_RECHECK && ToolId::from_name(tool) == Some(ToolId::Clasify)
}
