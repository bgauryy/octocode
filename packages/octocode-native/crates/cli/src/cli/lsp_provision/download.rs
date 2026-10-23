use super::{CONNECT_TIMEOUT, DOWNLOAD_DEADLINE, MAX_DOWNLOAD_BYTES, MAX_REDIRECTS, READ_TIMEOUT};
use std::time::Duration;

/// GitHub / HashiCorp release hosts permitted for downloads and every
/// redirect hop.
pub(super) const ALLOWED_HOSTS: [&str; 4] = [
    "github.com",
    "objects.githubusercontent.com",
    "release-assets.githubusercontent.com",
    "releases.hashicorp.com",
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

/// HTTP client for release downloads: redirects are followed by hand, and
/// connect and per-read stalls are bounded.
pub(super) fn download_client(read_timeout: Duration) -> Result<reqwest::Client, String> {
    octocode_io::http::client(&octocode_io::http::ClientPolicy {
        connect_timeout: Some(CONNECT_TIMEOUT),
        read_timeout: Some(read_timeout),
        ..octocode_io::http::ClientPolicy::default()
    })
    .map_err(|e| format!("http client init failed: {e}"))
}

/// Download `url` into `file`, following redirects manually and re-checking
/// the host allowlist on every hop, within `DOWNLOAD_DEADLINE`. Signed
/// release-asset query tokens are never echoed into errors.
pub(super) async fn fetch_allowlisted(
    url: &'static str,
    file: tokio::fs::File,
) -> Result<tokio::fs::File, String> {
    tokio::time::timeout(DOWNLOAD_DEADLINE, fetch_following_redirects(url, file))
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

pub(super) async fn fetch_following_redirects(
    url: &'static str,
    mut file: tokio::fs::File,
) -> Result<tokio::fs::File, String> {
    let client = download_client(READ_TIMEOUT)?;
    let mut current = url.to_string();
    for _ in 0..=MAX_REDIRECTS {
        if !host_allowed(&current) {
            let host = url::Url::parse(&current)
                .ok()
                .and_then(|u| u.host_str().map(str::to_string))
                .unwrap_or_else(|| "(unparseable url)".to_string());
            return Err(format!("Blocked non-allowlisted/insecure host: {host}"));
        }
        let response = client.get(&current).send().await.map_err(download_error)?;
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
            && len > MAX_DOWNLOAD_BYTES
        {
            return Err(format!(
                "Download exceeds the {MAX_DOWNLOAD_BYTES}-byte limit (Content-Length {len})"
            ));
        }
        use futures_util::StreamExt;
        use tokio::io::AsyncWriteExt;
        let mut written = 0_u64;
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(download_error)?;
            written = written.saturating_add(chunk.len() as u64);
            if written > MAX_DOWNLOAD_BYTES {
                return Err(format!(
                    "Download exceeds the {MAX_DOWNLOAD_BYTES}-byte limit"
                ));
            }
            file.write_all(&chunk)
                .await
                .map_err(|e| format!("Writing the download failed: {e}"))?;
        }
        return Ok(file);
    }
    Err("Too many redirects".to_string())
}
