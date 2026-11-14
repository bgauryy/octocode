use super::{CONNECT_TIMEOUT, DOWNLOAD_DEADLINE, MAX_REDIRECTS, READ_TIMEOUT};
use std::time::Duration;

/// GitHub release hosts permitted for downloads and every redirect hop. The
/// API host is not among them: it serves release metadata only
/// (`upstream::GITHUB_API`).
pub(super) const ALLOWED_HOSTS: [&str; 3] = [
    "github.com",
    "objects.githubusercontent.com",
    "release-assets.githubusercontent.com",
];

/// True when `url` is https and its host is on the release allowlist.
pub fn host_allowed(url: &str) -> bool {
    url::Url::parse(url).is_ok_and(|parsed| {
        parsed.scheme() == "https"
            && parsed
                .host_str()
                .is_some_and(|host| ALLOWED_HOSTS.contains(&host))
    })
}

/// HTTP client for release downloads starting at `url`: redirects are
/// followed by hand, and connect and per-read stalls are bounded.
pub(super) fn download_client(
    url: &str,
    read_timeout: Duration,
) -> Result<reqwest::Client, String> {
    octocode_io::http::client(&download_policy(url, read_timeout))
        .map_err(|e| format!("http client init failed: {e}"))
}

/// The client policy of [`download_client`]: a loopback host (a test's
/// release server) is reached directly, any other through the user's system
/// proxy ([`octocode_github::proxy_for`]).
pub(super) fn download_policy(
    url: &str,
    read_timeout: Duration,
) -> octocode_io::http::ClientPolicy {
    octocode_io::http::ClientPolicy {
        connect_timeout: Some(CONNECT_TIMEOUT),
        read_timeout: Some(read_timeout),
        proxy: url::Url::parse(url).map_or(octocode_io::http::Proxy::System, |url| {
            octocode_github::proxy_for(&url)
        }),
        ..octocode_io::http::ClientPolicy::default()
    }
}

/// Download at most `limit` bytes of `url` into `file`, following redirects
/// manually and re-checking the host allowlist on every hop, within
/// `DOWNLOAD_DEADLINE`. Signed release-asset query tokens are never echoed
/// into errors.
pub(super) async fn fetch_allowlisted(
    url: &str,
    file: tokio::fs::File,
    limit: u64,
) -> Result<tokio::fs::File, String> {
    tokio::time::timeout(
        DOWNLOAD_DEADLINE,
        follow_redirects(url, file, &host_allowed, limit),
    )
    .await
    .unwrap_or_else(|_| {
        Err(format!(
            "Download timed out after {}s",
            DOWNLOAD_DEADLINE.as_secs()
        ))
    })
}

/// A transport error without its request URL: release-asset redirects carry
/// signed query tokens, and reqwest's message would print them.
pub(super) fn download_error(error: reqwest::Error) -> String {
    format!("Download failed: {}", error.without_url())
}

/// [`fetch_allowlisted`] with the hop check given and no deadline, so a test
/// serves the asset from a loopback fixture.
pub(super) async fn follow_redirects(
    url: &str,
    mut file: tokio::fs::File,
    allowed: &dyn Fn(&str) -> bool,
    limit: u64,
) -> Result<tokio::fs::File, String> {
    // Built for the first hop the allowlist admits: a refused URL costs no
    // client (TLS roots, the system proxy lookup).
    let mut client: Option<reqwest::Client> = None;
    let mut current = url.to_string();
    for _ in 0..=MAX_REDIRECTS {
        if !allowed(&current) {
            let host = url::Url::parse(&current)
                .ok()
                .and_then(|u| u.host_str().map(str::to_string))
                .unwrap_or_else(|| "(unparseable url)".to_string());
            return Err(format!("Blocked non-allowlisted/insecure host: {host}"));
        }
        let request = match &client {
            Some(client) => client.get(&current),
            None => client
                .insert(download_client(&current, READ_TIMEOUT)?)
                .get(&current),
        };
        let response = request.send().await.map_err(download_error)?;
        let status = response.status();
        if status.is_redirection() {
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .ok_or_else(|| "Redirect without Location header".to_string())?;
            let next = url::Url::parse(&current)
                .map_err(|e| format!("invalid url: {e}"))?
                .join(location)
                .map_err(|e| format!("invalid redirect target: {e}"))?;
            current = next.to_string();
            continue;
        }
        if !status.is_success() {
            return Err(format!("Download failed: HTTP {}", status.as_u16()));
        }
        if let Some(len) = response.content_length()
            && len > limit
        {
            return Err(format!(
                "Download exceeds the {limit}-byte limit (Content-Length {len})"
            ));
        }
        use futures_util::StreamExt;
        use tokio::io::AsyncWriteExt;
        let mut written = 0_u64;
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(download_error)?;
            written = written.saturating_add(chunk.len() as u64);
            if written > limit {
                return Err(format!("Download exceeds the {limit}-byte limit"));
            }
            file.write_all(&chunk)
                .await
                .map_err(|e| format!("Writing the download failed: {e}"))?;
        }
        return Ok(file);
    }
    Err("Too many redirects".to_string())
}
