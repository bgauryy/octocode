//! What an install asks GitHub: the release asset's live digest (the API host
//! is used for this metadata request only), the asset's bytes through the
//! release-host allowlist, and, for an immutable release, its attestation.
use super::READ_TIMEOUT;
use super::download::{download_client, download_error, fetch_allowlisted};
use std::process::Output;

/// The GitHub REST API, the one host a release's metadata is read from.
pub(super) const GITHUB_API: &str = "https://api.github.com";

/// What an immutable release's attestation check found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Attestation {
    /// A verified attestation of the release covers the asset's digest.
    Verified,
    /// No attestation was verified, so none could contradict the asset: `gh`
    /// is missing, old, offline or unauthenticated, or the release has none.
    /// The lock and live-digest checks still decide the install.
    NotChecked(String),
    /// A verified attestation of the release does not cover the asset's digest.
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
    /// Whether the release attestation of `repo@tag` covers an asset with
    /// this `sha256`.
    async fn attest(&self, repo: &str, tag: &str, sha256: &str) -> Attestation;
}

/// GitHub itself, with the user's Octocode GitHub token when one applies
/// (5000 requests an hour instead of 60).
pub(super) struct GitHub {
    pub(super) api_base: String,
    pub(super) token: Option<String>,
    /// The GitHub CLI that verifies release attestations.
    pub(super) gh: std::ffi::OsString,
}

impl GitHub {
    pub(super) fn new(token: Option<String>) -> Self {
        Self {
            api_base: GITHUB_API.to_owned(),
            token,
            gh: "gh".into(),
        }
    }
}

/// What `gh release verify <tag> -R <repo> --format json` reported, as an
/// attestation outcome for an asset with `sha256`.
///
/// A zero exit means gh verified the release's signed attestation; its stdout
/// then lists every attested subject. Observed on cli/cli v2.103.3 (an
/// immutable release, gh 2.96.0):
/// `{"attestation":{..},"verificationResult":{"statement":{"subject":[
/// {"uri":"pkg:github/cli/cli@v2.103.3","digest":{"sha1":".."}},
/// {"name":"gh_2.103.3_linux_386.deb","digest":{"sha256":".."}}, ..]}}}`.
/// Only that is a refusal ground: the verified subjects omit this asset's
/// digest. Any other outcome (non-zero exit for no attestation, HTTP 401,
/// offline or an unknown subcommand; output of another shape) verified
/// nothing, so it is `NotChecked`.
fn classify_verify(output: &Output, sha256: &str) -> Attestation {
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let reason = stderr
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .unwrap_or("no error text");
        return Attestation::NotChecked(format!("gh could not verify the release: {reason}"));
    }
    let subjects = serde_json::from_slice::<serde_json::Value>(&output.stdout)
        .ok()
        .and_then(|json| {
            json.pointer("/verificationResult/statement/subject")
                .and_then(serde_json::Value::as_array)
                .cloned()
        });
    let Some(subjects) = subjects else {
        return Attestation::NotChecked(
            "gh printed an attestation in a shape this build does not read".to_owned(),
        );
    };
    let covered = subjects.iter().any(|subject| {
        subject
            .pointer("/digest/sha256")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|digest| digest.eq_ignore_ascii_case(sha256))
    });
    if covered {
        Attestation::Verified
    } else {
        Attestation::Failed(format!(
            "the verified attestation lists no asset with sha256 {sha256}"
        ))
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

    async fn attest(&self, repo: &str, tag: &str, sha256: &str) -> Attestation {
        let mut command = tokio::process::Command::new(&self.gh);
        command
            .args(["release", "verify", tag, "-R", repo, "--format", "json"])
            .stdin(std::process::Stdio::null())
            .kill_on_drop(true);
        match tokio::time::timeout(READ_TIMEOUT, command.output()).await {
            Ok(Ok(output)) => classify_verify(&output, sha256),
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
