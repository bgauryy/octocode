use super::*;

#[test]
fn a_stalled_download_fails_within_the_read_timeout() {
    // A peer that accepts and never answers must not hold the install lock.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("addr");
    let _peer = std::thread::spawn(move || {
        let (_stream, _) = listener.accept().expect("accept");
        std::thread::sleep(Duration::from_secs(5));
    });
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    // Built inside the runtime, as production builds it inside the async fetch.
    let (result, elapsed) = runtime.block_on(async {
        let client = download_client(Duration::from_millis(200)).expect("client");
        // Platform client setup is separate from a request's read timeout.
        let started = std::time::Instant::now();
        let result = client.get(format!("http://{addr}/")).send().await;
        (result, started.elapsed())
    });
    let error = result.expect_err("a stalled peer must not hang the request");
    assert!(
        error.is_timeout(),
        "the stalled read must time out: {error}"
    );
    assert!(elapsed < Duration::from_secs(4), "request took {elapsed:?}");
}

#[test]
fn host_allowlist_enforces_https_and_hosts() {
    assert!(host_allowed(
        "https://github.com/clangd/clangd/releases/download/x.zip"
    ));
    assert!(host_allowed(
        "https://release-assets.githubusercontent.com/foo?token=abc"
    ));
    assert!(!host_allowed("http://github.com/foo")); // not https
    assert!(!host_allowed("https://evil.example.com/foo")); // not allowlisted
    assert!(!host_allowed("not a url"));
}

#[test]
fn a_download_error_never_echoes_a_signed_url() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let message = runtime.block_on(async {
        let client = download_client(Duration::from_millis(200)).expect("client");
        // Port 1 refuses the connection; the URL carries a signed token.
        let error = client
            .get("http://127.0.0.1:1/asset?X-Amz-Signature=SECRET-TOKEN")
            .send()
            .await
            .expect_err("refused");
        download_error(error)
    });
    assert!(!message.contains("SECRET-TOKEN"), "{message}");
    assert!(message.starts_with("Download failed"), "{message}");
}

/// A loopback release host: each path answers with its own status, headers
/// and body; its base URL.
fn release_host(routes: &'static [(&'static str, &'static str)]) -> String {
    use std::io::{BufRead, BufReader};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let base = format!("http://{}", listener.local_addr().expect("addr"));
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut reader = BufReader::new(&stream);
            let mut request_line = String::new();
            let _ = reader.read_line(&mut request_line);
            let mut line = String::new();
            while reader.read_line(&mut line).is_ok_and(|n| n > 2) {
                line.clear();
            }
            let path = request_line.split(' ').nth(1).unwrap_or("/");
            let answer = routes.iter().find(|(route, _)| *route == path).map_or(
                "HTTP/1.1 404 X\r\ncontent-length: 0\r\n\r\n",
                |(_, answer)| answer,
            );
            let _ = (&stream).write_all(answer.as_bytes());
        }
    });
    base
}

const ROUTES: &[(&str, &str)] = &[
    (
        "/asset",
        "HTTP/1.1 200 OK\r\ncontent-length: 5\r\nconnection: close\r\n\r\nbytes",
    ),
    (
        "/moved",
        "HTTP/1.1 302 Found\r\nlocation: /asset\r\ncontent-length: 0\r\nconnection: close\r\n\r\n",
    ),
    (
        "/away",
        "HTTP/1.1 302 Found\r\nlocation: https://elsewhere.example/asset\r\ncontent-length: 0\r\nconnection: close\r\n\r\n",
    ),
    (
        "/nowhere",
        "HTTP/1.1 302 Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n",
    ),
    (
        "/loop",
        "HTTP/1.1 302 Found\r\nlocation: /loop\r\ncontent-length: 0\r\nconnection: close\r\n\r\n",
    ),
    (
        "/claims-huge",
        "HTTP/1.1 200 OK\r\ncontent-length: 1000000\r\nconnection: close\r\n\r\n",
    ),
    (
        "/streams-long",
        "HTTP/1.1 200 OK\r\nconnection: close\r\n\r\n0123456789",
    ),
];

/// Download `path` from `base` into a fresh file, with every hop on `base`
/// allowed and `limit` bytes accepted: the bytes written, or the refusal.
async fn fetch(base: &str, path: &str, limit: u64) -> Result<Vec<u8>, String> {
    let dir = tempfile::tempdir().expect("dir");
    let spool = dir.path().join("asset.download");
    let file = tokio::fs::File::create(&spool).await.expect("spool");
    let on_base = |url: &str| url.starts_with(base);
    let mut file =
        super::download::follow_redirects(&format!("{base}{path}"), file, &on_base, limit).await?;
    tokio::io::AsyncWriteExt::flush(&mut file)
        .await
        .expect("flush");
    Ok(std::fs::read(&spool).expect("downloaded"))
}

/// A redirect is followed to the asset; every hop is checked, so a redirect
/// off the allowed hosts is refused, and one without a target, a loop, an
/// error status or an asset over the limit fails without writing past it.
#[tokio::test(flavor = "current_thread")]
async fn a_download_follows_allowed_redirects_and_refuses_the_rest() {
    let base = release_host(ROUTES);
    assert_eq!(
        fetch(&base, "/asset", 1024).await.as_deref(),
        Ok(&b"bytes"[..])
    );
    assert_eq!(
        fetch(&base, "/moved", 1024).await.as_deref(),
        Ok(&b"bytes"[..])
    );
    let refused = |path: &'static str, limit: u64| {
        let base = base.clone();
        async move { fetch(&base, path, limit).await.expect_err(path) }
    };
    assert!(refused("/away", 1024).await.contains("elsewhere.example"));
    assert!(refused("/nowhere", 1024).await.contains("Location"));
    assert!(refused("/loop", 1024).await.contains("redirects"));
    assert!(refused("/missing", 1024).await.contains("404"));
    assert!(
        refused("/claims-huge", 1024)
            .await
            .contains("Content-Length")
    );
    assert!(refused("/streams-long", 4).await.contains("limit"));
}

/// Production's hop check is the release allowlist: a plain-HTTP first hop
/// is refused before any request.
#[tokio::test(flavor = "current_thread")]
async fn the_allowlist_refuses_a_plain_http_first_hop() {
    let dir = tempfile::tempdir().expect("dir");
    let file = tokio::fs::File::create(dir.path().join("spool"))
        .await
        .expect("spool");
    let error = super::download::fetch_allowlisted("http://github.com/asset", file)
        .await
        .expect_err("plain http");
    assert!(error.contains("github.com"), "{error}");
}
