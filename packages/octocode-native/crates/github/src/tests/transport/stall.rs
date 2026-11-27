//! A request ends when it stalls, not when its time is up: a body that keeps
//! flowing is never cut, and silence ends the request at the stall bound
//! while the caller's deadline stays far away.
use super::*;
use octocode_io::cancellation::Cancellation;
use std::time::Instant;
use tokio::io::AsyncWriteExt;

/// 14 bytes of JSON.
const BODY: &[u8; 14] = b"{\"ok\":true}   ";
const STALL: Duration = Duration::from_millis(200);

/// A request whose caller deadline is 3 s away and whose stall bound is `STALL`.
fn stalling_request() -> RequestContext {
    RequestContext::new(
        RequestBudget::new(
            Instant::now() + Duration::from_secs(3),
            Cancellation::new(),
            1 << 20,
        )
        .with_stall(STALL),
        None,
    )
}

/// A one-connection loopback API that runs `serve` on the first socket.
async fn serve_once<F, Fut>(serve: F) -> (GitHubEndpoint, tokio::task::JoinHandle<()>)
where
    F: FnOnce(tokio::net::TcpStream) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send,
{
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let endpoint = GitHubEndpoint::new(
        url::Url::parse(&format!(
            "http://{}/api/v3",
            listener.local_addr().expect("addr")
        ))
        .expect("URL"),
    )
    .expect("endpoint");
    let server = tokio::spawn(async move {
        if let Ok((mut socket, _)) = listener.accept().await {
            // Read the request head before answering.
            let mut buffer = [0_u8; 4096];
            let _ = tokio::io::AsyncReadExt::read(&mut socket, &mut buffer).await;
            serve(socket).await;
        }
    });
    (endpoint, server)
}

async fn get(
    endpoint: &GitHubEndpoint,
    context: &RequestContext,
) -> Result<ResponsePage, ProviderError> {
    anonymous_transport(endpoint)
        .execute(
            RequestSpec::get(endpoint.rest(&["x"]).expect("route")),
            context,
        )
        .await
}

#[tokio::test]
async fn a_body_that_keeps_flowing_past_the_stall_bound_is_read_whole() {
    let (endpoint, _server) = serve_once(|mut socket| async move {
        let _ = socket
            .write_all(
                b"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 14\r\n\r\n",
            )
            .await;
        // 14 bytes, one every 50 ms: 700 ms, over three stall bounds.
        for byte in BODY {
            let _ = socket.write_all(&[*byte]).await;
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await;
    let started = Instant::now();
    let page = get(&endpoint, &stalling_request())
        .await
        .expect("a flowing body is never cut");
    assert!(started.elapsed() > STALL * 3, "{:?}", started.elapsed());
    assert_eq!(page.body.as_ref(), BODY);
}

#[tokio::test]
async fn a_body_that_goes_silent_after_its_headers_fails_with_timeout_at_the_stall() {
    let (endpoint, _server) = serve_once(|mut socket| async move {
        let _ = socket
            .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 1000\r\n\r\nhello")
            .await;
        tokio::time::sleep(Duration::from_secs(30)).await;
    })
    .await;
    let started = Instant::now();
    let error = get(&endpoint, &stalling_request())
        .await
        .expect_err("silence ends the request");
    assert_eq!(error.kind, ProviderErrorKind::Timeout);
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "{:?}",
        started.elapsed()
    );
}

#[tokio::test]
async fn a_server_that_never_answers_fails_at_the_stall_not_the_callers_deadline() {
    let (endpoint, _server) = serve_once(|_socket| async move {
        tokio::time::sleep(Duration::from_secs(30)).await;
    })
    .await;
    let started = Instant::now();
    let error = get(&endpoint, &stalling_request())
        .await
        .expect_err("no headers");
    assert_eq!(error.kind, ProviderErrorKind::Timeout);
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "{:?}",
        started.elapsed()
    );
}
