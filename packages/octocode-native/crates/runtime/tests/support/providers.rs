//! Fixture settings that point tools at mock servers: the classification provider and GitHub.

use super::{FIXTURE_LIMITS, Workspace};
use octocode_native::runtime::ToolRuntime;
use octocode_runtime_base::execution::RuntimeLimits;
use wiremock::MockServer;

/// The environment pointing clasify at the mock classification provider
/// `server`, plus `extra`.
pub fn provider_env<'a>(
    server: &MockServer,
    extra: &[(&'a str, String)],
) -> Vec<(&'a str, String)> {
    let mut env = vec![
        ("OCTOCODE_BETA", "true".to_owned()),
        ("OCTOCODE_TYPESAFE_KEY", "secret".to_owned()),
        ("OCTOCODE_TYPESAFE_HOST", server.uri()),
    ];
    env.extend(extra.iter().cloned());
    env
}

/// Runtime whose classification provider is the mock `server`, plus `extra`,
/// at the production network timeout: the fixture's 5 s is too narrow during
/// a cold native build with several mock servers active.
pub fn clasify_runtime(
    workspace: &Workspace,
    server: &MockServer,
    extra: &[(&str, String)],
) -> ToolRuntime {
    workspace.runtime_with(&provider_env(server, extra), PROVIDER_LIMITS)
}

/// The workspace plus the URL capture directory, as `ALLOWED_PATHS` grants.
pub fn capture_allowed(workspace: &Workspace) -> (&'static str, String) {
    (
        "ALLOWED_PATHS",
        format!(
            "{},{}",
            workspace.workspace.display(),
            workspace.home.join("tmp/capture/url").display()
        ),
    )
}

/// The `GITHUB_API_URL` setting that points GitHub tools at the mock `server`.
pub fn github_api(server: &MockServer) -> [(&'static str, String); 1] {
    [("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]
}

/// The classification provider's stated default window, in tokens: a mock
/// window reports a taken request's tokens at this many per its byte limit.
pub const WINDOW_TOKENS: usize = 32_000;
/// The stated long-context window, in tokens.
pub const LONG_WINDOW_TOKENS: usize = 262_000;

/// [`FIXTURE_LIMITS`] at the production network timeout, for HTTP-backed
/// provider fixtures.
pub const PROVIDER_LIMITS: RuntimeLimits = RuntimeLimits {
    network_timeout: RuntimeLimits::DEFAULT.network_timeout,
    classification_timeout: RuntimeLimits::DEFAULT.classification_timeout,
    ..FIXTURE_LIMITS
};
