//! What an install asks GitHub: the release asset's live digest (the API host
//! is used for this metadata request only), the asset's bytes through the
//! release-host allowlist, and, for an immutable release, its attestation.
use super::READ_TIMEOUT;
use super::download::{download_client, download_error, fetch_allowlisted};
use std::path::Path;

/// The GitHub REST API, the one host a release's metadata is read from.
pub(super) const GITHUB_API: &str = "https://api.github.com";

/// What an immutable release's attestation check found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Attestation {
    Verified,
    /// The check could not run (for example, `gh` is not on PATH).
    NotChecked(String),
    Failed(String),
}

/// The sources an install verifies against; injected so every path is
/// testable without the network.
pub(super) trait Upstream {
    /// GitHub's live `sha256` digest of `asset` in `repo@tag`: `Ok(None)` when
    /// the release, the asset or its digest is gone, `Err` when GitHub could
    /// not be asked (offline, rate limit, proxy).
    async fn live_digest(
        &self,
        repo: &str,
        tag: &str,
        asset: &str,
    ) -> Result<Option<String>, String>;
    /// Write the asset at `url` into `file` and hand the file back.
    async fn download(&self, url: &str, file: tokio::fs::File) -> Result<tokio::fs::File, String>;
    /// Verify the release attestation of the downloaded `file`.
    async fn attest(&self, repo: &str, tag: &str, file: &Path) -> Attestation;
}

/// GitHub itself, with the user's Octocode GitHub token when one applies
/// (5000 requests an hour instead of 60).
pub(super) struct GitHub {
    pub(super) api_base: String,
    pub(super) token: Option<String>,
}

impl GitHub {
    pub(super) fn new(token: Option<String>) -> Self {
        Self {
            api_base: GITHUB_API.to_owned(),
            token,
        }
    }
}

/// The `sha256` digest a release JSON reports for `asset`, lowercase hex.
pub(super) fn release_asset_digest(release: &serde_json::Value, asset: &str) -> Option<String> {
    release
        .get("assets")?
        .as_array()?
        .iter()
        .find(|entry| entry.get("name").and_then(serde_json::Value::as_str) == Some(asset))?
        .get("digest")?
        .as_str()?
        .strip_prefix("sha256:")
        .filter(|hex| hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
        .map(str::to_ascii_lowercase)
}

impl Upstream for GitHub {
    async fn live_digest(
        &self,
        repo: &str,
        tag: &str,
        asset: &str,
    ) -> Result<Option<String>, String> {
        let url = format!("{}/repos/{repo}/releases/tags/{tag}", self.api_base);
        let client = download_client(READ_TIMEOUT)?;
        let mut request = client
            .get(&url)
            .header(reqwest::header::ACCEPT, "application/vnd.github+json")
            .header(reqwest::header::USER_AGENT, "octocode")
            .header("X-GitHub-Api-Version", "2022-11-28");
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        let response = tokio::time::timeout(READ_TIMEOUT, request.send())
            .await
            .map_err(|_| format!("no answer within {}s", READ_TIMEOUT.as_secs()))?
            .map_err(|e| download_error(e).replacen("Download failed", "request failed", 1))?;
        let status = response.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !status.is_success() {
            return Err(format!("GitHub API answered HTTP {}", status.as_u16()));
        }
        let release: serde_json::Value = response
            .json()
            .await
            .map_err(|e| format!("unreadable release JSON: {}", e.without_url()))?;
        Ok(release_asset_digest(&release, asset))
    }

    async fn download(&self, url: &str, file: tokio::fs::File) -> Result<tokio::fs::File, String> {
        fetch_allowlisted(url, file).await
    }

    async fn attest(&self, repo: &str, tag: &str, file: &Path) -> Attestation {
        let mut command = tokio::process::Command::new("gh");
        command
            .args(["release", "verify-asset", tag])
            .arg(file)
            .args(["-R", repo])
            .stdin(std::process::Stdio::null())
            .kill_on_drop(true);
        match tokio::time::timeout(READ_TIMEOUT, command.output()).await {
            Ok(Ok(output)) if output.status.success() => Attestation::Verified,
            Ok(Ok(output)) => {
                Attestation::Failed(String::from_utf8_lossy(&output.stderr).trim().to_owned())
            }
            Ok(Err(e)) if e.kind() == std::io::ErrorKind::NotFound => {
                Attestation::NotChecked("gh is not on PATH".to_owned())
            }
            Ok(Err(e)) => Attestation::NotChecked(format!("gh did not start: {e}")),
            Err(_) => Attestation::NotChecked(format!(
                "gh gave no answer within {}s",
                READ_TIMEOUT.as_secs()
            )),
        }
    }
}
