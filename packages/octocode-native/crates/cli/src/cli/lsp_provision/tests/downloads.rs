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
